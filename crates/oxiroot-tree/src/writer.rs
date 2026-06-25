//! Writing a flat (scalar-branch) `TTree` into a ROOT file.
//!
//! Mirrors the layout ROOT/uproot write (TTree v20, TBranch v13, TLeaf* v1) so
//! the result reads back in ROOT, uproot, and this crate. One basket per branch.
//! The embedded `TStreamerInfo` (a baked blob) makes the file self-describing.

use std::path::Path;

use oxiroot_io_core::buffer::{CountToken, Patch, WBuffer, K_BYTE_COUNT_MASK};
use oxiroot_io_core::error::Result;
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

/// One named branch to write. Use the typed constructors ([`Branch::i32`], …).
pub struct Branch {
    /// Branch (and leaf) name.
    pub name: String,
    /// Branch values (a scalar [`BranchValues`] variant).
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

/// The on-disk description of one scalar leaf type.
struct LeafInfo {
    /// `TLeafI`/`TLeafD`/… class name.
    class: &'static str,
    /// Leaflist type code (`I`/`D`/`O`/…) used in the branch title `"name/CODE"`.
    code: char,
    /// `fLenType` — element byte width.
    size: i32,
    /// `fIsUnsigned`.
    unsigned: bool,
}

impl Branch {
    /// Number of entries (one element per entry for a scalar branch).
    fn n_entries(&self) -> u32 {
        let n = match &self.values {
            BranchValues::Bool(v) => v.len(),
            BranchValues::I8(v) => v.len(),
            BranchValues::U8(v) => v.len(),
            BranchValues::I16(v) => v.len(),
            BranchValues::U16(v) => v.len(),
            BranchValues::I32(v) => v.len(),
            BranchValues::U32(v) => v.len(),
            BranchValues::I64(v) => v.len(),
            BranchValues::U64(v) => v.len(),
            BranchValues::F32(v) => v.len(),
            BranchValues::F64(v) => v.len(),
            _ => 0,
        };
        n as u32
    }

    fn leaf(&self) -> LeafInfo {
        match &self.values {
            BranchValues::Bool(_) => LeafInfo {
                class: "TLeafO",
                code: 'O',
                size: 1,
                unsigned: false,
            },
            BranchValues::I8(_) => LeafInfo {
                class: "TLeafB",
                code: 'B',
                size: 1,
                unsigned: false,
            },
            BranchValues::U8(_) => LeafInfo {
                class: "TLeafB",
                code: 'b',
                size: 1,
                unsigned: true,
            },
            BranchValues::I16(_) => LeafInfo {
                class: "TLeafS",
                code: 'S',
                size: 2,
                unsigned: false,
            },
            BranchValues::U16(_) => LeafInfo {
                class: "TLeafS",
                code: 's',
                size: 2,
                unsigned: true,
            },
            BranchValues::I32(_) => LeafInfo {
                class: "TLeafI",
                code: 'I',
                size: 4,
                unsigned: false,
            },
            BranchValues::U32(_) => LeafInfo {
                class: "TLeafI",
                code: 'i',
                size: 4,
                unsigned: true,
            },
            BranchValues::I64(_) => LeafInfo {
                class: "TLeafL",
                code: 'L',
                size: 8,
                unsigned: false,
            },
            BranchValues::U64(_) => LeafInfo {
                class: "TLeafL",
                code: 'l',
                size: 8,
                unsigned: true,
            },
            BranchValues::F32(_) => LeafInfo {
                class: "TLeafF",
                code: 'F',
                size: 4,
                unsigned: false,
            },
            BranchValues::F64(_) => LeafInfo {
                class: "TLeafD",
                code: 'D',
                size: 8,
                unsigned: false,
            },
            _ => LeafInfo {
                class: "TLeafI",
                code: 'I',
                size: 4,
                unsigned: false,
            },
        }
    }

    /// Big-endian on-disk bytes for the values.
    fn data(&self) -> Vec<u8> {
        macro_rules! be {
            ($v:expr, $w:expr) => {{
                let mut out = Vec::with_capacity($v.len() * $w);
                for x in $v {
                    out.extend_from_slice(&x.to_be_bytes());
                }
                out
            }};
        }
        match &self.values {
            BranchValues::Bool(v) => v.iter().map(|&b| b as u8).collect(),
            BranchValues::I8(v) => v.iter().map(|&x| x as u8).collect(),
            BranchValues::U8(v) => v.clone(),
            BranchValues::I16(v) => be!(v, 2),
            BranchValues::U16(v) => be!(v, 2),
            BranchValues::I32(v) => be!(v, 4),
            BranchValues::U32(v) => be!(v, 4),
            BranchValues::I64(v) => be!(v, 8),
            BranchValues::U64(v) => be!(v, 8),
            BranchValues::F32(v) => be!(v, 4),
            BranchValues::F64(v) => be!(v, 8),
            _ => Vec::new(),
        }
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
        tree_file_bytes(file_name, tree_name, branches, compression),
    )?;
    Ok(())
}

/// Build the bytes of a single-tree ROOT file.
pub fn tree_file_bytes(
    file_name: &str,
    tree_name: &str,
    branches: &[Branch],
    compression: Compression,
) -> Vec<u8> {
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

    w.into_vec()
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
    let data = branch.data();
    let payload = on_disk(&data, compression);
    let n_entries = branch.n_entries();
    let nev_buf_size = branch.leaf().size;

    let seek = w.len() as u64;
    let klen = key_len_fmt("TBasket", &branch.name, tree_name, true) as u32 + 19;
    let obj_len = data.len() as u32;
    let nbytes = klen + payload.len() as u32;
    let f_last = klen + obj_len; // border == obj_len for a scalar branch

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
    let title = format!("{}/{}", branch.name, leaf.code);
    let max_baskets = 10i32;

    let tok = w.begin_object(13); // TBranch v13
    write_tnamed(w, OBJ_BITS, &branch.name, &title);
    write_attfill(w);
    w.be_i32(0); // fCompress
    w.be_i32(32000); // fBasketSize
    w.be_i32(0); // fEntryOffsetLen
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
    let outer = w.begin_object(1); // TLeafX v1
    let base = w.begin_object(2); // TLeaf v2
    write_tnamed(w, OBJ_BITS, &branch.name, &branch.name);
    w.be_i32(1); // fLen
    w.be_i32(leaf.size); // fLenType
    w.be_i32(0); // fOffset
    w.u8(0); // fIsRange
    w.u8(leaf.unsigned as u8); // fIsUnsigned
    w.be_u32(0); // fLeafCount (null)
    w.end_object(base);
    // fMinimum, fMaximum in the leaf's element width.
    write_leaf_minmax(w, leaf.size);
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
