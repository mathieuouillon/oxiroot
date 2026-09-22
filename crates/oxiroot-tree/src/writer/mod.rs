//! Writing a `TTree` into a ROOT file.
//!
//! Supports scalar, fixed-size array (`x[N]`), variable-length / jagged
//! (`x[n<name>]`, with an auto-generated count branch and an `fLeafCount`
//! reference), string (`TLeafC`), and `std::vector<T>` (`TBranchElement`)
//! branches. Mirrors the layout ROOT/uproot write (TTree v20, TBranch v13,
//! TLeaf* v1, TBranchElement v10) so the result reads back in ROOT, uproot, and
//! this crate. The embedded `TStreamerInfo` ([`crate::streamer_gen`]) makes the
//! file self-describing.

use std::collections::HashMap;
use std::io::{Cursor, Seek, Write};
use std::path::Path;

use oxiroot_io_core::streamer_gen::Cls;
use oxiroot_io_core::{
    Compression, ContainerWriter, DirId, Error, Result, WriteInto, KSTART_BIG_FILE,
};

use crate::value::BranchValues;

mod baskets;
mod branch;
mod layout;
mod serialize;
mod streaming;

use baskets::{write_branch_baskets, BasketRec};
use branch::{check_split_members, split_class};
pub use branch::{Branch, SplitMember};
use serialize::build_tree_object;
pub use streaming::TreeWriter;

/// `fBits` ROOT writes for embedded `TObject`s.
const OBJ_BITS: u32 = 0x0300_0000;
/// ROOT's object-map displacement (`kMapOffset`): a referenced object is keyed
/// at `byte_count_position + keylen + 2`. Used to point a jagged leaf's
/// `fLeafCount` at the already-written count leaf.
const K_MAP_OFFSET: u32 = 2;

/// Maps a leaf name to the object-reference value (`pos + keylen + kMapOffset`)
/// of the first place that leaf was written, so a later `fLeafCount` can point
/// back to it the way ROOT does.
type LeafRefs = HashMap<String, u32>;

/// A write [`Branch`] is only ever built through the typed constructors, none of
/// which produce a [`BranchValues::Nested`] (a read-only doubly-nested
/// collection), so the writer's value matches never see one.
const NESTED_NOT_WRITABLE: &str = "a write Branch is never built with a Nested value";

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

/// Build the bytes of a single-tree ROOT file (one basket per branch).
///
/// Returns an error if a fixed-array branch ([`Branch::vec_f64`] …) was given
/// rows of differing length — use [`Branch::jagged_f64`] … for that.
pub fn tree_file_bytes(
    file_name: &str,
    tree_name: &str,
    branches: &[Branch],
    compression: Compression,
) -> Result<Vec<u8>> {
    tree_bytes(
        file_name,
        tree_name,
        branches,
        compression,
        0,
        KSTART_BIG_FILE,
    )
}

/// Write a single-tree ROOT file, splitting each branch into baskets of at most
/// `entries_per_basket` entries (`0` = one basket per branch). Multiple baskets
/// let a large tree be stored the way ROOT writes it; split `std::vector<Struct>`
/// branches are always one basket (their per-member alignment is not chunked).
pub fn write_tree_file_baskets(
    path: impl AsRef<Path>,
    tree_name: &str,
    branches: &[Branch],
    compression: Compression,
    entries_per_basket: usize,
) -> Result<()> {
    let path = path.as_ref();
    let file_name = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("file.root");
    std::fs::write(
        path,
        tree_bytes(
            file_name,
            tree_name,
            branches,
            compression,
            entries_per_basket,
            KSTART_BIG_FILE,
        )?,
    )?;
    Ok(())
}

/// A tree to write: a name and its [`Branch`]es. The method-based,
/// write-side counterpart to the free [`write_tree_file`] function (and to the
/// read-only [`TreeReader`](crate::TreeReader)) — build one, then call
/// [`write_root`](Tree::write_root), mirroring `hist.write_root`:
///
/// ```no_run
/// use oxiroot_tree::{Branch, Tree};
/// use oxiroot_io_core::Compression;
///
/// let branches = vec![
///     Branch::i32("event", vec![1, 2, 3]),
///     Branch::f64("energy", vec![10.5, 20.1, 5.0]),
/// ];
/// Tree::new("Events", branches).write_root("tree.root", Compression::None)?;
/// # Ok::<(), oxiroot_io_core::Error>(())
/// ```
pub struct Tree {
    name: String,
    branches: Vec<Branch>,
}

impl Tree {
    /// Create a writable tree from a name and its branches.
    pub fn new(name: impl Into<String>, branches: Vec<Branch>) -> Tree {
        Tree {
            name: name.into(),
            branches,
        }
    }

    /// The tree's name (the in-file `TTree` key).
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The tree's branches.
    pub fn branches(&self) -> &[Branch] {
        &self.branches
    }

    /// Write this tree as a new single-tree ROOT file (one basket per branch),
    /// readable by ROOT and uproot. The method form of [`write_tree_file`].
    pub fn write_root(&self, path: impl AsRef<Path>, compression: Compression) -> Result<()> {
        write_tree_file(path, &self.name, &self.branches, compression)
    }

    /// Like [`write_root`](Tree::write_root) but split each branch into baskets
    /// of at most `entries_per_basket` entries (`0` = one basket per branch).
    /// The method form of [`write_tree_file_baskets`].
    pub fn write_root_baskets(
        &self,
        path: impl AsRef<Path>,
        compression: Compression,
        entries_per_basket: usize,
    ) -> Result<()> {
        write_tree_file_baskets(
            path,
            &self.name,
            &self.branches,
            compression,
            entries_per_basket,
        )
    }

    /// The complete ROOT-file bytes for this tree (the method form of
    /// [`tree_file_bytes`]); `file_name` is the `TFile` name recorded in the
    /// file header.
    pub fn to_root_bytes(&self, file_name: &str, compression: Compression) -> Result<Vec<u8>> {
        tree_file_bytes(file_name, &self.name, &self.branches, compression)
    }
}

/// A `Tree` goes into a [`FileWriter`](oxiroot_io_core::FileWriter) with
/// [`put`](oxiroot_io_core::FileWriter::put), next to histograms or RNTuples, with
/// one basket per branch:
///
/// ```no_run
/// use oxiroot_io_core::{Compression, FileWriter, TParameter};
/// use oxiroot_tree::{Branch, Tree};
///
/// FileWriter::create("run.root")
///     .add(&TParameter::f64("lumi", 12.5))
///     .put(Tree::new("Events", vec![Branch::f64("energy", vec![10.5, 20.1])]))
///     .dir("cal", |d| d.put(Tree::new("Pedestals", vec![Branch::i32("adc", vec![3, 4])])))
///     .write(Compression::Zstd(5))?;
/// # Ok::<(), oxiroot_io_core::Error>(())
/// ```
impl WriteInto for Tree {
    fn root_class(&self) -> String {
        "TTree".to_string()
    }
    fn root_name(&self) -> &str {
        &self.name
    }
    fn write_into(&self, file: &mut ContainerWriter<Cursor<Vec<u8>>>, dir: DirId) -> Result<()> {
        check_branches(&self.branches)?;
        write_tree_records(file, dir, &self.name, &self.branches, 0)
    }
    fn streamer_classes(&self) -> Vec<Cls<'static>> {
        tree_streamer_classes(&self.branches)
            .into_iter()
            .map(Cls::into_owned)
            .collect()
    }
}

/// A representative `TLeafC` string value whose length yields `fLen` = `len`
/// (longest string + 1), used for a streamed string column's leaf metadata.
fn str_rep(len: i32) -> BranchValues {
    let n = (len - 1).max(0) as usize;
    BranchValues::Str(vec!["\0".repeat(n)])
}

/// Shared body of [`tree_file_bytes`] / [`write_tree_file_baskets`]:
/// `entries_per_basket` of `0` means one basket per branch. The file switches to
/// ROOT's 64-bit container form once it would exceed `big_threshold` bytes.
fn tree_bytes(
    file_name: &str,
    tree_name: &str,
    branches: &[Branch],
    compression: Compression,
    entries_per_basket: usize,
    big_threshold: u64,
) -> Result<Vec<u8>> {
    check_branches(branches)?;
    let classes = tree_streamer_classes(branches);
    ContainerWriter::build(file_name, compression, big_threshold, |file| {
        write_tree_records(file, DirId::TOP, tree_name, branches, entries_per_basket)?;
        file.place_streamer_info(&[], &classes)
    })
}

/// Reject branches whose entry counts differ from the first branch's: the tree
/// has one entry count, so the others would be cut or read past.
fn check_entry_counts(branches: &[Branch]) -> Result<()> {
    let expected = branches.first().map_or(0, Branch::n_entries);
    match branches.iter().find(|b| b.n_entries() != expected) {
        Some(b) => Err(Error::LengthMismatch {
            what: format!("branch {:?} entries", b.name),
            expected: expected as usize,
            found: b.n_entries() as usize,
        }),
        None => Ok(()),
    }
}

/// Reject branches the writer cannot lay out: branches with different entry
/// counts, a fixed-array branch with rows of differing length, and a split
/// branch whose members disagree.
fn check_branches(branches: &[Branch]) -> Result<()> {
    check_entry_counts(branches)?;
    for b in branches {
        if !b.jagged() && !b.stl_vector() && b.is_jagged() {
            return Err(Error::Format(format!(
                "branch {:?}: rows differ in length; use Branch::jagged_* or Branch::vector_* for \
                 variable-length arrays (Branch::vec_* requires every row to have the same length)",
                b.name
            )));
        }
        if let Some(spec) = b.split() {
            check_split_members(&b.name, spec)?;
        }
    }
    Ok(())
}

/// The `TStreamerInfo` entries a tree with these branches needs: the canonical
/// TTree hierarchy (including the TBranchElement/TLeafElement `std::vector`
/// streamers), then each split branch's struct.
fn tree_streamer_classes(branches: &[Branch]) -> Vec<Cls<'_>> {
    let mut classes = crate::streamer_gen::tree_classes();
    classes.extend(branches.iter().filter_map(Branch::split).map(split_class));
    classes
}

/// Write a tree's baskets at the end of `file`, then the `TTree` that lists
/// them under a key in `dir`. A leaf branch has one basket per chunk of
/// `entries_per_basket` entries (`0` = one basket); a split branch has a count
/// basket plus one per member sub-branch.
fn write_tree_records<W: Write + Seek>(
    file: &mut ContainerWriter<W>,
    dir: DirId,
    tree_name: &str,
    branches: &[Branch],
    entries_per_basket: usize,
) -> Result<()> {
    // Expand each jagged branch into [count branch, jagged branch], matching
    // ROOT/uproot. `counts` owns the synthetic count branches so the effective
    // list `eff` can borrow them alongside the caller's branches.
    let counts: Vec<Branch> = branches.iter().filter_map(Branch::count_branch).collect();
    let mut eff: Vec<&Branch> = Vec::with_capacity(branches.len() + counts.len());
    let mut ci = 0;
    for b in branches {
        if b.jagged() {
            eff.push(&counts[ci]);
            ci += 1;
        }
        eff.push(b);
    }
    let n_entries = eff.first().map(|b| b.n_entries()).unwrap_or(0);

    let basket_groups: Vec<Vec<BasketRec>> = eff
        .iter()
        .map(|&b| write_branch_baskets(file, dir, b, tree_name, entries_per_basket))
        .collect::<Result<_>>()?;
    let tot_bytes: i64 = basket_groups
        .iter()
        .flatten()
        .map(|r| i64::from(r.nbytes))
        .sum();
    let tree_obj = build_tree_object(
        tree_name,
        &eff,
        &basket_groups,
        i64::from(n_entries),
        tot_bytes,
        file.is_big(),
    );
    file.place_key(dir, "TTree", tree_name, "", &tree_obj)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::TreeReader;
    use oxiroot_io_core::FileReader;
    use std::io::Cursor;

    fn branches() -> Vec<Branch> {
        vec![
            Branch::i32("x", vec![1, 2, 3, 4]),
            Branch::jagged_f32("v", vec![vec![1.0], vec![], vec![2.0, 3.0], vec![4.0]]),
            Branch::strings(
                "s",
                vec!["a".into(), "bb".into(), String::new(), "d".into()],
            ),
        ]
    }

    /// Each leaf's `(name, fIsRange, fMaximum)`, as a generic reader sees them.
    fn leaf_ranges(bytes: Vec<u8>) -> Vec<(String, bool, i64)> {
        use oxiroot_io_core::Value;
        fn walk(v: &Value, out: &mut Vec<(String, bool, i64)>) {
            if v.class().is_some_and(|c| c.starts_with("TLeaf")) {
                let name = v.get("fName").and_then(Value::as_str).unwrap_or("");
                if !out.iter().any(|(n, ..)| n == name) {
                    let range = v.get("fIsRange").and_then(Value::as_bool).unwrap();
                    let max = v.get("fMaximum").and_then(Value::as_f64).unwrap() as i64;
                    out.push((name.to_string(), range, max));
                }
                return;
            }
            for (_, member) in v.members().unwrap_or_default() {
                walk(member, out);
            }
            for element in v.as_array().unwrap_or_default() {
                walk(element, out);
            }
        }
        let f = FileReader::from_bytes(bytes).unwrap();
        let mut out = Vec::new();
        walk(&f.get_value("T").unwrap(), &mut out);
        out
    }

    #[test]
    fn both_writers_fill_leaf_ranges_as_root_does() {
        // ROOT keeps a plain leaf's fMaximum at 0, marks a count leaf as a range
        // with the largest count, and gives a TLeafC the longest string + 1.
        // The streamed file sees the largest values only in its second batch.
        let first = || {
            vec![
                Branch::i32("x", vec![3, 7]),
                Branch::jagged_f32("v", vec![vec![1.0], vec![]]),
                Branch::strings("s", vec!["a".into(), "bbb".into()]),
            ]
        };
        let second = || {
            vec![
                Branch::i32("x", vec![9, 1]),
                Branch::jagged_f32("v", vec![vec![2.0, 3.0, 4.0], vec![5.0]]),
                Branch::strings("s", vec!["cc".into(), "dddd".into()]),
            ]
        };
        let all = vec![
            Branch::i32("x", vec![3, 7, 9, 1]),
            Branch::jagged_f32("v", vec![vec![1.0], vec![], vec![2.0, 3.0, 4.0], vec![5.0]]),
            Branch::strings(
                "s",
                vec!["a".into(), "bbb".into(), "cc".into(), "dddd".into()],
            ),
        ];
        let expected: Vec<(String, bool, i64)> = [
            ("x", false, 0),
            ("nv", true, 3),
            ("v", false, 0),
            ("s", false, 5),
        ]
        .into_iter()
        .map(|(n, r, m)| (n.to_string(), r, m))
        .collect();

        let one_shot =
            tree_bytes("t.root", "T", &all, Compression::None, 0, KSTART_BIG_FILE).unwrap();
        assert_eq!(leaf_ranges(one_shot), expected, "one-shot writer");

        let mut w =
            TreeWriter::new(Cursor::new(Vec::new()), "t.root", "T", Compression::None).unwrap();
        w.write_batch(&first()).unwrap();
        w.write_batch(&second()).unwrap();
        assert_eq!(
            leaf_ranges(w.finish().unwrap().into_inner()),
            expected,
            "streaming writer"
        );
    }

    #[test]
    fn one_shot_switches_to_the_big_form_past_the_threshold() {
        // Forced into the 64-bit form, a tiny tree matches the streaming writer's
        // big output for the same single batch.
        let scalars = || {
            vec![
                Branch::i32("x", vec![3, 7, 5]),
                Branch::f64("y", vec![0.5, 2.0, 1.0]),
            ]
        };
        let one_shot = tree_bytes("t.root", "T", &scalars(), Compression::Zstd(3), 0, 0).unwrap();
        let mut w =
            TreeWriter::new_large(Cursor::new(Vec::new()), "t.root", "T", Compression::Zstd(3))
                .unwrap();
        w.write_batch(&scalars()).unwrap();
        assert_eq!(one_shot, w.finish().unwrap().into_inner());

        let small = tree_bytes(
            "t.root",
            "T",
            &scalars(),
            Compression::Zstd(3),
            0,
            KSTART_BIG_FILE,
        )
        .unwrap();
        assert!(!FileReader::from_bytes(small).unwrap().header().is_big());

        // Every branch kind reads back from the big form.
        let bytes = tree_bytes("t.root", "T", &branches(), Compression::None, 2, 0).unwrap();
        let f = FileReader::from_bytes(bytes).unwrap();
        assert!(f.header().is_big());
        let t = TreeReader::open(&f, "T").unwrap();
        assert_eq!(
            t.read_branch(&f, "x").unwrap(),
            BranchValues::I32(vec![1, 2, 3, 4])
        );
        assert_eq!(
            t.read_branch(&f, "v").unwrap(),
            BranchValues::VecF32(vec![vec![1.0], vec![], vec![2.0, 3.0], vec![4.0]])
        );
        assert_eq!(
            t.read_branch(&f, "s").unwrap(),
            BranchValues::Str(vec!["a".into(), "bb".into(), String::new(), "d".into()])
        );
    }

    #[test]
    fn a_branch_name_longer_than_255_bytes_round_trips() {
        let name = "b".repeat(300);
        let bytes = tree_bytes(
            "t.root",
            "T",
            &[Branch::f64(name.as_str(), vec![0.5, 1.5])],
            Compression::None,
            1,
            KSTART_BIG_FILE,
        )
        .unwrap();
        let f = FileReader::from_bytes(bytes).unwrap();
        let t = TreeReader::open(&f, "T").unwrap();
        assert_eq!(
            t.read_branch(&f, &name).unwrap(),
            BranchValues::F64(vec![0.5, 1.5])
        );
    }
}
