//! Reading a `TTree` and its branches.
//!
//! `TTree`/`TBranch`/`TLeaf` are "core" classes whose member layout is read
//! version-aware and hardcoded (as uproot does), targeting the layout written by
//! current ROOT/uproot (TTree v20, TBranch v13, TLeaf v2, TLeaf* v1). The branch
//! data itself lives in [`crate::basket`]s. Handles single-leaf branches:
//! scalars, fixed (`x[N]`) and variable (`x[n]`) arrays, and `TLeafC` strings,
//! unsplit `std::vector<T>` `TBranchElement` branches (the element type comes
//! from `fClassName`, and each entry carries a 10-byte streamer header), and
//! *split* (`fSplitLevel > 0`) `std::vector<MyStruct>` branches, which are
//! exposed as their per-member jagged sub-branches (`hits.x`, `hits.y`, …).

use oxiroot_io_core::buffer::RBuffer;
use oxiroot_io_core::error::{Error, Result};
use oxiroot_io_core::object::TagReader;
use oxiroot_io_core::streamer::{read_tnamed, read_tobject, skip_versioned};
use oxiroot_io_core::RFile;

use crate::basket::Basket;
use crate::value::{BranchValues, LeafType};

/// A `TTree` read from a file: its name, entry count, and branches.
#[derive(Debug, Clone)]
pub struct TTree {
    name: String,
    entries: u64,
    branches: Vec<Branch>,
    /// Branches present in the file that this crate cannot (yet) read, as
    /// `(name, reason)` — surfaced via [`TTree::unsupported_branches`].
    unsupported: Vec<(String, String)>,
}

/// One branch's metadata: its leaf type and the location of its baskets.
#[derive(Debug, Clone)]
struct Branch {
    name: String,
    /// `fTitle` — the leaf list / shape string (e.g. `x[3]`, `n`).
    title: String,
    leaf_type: LeafType,
    /// `fLen` — elements per entry (1 for a scalar branch).
    leaf_len: i32,
    /// Number of baskets actually written (`fWriteBasket`).
    n_baskets: usize,
    /// File offset of each basket (`fBasketSeek`).
    basket_seek: Vec<u64>,
    /// Per-entry streamer-header bytes to skip before the element data — `0` for
    /// `TLeaf`-based branches, `10` for an unsplit `std::vector<T>`
    /// `TBranchElement` (byte count + version + size).
    elem_header: usize,
}

impl TTree {
    /// Open the `TTree` named `name` in `file`.
    pub fn open(file: &RFile, name: &str) -> Result<TTree> {
        let key = file
            .key(name)
            .ok_or_else(|| Error::Format(format!("no key named {name:?}")))?;
        if key.class_name != "TTree" {
            return Err(Error::Format(format!(
                "key {name:?} is a {}, not a TTree",
                key.class_name
            )));
        }
        let payload = key.payload(file.data())?;
        let object = oxiroot_compress::decompress(payload, key.obj_len as usize)
            .map_err(|e| Error::Format(format!("decompressing TTree: {e}")))?;
        read_tree(&object, key.key_len as usize)
    }

    /// Total number of entries in the tree (`fEntries`).
    pub fn num_entries(&self) -> u64 {
        self.entries
    }

    /// The tree name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The names of the (readable) branches, in tree order.
    pub fn branch_names(&self) -> Vec<&str> {
        self.branches.iter().map(|b| b.name.as_str()).collect()
    }

    /// The element type of branch `name` (without reading its data), or `None`
    /// if there is no such readable branch.
    pub fn branch_type(&self, name: &str) -> Option<LeafType> {
        self.branch(name).map(|b| b.leaf_type)
    }

    /// `fLen` for branch `name`: the per-entry element count of a fixed-size
    /// array branch (`1` for a scalar; jagged branches report `1` and vary per
    /// entry). `None` if there is no such branch.
    pub fn branch_len(&self, name: &str) -> Option<i32> {
        self.branch(name).map(|b| b.leaf_len)
    }

    /// The title (`fTitle`) of branch `name` — the leaf-list / shape string such
    /// as `x[3]` or `n` — or `None` if there is no such branch.
    pub fn branch_title(&self, name: &str) -> Option<&str> {
        self.branch(name).map(|b| b.title.as_str())
    }

    /// Branches present in the file that this crate cannot read yet, as
    /// `(name, reason)` pairs (e.g. multi-leaf/leaflist branches or unsupported
    /// element types). These are absent from [`branch_names`](Self::branch_names),
    /// so this is the way to see what was skipped and why.
    pub fn unsupported_branches(&self) -> Vec<(&str, &str)> {
        self.unsupported
            .iter()
            .map(|(n, r)| (n.as_str(), r.as_str()))
            .collect()
    }

    fn branch(&self, name: &str) -> Option<&Branch> {
        self.branches.iter().find(|b| b.name == name)
    }

    /// Read all values of branch `name` across every basket.
    ///
    /// Scalar branches yield a flat [`BranchValues`]; fixed (`x[N]`) and
    /// variable (`x[n]`) branches yield a nested one; `TLeafC` yields strings.
    pub fn read_branch(&self, file: &RFile, name: &str) -> Result<BranchValues> {
        let branch = self
            .branches
            .iter()
            .find(|b| b.name == name)
            .ok_or_else(|| Error::Format(format!("no branch named {name:?}")))?;

        let mut baskets = Vec::with_capacity(branch.n_baskets);
        for i in 0..branch.n_baskets {
            let seek = *branch.basket_seek.get(i).ok_or_else(|| {
                Error::Format(format!("branch {name:?}: missing basket {i} seek"))
            })?;
            baskets.push(Basket::read(file.data(), seek)?);
        }

        let variable = baskets.iter().any(|b| b.entry_offsets.is_some());
        if branch.leaf_type == LeafType::Str {
            return decode_strings(&baskets);
        }
        if variable {
            let mut regions = entry_regions_variable(&baskets)?;
            // A `std::vector` `TBranchElement` prefixes each entry with a
            // streamer header; strip it so only the element bytes remain.
            if branch.elem_header > 0 {
                for r in &mut regions {
                    *r = &r[branch.elem_header.min(r.len())..];
                }
            }
            return decode_array(branch.leaf_type, &regions);
        }
        if branch.leaf_len > 1 {
            let stride = branch.leaf_len as usize * branch.leaf_type.size();
            return decode_array(branch.leaf_type, &chunk_regions(&baskets, stride));
        }
        // Scalar: concatenate every basket's entry data, decode once.
        let mut bytes = Vec::new();
        let mut total = 0u64;
        for b in &baskets {
            bytes.extend_from_slice(b.entry_data());
            total += b.n_entries as u64;
        }
        if bytes.len() != total as usize * branch.leaf_type.size() {
            return Err(Error::Format(format!(
                "branch {name:?}: {} basket bytes for {total} {:?} entries",
                bytes.len(),
                branch.leaf_type
            )));
        }
        decode_scalar(branch.leaf_type, &bytes)
    }
}

/// Per-entry byte regions of a variable-length branch, from each basket's
/// `fEntryOffset` array.
fn entry_regions_variable(baskets: &[Basket]) -> Result<Vec<&[u8]>> {
    let mut regions = Vec::new();
    for b in baskets {
        let offs = b
            .entry_offsets
            .as_ref()
            .ok_or_else(|| Error::Format("variable branch basket missing fEntryOffset".into()))?;
        for i in 0..b.n_entries as usize {
            let (a, c) = (
                *offs.get(i).unwrap_or(&b.border),
                *offs.get(i + 1).unwrap_or(&b.border),
            );
            regions.push(b.data.get(a..c).unwrap_or(&[]));
        }
    }
    Ok(regions)
}

/// Per-entry byte regions of a fixed-size array branch: each basket's entry data
/// split into `stride`-byte chunks.
fn chunk_regions(baskets: &[Basket], stride: usize) -> Vec<&[u8]> {
    let mut regions = Vec::new();
    for b in baskets {
        if stride == 0 {
            continue;
        }
        for chunk in b.entry_data().chunks_exact(stride) {
            regions.push(chunk);
        }
    }
    regions
}

/// Parse a decompressed `TTree` object (`keylen` is its key's header length).
fn read_tree(object: &[u8], keylen: usize) -> Result<TTree> {
    let mut r = RBuffer::new(object);
    let mut tags = TagReader::new(keylen);

    let tree_hdr = r.read_version()?; // TTree (v20)
    let named = read_tnamed(&mut r)?; // TNamed: fName, fTitle
    skip_versioned(&mut r)?; // TAttLine
    skip_versioned(&mut r)?; // TAttFill
    skip_versioned(&mut r)?; // TAttMarker

    let entries = r.be_i64()?; // fEntries
    for _ in 0..4 {
        r.be_i64()?; // fTotBytes, fZipBytes, fSavedBytes, fFlushedBytes
    }
    r.be_f64()?; // fWeight
    for _ in 0..4 {
        r.be_i32()?; // fTimerInterval, fScanField, fUpdate, fDefaultEntryOffsetLen
    }
    let n_cluster_range = r.be_i32()?.max(0); // fNClusterRange
    for _ in 0..6 {
        r.be_i64()?; // fMaxEntries..fEstimate
    }
    // fClusterRangeEnd, fClusterSize: each a marker byte then fNClusterRange i64.
    for _ in 0..2 {
        r.u8()?; // is-array marker
        for _ in 0..n_cluster_range {
            r.be_i64()?;
        }
    }
    skip_object(&mut r)?; // fIOFeatures (ROOT::TIOFeatures)

    let mut unsupported = Vec::new();
    let branches = read_branch_array(&mut r, &mut tags, &mut unsupported)?;

    // fLeaves and everything after it is not needed; jump to the tree's end.
    if let Some(end) = tree_hdr.end {
        r.seek(end)?;
    }

    Ok(TTree {
        name: named.name,
        entries: entries.max(0) as u64,
        branches,
        unsupported,
    })
}

/// Read a `TObjArray` of `TBranch`es. Branch classes we don't yet handle
/// (e.g. `TBranchElement`) are skipped via the object byte count.
fn read_branch_array(
    r: &mut RBuffer,
    tags: &mut TagReader,
    diag: &mut Vec<(String, String)>,
) -> Result<Vec<Branch>> {
    read_version_tobject_header(r)?;
    let size = r.be_i32()?.max(0);
    let _lower = r.be_i32()?;

    let mut branches = Vec::new();
    for _ in 0..size {
        let header = tags.read_header(r)?;
        match header.class_name.as_deref() {
            Some("TBranch") => {
                if let Some(b) = read_branch(r, tags, diag)? {
                    branches.push(b);
                }
            }
            Some("TBranchElement") => {
                branches.extend(read_branch_element(r, tags, diag)?);
            }
            Some(other) => diag.push((other.to_string(), "unsupported branch class".to_string())),
            None => {}
        }
        if let Some(end) = header.end {
            r.seek(end)?;
        }
    }
    Ok(branches)
}

/// Read one `TBranch` (v13) body, after its object header. Returns `None` if its
/// single leaf has an unsupported type.
fn read_branch(
    r: &mut RBuffer,
    tags: &mut TagReader,
    diag: &mut Vec<(String, String)>,
) -> Result<Option<Branch>> {
    r.read_version()?; // TBranch (v13)
    let named = read_tnamed(r)?; // fName, fTitle
    skip_versioned(r)?; // TAttFill

    r.be_i32()?; // fCompress
    r.be_i32()?; // fBasketSize
    r.be_i32()?; // fEntryOffsetLen
    let write_basket = r.be_i32()?.max(0) as usize; // fWriteBasket
    r.be_i64()?; // fEntryNumber
    skip_object(r)?; // fIOFeatures
    r.be_i32()?; // fOffset
    let max_baskets = r.be_i32()?.max(0); // fMaxBaskets
    r.be_i32()?; // fSplitLevel
    r.be_i64()?; // fEntries
    r.be_i64()?; // fFirstEntry
    r.be_i64()?; // fTotBytes
    r.be_i64()?; // fZipBytes

    let sub = read_branch_array(r, tags, diag)?; // fBranches (sub-branches)
    let leaves = read_leaf_array(r, tags)?; // fLeaves
    read_skip_array(r, tags)?; // fBaskets (empty on disk)

    // fBasketBytes (int[fMaxBaskets]), fBasketEntry (i64[]), fBasketSeek (i64[]):
    // each preceded by a marker byte.
    let _basket_bytes = read_marked_array(r, max_baskets, |r| r.be_i32().map(|v| v as i64))?;
    let _basket_entry = read_marked_array(r, max_baskets, |r| r.be_i64())?;
    let basket_seek = read_marked_array(r, max_baskets, |r| r.be_i64())?;

    // (fFileName TString follows; ignored — we jump to the object end via the
    // caller's byte count.)

    // We handle a branch with no sub-branches and exactly one leaf; anything else
    // is recorded as unsupported (rather than silently vanishing).
    if !sub.is_empty() {
        diag.push((
            named.name,
            "branch with sub-branches is not supported".to_string(),
        ));
        return Ok(None);
    }
    if leaves.len() != 1 {
        let reason = if leaves.is_empty() {
            "no supported leaf type".to_string()
        } else {
            format!(
                "multi-leaf (leaflist) branch with {} leaves is not yet supported",
                leaves.len()
            )
        };
        diag.push((named.name, reason));
        return Ok(None);
    }
    let (leaf_type, leaf_len) = leaves[0];
    let basket_seek = basket_seek
        .into_iter()
        .map(|s| s.max(0) as u64)
        .collect::<Vec<_>>();

    Ok(Some(Branch {
        name: named.name,
        title: named.title,
        leaf_type,
        leaf_len,
        n_baskets: write_basket,
        basket_seek,
        elem_header: 0,
    }))
}

/// Read one `TBranchElement` (v10) body, after its object header. Returns the
/// readable branches it contributes:
/// - an unsplit `std::vector<T>` (`fType` 0) → one branch (element type from
///   `fClassName`, each entry prefixed by a 10-byte streamer header);
/// - a split STL/clones collection (`fType` 3/4) → its member sub-branches (the
///   parent holds no data of its own);
/// - a split member sub-branch (`fType` 41/31) → one jagged branch (element type
///   from `fStreamerType`, no per-entry header).
///
/// Unsupported element types contribute nothing.
fn read_branch_element(
    r: &mut RBuffer,
    tags: &mut TagReader,
    diag: &mut Vec<(String, String)>,
) -> Result<Vec<Branch>> {
    r.read_version()?; // TBranchElement (v10) — the object's own version
                       // Then the TBranch base — same layout as a standalone TBranch body.
    r.read_version()?; // TBranch base (v13)
    let named = read_tnamed(r)?; // fName, fTitle
    skip_versioned(r)?; // TAttFill
    r.be_i32()?; // fCompress
    r.be_i32()?; // fBasketSize
    r.be_i32()?; // fEntryOffsetLen
    let write_basket = r.be_i32()?.max(0) as usize; // fWriteBasket
    r.be_i64()?; // fEntryNumber
    skip_object(r)?; // fIOFeatures
    r.be_i32()?; // fOffset
    let max_baskets = r.be_i32()?.max(0); // fMaxBaskets
    r.be_i32()?; // fSplitLevel
    r.be_i64()?; // fEntries
    r.be_i64()?; // fFirstEntry
    r.be_i64()?; // fTotBytes
    r.be_i64()?; // fZipBytes
    let sub = read_branch_array(r, tags, diag)?; // fBranches (sub-branches if split)
    read_leaf_array(r, tags)?; // fLeaves (a TLeafElement; skipped)
    read_skip_array(r, tags)?; // fBaskets (empty on disk)
    let _basket_bytes = read_marked_array(r, max_baskets, |r| r.be_i32().map(|v| v as i64))?;
    let _basket_entry = read_marked_array(r, max_baskets, |r| r.be_i64())?;
    let basket_seek = read_marked_array(r, max_baskets, |r| r.be_i64())?;
    r.string()?; // fFileName (end of the TBranch base)

    // TBranchElement members. fType/fStreamerType decide how this branch is read.
    let class_name = r.string()?; // fClassName, e.g. "vector<float>" or "Hit"
    r.string()?; // fParentName
    r.string()?; // fClonesName
    r.be_u32()?; // fCheckSum
    r.be_i16()?; // fClassVersion
    r.be_i32()?; // fID
    let f_type = r.be_i32()?; // fType
    let f_streamer_type = r.be_i32()?; // fStreamerType
                                       // (fMaximum, fBranchCount, fBranchCount2 follow; skipped via the byte count.)

    // A split collection (STL `4`, TClonesArray `3`) holds no data itself — its
    // member sub-branches do, and they were just parsed into `sub`.
    if f_type == 3 || f_type == 4 {
        return Ok(sub);
    }

    // A member sub-branch (STL `41`, TClonesArray `31`) is a jagged array typed
    // by fStreamerType, with no per-entry header. An unsplit branch (`0`) is the
    // whole `std::vector<T>` typed by fClassName, with the 10-byte header.
    let member = f_type == 41 || f_type == 31;
    let leaf_type = if member {
        streamer_type_to_leaf(f_streamer_type)
    } else {
        parse_vector_elem(&class_name)
    };
    let Some(leaf_type) = leaf_type else {
        diag.push((
            named.name,
            format!("unsupported TBranchElement (fType={f_type}, class {class_name:?})"),
        ));
        return Ok(Vec::new());
    };
    let basket_seek = basket_seek.into_iter().map(|s| s.max(0) as u64).collect();
    Ok(vec![Branch {
        name: named.name,
        title: named.title,
        leaf_type,
        leaf_len: 1,
        n_baskets: write_basket,
        basket_seek,
        elem_header: if member { 0 } else { 10 },
    }])
}

/// Map a `TStreamerInfo` basic-type code (`fStreamerType`, ROOT's `EDataType`)
/// to its [`LeafType`], for split member sub-branches. `None` for unsupported.
fn streamer_type_to_leaf(st: i32) -> Option<LeafType> {
    Some(match st {
        1 => LeafType::I8,        // kChar
        2 => LeafType::I16,       // kShort
        3 => LeafType::I32,       // kInt
        4 | 16 => LeafType::I64,  // kLong / kLong64
        5 => LeafType::F32,       // kFloat
        8 => LeafType::F64,       // kDouble
        11 => LeafType::U8,       // kUChar
        12 => LeafType::U16,      // kUShort
        13 => LeafType::U32,      // kUInt
        14 | 17 => LeafType::U64, // kULong / kULong64
        18 => LeafType::Bool,     // kBool
        _ => return None,
    })
}

/// Map a `std::vector<T>` class name to its element [`LeafType`], or `None` for
/// an unsupported element type.
fn parse_vector_elem(class_name: &str) -> Option<LeafType> {
    let inner = class_name
        .strip_prefix("vector<")
        .or_else(|| class_name.strip_prefix("std::vector<"))?
        .strip_suffix('>')?
        .trim();
    Some(match inner {
        "float" => LeafType::F32,
        "double" => LeafType::F64,
        "int" | "Int_t" => LeafType::I32,
        "unsigned int" | "UInt_t" => LeafType::U32,
        "short" | "Short_t" => LeafType::I16,
        "unsigned short" | "UShort_t" => LeafType::U16,
        "char" | "Char_t" | "int8_t" => LeafType::I8,
        "unsigned char" | "UChar_t" | "uint8_t" => LeafType::U8,
        "bool" | "Bool_t" => LeafType::Bool,
        "long" | "long long" | "Long64_t" | "Long_t" => LeafType::I64,
        "unsigned long" | "unsigned long long" | "ULong64_t" | "ULong_t" => LeafType::U64,
        _ => return None,
    })
}

/// Read a `TObjArray` of `TLeaf`s, returning `(type, fLen)` for each supported
/// leaf (unsupported leaves are skipped).
fn read_leaf_array(r: &mut RBuffer, tags: &mut TagReader) -> Result<Vec<(LeafType, i32)>> {
    read_version_tobject_header(r)?;
    let size = r.be_i32()?.max(0);
    let _lower = r.be_i32()?;

    let mut leaves = Vec::new();
    for _ in 0..size {
        let header = tags.read_header(r)?;
        if let Some(class) = header.class_name.clone() {
            if let Some(leaf) = read_leaf(r, &class)? {
                leaves.push(leaf);
            }
        }
        if let Some(end) = header.end {
            r.seek(end)?;
        }
    }
    Ok(leaves)
}

/// Read one `TLeaf*` (v1) body enough to recover its element type and `fLen`.
fn read_leaf(r: &mut RBuffer, class: &str) -> Result<Option<(LeafType, i32)>> {
    r.read_version()?; // TLeafX (v1) — the leaf subclass wrapper
    r.read_version()?; // TLeaf base (v2)
    read_tnamed(r)?; // fName, fTitle
    let len = r.be_i32()?; // fLen
    r.be_i32()?; // fLenType
    r.be_i32()?; // fOffset
    r.u8()?; // fIsRange
    let unsigned = r.u8()? != 0; // fIsUnsigned
                                 // fLeafCount, fMinimum, fMaximum follow; we skip to the leaf's end via the
                                 // caller's byte count.
    Ok(LeafType::from_leaf(class, unsigned).map(|t| (t, len)))
}

/// Read a `TObjArray` and discard it (used for `fBaskets`, always empty here).
fn read_skip_array(r: &mut RBuffer, tags: &mut TagReader) -> Result<()> {
    read_version_tobject_header(r)?;
    let size = r.be_i32()?.max(0);
    let _lower = r.be_i32()?;
    for _ in 0..size {
        let header = tags.read_header(r)?;
        if let Some(end) = header.end {
            r.seek(end)?;
        }
    }
    Ok(())
}

/// Read the `{version}` + `TObject` + name prefix common to `TObjArray`/`TList`.
fn read_version_tobject_header(r: &mut RBuffer) -> Result<()> {
    r.read_version()?;
    read_tobject(r)?;
    r.string()?; // fName
    Ok(())
}

/// Read a `[fN]`-style member array: a single marker byte then `n` elements.
fn read_marked_array<T>(
    r: &mut RBuffer,
    n: i32,
    mut read: impl FnMut(&mut RBuffer) -> Result<T>,
) -> Result<Vec<T>> {
    r.u8()?; // is-array marker
    let n = n.max(0) as usize;
    let mut out = Vec::with_capacity(n.min(r.remaining()));
    for _ in 0..n {
        out.push(read(r)?);
    }
    Ok(out)
}

/// Skip an inline object member, using its version-header byte count when
/// present, else assuming a single trailing byte (e.g. `TIOFeatures`).
fn skip_object(r: &mut RBuffer) -> Result<()> {
    let vh = r.read_version()?;
    match vh.end {
        Some(end) => r.seek(end)?,
        None => {
            r.u8()?;
        }
    }
    Ok(())
}

/// Decode `bytes` as a contiguous big-endian array of `leaf`-typed scalars.
fn decode_scalar(leaf: LeafType, bytes: &[u8]) -> Result<BranchValues> {
    macro_rules! be {
        ($variant:ident, $ty:ty, $w:expr) => {{
            let mut v = Vec::with_capacity(bytes.len() / $w);
            for c in bytes.chunks_exact($w) {
                v.push(<$ty>::from_be_bytes(c.try_into().unwrap()));
            }
            BranchValues::$variant(v)
        }};
    }
    Ok(match leaf {
        LeafType::Bool => BranchValues::Bool(bytes.iter().map(|&b| b != 0).collect()),
        LeafType::I8 => BranchValues::I8(bytes.iter().map(|&b| b as i8).collect()),
        LeafType::U8 => BranchValues::U8(bytes.to_vec()),
        LeafType::I16 => be!(I16, i16, 2),
        LeafType::U16 => be!(U16, u16, 2),
        LeafType::I32 => be!(I32, i32, 4),
        LeafType::U32 => be!(U32, u32, 4),
        LeafType::I64 => be!(I64, i64, 8),
        LeafType::U64 => be!(U64, u64, 8),
        LeafType::F32 => be!(F32, f32, 4),
        LeafType::F64 => be!(F64, f64, 8),
        LeafType::Str => return Err(Error::Format("string branch decoded as scalar".into())),
    })
}

/// Decode each per-entry `region` into a vector of `leaf`-typed values, yielding
/// one inner vector per entry.
fn decode_array(leaf: LeafType, regions: &[&[u8]]) -> Result<BranchValues> {
    macro_rules! be {
        ($variant:ident, $ty:ty, $w:expr) => {{
            let mut out = Vec::with_capacity(regions.len());
            for r in regions {
                let mut g = Vec::with_capacity(r.len() / $w);
                for c in r.chunks_exact($w) {
                    g.push(<$ty>::from_be_bytes(c.try_into().unwrap()));
                }
                out.push(g);
            }
            BranchValues::$variant(out)
        }};
    }
    Ok(match leaf {
        LeafType::Bool => BranchValues::VecBool(
            regions
                .iter()
                .map(|r| r.iter().map(|&b| b != 0).collect())
                .collect(),
        ),
        LeafType::I8 => BranchValues::VecI8(
            regions
                .iter()
                .map(|r| r.iter().map(|&b| b as i8).collect())
                .collect(),
        ),
        LeafType::U8 => BranchValues::VecU8(regions.iter().map(|r| r.to_vec()).collect()),
        LeafType::I16 => be!(VecI16, i16, 2),
        LeafType::U16 => be!(VecU16, u16, 2),
        LeafType::I32 => be!(VecI32, i32, 4),
        LeafType::U32 => be!(VecU32, u32, 4),
        LeafType::I64 => be!(VecI64, i64, 8),
        LeafType::U64 => be!(VecU64, u64, 8),
        LeafType::F32 => be!(VecF32, f32, 4),
        LeafType::F64 => be!(VecF64, f64, 8),
        LeafType::Str => return Err(Error::Format("string branch decoded as array".into())),
    })
}

/// Decode `TLeafC` baskets: each entry is one ROOT-encoded (length-prefixed)
/// string, read sequentially from the entry data.
fn decode_strings(baskets: &[Basket]) -> Result<BranchValues> {
    let mut out = Vec::new();
    for b in baskets {
        let mut r = RBuffer::new(b.entry_data());
        for _ in 0..b.n_entries {
            out.push(r.string()?);
        }
    }
    Ok(BranchValues::Str(out))
}
