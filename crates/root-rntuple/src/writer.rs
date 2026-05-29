//! Writing a minimal RNTuple of scalar fields into a ROOT file.
//!
//! Produces an uncompressed, single-cluster RNTuple with non-split column
//! encodings — the simplest form the spec allows. The header/page/page-list/
//! footer envelopes are written as raw blobs at the offsets the anchor (and the
//! page locators) point to; only the anchor itself is a `TKey`. Validated by
//! reading the result back and by official ROOT / uproot.

use std::path::Path;

use root_io_core::buffer::WBuffer;
use root_io_core::{key_len, write_key_header};

use crate::column::ColumnType;

const K_BYTE_COUNT_MASK: u32 = 0x4000_0000;
const DATIME: u32 = 0x7d7a_79ca;
const FILE_VERSION: u32 = 62400;

/// A scalar column of data to store as one RNTuple field.
pub enum ScalarColumn {
    /// 32-bit signed integers (`Int32`).
    I32(Vec<i32>),
    /// 32-bit floats (`Real32`).
    F32(Vec<f32>),
    /// 64-bit floats (`Real64`).
    F64(Vec<f64>),
}

/// A named scalar field.
pub struct ScalarField {
    /// Field name.
    pub name: String,
    /// Field data.
    pub data: ScalarColumn,
}

struct Encoded {
    column_type: ColumnType,
    bits: u16,
    type_name: &'static str,
    page: Vec<u8>,
    n: u32,
}

fn encode(col: &ScalarColumn) -> Encoded {
    match col {
        ScalarColumn::I32(v) => Encoded {
            column_type: ColumnType::Int32,
            bits: 32,
            type_name: "std::int32_t",
            page: v.iter().flat_map(|x| x.to_le_bytes()).collect(),
            n: v.len() as u32,
        },
        ScalarColumn::F32(v) => Encoded {
            column_type: ColumnType::Real32,
            bits: 32,
            type_name: "float",
            page: v.iter().flat_map(|x| x.to_le_bytes()).collect(),
            n: v.len() as u32,
        },
        ScalarColumn::F64(v) => Encoded {
            column_type: ColumnType::Real64,
            bits: 64,
            type_name: "double",
            page: v.iter().flat_map(|x| x.to_le_bytes()).collect(),
            n: v.len() as u32,
        },
    }
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

fn build_header(name: &str, fields: &[ScalarField], cols: &[Encoded]) -> Vec<u8> {
    let mut p = Vec::new();
    p.extend_from_slice(&0i64.to_le_bytes()); // feature flags
    p.extend_from_slice(&rstr(name));
    p.extend_from_slice(&rstr("")); // description
    p.extend_from_slice(&rstr("root-rs")); // writer

    let field_records: Vec<Vec<u8>> = fields
        .iter()
        .zip(cols)
        .enumerate()
        .map(|(i, (f, c))| {
            let mut r = Vec::new();
            r.extend_from_slice(&0u32.to_le_bytes()); // field version
            r.extend_from_slice(&0u32.to_le_bytes()); // type version
            r.extend_from_slice(&(i as u32).to_le_bytes()); // parent id = self (top-level)
            r.extend_from_slice(&0u16.to_le_bytes()); // struct role = Leaf
            r.extend_from_slice(&0u16.to_le_bytes()); // flags
            r.extend_from_slice(&rstr(&f.name));
            r.extend_from_slice(&rstr(c.type_name));
            r.extend_from_slice(&rstr("")); // type alias
            r.extend_from_slice(&rstr("")); // description
            record_frame(&r)
        })
        .collect();
    p.extend_from_slice(&list_frame(&field_records));

    let column_records: Vec<Vec<u8>> = cols
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let mut r = Vec::new();
            r.extend_from_slice(&(c.column_type as u16).to_le_bytes());
            r.extend_from_slice(&c.bits.to_le_bytes());
            r.extend_from_slice(&(i as u32).to_le_bytes()); // field id
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
    cols: &[Encoded],
    header_checksum: u64,
) -> Vec<u8> {
    let mut p = Vec::new();
    p.extend_from_slice(&header_checksum.to_le_bytes());

    // Cluster summaries: one record frame.
    let mut summary = Vec::new();
    summary.extend_from_slice(&0u64.to_le_bytes()); // first entry
    summary.extend_from_slice(&(n_entries as u64).to_le_bytes()); // num entries (flags=0)
    p.extend_from_slice(&list_frame(&[record_frame(&summary)]));

    // Page locations: outer list (clusters) of inner list (columns).
    let column_frames: Vec<Vec<u8>> = cols
        .iter()
        .enumerate()
        .map(|(i, c)| {
            // One page: num_elements (positive = no checksum) + locator(size, offset).
            let mut page = Vec::new();
            page.extend_from_slice(&(c.n as i32).to_le_bytes());
            page.extend_from_slice(&(c.page.len() as i32).to_le_bytes()); // locator size
            page.extend_from_slice(&(page_offsets[i] as u64).to_le_bytes()); // locator offset
                                                                             // Column frame: a list frame whose body is [pages][element_offset][compression].
            let mut body = Vec::new();
            body.extend_from_slice(&1u32.to_le_bytes()); // one page
            body.extend_from_slice(&page);
            body.extend_from_slice(&0i64.to_le_bytes()); // element offset
            body.extend_from_slice(&0u32.to_le_bytes()); // compression (uncompressed)
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
    page_list_offset: usize,
    page_list_len: usize,
    header_checksum: u64,
) -> Vec<u8> {
    let mut p = Vec::new();
    p.extend_from_slice(&0i64.to_le_bytes()); // feature flags
    p.extend_from_slice(&header_checksum.to_le_bytes());

    // Schema extension: a record frame of four empty list frames.
    let mut ext = Vec::new();
    for _ in 0..4 {
        ext.extend_from_slice(&list_frame(&[]));
    }
    p.extend_from_slice(&record_frame(&ext));

    // Cluster groups: one record frame with the page-list envelope link.
    let mut cg = Vec::new();
    cg.extend_from_slice(&0u64.to_le_bytes()); // min entry
    cg.extend_from_slice(&(n_entries as u64).to_le_bytes()); // entry span
    cg.extend_from_slice(&1u32.to_le_bytes()); // num clusters
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
    fields.extend_from_slice(&(len_header as u64).to_be_bytes()); // nbytes (uncompressed)
    fields.extend_from_slice(&(len_header as u64).to_be_bytes()); // len
    fields.extend_from_slice(&(seek_footer as u64).to_be_bytes());
    fields.extend_from_slice(&(len_footer as u64).to_be_bytes());
    fields.extend_from_slice(&(len_footer as u64).to_be_bytes());
    fields.extend_from_slice(&0x4000_0000u64.to_be_bytes()); // max key size

    let checksum = xxhash_rust::xxh3::xxh3_64(&fields);

    // ROOT object framing: a byte count (covering version + the 64 fields, but
    // NOT the trailing checksum) + version, then the fields, then the checksum.
    let mut obj = Vec::new();
    obj.extend_from_slice(&((66u32) | K_BYTE_COUNT_MASK).to_be_bytes());
    obj.extend_from_slice(&2u16.to_be_bytes()); // class version
    obj.extend_from_slice(&fields);
    obj.extend_from_slice(&checksum.to_be_bytes());
    obj
}

/// Build a complete ROOT file containing one RNTuple named `ntuple_name`.
pub fn rntuple_file_bytes(file_name: &str, ntuple_name: &str, fields: &[ScalarField]) -> Vec<u8> {
    let cols: Vec<Encoded> = fields.iter().map(|f| encode(&f.data)).collect();
    let n_entries = cols.first().map(|c| c.n).unwrap_or(0);

    let header_env = build_header(ntuple_name, fields, &cols);
    let header_checksum =
        u64::from_le_bytes(header_env[header_env.len() - 8..].try_into().unwrap());

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
    w.be_u32(0); // fCompress
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
    for c in &cols {
        page_offsets.push(w.len());
        w.bytes(&c.page);
    }
    let page_list_offset = w.len();
    let page_list_env = build_page_list(n_entries, &page_offsets, &cols, header_checksum);
    w.bytes(&page_list_env);
    let seek_footer = w.len();
    let footer_env = build_footer(
        n_entries,
        page_list_offset,
        page_list_env.len(),
        header_checksum,
    );
    w.bytes(&footer_env);

    // --- Anchor key + object. ---
    let anchor_obj = build_anchor(seek_header, header_env.len(), seek_footer, footer_env.len());
    let anchor_seek = w.len();
    write_key_header(
        &mut w,
        "ROOT::RNTuple",
        ntuple_name,
        "",
        anchor_obj.len() as u32,
        anchor_obj.len() as u32,
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
        anchor_obj.len() as u32,
        anchor_obj.len() as u32,
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

/// Write a one-RNTuple ROOT file to `path`.
pub fn write_rntuple_file(
    path: &Path,
    ntuple_name: &str,
    fields: &[ScalarField],
) -> std::io::Result<()> {
    let file_name = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("file.root");
    std::fs::write(path, rntuple_file_bytes(file_name, ntuple_name, fields))
}
