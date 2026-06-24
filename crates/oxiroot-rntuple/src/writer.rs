//! Writing an RNTuple into a ROOT file.
//!
//! [`write_rntuple_file`] writes a whole RNTuple in one shot, supporting scalar
//! (`bool`/`i32`/`i64`/`f32`/`f64`), `std::string`, and `std::vector<T>` fields
//! in a single cluster, with non-split column encodings and optional page
//! compression. [`RNTupleWriter`] writes those same field types one cluster per
//! batch, so a large dataset need not be held in memory at once. The header/page/
//! page-list/footer envelopes are written as raw blobs at the offsets the anchor
//! (and the page locators) point to; only the anchor is a `TKey`. Validated by
//! reading the result back and by official ROOT / uproot.

use std::io::{self, Seek, SeekFrom, Write};
use std::path::Path;

use oxiroot_io_core::buffer::WBuffer;
use oxiroot_io_core::error::{Error, Result};
use oxiroot_io_core::{key_len, write_key_header, Compression};

use crate::column::ColumnType;

const K_BYTE_COUNT_MASK: u32 = 0x4000_0000;
const DATIME: u32 = 0x7d7a_79ca;
const FILE_VERSION: u32 = 62400;

/// The on-disk bytes for one page: ROOT-compressed when `compression != 0` and
/// the result is actually smaller, otherwise the raw column bytes. A reader
/// tells the two apart by comparing the on-disk size to the uncompressed size
/// (derived from the element count), exactly as ROOT does.
fn on_disk_page(page: &[u8], compression: u32) -> Vec<u8> {
    if compression == 0 {
        return page.to_vec();
    }
    match oxiroot_compress::compress(page, compression) {
        Ok(compressed) if compressed.len() < page.len() => compressed,
        _ => page.to_vec(),
    }
}

const ROLE_LEAF: u16 = 0;
const ROLE_COLLECTION: u16 = 1;

/// A column of data for one RNTuple field.
pub enum Column {
    /// `bool` (Bit column).
    Bool(Vec<bool>),
    /// 32-bit signed integers.
    I32(Vec<i32>),
    /// 64-bit signed integers.
    I64(Vec<i64>),
    /// 32-bit unsigned integers.
    U32(Vec<u32>),
    /// 64-bit unsigned integers.
    U64(Vec<u64>),
    /// 32-bit floats.
    F32(Vec<f32>),
    /// 64-bit floats.
    F64(Vec<f64>),
    /// `std::string`.
    Str(Vec<String>),
    /// `std::vector<bool>`.
    VecBool(Vec<Vec<bool>>),
    /// `std::vector<float>`.
    VecF32(Vec<Vec<f32>>),
    /// `std::vector<double>`.
    VecF64(Vec<Vec<f64>>),
    /// `std::vector<int32_t>`.
    VecI32(Vec<Vec<i32>>),
    /// `std::vector<int64_t>`.
    VecI64(Vec<Vec<i64>>),
}

impl Column {
    /// Number of top-level entries.
    fn len(&self) -> usize {
        match self {
            Column::Bool(v) => v.len(),
            Column::I32(v) => v.len(),
            Column::I64(v) => v.len(),
            Column::U32(v) => v.len(),
            Column::U64(v) => v.len(),
            Column::F32(v) => v.len(),
            Column::F64(v) => v.len(),
            Column::Str(v) => v.len(),
            Column::VecBool(v) => v.len(),
            Column::VecF32(v) => v.len(),
            Column::VecF64(v) => v.len(),
            Column::VecI32(v) => v.len(),
            Column::VecI64(v) => v.len(),
        }
    }
}

/// A named RNTuple field.
pub struct Field {
    /// Field name.
    pub name: String,
    /// Field data.
    pub data: Column,
}

impl Field {
    /// A field named `name` holding `data`.
    pub fn new(name: impl Into<String>, data: Column) -> Field {
        Field {
            name: name.into(),
            data,
        }
    }
}

/// Generate `Field::<name>(name, Vec<T>)` shortcuts, e.g. `Field::f64("pt", v)`.
macro_rules! field_ctors {
    ($($method:ident => $variant:ident($elem:ty)),* $(,)?) => {
        impl Field {
            $(
                #[doc = concat!("A field holding `", stringify!($variant), "` data.")]
                pub fn $method(name: impl Into<String>, data: Vec<$elem>) -> Field {
                    Field::new(name, Column::$variant(data))
                }
            )*
        }
    };
}

field_ctors! {
    bools => Bool(bool),
    i32 => I32(i32),
    i64 => I64(i64),
    u32 => U32(u32),
    u64 => U64(u64),
    f32 => F32(f32),
    f64 => F64(f64),
    strings => Str(String),
    vec_bool => VecBool(Vec<bool>),
    vec_i32 => VecI32(Vec<i32>),
    vec_i64 => VecI64(Vec<i64>),
    vec_f32 => VecF32(Vec<f32>),
    vec_f64 => VecF64(Vec<f64>),
}

// --- internal lowered model ------------------------------------------------

struct FieldPlan {
    name: String,
    type_name: String,
    parent_id: u32,
    role: u16,
}

struct ColumnPlan {
    column_type: ColumnType,
    bits: u16,
    field_id: u32,
    page: Vec<u8>,
    n: u32,
}

fn le_bytes<T, const N: usize>(values: &[T], to: impl Fn(&T) -> [u8; N]) -> Vec<u8> {
    let mut out = Vec::with_capacity(values.len() * N);
    for v in values {
        out.extend_from_slice(&to(v));
    }
    out
}

fn pack_bits(v: &[bool]) -> Vec<u8> {
    let mut out = vec![0u8; v.len().div_ceil(8)];
    for (i, &b) in v.iter().enumerate() {
        if b {
            out[i >> 3] |= 1 << (i & 7);
        }
    }
    out
}

/// Cumulative end offsets (Index64) for collections, plus the flattened data.
fn flatten<T: Clone>(v: &[Vec<T>]) -> (Vec<u64>, Vec<T>) {
    let mut offsets = Vec::with_capacity(v.len());
    let mut data = Vec::new();
    for inner in v {
        data.extend_from_slice(inner);
        offsets.push(data.len() as u64);
    }
    (offsets, data)
}

/// Lower user fields into field and column plans (children appended after the
/// top-level fields), returning the top-level entry count.
fn lower(fields: &[Field]) -> (Vec<FieldPlan>, Vec<ColumnPlan>, u32) {
    let n_entries = fields.first().map(|f| f.data.len() as u32).unwrap_or(0);
    let mut field_plans: Vec<FieldPlan> = Vec::new();
    let mut columns: Vec<ColumnPlan> = Vec::new();
    let mut children: Vec<(u32, FieldPlan, ColumnPlan)> = Vec::new();

    let leaf = |name: &str, ty: &str, parent: u32| FieldPlan {
        name: name.to_string(),
        type_name: ty.to_string(),
        parent_id: parent,
        role: ROLE_LEAF,
    };

    for (i, f) in fields.iter().enumerate() {
        let fid = i as u32;
        let mut col = |ct, bits, page, n: usize| {
            columns.push(ColumnPlan {
                column_type: ct,
                bits,
                field_id: fid,
                page,
                n: n as u32,
            })
        };
        match &f.data {
            Column::Bool(v) => {
                field_plans.push(leaf(&f.name, "bool", fid));
                col(ColumnType::Bit, 1, pack_bits(v), v.len());
            }
            Column::I32(v) => {
                field_plans.push(leaf(&f.name, "std::int32_t", fid));
                col(
                    ColumnType::Int32,
                    32,
                    le_bytes(v, |x| x.to_le_bytes()),
                    v.len(),
                );
            }
            Column::I64(v) => {
                field_plans.push(leaf(&f.name, "std::int64_t", fid));
                col(
                    ColumnType::Int64,
                    64,
                    le_bytes(v, |x| x.to_le_bytes()),
                    v.len(),
                );
            }
            Column::U32(v) => {
                field_plans.push(leaf(&f.name, "std::uint32_t", fid));
                col(
                    ColumnType::UInt32,
                    32,
                    le_bytes(v, |x| x.to_le_bytes()),
                    v.len(),
                );
            }
            Column::U64(v) => {
                field_plans.push(leaf(&f.name, "std::uint64_t", fid));
                col(
                    ColumnType::UInt64,
                    64,
                    le_bytes(v, |x| x.to_le_bytes()),
                    v.len(),
                );
            }
            Column::F32(v) => {
                field_plans.push(leaf(&f.name, "float", fid));
                col(
                    ColumnType::Real32,
                    32,
                    le_bytes(v, |x| x.to_le_bytes()),
                    v.len(),
                );
            }
            Column::F64(v) => {
                field_plans.push(leaf(&f.name, "double", fid));
                col(
                    ColumnType::Real64,
                    64,
                    le_bytes(v, |x| x.to_le_bytes()),
                    v.len(),
                );
            }
            Column::Str(v) => {
                field_plans.push(leaf(&f.name, "std::string", fid));
                let mut bytes = Vec::new();
                let mut offsets = Vec::with_capacity(v.len());
                for s in v {
                    bytes.extend_from_slice(s.as_bytes());
                    offsets.push(bytes.len() as u64);
                }
                let n_chars = bytes.len();
                col(
                    ColumnType::Index64,
                    64,
                    le_bytes(&offsets, |x| x.to_le_bytes()),
                    v.len(),
                );
                col(ColumnType::Char, 8, bytes, n_chars);
            }
            Column::VecF32(v) => {
                field_plans.push(FieldPlan {
                    name: f.name.clone(),
                    type_name: "std::vector<float>".into(),
                    parent_id: fid,
                    role: ROLE_COLLECTION,
                });
                let (offsets, data) = flatten(v);
                col(
                    ColumnType::Index64,
                    64,
                    le_bytes(&offsets, |x| x.to_le_bytes()),
                    v.len(),
                );
                children.push((
                    fid,
                    leaf("_0", "float", 0),
                    ColumnPlan {
                        column_type: ColumnType::Real32,
                        bits: 32,
                        field_id: 0,
                        page: le_bytes(&data, |x| x.to_le_bytes()),
                        n: data.len() as u32,
                    },
                ));
            }
            Column::VecF64(v) => {
                field_plans.push(FieldPlan {
                    name: f.name.clone(),
                    type_name: "std::vector<double>".into(),
                    parent_id: fid,
                    role: ROLE_COLLECTION,
                });
                let (offsets, data) = flatten(v);
                col(
                    ColumnType::Index64,
                    64,
                    le_bytes(&offsets, |x| x.to_le_bytes()),
                    v.len(),
                );
                children.push((
                    fid,
                    leaf("_0", "double", 0),
                    ColumnPlan {
                        column_type: ColumnType::Real64,
                        bits: 64,
                        field_id: 0,
                        page: le_bytes(&data, |x| x.to_le_bytes()),
                        n: data.len() as u32,
                    },
                ));
            }
            Column::VecI32(v) => {
                field_plans.push(FieldPlan {
                    name: f.name.clone(),
                    type_name: "std::vector<std::int32_t>".into(),
                    parent_id: fid,
                    role: ROLE_COLLECTION,
                });
                let (offsets, data) = flatten(v);
                col(
                    ColumnType::Index64,
                    64,
                    le_bytes(&offsets, |x| x.to_le_bytes()),
                    v.len(),
                );
                children.push((
                    fid,
                    leaf("_0", "std::int32_t", 0),
                    ColumnPlan {
                        column_type: ColumnType::Int32,
                        bits: 32,
                        field_id: 0,
                        page: le_bytes(&data, |x| x.to_le_bytes()),
                        n: data.len() as u32,
                    },
                ));
            }
            Column::VecI64(v) => {
                field_plans.push(FieldPlan {
                    name: f.name.clone(),
                    type_name: "std::vector<std::int64_t>".into(),
                    parent_id: fid,
                    role: ROLE_COLLECTION,
                });
                let (offsets, data) = flatten(v);
                col(
                    ColumnType::Index64,
                    64,
                    le_bytes(&offsets, |x| x.to_le_bytes()),
                    v.len(),
                );
                children.push((
                    fid,
                    leaf("_0", "std::int64_t", 0),
                    ColumnPlan {
                        column_type: ColumnType::Int64,
                        bits: 64,
                        field_id: 0,
                        page: le_bytes(&data, |x| x.to_le_bytes()),
                        n: data.len() as u32,
                    },
                ));
            }
            Column::VecBool(v) => {
                field_plans.push(FieldPlan {
                    name: f.name.clone(),
                    type_name: "std::vector<bool>".into(),
                    parent_id: fid,
                    role: ROLE_COLLECTION,
                });
                let (offsets, data) = flatten(v);
                col(
                    ColumnType::Index64,
                    64,
                    le_bytes(&offsets, |x| x.to_le_bytes()),
                    v.len(),
                );
                children.push((
                    fid,
                    leaf("_0", "bool", 0),
                    ColumnPlan {
                        column_type: ColumnType::Bit,
                        bits: 1,
                        field_id: 0,
                        page: pack_bits(&data),
                        n: data.len() as u32,
                    },
                ));
            }
        }
    }

    for (parent, mut field, mut column) in children {
        let child_id = field_plans.len() as u32;
        field.parent_id = parent;
        column.field_id = child_id;
        field_plans.push(field);
        columns.push(column);
    }

    (field_plans, columns, n_entries)
}

// --- envelope / frame / string primitives ---------------------------------

fn rstr(s: &str) -> Vec<u8> {
    let mut out = (s.len() as u32).to_le_bytes().to_vec();
    out.extend_from_slice(s.as_bytes());
    out
}

fn envelope(type_id: u16, payload: &[u8]) -> Vec<u8> {
    let length = (8 + payload.len() + 8) as u64;
    let word = (type_id as u64) | (length << 16);
    let mut out = word.to_le_bytes().to_vec();
    out.extend_from_slice(payload);
    let checksum = xxhash_rust::xxh3::xxh3_64(&out);
    out.extend_from_slice(&checksum.to_le_bytes());
    out
}

fn record_frame(payload: &[u8]) -> Vec<u8> {
    let size = (8 + payload.len()) as i64;
    let mut out = size.to_le_bytes().to_vec();
    out.extend_from_slice(payload);
    out
}

fn list_frame(items: &[Vec<u8>]) -> Vec<u8> {
    let body_len: usize = items.iter().map(|i| i.len()).sum();
    let size = (8 + 4 + body_len) as i64;
    let mut out = (-size).to_le_bytes().to_vec();
    out.extend_from_slice(&(items.len() as u32).to_le_bytes());
    for item in items {
        out.extend_from_slice(item);
    }
    out
}

// --- envelope builders ------------------------------------------------------

fn build_header(name: &str, fields: &[FieldPlan], cols: &[ColumnPlan]) -> Vec<u8> {
    let mut p = Vec::new();
    p.extend_from_slice(&0i64.to_le_bytes()); // feature flags
    p.extend_from_slice(&rstr(name));
    p.extend_from_slice(&rstr("")); // description
    p.extend_from_slice(&rstr("oxiroot")); // writer

    let field_records: Vec<Vec<u8>> = fields
        .iter()
        .map(|f| {
            let mut r = Vec::new();
            r.extend_from_slice(&0u32.to_le_bytes()); // field version
            r.extend_from_slice(&0u32.to_le_bytes()); // type version
            r.extend_from_slice(&f.parent_id.to_le_bytes());
            r.extend_from_slice(&f.role.to_le_bytes()); // struct role
            r.extend_from_slice(&0u16.to_le_bytes()); // flags
            r.extend_from_slice(&rstr(&f.name));
            r.extend_from_slice(&rstr(&f.type_name));
            r.extend_from_slice(&rstr("")); // type alias
            r.extend_from_slice(&rstr("")); // description
            record_frame(&r)
        })
        .collect();
    p.extend_from_slice(&list_frame(&field_records));

    let column_records: Vec<Vec<u8>> = cols
        .iter()
        .map(|c| {
            let mut r = Vec::new();
            r.extend_from_slice(&(c.column_type as u16).to_le_bytes());
            r.extend_from_slice(&c.bits.to_le_bytes());
            r.extend_from_slice(&c.field_id.to_le_bytes());
            r.extend_from_slice(&0u16.to_le_bytes()); // flags
            r.extend_from_slice(&0u16.to_le_bytes()); // representation index
            record_frame(&r)
        })
        .collect();
    p.extend_from_slice(&list_frame(&column_records));

    p.extend_from_slice(&list_frame(&[])); // alias columns
    p.extend_from_slice(&list_frame(&[])); // extra type info

    envelope(0x01, &p)
}

fn build_page_list(
    n_entries: u32,
    page_offsets: &[usize],
    disk_sizes: &[usize],
    cols: &[ColumnPlan],
    compression: u32,
    header_checksum: u64,
) -> Vec<u8> {
    let mut p = Vec::new();
    p.extend_from_slice(&header_checksum.to_le_bytes());

    let mut summary = Vec::new();
    summary.extend_from_slice(&0u64.to_le_bytes()); // first entry
    summary.extend_from_slice(&(n_entries as u64).to_le_bytes()); // num entries (flags=0)
    p.extend_from_slice(&list_frame(&[record_frame(&summary)]));

    let column_frames: Vec<Vec<u8>> = cols
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let mut page = Vec::new();
            page.extend_from_slice(&(c.n as i32).to_le_bytes()); // positive: no checksum
            page.extend_from_slice(&(disk_sizes[i] as i32).to_le_bytes()); // on-disk locator size
            page.extend_from_slice(&(page_offsets[i] as u64).to_le_bytes()); // locator offset
            let mut body = Vec::new();
            body.extend_from_slice(&1u32.to_le_bytes()); // one page
            body.extend_from_slice(&page);
            body.extend_from_slice(&0i64.to_le_bytes()); // element offset
            body.extend_from_slice(&compression.to_le_bytes()); // compression settings
            let size = (8 + body.len()) as i64;
            let mut frame = (-size).to_le_bytes().to_vec();
            frame.extend_from_slice(&body);
            frame
        })
        .collect();
    let inner = list_frame(&column_frames); // over columns
    p.extend_from_slice(&list_frame(&[inner])); // over clusters

    envelope(0x03, &p)
}

fn build_footer(
    n_entries: u32,
    num_clusters: u32,
    page_list_offset: usize,
    page_list_len: usize,
    header_checksum: u64,
) -> Vec<u8> {
    let mut p = Vec::new();
    p.extend_from_slice(&0i64.to_le_bytes()); // feature flags
    p.extend_from_slice(&header_checksum.to_le_bytes());

    let mut ext = Vec::new();
    for _ in 0..4 {
        ext.extend_from_slice(&list_frame(&[]));
    }
    p.extend_from_slice(&record_frame(&ext));

    // One cluster group spanning every cluster; it links to the single page-list
    // envelope that details all clusters' pages.
    let mut cg = Vec::new();
    cg.extend_from_slice(&0u64.to_le_bytes()); // min entry
    cg.extend_from_slice(&(n_entries as u64).to_le_bytes()); // entry span
    cg.extend_from_slice(&num_clusters.to_le_bytes()); // num clusters
    cg.extend_from_slice(&(page_list_len as u64).to_le_bytes()); // envelope link: uncompressed len
    cg.extend_from_slice(&(page_list_len as i32).to_le_bytes()); // locator size
    cg.extend_from_slice(&(page_list_offset as u64).to_le_bytes()); // locator offset
    p.extend_from_slice(&list_frame(&[record_frame(&cg)]));

    // Linked attribute sets (RNTuple format >= 1.0.1.0); empty here.
    p.extend_from_slice(&list_frame(&[]));

    envelope(0x02, &p)
}

fn build_anchor(
    seek_header: usize,
    len_header: usize,
    seek_footer: usize,
    len_footer: usize,
) -> Vec<u8> {
    let mut fields = Vec::with_capacity(64);
    fields.extend_from_slice(&1u16.to_be_bytes()); // epoch
    fields.extend_from_slice(&0u16.to_be_bytes()); // major
    fields.extend_from_slice(&1u16.to_be_bytes()); // minor
    fields.extend_from_slice(&1u16.to_be_bytes()); // patch
    fields.extend_from_slice(&(seek_header as u64).to_be_bytes());
    fields.extend_from_slice(&(len_header as u64).to_be_bytes());
    fields.extend_from_slice(&(len_header as u64).to_be_bytes());
    fields.extend_from_slice(&(seek_footer as u64).to_be_bytes());
    fields.extend_from_slice(&(len_footer as u64).to_be_bytes());
    fields.extend_from_slice(&(len_footer as u64).to_be_bytes());
    fields.extend_from_slice(&0x4000_0000u64.to_be_bytes()); // max key size

    let checksum = xxhash_rust::xxh3::xxh3_64(&fields);

    let mut obj = Vec::new();
    obj.extend_from_slice(&((66u32) | K_BYTE_COUNT_MASK).to_be_bytes());
    obj.extend_from_slice(&2u16.to_be_bytes()); // class version
    obj.extend_from_slice(&fields);
    obj.extend_from_slice(&checksum.to_be_bytes());
    obj
}

/// Build a complete ROOT file containing one RNTuple named `ntuple_name`,
/// optionally compressing pages (`compression` is e.g. `Compression::None` or
/// `Compression::Zstd(5)`).
pub fn rntuple_file_bytes(
    file_name: &str,
    ntuple_name: &str,
    fields: &[Field],
    compression: Compression,
) -> Vec<u8> {
    let compression = compression.setting();
    let (field_plans, cols, n_entries) = lower(fields);

    let header_env = build_header(ntuple_name, &field_plans, &cols);
    let header_checksum =
        u64::from_le_bytes(header_env[header_env.len() - 8..].try_into().unwrap());

    // On-disk page bytes (compressed when it helps) and their sizes.
    let disk_pages: Vec<Vec<u8>> = cols
        .iter()
        .map(|c| on_disk_page(&c.page, compression))
        .collect();
    let disk_sizes: Vec<usize> = disk_pages.iter().map(|p| p.len()).collect();

    let mut w = WBuffer::new();

    // --- File header (100 bytes). ---
    w.bytes(b"root");
    w.be_u32(FILE_VERSION);
    w.be_u32(100);
    let p_end = w.reserve(4);
    w.be_u32(0); // fSeekFree
    w.be_u32(0); // fNbytesFree
    w.be_u32(0); // nfree
    let p_nbytes_name = w.reserve(4);
    w.u8(4); // fUnits
    w.be_u32(compression); // fCompress
    w.be_u32(0); // fSeekInfo
    w.be_u32(0); // fNbytesInfo
    w.be_u16(1);
    w.bytes(&[0u8; 16]);
    while w.len() < 100 {
        w.u8(0);
    }

    // --- Root directory name key + TDirectory (at 100). ---
    let first_klen = key_len("TFile", file_name, "");
    let name_title_len = (1 + file_name.len()) + 1;
    let f_nbytes_name = first_klen as usize + name_title_len;
    let first_obj_len = (name_title_len + 30 + 18) as u32;
    write_key_header(
        &mut w,
        "TFile",
        file_name,
        "",
        first_obj_len,
        first_obj_len,
        100,
        0,
    );
    w.string(file_name);
    w.string("");
    w.be_i16(5);
    w.be_u32(DATIME);
    w.be_u32(DATIME);
    let p_dir_nbytes_keys = w.reserve(4);
    w.be_i32(f_nbytes_name as i32);
    w.be_u32(100);
    w.be_u32(0);
    let p_dir_seek_keys = w.reserve(4);
    w.be_u16(1);
    w.bytes(&[0u8; 16]);

    // --- RNTuple blobs: header, pages, page list, footer. ---
    let seek_header = w.len();
    w.bytes(&header_env);
    let mut page_offsets = Vec::with_capacity(cols.len());
    for dp in &disk_pages {
        page_offsets.push(w.len());
        w.bytes(dp);
    }
    let page_list_offset = w.len();
    let page_list_env = build_page_list(
        n_entries,
        &page_offsets,
        &disk_sizes,
        &cols,
        compression,
        header_checksum,
    );
    w.bytes(&page_list_env);
    let seek_footer = w.len();
    let footer_env = build_footer(
        n_entries,
        1,
        page_list_offset,
        page_list_env.len(),
        header_checksum,
    );
    w.bytes(&footer_env);

    // --- Anchor key + object. ---
    let anchor_obj = build_anchor(seek_header, header_env.len(), seek_footer, footer_env.len());
    let anchor_seek = w.len();
    let anchor_len = anchor_obj.len() as u32;
    write_key_header(
        &mut w,
        "ROOT::RNTuple",
        ntuple_name,
        "",
        anchor_len,
        anchor_len,
        anchor_seek as u64,
        100,
    );
    w.bytes(&anchor_obj);

    // --- Key list (one entry: the anchor). ---
    let keylist_seek = w.len();
    let keylist_obj_len = 4 + key_len("ROOT::RNTuple", ntuple_name, "") as u32;
    write_key_header(
        &mut w,
        "TFile",
        file_name,
        "",
        keylist_obj_len,
        keylist_obj_len,
        keylist_seek as u64,
        100,
    );
    w.be_i32(1); // nkeys
    write_key_header(
        &mut w,
        "ROOT::RNTuple",
        ntuple_name,
        "",
        anchor_len,
        anchor_len,
        anchor_seek as u64,
        100,
    );
    let keylist_nbytes = key_len("TFile", file_name, "") as u32 + keylist_obj_len;
    let f_end = w.len() as u32;

    w.patch_be_u32(p_end, f_end);
    w.patch_be_u32(p_nbytes_name, f_nbytes_name as u32);
    w.patch_be_u32(p_dir_nbytes_keys, keylist_nbytes);
    w.patch_be_u32(p_dir_seek_keys, keylist_seek as u32);

    w.into_vec()
}

/// Write a one-RNTuple ROOT file to `path`, optionally compressing pages
/// (`compression` is e.g. `Compression::None` or `Compression::Zstd(5)`).
pub fn write_rntuple_file(
    path: &Path,
    ntuple_name: &str,
    fields: &[Field],
    compression: Compression,
) -> Result<()> {
    let file_name = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("file.root");
    std::fs::write(
        path,
        rntuple_file_bytes(file_name, ntuple_name, fields, compression),
    )?;
    Ok(())
}

// --- streaming, multi-cluster writer --------------------------------------

/// One page's location for the page list (one page per column per cluster).
struct PageRec {
    offset: u64,
    disk_size: usize,
    n_elements: u32,
    element_offset: i64,
}

/// A batch's full lowered schema identity. Every batch must produce an equal
/// one, otherwise its pages would be appended under the first batch's header and
/// silently mis-described. Compares field identity (name, type, parent, role) as
/// well as physical columns — `(type, bits)` alone would let a field rename or
/// reorder slip through.
#[derive(PartialEq, Eq)]
struct SchemaSig {
    /// `(name, type_name, parent_id, role)` per field, in lowered order.
    fields: Vec<(String, String, u32, u16)>,
    /// `(column type, bit width, owning field id)` per physical column.
    columns: Vec<(u16, u16, u32)>,
}

fn schema_sig(field_plans: &[FieldPlan], cols: &[ColumnPlan]) -> SchemaSig {
    SchemaSig {
        fields: field_plans
            .iter()
            .map(|f| (f.name.clone(), f.type_name.clone(), f.parent_id, f.role))
            .collect(),
        columns: cols
            .iter()
            .map(|c| (c.column_type as u16, c.bits, c.field_id))
            .collect(),
    }
}

/// Schema + header bookkeeping, fixed once the first batch defines it.
struct HeaderState {
    seek: u64,
    len: usize,
    checksum: u64,
    /// The lowered schema the first batch committed — must match every batch.
    signature: SchemaSig,
}

/// A streaming RNTuple writer: each [`write_batch`](RNTupleWriter::write_batch)
/// flushes one *cluster* to the sink, so a large dataset can be written one
/// chunk at a time without ever holding it all in memory. Call
/// [`finish`](RNTupleWriter::finish) to write the page list, footer, and anchor.
///
/// Handles the same field types as [`write_rntuple_file`] — scalars,
/// `std::string`, and `std::vector<T>` — writing each batch's collection/string
/// index offsets relative to its own cluster, as the format requires.
pub struct RNTupleWriter<W: Write + Seek> {
    sink: W,
    pos: u64,
    file_name: String,
    ntuple_name: String,
    compression: u32,
    // TFile pointers to patch once the layout is known.
    p_end: u64,
    p_nbytes_name: u64,
    p_dir_nbytes_keys: u64,
    p_dir_seek_keys: u64,
    f_nbytes_name: u32,
    // Set when the first batch defines the schema and writes the header.
    header: Option<HeaderState>,
    element_base: Vec<u64>,
    // Accumulated per-cluster metadata.
    total_entries: u64,
    summaries: Vec<(u64, u64)>,
    cluster_pages: Vec<Vec<PageRec>>,
}

impl RNTupleWriter<std::fs::File> {
    /// Create a streaming RNTuple file at `path`.
    pub fn create(path: &Path, ntuple_name: &str, compression: Compression) -> Result<Self> {
        let file_name = path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("file.root")
            .to_string();
        let file = std::fs::File::create(path)?;
        RNTupleWriter::new(file, &file_name, ntuple_name, compression)
    }
}

impl<W: Write + Seek> RNTupleWriter<W> {
    /// Begin writing into an arbitrary seekable sink (the TFile header and root
    /// directory are written immediately, with pointers to patch at the end).
    pub fn new(
        mut sink: W,
        file_name: &str,
        ntuple_name: &str,
        compression: Compression,
    ) -> Result<Self> {
        let compression = compression.setting();
        let mut w = WBuffer::new();

        // TFile header (100 bytes); record the offsets to patch later.
        w.bytes(b"root");
        w.be_u32(FILE_VERSION);
        w.be_u32(100); // fBEGIN
        let p_end = w.len() as u64;
        w.be_u32(0); // fEND
        w.be_u32(0); // fSeekFree
        w.be_u32(0); // fNbytesFree
        w.be_u32(0); // nfree
        let p_nbytes_name = w.len() as u64;
        w.be_u32(0); // fNbytesName
        w.u8(4); // fUnits
        w.be_u32(compression); // fCompress
        w.be_u32(0); // fSeekInfo
        w.be_u32(0); // fNbytesInfo
        w.be_u16(1);
        w.bytes(&[0u8; 16]);
        while w.len() < 100 {
            w.u8(0);
        }

        // Root directory name key + TDirectory record (at fBEGIN = 100).
        let first_klen = key_len("TFile", file_name, "");
        let name_title_len = (1 + file_name.len()) + 1;
        let f_nbytes_name = (first_klen as usize + name_title_len) as u32;
        let first_obj_len = (name_title_len + 30 + 18) as u32;
        write_key_header(
            &mut w,
            "TFile",
            file_name,
            "",
            first_obj_len,
            first_obj_len,
            100,
            0,
        );
        w.string(file_name);
        w.string("");
        w.be_i16(5);
        w.be_u32(DATIME);
        w.be_u32(DATIME);
        let p_dir_nbytes_keys = w.len() as u64;
        w.be_u32(0); // fNbytesKeys
        w.be_i32(f_nbytes_name as i32);
        w.be_u32(100); // fSeekDir
        w.be_u32(0); // fSeekParent
        let p_dir_seek_keys = w.len() as u64;
        w.be_u32(0); // fSeekKeys
        w.be_u16(1);
        w.bytes(&[0u8; 16]);

        let prefix = w.into_vec();
        let pos = prefix.len() as u64;
        sink.write_all(&prefix)?;

        Ok(RNTupleWriter {
            sink,
            pos,
            file_name: file_name.to_string(),
            ntuple_name: ntuple_name.to_string(),
            compression,
            p_end,
            p_nbytes_name,
            p_dir_nbytes_keys,
            p_dir_seek_keys,
            f_nbytes_name,
            header: None,
            element_base: Vec::new(),
            total_entries: 0,
            summaries: Vec::new(),
            cluster_pages: Vec::new(),
        })
    }

    fn put(&mut self, bytes: &[u8]) -> io::Result<()> {
        self.sink.write_all(bytes)?;
        self.pos += bytes.len() as u64;
        Ok(())
    }

    fn patch(&mut self, offset: u64, value: u32) -> io::Result<()> {
        self.sink.seek(SeekFrom::Start(offset))?;
        self.sink.write_all(&value.to_be_bytes())
    }

    /// Append one cluster holding the entries in `fields`. All batches must share
    /// the same field schema; the first batch fixes it and writes the header.
    pub fn write_batch(&mut self, fields: &[Field]) -> Result<()> {
        if fields.is_empty() {
            return Ok(());
        }
        let (field_plans, cols, n_entries) = lower(fields);
        if n_entries == 0 {
            return Ok(());
        }
        let signature = schema_sig(&field_plans, &cols);

        match self.header.as_ref().map(|h| h.signature == signature) {
            Some(true) => {} // schema matches; header already written
            Some(false) => {
                return Err(Error::SchemaChanged {
                    detail: "this batch's field schema differs from the first batch's".into(),
                })
            }
            None => {
                // First batch fixes the schema and writes the header.
                let header_env = build_header(&self.ntuple_name, &field_plans, &cols);
                let checksum =
                    u64::from_le_bytes(header_env[header_env.len() - 8..].try_into().unwrap());
                let seek = self.pos;
                self.put(&header_env)?;
                self.element_base = vec![0u64; cols.len()];
                self.header = Some(HeaderState {
                    seek,
                    len: header_env.len(),
                    checksum,
                    signature,
                });
            }
        }

        let first_entry = self.total_entries;
        let mut recs = Vec::with_capacity(cols.len());
        for (i, c) in cols.iter().enumerate() {
            let disk = on_disk_page(&c.page, self.compression);
            let offset = self.pos;
            let element_offset = self.element_base[i] as i64;
            self.put(&disk)?;
            recs.push(PageRec {
                offset,
                disk_size: disk.len(),
                n_elements: c.n,
                element_offset,
            });
            self.element_base[i] += c.n as u64;
        }
        self.cluster_pages.push(recs);
        self.summaries.push((first_entry, n_entries as u64));
        self.total_entries += n_entries as u64;
        Ok(())
    }

    /// Finish the file: write the page list (all clusters), footer, anchor key,
    /// and key list, then patch the header pointers.
    pub fn finish(mut self) -> Result<()> {
        let header = self.header.take().ok_or_else(|| {
            Error::Format("RNTuple writer finished with no batches written".into())
        })?;
        let num_clusters = self.summaries.len() as u32;

        let page_list_offset = self.pos;
        let page_list_env = build_page_list_multi(
            &self.summaries,
            &self.cluster_pages,
            self.compression,
            header.checksum,
        );
        self.put(&page_list_env)?;

        let seek_footer = self.pos;
        let footer_env = build_footer(
            self.total_entries as u32,
            num_clusters,
            page_list_offset as usize,
            page_list_env.len(),
            header.checksum,
        );
        self.put(&footer_env)?;

        let anchor_obj = build_anchor(
            header.seek as usize,
            header.len,
            seek_footer as usize,
            footer_env.len(),
        );
        let anchor_seek = self.pos;
        let anchor_len = anchor_obj.len() as u32;
        let mut kb = WBuffer::new();
        write_key_header(
            &mut kb,
            "ROOT::RNTuple",
            &self.ntuple_name,
            "",
            anchor_len,
            anchor_len,
            anchor_seek,
            100,
        );
        let kb = kb.into_vec();
        self.put(&kb)?;
        self.put(&anchor_obj)?;

        let keylist_seek = self.pos;
        let keylist_obj_len = 4 + key_len("ROOT::RNTuple", &self.ntuple_name, "") as u32;
        let mut klb = WBuffer::new();
        write_key_header(
            &mut klb,
            "TFile",
            &self.file_name,
            "",
            keylist_obj_len,
            keylist_obj_len,
            keylist_seek,
            100,
        );
        klb.be_i32(1);
        write_key_header(
            &mut klb,
            "ROOT::RNTuple",
            &self.ntuple_name,
            "",
            anchor_len,
            anchor_len,
            anchor_seek,
            100,
        );
        let klb = klb.into_vec();
        self.put(&klb)?;
        let keylist_nbytes = key_len("TFile", &self.file_name, "") as u32 + keylist_obj_len;
        let f_end = self.pos as u32;

        self.patch(self.p_end, f_end)?;
        self.patch(self.p_nbytes_name, self.f_nbytes_name)?;
        self.patch(self.p_dir_nbytes_keys, keylist_nbytes)?;
        self.patch(self.p_dir_seek_keys, keylist_seek as u32)?;
        self.sink.flush()?;
        Ok(())
    }
}

/// Build the page-list envelope for any number of clusters: cluster summaries,
/// then page locations nested clusters → columns → (one) page.
fn build_page_list_multi(
    summaries: &[(u64, u64)],
    cluster_pages: &[Vec<PageRec>],
    compression: u32,
    header_checksum: u64,
) -> Vec<u8> {
    let mut p = Vec::new();
    p.extend_from_slice(&header_checksum.to_le_bytes());

    let summary_frames: Vec<Vec<u8>> = summaries
        .iter()
        .map(|&(first, n)| {
            let mut s = Vec::new();
            s.extend_from_slice(&first.to_le_bytes());
            s.extend_from_slice(&n.to_le_bytes()); // high byte = flags (0)
            record_frame(&s)
        })
        .collect();
    p.extend_from_slice(&list_frame(&summary_frames));

    let cluster_frames: Vec<Vec<u8>> = cluster_pages
        .iter()
        .map(|cols| {
            let col_frames: Vec<Vec<u8>> = cols
                .iter()
                .map(|pr| {
                    let mut page = Vec::new();
                    page.extend_from_slice(&(pr.n_elements as i32).to_le_bytes()); // no checksum
                    page.extend_from_slice(&(pr.disk_size as i32).to_le_bytes()); // on-disk size
                    page.extend_from_slice(&pr.offset.to_le_bytes()); // locator offset
                    let mut body = Vec::new();
                    body.extend_from_slice(&1u32.to_le_bytes()); // one page
                    body.extend_from_slice(&page);
                    body.extend_from_slice(&pr.element_offset.to_le_bytes());
                    body.extend_from_slice(&compression.to_le_bytes());
                    let size = (8 + body.len()) as i64;
                    let mut frame = (-size).to_le_bytes().to_vec();
                    frame.extend_from_slice(&body);
                    frame
                })
                .collect();
            list_frame(&col_frames)
        })
        .collect();
    p.extend_from_slice(&list_frame(&cluster_frames));

    envelope(0x03, &p)
}
