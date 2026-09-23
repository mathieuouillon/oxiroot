//! Reading a `TTree` and its branches.
//!
//! `TTree`/`TBranch`/`TBranchElement` are parsed by walking the member list in
//! the file's own `TStreamerInfo` (see [`walk_members`](members::walk_members)) rather than at fixed
//! offsets, so the reader follows whatever schema the file declares; an unknown
//! member type is reported instead of parsed at a guessed offset. (`TLeaf*` are
//! still read by their compact, byte-count-bounded layout.) The branch data
//! itself lives in [`crate::basket`]s. Handles single-leaf branches:
//! scalars, fixed (`x[N]`) and variable (`x[n]`) arrays, and `TLeafC` strings,
//! unsplit `std::vector<T>` `TBranchElement` branches (the element type comes
//! from `fClassName`, and each entry carries a 10-byte streamer header), and
//! *split* (`fSplitLevel > 0`) `std::vector<MyStruct>` branches, which are
//! exposed as their per-member jagged sub-branches (`hits.x`, `hits.y`, …).

use oxiroot_io_core::{
    decompress_payload, find_key, Error, FileReader, Result, StreamerElement, TKey,
};

use crate::value::{BranchValues, Jagged, LeafType};

mod decode;
mod members;
mod parse;
mod types;

use decode::{decode_baskets, decode_scalar, entry_regions, read_baskets, slice_values, Decode};
use parse::read_tree;

/// Reads a `TTree` from a [`FileReader`]. Opening parses the tree's name, entry
/// count, and branches; branch data is read on demand, one branch at a time.
#[doc(alias = "TTree", alias = "TTreeReader")]
#[derive(Debug, Clone)]
pub struct TreeReader {
    name: String,
    entries: u64,
    branches: Vec<Branch>,
    /// Branches present in the file that this crate cannot (yet) read, as
    /// `(name, reason)` — surfaced via [`TreeReader::unsupported_branches`].
    unsupported: Vec<(String, String)>,
    /// The classes (and versions) declared in the file's `TStreamerInfo` — the
    /// schema this tree was written against; surfaced via
    /// [`TreeReader::streamer_classes`]. Empty if the file has no streamer info.
    streamer_classes: Vec<(String, i32)>,
    /// Friend trees recorded by `TTree::AddFriend` (read from `fFriends`);
    /// surfaced via [`TreeReader::friends`].
    friends: Vec<Friend>,
    /// `(alias, expression)` pairs set with `TTree::SetAlias` (read from
    /// `fAliases`); surfaced via [`TreeReader::aliases`] / [`TreeReader::alias`].
    aliases: Vec<(String, String)>,
}

/// A friend tree attached to a `TTree` via `TTree::AddFriend`, persisted in the
/// main tree's `fFriends` list. Friends are read **positionally**: entry *i* of
/// the main tree pairs with entry *i* of the friend (the standard way HEP
/// analyses join per-event datasets without copying columns).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Friend {
    /// The friend tree's key name (`fTreeName`) — what to open in its file.
    tree_name: String,
    /// The file holding the friend, or empty when it lives in the same file as
    /// the main tree (`AddFriend` with no explicit file name).
    file_name: String,
    /// The alias the friend is referred to by (`TFriendElement`'s name);
    /// defaults to the tree name.
    alias: String,
}

impl Friend {
    /// The friend tree's key name (`fTreeName`).
    pub fn tree_name(&self) -> &str {
        &self.tree_name
    }

    /// The file holding the friend, or `""` when it is in the same file as the
    /// main tree.
    pub fn file_name(&self) -> &str {
        &self.file_name
    }

    /// The alias the friend is referred to by.
    pub fn alias(&self) -> &str {
        &self.alias
    }

    /// Whether the friend lives in the same file as the main tree (`AddFriend`
    /// called without an explicit file name stores an empty file name).
    pub fn is_same_file(&self) -> bool {
        self.file_name.is_empty()
    }
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
    /// On-disk byte size of each basket (`fBasketBytes`), so a basket is fetched
    /// at its exact size — never over-fetched — over a ranged/remote source.
    /// Empty when the file omits it; then the reader probes the key header.
    basket_bytes: Vec<u64>,
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
    /// Set for a synthesized member column of an old unsplit `TBranchObject`: the
    /// object class to decode out of each entry, and which member this column is.
    /// The branch data is one whole object per entry (`[className][version][members]`),
    /// from which this member is extracted at read time.
    object_member: Option<ObjectMember>,
}

/// One member column synthesized from an old unsplit `TBranchObject` (a whole
/// object stored per entry behind a `TLeafObject`). Carries the object class's
/// streamer layout so the per-entry object can be decoded without the registry.
#[derive(Debug, Clone)]
struct ObjectMember {
    /// The member this column extracts (e.g. `fName`).
    member: String,
    /// The object class's streamer elements, in order (so the per-entry object
    /// can be walked up to `member`).
    class_elements: Vec<StreamerElement>,
}

/// The write-reconstruction metadata of a read branch — the private-field
/// summary [`concat_trees`](crate::concat_trees) needs to rebuild a branch as a
/// writable [`Branch`](crate::Branch) with the correct kind (scalar / fixed
/// array / jagged / `std::vector` / string), or to reject the kinds this crate
/// cannot write back.
pub(crate) struct BranchMetaLite {
    /// Number of fixed-array dimensions parsed from the title (`x[N]` → 1,
    /// `x[N][M]` → 2, scalar / jagged / `std::vector` → 0).
    pub dims_len: usize,
    /// Per-entry streamer-header size: `>0` marks a `std::vector<T>`
    /// `TBranchElement` (distinguishing it from a jagged `x[n]` leaf).
    pub elem_header: usize,
    /// Whether this is one leaf of a multi-leaf (leaflist) branch.
    pub has_leaflist: bool,
    /// Whether this is a synthesized member column of an unsplit object branch.
    pub has_object_member: bool,
    /// Whether this is a `std::vector<std::vector<T>>` branch.
    pub has_nested: bool,
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

impl TreeReader {
    /// Open the `TTree` named `name` in `file`.
    pub fn open(file: &FileReader, name: &str) -> Result<TreeReader> {
        let key = file.key(name).ok_or_else(|| Error::NotFound {
            what: "key",
            name: name.to_string(),
        })?;
        Self::open_from_key(file, key)
    }

    /// Open the `TTree` named `name` from the subdirectory `subdir` (a
    /// `/`-separated path descends through nested `TDirectory`s).
    pub fn open_in(file: &FileReader, subdir: &str, name: &str) -> Result<TreeReader> {
        let dir = file.subdir(subdir)?;
        // The cycle `name` asks for, or the highest — `FileReader::key`'s rule.
        let key = find_key(&dir.keys, name).ok_or_else(|| Error::NotFound {
            what: "key",
            name: format!("{}/{name}", subdir.trim_end_matches('/')),
        })?;
        // Detach from the borrowed `dir` so the returned tree owns nothing tied to
        // it; the key's seek offsets are absolute, so decoding is identical.
        let key = key.clone();
        Self::open_from_key(file, &key)
    }

    /// Decode a `TTree` (or `TNtuple`/`TNtupleD`) from an already-located key.
    fn open_from_key(file: &FileReader, key: &TKey) -> Result<TreeReader> {
        // `TNtuple` / `TNtupleD` are `TTree` subclasses (a `TTree` base wrapped in
        // one extra header plus a trailing `Int_t fNvar`); read them as trees too.
        if !matches!(key.class_name.as_str(), "TTree" | "TNtuple" | "TNtupleD") {
            return Err(Error::WrongClass {
                name: key.name.clone(),
                found: key.class_name.clone(),
                expected: "TTree".to_string(),
            });
        }
        // The file's TStreamerInfo is the authoritative schema: the reader walks
        // each class's declared member list rather than assuming a fixed layout,
        // so it adapts to the version the file was written with.
        let registry = file.streamer_registry()?;

        let payload = file.key_payload(key)?;
        let object = decompress_payload(&payload, key.obj_len as usize, "TTree")?;
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

    /// The friend trees attached with `TTree::AddFriend` (persisted in the main
    /// tree's `fFriends`). A friend is read positionally — entry *i* of this tree
    /// pairs with entry *i* of the friend. Open a friend with
    /// [`TreeReader::open`]`(file, friend.tree_name())` (the same `file` when
    /// [`Friend::is_same_file`]) and read its branches as usual; the columns line
    /// up by entry.
    pub fn friends(&self) -> &[Friend] {
        &self.friends
    }

    /// The `(alias, expression)` pairs defined with `TTree::SetAlias` (read from
    /// `fAliases`). An alias is a shorthand name standing for a branch
    /// expression (e.g. `"pt"` → `"sqrt(px*px+py*py)"`); oxiroot reads the pairs
    /// but does not evaluate the expressions.
    pub fn aliases(&self) -> &[(String, String)] {
        &self.aliases
    }

    /// The expression that alias `name` stands for, or `None` if there is no
    /// such alias.
    pub fn alias(&self, name: &str) -> Option<&str> {
        self.aliases
            .iter()
            .find(|(a, _)| a == name)
            .map(|(_, expr)| expr.as_str())
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

    /// The write-reconstruction metadata of branch `name` (for [`concat_trees`]),
    /// or `None` if there is no such branch.
    ///
    /// [`concat_trees`]: crate::concat_trees
    pub(crate) fn branch_meta(&self, name: &str) -> Option<BranchMetaLite> {
        let b = self.branch(name)?;
        Some(BranchMetaLite {
            dims_len: b.dims.len(),
            elem_header: b.elem_header,
            has_leaflist: b.leaflist.is_some(),
            has_object_member: b.object_member.is_some(),
            has_nested: b.nested_elem.is_some(),
        })
    }

    /// Read all values of branch `name` across every basket.
    ///
    /// Scalar branches yield a flat [`BranchValues`]; fixed (`x[N]`) and
    /// variable (`x[n]`) branches yield a nested one; `TLeafC` yields strings.
    ///
    /// Baskets are decompressed in order on the calling thread; see
    /// [`read_branch_par`](Self::read_branch_par) for the parallel variant.
    pub fn read_branch(&self, file: &FileReader, name: &str) -> Result<BranchValues> {
        self.read_branch_with(file, name, Decode::Serial)
    }

    /// [`read_branch`](Self::read_branch), decompressing the baskets in parallel
    /// on rayon's global thread pool. Requires the `rayon` feature.
    #[cfg(feature = "rayon")]
    pub fn read_branch_par(&self, file: &FileReader, name: &str) -> Result<BranchValues> {
        self.read_branch_with(file, name, Decode::Parallel)
    }

    fn read_branch_with(
        &self,
        file: &FileReader,
        name: &str,
        decode: Decode,
    ) -> Result<BranchValues> {
        let branch = self.branch(name).ok_or_else(|| Error::NotFound {
            what: "branch",
            name: name.to_string(),
        })?;
        let baskets = read_baskets(file, branch, 0..branch.n_baskets, decode)?;
        decode_baskets(branch, &baskets)
    }

    /// Read only entries `[start, stop)` of branch `name`, fetching just the
    /// baskets that cover the range rather than the whole branch. `stop` is
    /// clamped to the entry count and `start` to `stop`, so an out-of-range
    /// window yields fewer (or no) entries instead of an error.
    ///
    /// Baskets are decompressed in order on the calling thread; see
    /// [`read_branch_range_par`](Self::read_branch_range_par) for the parallel
    /// variant.
    pub fn read_branch_range(
        &self,
        file: &FileReader,
        name: &str,
        start: u64,
        stop: u64,
    ) -> Result<BranchValues> {
        self.read_branch_range_with(file, name, start, stop, Decode::Serial)
    }

    /// [`read_branch_range`](Self::read_branch_range), decompressing the baskets
    /// in parallel on rayon's global thread pool. Requires the `rayon` feature.
    #[cfg(feature = "rayon")]
    pub fn read_branch_range_par(
        &self,
        file: &FileReader,
        name: &str,
        start: u64,
        stop: u64,
    ) -> Result<BranchValues> {
        self.read_branch_range_with(file, name, start, stop, Decode::Parallel)
    }

    fn read_branch_range_with(
        &self,
        file: &FileReader,
        name: &str,
        start: u64,
        stop: u64,
        decode: Decode,
    ) -> Result<BranchValues> {
        let branch = self.branch(name).ok_or_else(|| Error::NotFound {
            what: "branch",
            name: name.to_string(),
        })?;
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

        let baskets = read_baskets(file, branch, indices.iter().copied(), decode)?;
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
    ///
    /// Baskets are decompressed in order on the calling thread; see
    /// [`read_branch_flat_par`](Self::read_branch_flat_par) for the parallel
    /// variant.
    pub fn read_branch_flat(&self, file: &FileReader, name: &str) -> Result<Jagged> {
        self.read_branch_flat_with(file, name, Decode::Serial)
    }

    /// [`read_branch_flat`](Self::read_branch_flat), decompressing the baskets in
    /// parallel on rayon's global thread pool. Requires the `rayon` feature.
    #[cfg(feature = "rayon")]
    pub fn read_branch_flat_par(&self, file: &FileReader, name: &str) -> Result<Jagged> {
        self.read_branch_flat_with(file, name, Decode::Parallel)
    }

    fn read_branch_flat_with(
        &self,
        file: &FileReader,
        name: &str,
        decode: Decode,
    ) -> Result<Jagged> {
        let branch = self.branch(name).ok_or_else(|| Error::NotFound {
            what: "branch",
            name: name.to_string(),
        })?;
        if branch.leaf_type == LeafType::Str {
            return Err(Error::InvalidInput(format!(
                "branch {name:?} is a string branch; use read_branch"
            )));
        }
        if branch.object_member.is_some() {
            return Err(Error::InvalidInput(format!(
                "branch {name:?} is a TBranchObject member; use read_branch"
            )));
        }
        let baskets = read_baskets(file, branch, 0..branch.n_baskets, decode)?;
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
