//! Writing a `TTree` into a ROOT file.
//!
//! Supports scalar, fixed-size array (`x[N]`), and string (`TLeafC`) branches —
//! the variable-length numeric case (a jagged `x[n]` with a count branch) is not
//! yet written. Mirrors the layout ROOT/uproot write (TTree v20, TBranch v13,
//! TLeaf* v1) so the result reads back in ROOT, uproot, and this crate. One
//! basket per branch. The embedded `TStreamerInfo` (a baked blob) makes the file
//! self-describing.

use std::path::Path;

use oxiroot_io_core::buffer::{CountToken, Patch, WBuffer, K_BYTE_COUNT_MASK};
use oxiroot_io_core::error::{Error, Result};
use oxiroot_io_core::streamer::{write_tnamed, write_tobject};
use oxiroot_io_core::{key_len, key_len_fmt, write_key_header, Compression};

use crate::value::BranchValues;

/// Fixed creation/modification timestamp (`TDatime`); readers don't validate it.
const DATIME: u32 = 0x7d7a_79ca;
/// Small-format on-disk file version.
const FILE_VERSION: u32 = 62400;
/// `fBits` ROOT writes for embedded `TObject`s.
const OBJ_BITS: u32 = 0x0300_0000;
/// The baked `TStreamerInfo` for the TTree hierarchy (TTree/TBranch/TLeaf*/…),
/// extracted from a uproot-written tree. Embedded so the file is self-describing.
const TREE_STREAMER_INFO: &[u8] = include_bytes!("tree.streamerinfo.bin");

/// One named branch to write. Use the typed constructors: [`Branch::i32`] … for
/// scalars, [`Branch::vec_f64`] … for fixed-size arrays, [`Branch::strings`] for
/// strings.
pub struct Branch {
    /// Branch (and leaf) name.
    pub name: String,
    /// Branch values (a [`BranchValues`] variant — scalar, array, or string).
    pub values: BranchValues,
}

macro_rules! branch_ctors {
    ($($method:ident => $variant:ident($elem:ty)),* $(,)?) => {
        impl Branch {
            $(
                #[doc = concat!("A branch holding `", stringify!($variant), "` values.")]
                pub fn $method(name: impl Into<String>, values: Vec<$elem>) -> Branch {
                    Branch { name: name.into(), values: BranchValues::$variant(values) }
                }
            )*
        }
    };
}
branch_ctors! {
    bools => Bool(bool), i8 => I8(i8), u8 => U8(u8), i16 => I16(i16), u16 => U16(u16),
    i32 => I32(i32), u32 => U32(u32), i64 => I64(i64), u64 => U64(u64),
    f32 => F32(f32), f64 => F64(f64),
}

/// Generate `Branch::vec_<name>` shortcuts for fixed-size array branches (each
/// inner vector must have the same length `N`, written as `x[N]`).
macro_rules! vec_ctors {
    ($($method:ident => $variant:ident($elem:ty)),* $(,)?) => {
        impl Branch {
            $(
                #[doc = concat!("A fixed-size array branch holding `", stringify!($variant), "` rows.")]
                pub fn $method(name: impl Into<String>, values: Vec<Vec<$elem>>) -> Branch {
                    Branch { name: name.into(), values: BranchValues::$variant(values) }
                }
            )*
        }
    };
}
vec_ctors! {
    vec_bool => VecBool(bool), vec_i8 => VecI8(i8), vec_u8 => VecU8(u8),
    vec_i16 => VecI16(i16), vec_u16 => VecU16(u16), vec_i32 => VecI32(i32),
    vec_u32 => VecU32(u32), vec_i64 => VecI64(i64), vec_u64 => VecU64(u64),
    vec_f32 => VecF32(f32), vec_f64 => VecF64(f64),
}

impl Branch {
    /// A string branch (`TLeafC`).
    pub fn strings(name: impl Into<String>, values: Vec<String>) -> Branch {
        Branch {
            name: name.into(),
            values: BranchValues::Str(values),
        }
    }
}

/// Whether a branch is a scalar, a fixed-size array, or a string.
enum Kind {
    Scalar,
    FixedArray(usize),
    Str,
}

/// The on-disk description of one leaf type.
struct LeafInfo {
    /// `TLeafI`/`TLeafD`/`TLeafC`/… class name.
    class: &'static str,
    /// Leaflist type code (`I`/`D`/`C`/…) used in the branch title.
    code: char,
    /// Element byte width (the data stride; 1 for a `TLeafC` char).
    size: i32,
    /// `fLenType` (the element width for numerics, 0 for `TLeafC`).
    len_type: i32,
    /// `fIsUnsigned`.
    unsigned: bool,
}

impl Branch {
    /// Number of entries (rows for arrays/strings).
    fn n_entries(&self) -> u32 {
        use BranchValues::*;
        let n = match &self.values {
            Bool(v) => v.len(),
            I8(v) => v.len(),
            U8(v) => v.len(),
            I16(v) => v.len(),
            U16(v) => v.len(),
            I32(v) => v.len(),
            U32(v) => v.len(),
            I64(v) => v.len(),
            U64(v) => v.len(),
            F32(v) => v.len(),
            F64(v) => v.len(),
            VecBool(r) => r.len(),
            VecI8(r) => r.len(),
            VecU8(r) => r.len(),
            VecI16(r) => r.len(),
            VecU16(r) => r.len(),
            VecI32(r) => r.len(),
            VecU32(r) => r.len(),
            VecI64(r) => r.len(),
            VecU64(r) => r.len(),
            VecF32(r) => r.len(),
            VecF64(r) => r.len(),
            Str(v) => v.len(),
        };
        n as u32
    }

    /// The element leaf type (the inner type for arrays).
    fn leaf(&self) -> LeafInfo {
        use BranchValues::*;
        let (class, code, size, unsigned) = match &self.values {
            Bool(_) | VecBool(_) => ("TLeafO", 'O', 1, false),
            I8(_) | VecI8(_) => ("TLeafB", 'B', 1, false),
            U8(_) | VecU8(_) => ("TLeafB", 'b', 1, true),
            I16(_) | VecI16(_) => ("TLeafS", 'S', 2, false),
            U16(_) | VecU16(_) => ("TLeafS", 's', 2, true),
            I32(_) | VecI32(_) => ("TLeafI", 'I', 4, false),
            U32(_) | VecU32(_) => ("TLeafI", 'i', 4, true),
            I64(_) | VecI64(_) => ("TLeafL", 'L', 8, false),
            U64(_) | VecU64(_) => ("TLeafL", 'l', 8, true),
            F32(_) | VecF32(_) => ("TLeafF", 'F', 4, false),
            F64(_) | VecF64(_) => ("TLeafD", 'D', 8, false),
            Str(_) => ("TLeafC", 'C', 1, false),
        };
        let len_type = if matches!(self.values, Str(_)) {
            0
        } else {
            size
        };
        LeafInfo {
            class,
            code,
            size,
            len_type,
            unsigned,
        }
    }

    /// Elements per entry: 1 (scalar/string) or `N` (fixed array, from row 0).
    fn flen(&self) -> i32 {
        use BranchValues::*;
        let n = match &self.values {
            VecBool(r) => r.first().map_or(0, Vec::len),
            VecI8(r) => r.first().map_or(0, Vec::len),
            VecU8(r) => r.first().map_or(0, Vec::len),
            VecI16(r) => r.first().map_or(0, Vec::len),
            VecU16(r) => r.first().map_or(0, Vec::len),
            VecI32(r) => r.first().map_or(0, Vec::len),
            VecU32(r) => r.first().map_or(0, Vec::len),
            VecI64(r) => r.first().map_or(0, Vec::len),
            VecU64(r) => r.first().map_or(0, Vec::len),
            VecF32(r) => r.first().map_or(0, Vec::len),
            VecF64(r) => r.first().map_or(0, Vec::len),
            _ => 1,
        };
        n as i32
    }

    /// Whether this is an array branch whose rows differ in length (not yet
    /// writable — variable-length numeric arrays need a separate count branch).
    fn is_jagged(&self) -> bool {
        use BranchValues::*;
        macro_rules! jag {
            ($r:expr) => {{
                let n = $r.first().map_or(0, Vec::len);
                $r.iter().any(|x| x.len() != n)
            }};
        }
        match &self.values {
            VecBool(r) => jag!(r),
            VecI8(r) => jag!(r),
            VecU8(r) => jag!(r),
            VecI16(r) => jag!(r),
            VecU16(r) => jag!(r),
            VecI32(r) => jag!(r),
            VecU32(r) => jag!(r),
            VecI64(r) => jag!(r),
            VecU64(r) => jag!(r),
            VecF32(r) => jag!(r),
            VecF64(r) => jag!(r),
            _ => false,
        }
    }

    fn kind(&self) -> Kind {
        use BranchValues::*;
        match &self.values {
            Str(_) => Kind::Str,
            VecBool(_) | VecI8(_) | VecU8(_) | VecI16(_) | VecU16(_) | VecI32(_) | VecU32(_)
            | VecI64(_) | VecU64(_) | VecF32(_) | VecF64(_) => {
                Kind::FixedArray(self.flen() as usize)
            }
            _ => Kind::Scalar,
        }
    }

    /// The basket's uncompressed entry data, plus (for string branches) the
    /// data-relative `fEntryOffset` array (`n_entries + 1` offsets).
    fn basket_content(&self) -> (Vec<u8>, Option<Vec<u32>>) {
        use BranchValues::*;
        macro_rules! be {
            ($v:expr, $w:expr) => {{
                let mut out = Vec::with_capacity($v.len() * $w);
                for x in $v {
                    out.extend_from_slice(&x.to_be_bytes());
                }
                out
            }};
        }
        macro_rules! be_rows {
            ($r:expr, $w:expr) => {{
                let mut out = Vec::new();
                for row in $r {
                    for x in row {
                        out.extend_from_slice(&x.to_be_bytes());
                    }
                }
                out
            }};
        }
        let data = match &self.values {
            Bool(v) => v.iter().map(|&b| b as u8).collect(),
            I8(v) => v.iter().map(|&x| x as u8).collect(),
            U8(v) => v.clone(),
            I16(v) => be!(v, 2),
            U16(v) => be!(v, 2),
            I32(v) => be!(v, 4),
            U32(v) => be!(v, 4),
            I64(v) => be!(v, 8),
            U64(v) => be!(v, 8),
            F32(v) => be!(v, 4),
            F64(v) => be!(v, 8),
            VecBool(r) => r.iter().flatten().map(|&b| b as u8).collect(),
            VecI8(r) => r.iter().flatten().map(|&x| x as u8).collect(),
            VecU8(r) => r.concat(),
            VecI16(r) => be_rows!(r, 2),
            VecU16(r) => be_rows!(r, 2),
            VecI32(r) => be_rows!(r, 4),
            VecU32(r) => be_rows!(r, 4),
            VecI64(r) => be_rows!(r, 8),
            VecU64(r) => be_rows!(r, 8),
            VecF32(r) => be_rows!(r, 4),
            VecF64(r) => be_rows!(r, 8),
            Str(strings) => {
                let mut data = Vec::new();
                let mut offsets = vec![0u32];
                for s in strings {
                    let b = s.as_bytes();
                    if b.len() < 255 {
                        data.push(b.len() as u8);
                    } else {
                        data.push(255);
                        data.extend_from_slice(&(b.len() as u32).to_be_bytes());
                    }
                    data.extend_from_slice(b);
                    offsets.push(data.len() as u32);
                }
                return (data, Some(offsets));
            }
        };
        (data, None)
    }
}

/// One basket's recorded location, for the branch metadata.
struct BasketRec {
    seek: u64,
    nbytes: u32,
}

/// Write a single-tree ROOT file containing the flat scalar `branches`.
pub fn write_tree_file(
    path: impl AsRef<Path>,
    tree_name: &str,
    branches: &[Branch],
    compression: Compression,
) -> Result<()> {
    let path = path.as_ref();
    let file_name = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("file.root");
    std::fs::write(
        path,
        tree_file_bytes(file_name, tree_name, branches, compression)?,
    )?;
    Ok(())
}

/// Build the bytes of a single-tree ROOT file.
///
/// Returns an error if any branch is a jagged (variable-length) numeric array,
/// which is not yet writable — every row of an array branch must share a length.
pub fn tree_file_bytes(
    file_name: &str,
    tree_name: &str,
    branches: &[Branch],
    compression: Compression,
) -> Result<Vec<u8>> {
    for b in branches {
        if b.is_jagged() {
            return Err(Error::Format(format!(
                "branch {:?}: variable-length (jagged) array writing is not supported yet; \
                 every row must have the same length",
                b.name
            )));
        }
    }
    let compression = compression.setting();
    let n_entries = branches.first().map(|b| b.n_entries()).unwrap_or(0);

    let mut w = WBuffer::new();

    // --- File header (100 bytes; pointers patched at the end). ---
    w.bytes(b"root");
    w.be_u32(FILE_VERSION);
    w.be_u32(100); // fBEGIN
    let p_end = w.reserve(4);
    w.be_u32(0); // fSeekFree
    w.be_u32(0); // fNbytesFree
    w.be_u32(0); // nfree
    let p_nbytes_name = w.reserve(4);
    w.u8(4); // fUnits
    w.be_u32(compression); // fCompress
    let p_seek_info = w.reserve(4);
    let p_nbytes_info = w.reserve(4);
    w.be_u16(1);
    w.bytes(&[0u8; 16]);
    while w.len() < 100 {
        w.u8(0);
    }

    // --- Root directory name key + TDirectory record. ---
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
    w.be_u32(100); // fSeekDir
    w.be_u32(0); // fSeekParent
    let p_dir_seek_keys = w.reserve(4);
    w.be_u16(1);
    w.bytes(&[0u8; 16]);

    // --- One basket per branch (TBasket TKeys, not directory keys). ---
    let baskets: Vec<BasketRec> = branches
        .iter()
        .map(|b| write_basket(&mut w, b, tree_name, compression))
        .collect();
    let tot_bytes: i64 = baskets.iter().map(|r| r.nbytes as i64).sum();

    // --- TTree object key + object. ---
    let tree_obj = build_tree_object(tree_name, branches, &baskets, n_entries as i64, tot_bytes);
    let tree_payload = on_disk(&tree_obj, compression);
    let tree_seek = w.len();
    write_key_header(
        &mut w,
        "TTree",
        tree_name,
        "",
        tree_obj.len() as u32,
        tree_payload.len() as u32,
        tree_seek as u64,
        100,
    );
    w.bytes(&tree_payload);

    // --- Streamer-info record (referenced by fSeekInfo only). ---
    let si_payload = on_disk(TREE_STREAMER_INFO, compression);
    let seek_info = w.len() as u32;
    write_key_header(
        &mut w,
        "TList",
        "StreamerInfo",
        "Doubly linked list",
        TREE_STREAMER_INFO.len() as u32,
        si_payload.len() as u32,
        seek_info as u64,
        100,
    );
    w.bytes(&si_payload);
    let nbytes_info =
        key_len("TList", "StreamerInfo", "Doubly linked list") as u32 + si_payload.len() as u32;

    // --- Directory key list (one entry: the TTree). ---
    let keylist_seek = w.len();
    let tree_klen = key_len("TTree", tree_name, "");
    let keylist_obj_len = 4 + tree_klen as u32;
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
        "TTree",
        tree_name,
        "",
        tree_obj.len() as u32,
        tree_payload.len() as u32,
        tree_seek as u64,
        100,
    );
    let keylist_nbytes = key_len("TFile", file_name, "") as u32 + keylist_obj_len;
    let f_end = w.len() as u32;

    w.patch_be_u32(p_end, f_end);
    w.patch_be_u32(p_nbytes_name, f_nbytes_name as u32);
    w.patch_be_u32(p_seek_info, seek_info);
    w.patch_be_u32(p_nbytes_info, nbytes_info);
    w.patch_be_u32(p_dir_nbytes_keys, keylist_nbytes);
    w.patch_be_u32(p_dir_seek_keys, keylist_seek as u32);

    Ok(w.into_vec())
}

/// On-disk bytes for an object payload: compressed when it helps, else raw.
fn on_disk(object: &[u8], compression: u32) -> Vec<u8> {
    if compression == 0 {
        return object.to_vec();
    }
    match oxiroot_compress::compress(object, compression) {
        Ok(c) if c.len() < object.len() => c,
        _ => object.to_vec(),
    }
}

/// Write a `TBasket` (a big-format `TKey` whose `fKeyLen` includes the 19-byte
/// extension), returning its location.
fn write_basket(w: &mut WBuffer, branch: &Branch, tree_name: &str, compression: u32) -> BasketRec {
    let (data, offsets) = branch.basket_content();
    let n_entries = branch.n_entries();
    let leaf = branch.leaf();
    // `fNevBufSize` is the per-entry buffer size: `flen * elem_size` for a
    // fixed/scalar branch; ROOT writes a default (1000) for variable baskets.
    let nev_buf_size = match branch.kind() {
        Kind::Str => 1000,
        _ => branch.flen() * leaf.size,
    };

    let seek = w.len() as u64;
    let klen = key_len_fmt("TBasket", &branch.name, tree_name, true) as u32 + 19;
    let border = data.len() as u32;

    // The uncompressed buffer is the entry data, then (for a variable branch)
    // the `fEntryOffset` array: `int32 count(=n_entries+1)` followed by
    // basket-relative offsets (the data-relative offsets plus `fKeyLen`).
    let mut buffer = data;
    if let Some(offs) = &offsets {
        buffer.extend_from_slice(&(offs.len() as i32).to_be_bytes());
        for &o in offs {
            buffer.extend_from_slice(&((o + klen) as i32).to_be_bytes());
        }
    }
    let obj_len = buffer.len() as u32;
    let payload = on_disk(&buffer, compression);
    let nbytes = klen + payload.len() as u32;
    let f_last = klen + border; // entry data ends at the border

    // Big-format TKey header.
    w.be_i32(nbytes as i32);
    w.be_u16(1004); // big-format key version
    w.be_u32(obj_len);
    w.be_u32(DATIME);
    w.be_u16(klen as u16);
    w.be_u16(0); // cycle
    w.be_u64(seek);
    w.be_u64(100); // fSeekPdir
    w.string("TBasket");
    w.string(&branch.name);
    w.string(tree_name);
    // TBasket extension (the tail of fKeyLen).
    w.be_u16(3); // basket version
    w.be_i32(32000); // fBufferSize
    w.be_i32(nev_buf_size); // fNevBufSize
    w.be_i32(n_entries as i32); // fNevBuf
    w.be_i32(f_last as i32); // fLast
    w.u8(0); // flag
    w.bytes(&payload);

    BasketRec { seek, nbytes }
}

/// Write a byte-counted att base (`TAttLine`/`Fill`/`Marker`).
fn write_attline(w: &mut WBuffer) {
    let t = w.begin_object(2);
    w.be_i16(602);
    w.be_i16(1);
    w.be_i16(1);
    w.end_object(t);
}
fn write_attfill(w: &mut WBuffer) {
    let t = w.begin_object(2);
    w.be_i16(0);
    w.be_i16(1001);
    w.end_object(t);
}
fn write_attmarker(w: &mut WBuffer) {
    let t = w.begin_object(2);
    w.be_i16(1);
    w.be_i16(1);
    w.be_f32(1.0);
    w.end_object(t);
}

/// Write `ROOT::TIOFeatures` (a byte-counted object with a single `fIOBits`).
fn write_iofeatures(w: &mut WBuffer) {
    let t = w.begin_object(1);
    w.u8(0); // fIOBits
    w.end_object(t);
}

/// Begin a `ReadObjectAny` object: a byte-count placeholder then a fresh class
/// tag (`kNewClassTag` + name). Every object is written with a fresh tag (no
/// back-references), which ROOT/uproot read correctly. Returns the byte-count
/// patch.
fn begin_object_any(w: &mut WBuffer, class: &str) -> Patch {
    let bc = w.reserve(4); // byte-count placeholder
    w.be_u32(0xFFFF_FFFF); // kNewClassTag
    w.bytes(class.as_bytes());
    w.u8(0); // NUL terminator
    bc
}

/// Finish a `ReadObjectAny` object, back-patching its byte count (which covers
/// everything after the 4-byte count word).
fn end_object_any(w: &mut WBuffer, bc: Patch) {
    let start = w.patch_offset(bc) + 4;
    let nbytes = (w.len() - start) as u32 | K_BYTE_COUNT_MASK;
    w.patch_be_u32(bc, nbytes);
}

/// Build the `TObjArray` of branches, then the tree-level `TObjArray` of leaves.
fn build_tree_object(
    tree_name: &str,
    branches: &[Branch],
    baskets: &[BasketRec],
    n_entries: i64,
    tot_bytes: i64,
) -> Vec<u8> {
    let mut w = WBuffer::new();
    let tree = w.begin_object(20); // TTree v20
    write_tnamed(&mut w, OBJ_BITS, tree_name, "");
    write_attline(&mut w);
    write_attfill(&mut w);
    write_attmarker(&mut w);

    w.be_i64(n_entries); // fEntries
    w.be_i64(tot_bytes); // fTotBytes
    w.be_i64(tot_bytes); // fZipBytes
    w.be_i64(0); // fSavedBytes
    w.be_i64(0); // fFlushedBytes
    w.be_f64(1.0); // fWeight
    w.be_i32(0); // fTimerInterval
    w.be_i32(25); // fScanField
    w.be_i32(0); // fUpdate
    w.be_i32(1000); // fDefaultEntryOffsetLen
    w.be_i32(0); // fNClusterRange
    w.be_i64(1_000_000_000_000); // fMaxEntries
    w.be_i64(1_000_000_000_000); // fMaxEntryLoop
    w.be_i64(0); // fMaxVirtualSize
    w.be_i64(-300_000_000); // fAutoSave
    w.be_i64(-30_000_000); // fAutoFlush
    w.be_i64(1_000_000); // fEstimate
    w.u8(0); // fClusterRangeEnd (empty array marker)
    w.u8(0); // fClusterSize (empty array marker)
    write_iofeatures(&mut w);

    write_branch_array(&mut w, tree_name, branches, baskets, n_entries);
    write_tree_leaf_array(&mut w, branches);

    w.be_u32(0); // fAliases (null TList*)
    w.be_i32(0); // fIndexValues (TArrayD, empty)
    w.be_i32(0); // fIndex (TArrayI, empty)
    w.be_u32(0); // fTreeIndex (null)
    w.be_u32(0); // fFriends (null)
    w.be_u32(0); // fUserInfo (null)
    w.be_u32(0); // fBranchRef (null)

    w.end_object(tree);
    w.into_vec()
}

/// The `TObjArray` header (`{version} TObject name fSize fLowerBound`).
fn obj_array_header(w: &mut WBuffer, size: usize) -> CountToken {
    let tok = w.begin_object(3); // TObjArray v3
    write_tobject(w, 0);
    w.string("");
    w.be_i32(size as i32);
    w.be_i32(0); // fLowerBound
    tok
}

/// Write `fBranches`: a `TObjArray<TBranch>`.
fn write_branch_array(
    w: &mut WBuffer,
    tree_name: &str,
    branches: &[Branch],
    baskets: &[BasketRec],
    n_entries: i64,
) {
    let _ = tree_name;
    let tok = obj_array_header(w, branches.len());
    for (b, basket) in branches.iter().zip(baskets) {
        let bc = begin_object_any(w, "TBranch");
        write_branch(w, b, basket, n_entries);
        end_object_any(w, bc);
    }
    w.end_object(tok);
}

/// Write one `TBranch` (v13).
fn write_branch(w: &mut WBuffer, branch: &Branch, basket: &BasketRec, n_entries: i64) {
    let leaf = branch.leaf();
    // Branch title encodes the layout: `name/CODE`, `name[N]/CODE`, or `name/C`.
    let title = match branch.kind() {
        Kind::Scalar => format!("{}/{}", branch.name, leaf.code),
        Kind::FixedArray(n) => format!("{}[{}]/{}", branch.name, n, leaf.code),
        Kind::Str => format!("{}/C", branch.name),
    };
    // Variable (string) branches carry an `fEntryOffset` array, flagged by a
    // non-zero `fEntryOffsetLen`; fixed/scalar branches set it to 0.
    let entry_offset_len = match branch.kind() {
        Kind::Str => 1000,
        _ => 0,
    };
    let max_baskets = 10i32;

    let tok = w.begin_object(13); // TBranch v13
    write_tnamed(w, OBJ_BITS, &branch.name, &title);
    write_attfill(w);
    w.be_i32(0); // fCompress
    w.be_i32(32000); // fBasketSize
    w.be_i32(entry_offset_len); // fEntryOffsetLen
    w.be_i32(1); // fWriteBasket
    w.be_i64(n_entries); // fEntryNumber
    write_iofeatures(w);
    w.be_i32(0); // fOffset
    w.be_i32(max_baskets); // fMaxBaskets
    w.be_i32(0); // fSplitLevel
    w.be_i64(n_entries); // fEntries
    w.be_i64(0); // fFirstEntry
    w.be_i64(basket.nbytes as i64); // fTotBytes
    w.be_i64(basket.nbytes as i64); // fZipBytes

    // fBranches (empty), fLeaves (one leaf), fBaskets (empty TObjArrays).
    let e = obj_array_header(w, 0);
    w.end_object(e);
    write_leaf_array(w, std::slice::from_ref(branch));
    let e = obj_array_header(w, 0);
    w.end_object(e);

    // fBasketBytes (int[fMaxBaskets]), fBasketEntry (i64[]), fBasketSeek (i64[]):
    // each a marker byte then fMaxBaskets elements.
    w.u8(1);
    for i in 0..max_baskets {
        w.be_i32(if i == 0 { basket.nbytes as i32 } else { 0 });
    }
    w.u8(1);
    for i in 0..max_baskets {
        w.be_i64(if i == 0 {
            0
        } else if i == 1 {
            n_entries
        } else {
            0
        });
    }
    w.u8(1);
    for i in 0..max_baskets {
        w.be_i64(if i == 0 { basket.seek as i64 } else { 0 });
    }
    w.string(""); // fFileName
    w.end_object(tok);
}

/// Write a `TObjArray<TLeaf>` for `branches` (one leaf each).
fn write_leaf_array(w: &mut WBuffer, branches: &[Branch]) {
    let tok = obj_array_header(w, branches.len());
    for b in branches {
        let bc = begin_object_any(w, b.leaf().class);
        write_leaf(w, b);
        end_object_any(w, bc);
    }
    w.end_object(tok);
}

/// The tree-level `fLeaves` lists every branch's leaf.
fn write_tree_leaf_array(w: &mut WBuffer, branches: &[Branch]) {
    write_leaf_array(w, branches);
}

/// Write one `TLeaf*` (v1): the `TLeaf` base then the subclass min/max.
fn write_leaf(w: &mut WBuffer, branch: &Branch) {
    let leaf = branch.leaf();
    // The leaf title carries `[N]` for a fixed array, else just the name.
    let title = match branch.kind() {
        Kind::FixedArray(n) => format!("{}[{}]", branch.name, n),
        _ => branch.name.clone(),
    };
    let outer = w.begin_object(1); // TLeafX v1
    let base = w.begin_object(2); // TLeaf v2
    write_tnamed(w, OBJ_BITS, &branch.name, &title);
    w.be_i32(branch.flen()); // fLen (elements per entry)
    w.be_i32(leaf.len_type); // fLenType (0 for TLeafC)
    w.be_i32(0); // fOffset
    w.u8(0); // fIsRange
    w.u8(leaf.unsigned as u8); // fIsUnsigned
    w.be_u32(0); // fLeafCount (null)
    w.end_object(base);
    // fMinimum, fMaximum: TLeafC stores them as 4-byte ints (string lengths);
    // every other leaf uses its element width.
    let minmax_size = if leaf.code == 'C' { 4 } else { leaf.size };
    write_leaf_minmax(w, minmax_size);
    w.end_object(outer);
}

/// Write a leaf's `fMinimum`/`fMaximum` (both 0) in the element width.
fn write_leaf_minmax(w: &mut WBuffer, size: i32) {
    match size {
        1 => {
            w.u8(0);
            w.u8(0);
        }
        2 => {
            w.be_i16(0);
            w.be_i16(0);
        }
        8 => {
            w.be_i64(0);
            w.be_i64(0);
        }
        _ => {
            w.be_i32(0);
            w.be_i32(0);
        }
    }
}
