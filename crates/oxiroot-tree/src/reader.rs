//! Reading a `TTree` and its branches.
//!
//! `TTree`/`TBranch`/`TBranchElement` are parsed by walking the member list in
//! the file's own `TStreamerInfo` (see [`walk_members`]) rather than at fixed
//! offsets, so the reader follows whatever schema the file declares; an unknown
//! member type is reported instead of parsed at a guessed offset. (`TLeaf*` are
//! still read by their compact, byte-count-bounded layout.) The branch data
//! itself lives in [`crate::basket`]s. Handles single-leaf branches:
//! scalars, fixed (`x[N]`) and variable (`x[n]`) arrays, and `TLeafC` strings,
//! unsplit `std::vector<T>` `TBranchElement` branches (the element type comes
//! from `fClassName`, and each entry carries a 10-byte streamer header), and
//! *split* (`fSplitLevel > 0`) `std::vector<MyStruct>` branches, which are
//! exposed as their per-member jagged sub-branches (`hits.x`, `hits.y`, …).

use oxiroot_io_core::buffer::RBuffer;
use oxiroot_io_core::error::{Error, Result};
use oxiroot_io_core::object::TagReader;
use oxiroot_io_core::streamer::{read_tnamed, read_tobject, skip_versioned};
use oxiroot_io_core::streamer_info::{StreamerElement, StreamerRegistry};
use oxiroot_io_core::RFile;

use crate::basket::Basket;
use crate::value::{BranchValues, Jagged, LeafType};

/// A `TTree` read from a file: its name, entry count, and branches.
#[derive(Debug, Clone)]
pub struct TTree {
    name: String,
    entries: u64,
    branches: Vec<Branch>,
    /// Branches present in the file that this crate cannot (yet) read, as
    /// `(name, reason)` — surfaced via [`TTree::unsupported_branches`].
    unsupported: Vec<(String, String)>,
    /// The classes (and versions) declared in the file's `TStreamerInfo` — the
    /// schema this tree was written against; surfaced via
    /// [`TTree::streamer_classes`]. Empty if the file has no streamer info.
    streamer_classes: Vec<(String, i32)>,
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
    /// First entry number of each basket (`fBasketEntry`, `n_baskets` values),
    /// for selecting the baskets that cover an entry range.
    basket_entry: Vec<i64>,
    /// Per-entry streamer-header bytes to skip before the element data — `0` for
    /// `TLeaf`-based branches, `10` for an unsplit `std::vector<T>`
    /// `TBranchElement` (byte count + version + size).
    elem_header: usize,
    /// For one leaf of a multi-leaf (leaflist) branch: `(byte offset of this leaf
    /// within an entry, total entry stride)`. `None` for a single-leaf branch.
    leaflist: Option<(usize, usize)>,
    /// Per-entry array shape parsed from the leaf title — `[N]` for `x[N]`,
    /// `[N, M]` for a multidimensional `x[N][M]`, empty for a scalar. The data is
    /// stored row-major flat (`fLen` = the product); this records the split.
    dims: Vec<usize>,
    /// Set for a `std::vector<std::vector<T>>` branch: `T`'s element type. The
    /// entry data is decoded as a doubly-nested collection ([`BranchValues::Nested`])
    /// rather than a flat jagged array.
    nested_elem: Option<LeafType>,
}

/// One `TLeaf` of a branch: its name/title, element type, fixed length, and byte
/// offset within an entry (`fOffset`, non-zero only inside a leaflist).
struct Leaf {
    name: String,
    title: String,
    leaf_type: LeafType,
    len: i32,
    offset: usize,
}

impl TTree {
    /// Open the `TTree` named `name` in `file`.
    pub fn open(file: &RFile, name: &str) -> Result<TTree> {
        let key = file
            .key(name)
            .ok_or_else(|| Error::Format(format!("no key named {name:?}")))?;
        // `TNtuple` / `TNtupleD` are `TTree` subclasses (a `TTree` base wrapped in
        // one extra header plus a trailing `Int_t fNvar`); read them as trees too.
        if !matches!(key.class_name.as_str(), "TTree" | "TNtuple" | "TNtupleD") {
            return Err(Error::Format(format!(
                "key {name:?} is a {}, not a TTree",
                key.class_name
            )));
        }
        // The file's TStreamerInfo is the authoritative schema: the reader walks
        // each class's declared member list rather than assuming a fixed layout,
        // so it adapts to the version the file was written with.
        let registry = file.streamer_registry()?;

        let payload = key.payload(file.data())?;
        let object = oxiroot_compress::decompress(payload, key.obj_len as usize)
            .map_err(|e| Error::Format(format!("decompressing TTree: {e}")))?;
        let mut tree = read_tree(&object, key.key_len as usize, &registry, &key.class_name)?;
        tree.streamer_classes = registry
            .infos()
            .iter()
            .map(|i| (i.class_name.clone(), i.class_version))
            .collect();
        Ok(tree)
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

    /// The per-entry fixed array shape of branch `name`: `[N]` for `x[N]`,
    /// `[N, M]` for a multidimensional `x[N][M]`, and `[]` for a scalar or a
    /// variable-length branch. The values are stored row-major flat (each entry's
    /// inner vector has `N` or `N*M` elements); use this to reshape them. `None`
    /// if there is no such branch.
    pub fn branch_shape(&self, name: &str) -> Option<&[usize]> {
        self.branch(name).map(|b| b.dims.as_slice())
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

    /// The classes (and their versions) declared in the file's `TStreamerInfo` —
    /// the schema this tree was written against (e.g. `("TTree", 20)`,
    /// `("TBranch", 13)`). Empty if the file carries no streamer info. This is the
    /// member layout the reader walks on [`open`](Self::open) to parse the tree.
    pub fn streamer_classes(&self) -> Vec<(&str, i32)> {
        self.streamer_classes
            .iter()
            .map(|(n, v)| (n.as_str(), *v))
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
            .branch(name)
            .ok_or_else(|| Error::Format(format!("no branch named {name:?}")))?;
        let baskets = read_baskets(file, branch, 0..branch.n_baskets)?;
        decode_baskets(branch, &baskets)
    }

    /// Read several branches at once, in the requested order — a columnar
    /// `tree.arrays`-style read. Each is read with [`read_branch`](Self::read_branch).
    pub fn read_branches(&self, file: &RFile, names: &[&str]) -> Result<Vec<BranchValues>> {
        names.iter().map(|n| self.read_branch(file, n)).collect()
    }

    /// Read only entries `[start, stop)` of branch `name`, fetching just the
    /// baskets that cover the range rather than the whole branch. `stop` is
    /// clamped to the entry count and `start` to `stop`, so an out-of-range
    /// window yields fewer (or no) entries instead of an error.
    pub fn read_branch_range(
        &self,
        file: &RFile,
        name: &str,
        start: u64,
        stop: u64,
    ) -> Result<BranchValues> {
        let branch = self
            .branch(name)
            .ok_or_else(|| Error::Format(format!("no branch named {name:?}")))?;
        let stop = stop.min(self.entries);
        let start = start.min(stop);

        // Per-basket entry boundaries: basket i covers [start_i, start_{i+1}),
        // the last basket ending at the tree's entry count.
        let have_bounds = branch.basket_entry.len() == branch.n_baskets;
        let basket_start = |i: usize| branch.basket_entry.get(i).map_or(0, |&e| e.max(0) as u64);
        let basket_stop = |i: usize| {
            if i + 1 < branch.basket_entry.len() {
                basket_start(i + 1)
            } else {
                self.entries
            }
        };

        // Select the baskets overlapping [start, stop). Without boundaries we
        // can't tell, so read them all (still correct after slicing).
        let mut indices = Vec::new();
        let mut first_entry = 0u64;
        for i in 0..branch.n_baskets {
            let keep = !have_bounds || (basket_start(i) < stop && basket_stop(i) > start);
            if keep {
                if indices.is_empty() {
                    first_entry = if have_bounds { basket_start(i) } else { 0 };
                }
                indices.push(i);
            }
        }

        let baskets = read_baskets(file, branch, indices.iter().copied())?;
        let values = decode_baskets(branch, &baskets)?;
        // `values` covers [first_entry, ..); slice out [start, stop).
        let off = start.saturating_sub(first_entry) as usize;
        let len = (stop - start) as usize;
        Ok(slice_values(values, off, len))
    }

    /// Read branch `name` as a [`Jagged`] view — cumulative `offsets` over one
    /// flat scalar [`BranchValues`] — without allocating a `Vec` per entry. Works
    /// for scalar (one element per entry), fixed `x[N]`, multidimensional, and
    /// variable/jagged numeric branches; string branches are not supported (use
    /// [`read_branch`](Self::read_branch)).
    pub fn read_branch_flat(&self, file: &RFile, name: &str) -> Result<Jagged> {
        let branch = self
            .branch(name)
            .ok_or_else(|| Error::Format(format!("no branch named {name:?}")))?;
        if branch.leaf_type == LeafType::Str {
            return Err(Error::Format(format!(
                "branch {name:?} is a string branch; use read_branch"
            )));
        }
        let baskets = read_baskets(file, branch, 0..branch.n_baskets)?;
        let regions = entry_regions(branch, &baskets);
        let size = branch.leaf_type.size().max(1);

        let mut offsets = Vec::with_capacity(regions.len() + 1);
        offsets.push(0u64);
        let mut bytes = Vec::new();
        let mut acc = 0u64;
        for r in &regions {
            bytes.extend_from_slice(r);
            acc += (r.len() / size) as u64;
            offsets.push(acc);
        }
        Ok(Jagged {
            offsets,
            values: decode_scalar(branch.leaf_type, &bytes)?,
        })
    }
}

/// Per-entry byte regions of a numeric branch (the shared shape behind the
/// jagged/array/scalar read paths), for the flat [`TTree::read_branch_flat`].
fn entry_regions<'a>(branch: &Branch, baskets: &'a [Basket]) -> Vec<&'a [u8]> {
    if let Some((offset, stride)) = branch.leaflist {
        if stride == 0 {
            return Vec::new();
        }
        let width = branch.leaf_len.max(1) as usize * branch.leaf_type.size();
        let mut regions = Vec::new();
        for b in baskets {
            for chunk in b.entry_data().chunks_exact(stride) {
                let end = (offset + width).min(chunk.len());
                regions.push(chunk.get(offset..end).unwrap_or(&[]));
            }
        }
        return regions;
    }
    if baskets.iter().any(|b| b.entry_offsets.is_some()) {
        let mut regions = entry_regions_variable(baskets).unwrap_or_default();
        if branch.elem_header > 0 {
            for r in &mut regions {
                *r = &r[branch.elem_header.min(r.len())..];
            }
        }
        return regions;
    }
    // Fixed array or scalar: one chunk of `leaf_len` elements per entry.
    let stride = branch.leaf_len.max(1) as usize * branch.leaf_type.size();
    chunk_regions(baskets, stride)
}

/// Read the requested baskets of `branch` (by index) and decompress them, in
/// order. With the `rayon` feature the per-basket decompress runs in parallel.
fn read_baskets(
    file: &RFile,
    branch: &Branch,
    indices: impl Iterator<Item = usize>,
) -> Result<Vec<Basket>> {
    let data = file.data();
    let seek_of = |i: usize| -> Result<u64> {
        branch.basket_seek.get(i).copied().ok_or_else(|| {
            Error::Format(format!("branch {:?}: missing basket {i} seek", branch.name))
        })
    };

    #[cfg(feature = "rayon")]
    {
        use rayon::prelude::*;
        let indices: Vec<usize> = indices.collect();
        // par_iter().collect() into a Result preserves order and short-circuits
        // on the first error; the file data and branch are read-only (Sync).
        indices
            .into_par_iter()
            .map(|i| Basket::read(data, seek_of(i)?))
            .collect()
    }
    #[cfg(not(feature = "rayon"))]
    {
        let mut out = Vec::new();
        for i in indices {
            out.push(Basket::read(data, seek_of(i)?)?);
        }
        Ok(out)
    }
}

/// Decode the given (contiguous, in-order) baskets of `branch` into per-entry
/// [`BranchValues`] — the shared body of [`TTree::read_branch`] and
/// [`TTree::read_branch_range`].
fn decode_baskets(branch: &Branch, baskets: &[Basket]) -> Result<BranchValues> {
    // A leaflist leaf: take this leaf's bytes out of each entry's fixed stride
    // at its offset, then decode like a scalar / fixed array.
    if let Some((offset, stride)) = branch.leaflist {
        if stride == 0 {
            return decode_scalar(branch.leaf_type, &[]);
        }
        let width = branch.leaf_len.max(1) as usize * branch.leaf_type.size();
        let mut regions: Vec<&[u8]> = Vec::new();
        for b in baskets {
            for chunk in b.entry_data().chunks_exact(stride) {
                let end = (offset + width).min(chunk.len());
                regions.push(chunk.get(offset..end).unwrap_or(&[]));
            }
        }
        if branch.leaf_len > 1 {
            return decode_array(branch.leaf_type, &regions);
        }
        let bytes: Vec<u8> = regions.concat();
        return decode_scalar(branch.leaf_type, &bytes);
    }

    // A `std::vector<std::vector<T>>`: each entry's region is the outer vector's
    // 10-byte streamer header (whose last 4 bytes are the outer count) followed
    // by the inner vectors, each a `{count, elements}` block.
    if let Some(elem) = branch.nested_elem {
        return decode_nested_vec(&entry_regions_variable(baskets)?, elem);
    }

    let variable = baskets.iter().any(|b| b.entry_offsets.is_some());
    if branch.leaf_type == LeafType::Str {
        // A `std::vector<std::string>` branch carries a 10-byte streamer header
        // (with the element count); a plain `TLeafC` is one string per entry.
        if branch.elem_header > 0 {
            return decode_vec_strings(&entry_regions_variable(baskets)?);
        }
        return decode_strings(baskets);
    }
    if variable {
        let mut regions = entry_regions_variable(baskets)?;
        // A `std::vector` `TBranchElement` prefixes each entry with a streamer
        // header; strip it so only the element bytes remain.
        if branch.elem_header > 0 {
            for r in &mut regions {
                *r = &r[branch.elem_header.min(r.len())..];
            }
        }
        return decode_array(branch.leaf_type, &regions);
    }
    if branch.leaf_len > 1 {
        let stride = branch.leaf_len as usize * branch.leaf_type.size();
        return decode_array(branch.leaf_type, &chunk_regions(baskets, stride));
    }
    // Scalar: concatenate every basket's entry data, decode once.
    let mut bytes = Vec::new();
    let mut total = 0u64;
    for b in baskets {
        bytes.extend_from_slice(b.entry_data());
        total += b.n_entries as u64;
    }
    if bytes.len() != total as usize * branch.leaf_type.size() {
        return Err(Error::Format(format!(
            "branch {:?}: {} basket bytes for {total} {:?} entries",
            branch.name,
            bytes.len(),
            branch.leaf_type
        )));
    }
    decode_scalar(branch.leaf_type, &bytes)
}

/// Slice a decoded branch's values to the sub-range `[offset, offset + len)`
/// (clamped), preserving the variant.
fn slice_values(bv: BranchValues, offset: usize, len: usize) -> BranchValues {
    use BranchValues::*;
    macro_rules! sl {
        ($variant:ident, $v:ident) => {{
            let end = offset.saturating_add(len).min($v.len());
            let start = offset.min(end);
            $variant($v[start..end].to_vec())
        }};
    }
    match bv {
        Bool(v) => sl!(Bool, v),
        I8(v) => sl!(I8, v),
        U8(v) => sl!(U8, v),
        I16(v) => sl!(I16, v),
        U16(v) => sl!(U16, v),
        I32(v) => sl!(I32, v),
        U32(v) => sl!(U32, v),
        I64(v) => sl!(I64, v),
        U64(v) => sl!(U64, v),
        F32(v) => sl!(F32, v),
        F64(v) => sl!(F64, v),
        VecBool(v) => sl!(VecBool, v),
        VecI8(v) => sl!(VecI8, v),
        VecU8(v) => sl!(VecU8, v),
        VecI16(v) => sl!(VecI16, v),
        VecU16(v) => sl!(VecU16, v),
        VecI32(v) => sl!(VecI32, v),
        VecU32(v) => sl!(VecU32, v),
        VecI64(v) => sl!(VecI64, v),
        VecU64(v) => sl!(VecU64, v),
        VecF32(v) => sl!(VecF32, v),
        VecF64(v) => sl!(VecF64, v),
        Str(v) => sl!(Str, v),
        VecStr(v) => sl!(VecStr, v),
        Nested { offsets, items } => {
            // Slice the entry range, then slice `items` to the covered inner
            // vectors and rebase the offsets to start at 0.
            let n = offsets.len().saturating_sub(1);
            let end = offset.saturating_add(len).min(n);
            let start = offset.min(end);
            let item_start = offsets[start] as usize;
            let item_end = offsets[end] as usize;
            let base = offsets[start];
            let new_offsets = offsets[start..=end].iter().map(|&o| o - base).collect();
            BranchValues::Nested {
                offsets: new_offsets,
                items: Box::new(slice_values(*items, item_start, item_end - item_start)),
            }
        }
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

/// A scalar or array member captured while walking a class's streamer elements.
/// Integers (any width) are widened to `i64`; arrays keep their element values.
enum MemberVal {
    Int(i64),
    /// A floating member (e.g. `fWeight`). Captured for completeness so the walk
    /// stays generic; no member the tree reader consumes is a float yet.
    Float(#[allow(dead_code)] f64),
    IntArray(Vec<i64>),
    Str(String),
}

impl MemberVal {
    fn int(&self) -> i64 {
        match self {
            MemberVal::Int(v) => *v,
            _ => 0,
        }
    }
    fn ints(&self) -> &[i64] {
        match self {
            MemberVal::IntArray(v) => v,
            _ => &[],
        }
    }
    fn str(&self) -> &str {
        match self {
            MemberVal::Str(s) => s,
            _ => "",
        }
    }
}

/// Members captured from one object, keyed by streamer-element name.
type Members = std::collections::HashMap<String, MemberVal>;

fn member_int(m: &Members, name: &str) -> i64 {
    m.get(name).map_or(0, MemberVal::int)
}
fn member_str(m: &Members, name: &str) -> String {
    m.get(name).map_or(String::new(), |v| v.str().to_string())
}

/// The on-disk width (bytes) and float-ness of a basic streamer type code
/// (`fType` < 20). `None` for codes whose on-disk encoding we don't handle
/// (e.g. `Double32`/`Float16`), so the walker errors rather than misparsing.
fn basic_kind(t: i32) -> Option<(usize, bool)> {
    Some(match t {
        1 | 11 | 18 => (1, false),      // Char / UChar / Bool
        2 | 12 => (2, false),           // Short / UShort
        3 | 13 | 6 | 15 => (4, false),  // Int / UInt / Counter / Bits
        4 | 14 | 16 | 17 => (8, false), // Long / ULong / Long64 / ULong64
        5 => (4, true),                 // Float
        8 => (8, true),                 // Double
        _ => return None,
    })
}

/// Read one basic value of the given width, widening integers to `i64`.
fn read_basic(r: &mut RBuffer, width: usize, is_float: bool) -> Result<MemberVal> {
    Ok(match (width, is_float) {
        (1, false) => MemberVal::Int(i64::from(r.u8()? as i8)),
        (2, false) => MemberVal::Int(i64::from(r.be_i16()?)),
        (4, false) => MemberVal::Int(i64::from(r.be_i32()?)),
        (8, false) => MemberVal::Int(r.be_i64()?),
        (4, true) => MemberVal::Float(f64::from(r.be_f32()?)),
        (_, true) => MemberVal::Float(r.be_f64()?),
        _ => MemberVal::Int(0),
    })
}

fn unsupported_element(el: &StreamerElement) -> Error {
    Error::Format(format!(
        "streamer element {:?} has unsupported type code {} ({})",
        el.name, el.el_type, el.type_name
    ))
}

/// Walk a class's streamer `elements`, reading each member from `r`. Scalar and
/// array members are captured into `out` by name (counted pointer arrays use the
/// already-read counter named by `fCountName`); base classes are read in place
/// (recursing through their own streamer info); object members are handed to
/// `on_object`. Reading stops after the element named `stop_after` (the caller
/// then seeks to the object end), so a long trailing tail of unread members is
/// skipped via the enclosing byte count. This is the adaptive replacement for
/// fixed-offset parsing: layout comes from the file's `TStreamerInfo`, not pins.
fn walk_members(
    r: &mut RBuffer,
    reg: &StreamerRegistry,
    elements: &[StreamerElement],
    out: &mut Members,
    on_object: &mut dyn FnMut(&str, &mut RBuffer) -> Result<()>,
    stop_after: &str,
) -> Result<()> {
    for el in elements {
        let t = el.el_type;
        if el.element_class == "TStreamerBase" {
            read_base(r, reg, &el.name, out, on_object, stop_after)?;
        } else if t == 65 {
            // kTString
            out.insert(el.name.clone(), MemberVal::Str(r.string()?));
        } else if (61..=71).contains(&t) || (300..=365).contains(&t) || t == 500 || t == 501 {
            // Object / object-pointer / STL / streamer member: the caller reads
            // the ones it needs (e.g. fBranches/fLeaves) and skips the rest.
            on_object(&el.name, r)?;
        } else if (40..61).contains(&t) {
            // kOffsetP + basic: a `T* //[fCount]` variable-length array.
            let (width, is_float) = basic_kind(t - 40).ok_or_else(|| unsupported_element(el))?;
            let count = el
                .count_name
                .as_deref()
                .map_or(0, |c| member_int(out, c))
                .max(0) as usize;
            r.u8()?; // is-array marker
            let mut vals = Vec::with_capacity(count.min(r.remaining()));
            for _ in 0..count {
                vals.push(read_basic(r, width, is_float)?.int());
            }
            out.insert(el.name.clone(), MemberVal::IntArray(vals));
        } else if (20..40).contains(&t) {
            // kOffsetL + basic: a fixed `T[fArrayLength]` member (read, not kept).
            let (width, is_float) = basic_kind(t - 20).ok_or_else(|| unsupported_element(el))?;
            for _ in 0..el.array_length.max(0) {
                read_basic(r, width, is_float)?;
            }
        } else {
            let (width, is_float) = basic_kind(t).ok_or_else(|| unsupported_element(el))?;
            out.insert(el.name.clone(), read_basic(r, width, is_float)?);
        }
        if el.name == stop_after {
            break;
        }
    }
    Ok(())
}

/// Read a base-class slot named `class`. `TObject`/`TNamed` are read with their
/// dedicated readers (the latter captures `fName`/`fTitle`); any other base is
/// walked through its own streamer info when present, else skipped via its
/// version byte count.
fn read_base(
    r: &mut RBuffer,
    reg: &StreamerRegistry,
    class: &str,
    out: &mut Members,
    on_object: &mut dyn FnMut(&str, &mut RBuffer) -> Result<()>,
    stop_after: &str,
) -> Result<()> {
    match class {
        "TObject" => {
            read_tobject(r)?;
        }
        "TNamed" => {
            let named = read_tnamed(r)?;
            out.insert("fName".to_string(), MemberVal::Str(named.name));
            out.insert("fTitle".to_string(), MemberVal::Str(named.title));
        }
        _ => match reg.get(class) {
            Some(info) => {
                let vh = r.read_version()?;
                walk_members(r, reg, &info.elements, out, on_object, stop_after)?;
                if let Some(end) = vh.end {
                    r.seek(end)?;
                }
            }
            None => {
                skip_versioned(r)?;
            }
        },
    }
    Ok(())
}

/// Parse a decompressed `TTree` object (`keylen` is its key's header length),
/// driving the member layout from the file's `TStreamerInfo`.
fn read_tree(
    object: &[u8],
    keylen: usize,
    reg: &StreamerRegistry,
    class_name: &str,
) -> Result<TTree> {
    let info = reg.get("TTree").ok_or_else(|| {
        Error::Format("file has no TStreamerInfo for TTree; cannot parse the tree".to_string())
    })?;
    let mut r = RBuffer::new(object);
    let mut tags = TagReader::new(keylen);

    // A `TNtuple`/`TNtupleD` object is a `TTree` base wrapped in one extra
    // `{byte count, version}` header (plus a trailing `Int_t fNvar`). Peel that
    // outer header first; the inner `TTree` base is then read like a plain tree.
    let outer = if class_name == "TNtuple" || class_name == "TNtupleD" {
        Some(r.read_version()?)
    } else {
        None
    };
    let tree_hdr = r.read_version()?; // TTree
    let mut out = Members::new();
    let mut branches = Vec::new();
    let mut unsupported = Vec::new();
    {
        let mut on_object = |name: &str, rb: &mut RBuffer| -> Result<()> {
            // fBranches holds the tree's branches; fLeaves (not reached — we stop
            // after fBranches) and every other object member are skipped.
            if name == "fBranches" {
                branches = read_branch_array(rb, &mut tags, &mut unsupported, reg)?;
            } else {
                skip_object(rb)?;
            }
            Ok(())
        };
        walk_members(
            &mut r,
            reg,
            &info.elements,
            &mut out,
            &mut on_object,
            "fBranches",
        )?;
    }

    // Everything after fBranches is unneeded; jump to the object's end (the
    // outer subclass wrapper's end for a TNtuple, so its trailing fNvar is
    // skipped; otherwise the TTree header's end).
    if let Some(end) = outer.and_then(|o| o.end).or(tree_hdr.end) {
        r.seek(end)?;
    }

    Ok(TTree {
        name: member_str(&out, "fName"),
        entries: member_int(&out, "fEntries").max(0) as u64,
        branches,
        unsupported,
        streamer_classes: Vec::new(),
    })
}

/// Read a `TObjArray` of `TBranch`es. Branch classes we don't yet handle are
/// skipped via the object byte count.
fn read_branch_array(
    r: &mut RBuffer,
    tags: &mut TagReader,
    diag: &mut Vec<(String, String)>,
    reg: &StreamerRegistry,
) -> Result<Vec<Branch>> {
    read_version_tobject_header(r)?;
    let size = r.be_i32()?.max(0);
    let _lower = r.be_i32()?;

    let mut branches = Vec::new();
    for _ in 0..size {
        let header = tags.read_header(r)?;
        match header.class_name.as_deref() {
            Some("TBranch") => {
                branches.extend(read_branch(r, tags, diag, reg)?);
            }
            Some("TBranchElement") => {
                branches.extend(read_branch_element(r, tags, diag, reg)?);
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

/// Read a `TBranch`'s scalar members (`fName`, `fWriteBasket`, `fBasketSeek`, …)
/// by walking `reg`'s `TBranch` streamer elements, dispatching the object
/// members (`fBranches`/`fLeaves`/`fBaskets`) to the readers that consume them.
/// Shared by [`read_branch`] and (as the `TBranch` base) [`read_branch_element`].
/// Returns the captured members, the sub-branches, and the leaves.
fn read_tbranch_base(
    r: &mut RBuffer,
    tags: &mut TagReader,
    diag: &mut Vec<(String, String)>,
    reg: &StreamerRegistry,
    elements: &[StreamerElement],
    stop_after: &str,
) -> Result<(Members, Vec<Branch>, Vec<Leaf>)> {
    let mut out = Members::new();
    let mut sub = Vec::new();
    let mut leaves = Vec::new();
    {
        let mut on_object = |name: &str, rb: &mut RBuffer| -> Result<()> {
            match name {
                "fBranches" => sub = read_branch_array(rb, tags, diag, reg)?,
                "fLeaves" => leaves = read_leaf_array(rb, tags)?,
                "fBaskets" => read_skip_array(rb, tags)?,
                _ => skip_object(rb)?, // fIOFeatures, and any other object member
            }
            Ok(())
        };
        walk_members(r, reg, elements, &mut out, &mut on_object, stop_after)?;
    }
    Ok((out, sub, leaves))
}

/// Assemble `(write_basket, basket_entry, basket_seek)` from a branch's captured
/// members: `fBasketEntry` is truncated to `fWriteBasket` (the live baskets) and
/// `fBasketSeek` clamped to non-negative file offsets.
fn basket_locators(out: &Members) -> (usize, Vec<i64>, Vec<u64>) {
    let write_basket = member_int(out, "fWriteBasket").max(0) as usize;
    let basket_entry = out
        .get("fBasketEntry")
        .map(|m| m.ints().iter().copied().take(write_basket).collect())
        .unwrap_or_default();
    let basket_seek = out
        .get("fBasketSeek")
        .map(|m| m.ints().iter().map(|&s| s.max(0) as u64).collect())
        .unwrap_or_default();
    (write_basket, basket_entry, basket_seek)
}

/// Read one `TBranch` body (after its object header) by walking the file's
/// `TBranch` streamer elements. Yields one [`Branch`] for a single-leaf branch,
/// several for a leaflist branch, or none (recorded in `diag`) when the branch
/// has sub-branches or an unsupported leaf type.
fn read_branch(
    r: &mut RBuffer,
    tags: &mut TagReader,
    diag: &mut Vec<(String, String)>,
    reg: &StreamerRegistry,
) -> Result<Vec<Branch>> {
    let info = reg
        .get("TBranch")
        .ok_or_else(|| Error::Format("file has no TStreamerInfo for TBranch".to_string()))?;
    let _vh = r.read_version()?; // TBranch
    let (out, sub, leaves) = read_tbranch_base(r, tags, diag, reg, &info.elements, "")?;

    let name = member_str(&out, "fName");
    let title = member_str(&out, "fTitle");
    let (write_basket, basket_entry, basket_seek) = basket_locators(&out);

    // A branch with its own sub-branches (other than the split-element path) is
    // not handled here.
    if !sub.is_empty() {
        diag.push((
            name,
            "branch with sub-branches is not supported".to_string(),
        ));
        return Ok(Vec::new());
    }
    if leaves.is_empty() {
        diag.push((name, "no supported leaf type".to_string()));
        return Ok(Vec::new());
    }
    if leaves.len() > 1 && leaves.iter().any(|l| l.leaf_type == LeafType::Str) {
        diag.push((
            name,
            "leaflist containing a string leaf is not supported".to_string(),
        ));
        return Ok(Vec::new());
    }

    // Single-leaf branch: the branch *is* the leaf.
    if leaves.len() == 1 {
        let leaf = &leaves[0];
        return Ok(vec![Branch {
            name,
            title,
            leaf_type: leaf.leaf_type,
            leaf_len: leaf.len,
            n_baskets: write_basket,
            basket_seek,
            basket_entry,
            elem_header: 0,
            leaflist: None,
            dims: parse_dims(&leaf.title),
            nested_elem: None,
        }]);
    }

    // Leaflist branch: each entry packs the fixed-size leaves at their offsets;
    // expose each as a `branch.leaf` sub-branch sliced from the per-entry stride.
    let stride = leaves
        .iter()
        .map(|l| l.offset + l.len.max(1) as usize * l.leaf_type.size())
        .max()
        .unwrap_or(0);
    let out = leaves
        .iter()
        .map(|leaf| Branch {
            name: format!("{}.{}", name, leaf.name),
            title: title.clone(),
            leaf_type: leaf.leaf_type,
            leaf_len: leaf.len,
            n_baskets: write_basket,
            basket_seek: basket_seek.clone(),
            basket_entry: basket_entry.clone(),
            elem_header: 0,
            leaflist: Some((leaf.offset, stride)),
            dims: parse_dims(&leaf.title),
            nested_elem: None,
        })
        .collect();
    Ok(out)
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
    reg: &StreamerRegistry,
) -> Result<Vec<Branch>> {
    let info = reg
        .get("TBranchElement")
        .ok_or_else(|| Error::Format("file has no TStreamerInfo for TBranchElement".to_string()))?;
    let _vh = r.read_version()?; // TBranchElement — the object's own version
                                 // Walk the TBranchElement elements: the first is the `TBranch` base (read
                                 // in place via its own streamer info, capturing the basket locators and the
                                 // sub-branches), then fClassName/fType/fStreamerType. We stop after
                                 // fStreamerType — fMaximum/fBranchCount* are not needed and would mean
                                 // streaming object pointers.
    let (out, sub, _leaves) =
        read_tbranch_base(r, tags, diag, reg, &info.elements, "fStreamerType")?;

    let name = member_str(&out, "fName");
    let class_name = member_str(&out, "fClassName");
    let f_type = member_int(&out, "fType") as i32;
    let f_streamer_type = member_int(&out, "fStreamerType") as i32;
    let (write_basket, basket_entry, basket_seek) = basket_locators(&out);

    // Any branch with sub-branches is a split parent — an STL/clones collection
    // (`fType` 3/4), or a split single object or its sub-object member (`fType`
    // 0/2). It holds no data itself; its members do, and they were just parsed
    // into `sub`.
    if !sub.is_empty() {
        return Ok(sub);
    }

    // An unsplit `std::vector<std::vector<T>>` (`0`) reads as a doubly-nested
    // collection: the inner element type drives a [`BranchValues::Nested`].
    let nested_elem = if f_type == 0 {
        parse_nested_vector_elem(&class_name)
    } else {
        None
    };

    // Pick the element type and per-entry header for a data-bearing leaf branch:
    // - a member sub-branch (STL `41`, TClonesArray `31`) — a jagged array typed
    //   by `fStreamerType`, no header;
    // - an unsplit `std::vector<T>` (`0`, class `vector<...>`) — typed by the
    //   class, each entry prefixed by the 10-byte streamer header;
    // - a scalar member of a split single object (`0`, a plain class) — one value
    //   per entry, typed by `fStreamerType`, no header.
    let member = f_type == 41 || f_type == 31;
    let (leaf_type, elem_header) = if let Some(elem) = nested_elem {
        (Some(elem), 10) // the 10-byte header carries the outer count
    } else if member {
        (streamer_type_to_leaf(f_streamer_type), 0)
    } else if let Some(elem) = parse_vector_elem(&class_name) {
        (Some(elem), 10)
    } else if f_type == 0 {
        (streamer_type_to_leaf(f_streamer_type), 0)
    } else {
        (None, 0)
    };
    let Some(leaf_type) = leaf_type else {
        diag.push((
            name,
            format!("unsupported TBranchElement (fType={f_type}, class {class_name:?})"),
        ));
        return Ok(Vec::new());
    };
    Ok(vec![Branch {
        name,
        title: member_str(&out, "fTitle"),
        leaf_type,
        leaf_len: 1,
        n_baskets: write_basket,
        basket_seek,
        basket_entry,
        elem_header,
        leaflist: None,
        dims: Vec::new(),
        nested_elem,
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

/// Strip a `vector<...>` / `std::vector<...>` wrapper, returning the (trimmed)
/// element type spelling.
fn strip_vector(class_name: &str) -> Option<&str> {
    Some(
        class_name
            .strip_prefix("vector<")
            .or_else(|| class_name.strip_prefix("std::vector<"))?
            .strip_suffix('>')?
            .trim(),
    )
}

/// Map a C++ fundamental type name to its [`LeafType`], or `None` if unsupported.
fn basic_leaf_type(name: &str) -> Option<LeafType> {
    Some(match name {
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
        // std::vector<std::string>: the element is a string (decoded specially).
        "string" | "std::string" => LeafType::Str,
        _ => return None,
    })
}

/// Strip a single-element STL container wrapper — `vector` / `set` / `multiset`
/// (bare or `std::`) — returning the trimmed element type. `std::set` and
/// `std::vector` share an object-wise on-disk layout (a 10-byte streamer header
/// then the contiguous elements), so the reader treats them the same.
fn strip_collection(class_name: &str) -> Option<&str> {
    for prefix in [
        "vector<",
        "std::vector<",
        "set<",
        "std::set<",
        "multiset<",
        "std::multiset<",
    ] {
        if let Some(inner) = class_name.strip_prefix(prefix) {
            return inner.strip_suffix('>').map(str::trim);
        }
    }
    None
}

/// Map an unsplit single-element STL container (`std::vector<T>`/`std::set<T>`/…)
/// class name to its element [`LeafType`], or `None` for an unsupported element.
fn parse_vector_elem(class_name: &str) -> Option<LeafType> {
    basic_leaf_type(strip_collection(class_name)?)
}

/// Map a `std::vector<std::vector<T>>` class name to `T`'s element [`LeafType`],
/// or `None` if it is not a doubly-nested vector of a supported basic type.
/// `std::vector<std::vector<std::string>>` is excluded (the inner string
/// decoding does not compose with the nested reader).
fn parse_nested_vector_elem(class_name: &str) -> Option<LeafType> {
    let inner = strip_vector(class_name)?; // e.g. "vector<int>"
    let elem = basic_leaf_type(strip_vector(inner)?)?;
    if elem == LeafType::Str {
        return None;
    }
    Some(elem)
}

/// Read a `TObjArray` of `TLeaf`s, returning `(type, fLen)` for each supported
/// leaf (unsupported leaves are skipped).
fn read_leaf_array(r: &mut RBuffer, tags: &mut TagReader) -> Result<Vec<Leaf>> {
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

/// Read one `TLeaf*` (v1) body enough to recover its name, element type, `fLen`,
/// and `fOffset` (its byte position within an entry, for leaflist branches).
fn read_leaf(r: &mut RBuffer, class: &str) -> Result<Option<Leaf>> {
    r.read_version()?; // TLeafX (v1) — the leaf subclass wrapper
    r.read_version()?; // TLeaf base (v2)
    let named = read_tnamed(r)?; // fName, fTitle
    let len = r.be_i32()?; // fLen
    r.be_i32()?; // fLenType
    let offset = r.be_i32()?; // fOffset
    r.u8()?; // fIsRange
    let unsigned = r.u8()? != 0; // fIsUnsigned
                                 // fLeafCount, fMinimum, fMaximum follow; we skip to the leaf's end via the
                                 // caller's byte count.
    Ok(LeafType::from_leaf(class, unsigned).map(|leaf_type| Leaf {
        name: named.name,
        title: named.title,
        leaf_type,
        len,
        offset: offset.max(0) as usize,
    }))
}

/// Parse the per-entry array shape from a leaf title: `x[2][3]` → `[2, 3]`,
/// `x[5]` → `[5]`, a scalar (no brackets) → `[]`.
fn parse_dims(title: &str) -> Vec<usize> {
    let mut dims = Vec::new();
    let mut rest = title;
    while let Some(open) = rest.find('[') {
        let after = &rest[open + 1..];
        let Some(close) = after.find(']') else { break };
        if let Ok(n) = after[..close].parse::<usize>() {
            dims.push(n);
        }
        rest = &after[close + 1..];
    }
    dims
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

/// Decode `std::vector<std::vector<T>>` entries into a [`BranchValues::Nested`].
/// Each region is the outer vector's 10-byte streamer header (byte count +
/// version + outer count) followed by that many inner vectors, each a `u32`
/// element count then the contiguous big-endian elements. The flattened inner
/// vectors become `items` (a `Vec*` of element type `elem`), partitioned per
/// entry by the cumulative `offsets`.
fn decode_nested_vec(regions: &[&[u8]], elem: LeafType) -> Result<BranchValues> {
    let size = elem.size();
    let mut offsets = Vec::with_capacity(regions.len() + 1);
    offsets.push(0u64);
    let mut total = 0u64;
    let mut inner: Vec<&[u8]> = Vec::new();
    for region in regions {
        if region.len() >= 10 {
            // The outer count is the last 4 bytes of the 10-byte header.
            let outer = u32::from_be_bytes(region[6..10].try_into().unwrap()) as usize;
            let mut pos = 10;
            for _ in 0..outer {
                let count = u32::from_be_bytes(
                    region
                        .get(pos..pos + 4)
                        .ok_or_else(short_entry)?
                        .try_into()
                        .unwrap(),
                ) as usize;
                pos += 4;
                let nbytes = count * size;
                inner.push(region.get(pos..pos + nbytes).ok_or_else(short_entry)?);
                pos += nbytes;
            }
            total += outer as u64;
        }
        offsets.push(total);
    }
    Ok(BranchValues::Nested {
        offsets,
        items: Box::new(decode_array(elem, &inner)?),
    })
}

/// Error for a `std::vector<std::vector<T>>` entry that ends mid-element.
fn short_entry() -> Error {
    Error::Format("nested vector entry truncated".into())
}

/// Decode `std::vector<std::string>` entries: each region is a 10-byte streamer
/// header (byte count + version + `u32` element count) followed by that many
/// ROOT-encoded strings.
fn decode_vec_strings(regions: &[&[u8]]) -> Result<BranchValues> {
    let mut out = Vec::with_capacity(regions.len());
    for r in regions {
        let mut buf = RBuffer::new(r);
        let mut row = Vec::new();
        if buf.remaining() >= 10 {
            buf.be_u32()?; // byte count
            buf.be_u16()?; // version
            let count = buf.be_u32()? as usize;
            row.reserve(count);
            for _ in 0..count {
                row.push(buf.string()?);
            }
        }
        out.push(row);
    }
    Ok(BranchValues::VecStr(out))
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

#[cfg(test)]
mod tests {
    use super::{member_int, walk_members, MemberVal, Members};
    use oxiroot_io_core::buffer::RBuffer;
    use oxiroot_io_core::error::Result;
    use oxiroot_io_core::streamer_info::{StreamerElement, StreamerRegistry};

    fn elem(class: &str, name: &str, el_type: i32, count: Option<&str>) -> StreamerElement {
        StreamerElement {
            element_class: class.to_string(),
            name: name.to_string(),
            title: String::new(),
            el_type,
            size: 0,
            array_length: 0,
            type_name: String::new(),
            base_version: None,
            count_name: count.map(str::to_string),
        }
    }

    /// The walker reads members by the streamer element list, so it adapts to a
    /// member order this reader was never compiled against, picks up an extra
    /// member ROOT might add in a future version, and sizes a `//[fCount]` array
    /// from the named counter — none of which a fixed-offset reader could do.
    #[test]
    fn walker_reads_by_element_list_not_fixed_offsets() {
        // Layout: a:int, b:Long64, c:int (a hypothetical *new* member), n:counter,
        // arr:Long64*[n]. A pinned reader keyed to "a,b" would misread c and arr.
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&7i32.to_be_bytes()); // a
        bytes.extend_from_slice(&100i64.to_be_bytes()); // b
        bytes.extend_from_slice(&42i32.to_be_bytes()); // c (the evolved member)
        bytes.extend_from_slice(&2i32.to_be_bytes()); // n = 2
        bytes.push(1); // is-array marker
        bytes.extend_from_slice(&555i64.to_be_bytes()); // arr[0]
        bytes.extend_from_slice(&666i64.to_be_bytes()); // arr[1]

        let elements = vec![
            elem("TStreamerBasicType", "a", 3, None),            // kInt
            elem("TStreamerBasicType", "b", 16, None),           // kLong64
            elem("TStreamerBasicType", "c", 3, None),            // kInt
            elem("TStreamerBasicType", "n", 6, None),            // kCounter
            elem("TStreamerBasicPointer", "arr", 56, Some("n")), // kOffsetP + kLong64
        ];

        let reg = StreamerRegistry::default();
        let mut out = Members::new();
        let mut r = RBuffer::new(&bytes);
        let mut on_object = |_: &str, _: &mut RBuffer| -> Result<()> { Ok(()) };
        walk_members(&mut r, &reg, &elements, &mut out, &mut on_object, "").unwrap();

        assert_eq!(member_int(&out, "a"), 7);
        assert_eq!(member_int(&out, "b"), 100);
        assert_eq!(member_int(&out, "c"), 42); // read purely by its element name
        assert_eq!(member_int(&out, "n"), 2);
        match out.get("arr") {
            Some(MemberVal::IntArray(v)) => assert_eq!(v, &[555, 666]),
            other => panic!("arr not a counted array: {:?}", other.map(|_| ())),
        }
        assert_eq!(r.remaining(), 0, "the whole record was consumed");
    }

    /// `stop_after` ends the walk early (the caller then seeks past the rest via
    /// the object byte count), the way the tree reader stops after `fBranches`.
    #[test]
    fn walker_stops_after_named_member() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&1i32.to_be_bytes()); // a
        bytes.extend_from_slice(&2i32.to_be_bytes()); // b
        bytes.extend_from_slice(&3i32.to_be_bytes()); // c (must remain unread)
        let elements = vec![
            elem("TStreamerBasicType", "a", 3, None),
            elem("TStreamerBasicType", "b", 3, None),
            elem("TStreamerBasicType", "c", 3, None),
        ];
        let reg = StreamerRegistry::default();
        let mut out = Members::new();
        let mut r = RBuffer::new(&bytes);
        let mut on_object = |_: &str, _: &mut RBuffer| -> Result<()> { Ok(()) };
        walk_members(&mut r, &reg, &elements, &mut out, &mut on_object, "b").unwrap();
        assert_eq!(member_int(&out, "a"), 1);
        assert_eq!(member_int(&out, "b"), 2);
        assert!(!out.contains_key("c"), "stopped before reading c");
        assert_eq!(
            r.remaining(),
            4,
            "c's 4 bytes are left for the caller to skip"
        );
    }
}
