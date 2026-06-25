//! Reading a `TTree` and its branches.
//!
//! `TTree`/`TBranch`/`TLeaf` are "core" classes whose member layout is read
//! version-aware and hardcoded (as uproot does), targeting the layout written by
//! current ROOT/uproot (TTree v20, TBranch v13, TLeaf v2, TLeaf* v1). The branch
//! data itself lives in [`crate::basket`]s. Tier 1: flat branches with a single
//! primitive leaf.

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
}

/// One branch's metadata: its leaf type and the location of its baskets.
#[derive(Debug, Clone)]
struct Branch {
    name: String,
    leaf_type: LeafType,
    /// `fLen` — elements per entry (1 for a scalar branch).
    leaf_len: i32,
    /// Number of baskets actually written (`fWriteBasket`).
    n_baskets: usize,
    /// File offset of each basket (`fBasketSeek`).
    basket_seek: Vec<u64>,
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

    /// Read all values of branch `name` across every basket.
    pub fn read_branch(&self, file: &RFile, name: &str) -> Result<BranchValues> {
        let branch = self
            .branches
            .iter()
            .find(|b| b.name == name)
            .ok_or_else(|| Error::Format(format!("no branch named {name:?}")))?;
        if branch.leaf_len != 1 {
            return Err(Error::Format(format!(
                "branch {name:?}: fixed-array leaves (fLen={}) are not supported yet",
                branch.leaf_len
            )));
        }

        // Concatenate every basket's entry data, then decode the column once.
        let mut bytes = Vec::new();
        let mut total = 0u64;
        for i in 0..branch.n_baskets {
            let seek = *branch.basket_seek.get(i).ok_or_else(|| {
                Error::Format(format!("branch {name:?}: missing basket {i} seek"))
            })?;
            let basket = Basket::read(file.data(), seek)?;
            bytes.extend_from_slice(basket.entry_data());
            total += basket.n_entries as u64;
        }
        // The concatenated entry bytes must be exactly one element per entry.
        if bytes.len() != total as usize * branch.leaf_type.size() {
            return Err(Error::Format(format!(
                "branch {name:?}: {} basket bytes for {total} {:?} entries",
                bytes.len(),
                branch.leaf_type
            )));
        }
        decode(branch.leaf_type, &bytes)
    }
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

    let branches = read_branch_array(&mut r, &mut tags)?;

    // fLeaves and everything after it is not needed; jump to the tree's end.
    if let Some(end) = tree_hdr.end {
        r.seek(end)?;
    }

    Ok(TTree {
        name: named.name,
        entries: entries.max(0) as u64,
        branches,
    })
}

/// Read a `TObjArray` of `TBranch`es. Branch classes we don't yet handle
/// (e.g. `TBranchElement`) are skipped via the object byte count.
fn read_branch_array(r: &mut RBuffer, tags: &mut TagReader) -> Result<Vec<Branch>> {
    read_version_tobject_header(r)?;
    let size = r.be_i32()?.max(0);
    let _lower = r.be_i32()?;

    let mut branches = Vec::new();
    for _ in 0..size {
        let header = tags.read_header(r)?;
        if header.class_name.as_deref() == Some("TBranch") {
            if let Some(b) = read_branch(r, tags)? {
                branches.push(b);
            }
        }
        if let Some(end) = header.end {
            r.seek(end)?;
        }
    }
    Ok(branches)
}

/// Read one `TBranch` (v13) body, after its object header. Returns `None` if its
/// single leaf has an unsupported type.
fn read_branch(r: &mut RBuffer, tags: &mut TagReader) -> Result<Option<Branch>> {
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

    let sub = read_branch_array(r, tags)?; // fBranches (sub-branches)
    let leaves = read_leaf_array(r, tags)?; // fLeaves
    read_skip_array(r, tags)?; // fBaskets (empty on disk)

    // fBasketBytes (int[fMaxBaskets]), fBasketEntry (i64[]), fBasketSeek (i64[]):
    // each preceded by a marker byte.
    let _basket_bytes = read_marked_array(r, max_baskets, |r| r.be_i32().map(|v| v as i64))?;
    let _basket_entry = read_marked_array(r, max_baskets, |r| r.be_i64())?;
    let basket_seek = read_marked_array(r, max_baskets, |r| r.be_i64())?;

    // (fFileName TString follows; ignored — we jump to the object end via the
    // caller's byte count.)

    // Tier 1 only handles a branch with no sub-branches and exactly one leaf.
    if !sub.is_empty() || leaves.len() != 1 {
        return Ok(None);
    }
    let (leaf_type, leaf_len) = leaves[0];
    let basket_seek = basket_seek
        .into_iter()
        .map(|s| s.max(0) as u64)
        .collect::<Vec<_>>();

    Ok(Some(Branch {
        name: named.name,
        leaf_type,
        leaf_len,
        n_baskets: write_basket,
        basket_seek,
    }))
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

/// Decode `bytes` as a contiguous big-endian array of `leaf`-typed values.
fn decode(leaf: LeafType, bytes: &[u8]) -> Result<BranchValues> {
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
    })
}
