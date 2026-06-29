//! [`TChain`] — read a branch across several files' trees as one concatenated
//! column, the way ROOT's `TChain` spans a dataset split over many files.

use oxiroot_io_core::error::{Error, Result};
use oxiroot_io_core::RFile;

use crate::reader::TTree;
use crate::value::BranchValues;

/// A chain of same-schema `TTree`s across several open files. Reading a branch
/// concatenates that branch's values from every tree in add order.
///
/// The files are borrowed, so keep them alive for the chain's lifetime:
/// ```no_run
/// # use oxiroot_io_core::RFile;
/// # use oxiroot_tree::TChain;
/// let f1 = RFile::open("a.root")?;
/// let f2 = RFile::open("b.root")?;
/// let mut chain = TChain::new();
/// chain.add(&f1, "Events")?;
/// chain.add(&f2, "Events")?;
/// let all = chain.read_branch("pt")?; // values from both files
/// # Ok::<(), oxiroot_io_core::Error>(())
/// ```
#[derive(Default)]
pub struct TChain<'a> {
    trees: Vec<(&'a RFile, TTree)>,
}

impl<'a> TChain<'a> {
    /// An empty chain.
    #[must_use]
    pub fn new() -> TChain<'a> {
        TChain { trees: Vec::new() }
    }

    /// Open the tree named `tree_name` in `file` and append it to the chain.
    pub fn add(&mut self, file: &'a RFile, tree_name: &str) -> Result<()> {
        let tree = TTree::open(file, tree_name)?;
        self.trees.push((file, tree));
        Ok(())
    }

    /// Total number of entries across all trees in the chain.
    #[must_use]
    pub fn num_entries(&self) -> u64 {
        self.trees.iter().map(|(_, t)| t.num_entries()).sum()
    }

    /// The number of trees (files) in the chain.
    #[must_use]
    pub fn num_trees(&self) -> usize {
        self.trees.len()
    }

    /// The branch names of the first tree (the chain's schema), or empty.
    #[must_use]
    pub fn branch_names(&self) -> Vec<&str> {
        self.trees
            .first()
            .map(|(_, t)| t.branch_names())
            .unwrap_or_default()
    }

    /// Read branch `name` across the whole chain, concatenating each tree's
    /// values in add order. Fails if a tree lacks the branch or yields a
    /// different element type than the others.
    pub fn read_branch(&self, name: &str) -> Result<BranchValues> {
        let parts = self
            .trees
            .iter()
            .map(|(file, tree)| tree.read_branch(file, name))
            .collect::<Result<Vec<_>>>()?;
        concat_values(parts, name)
    }
}

/// Concatenate same-variant [`BranchValues`] into one.
fn concat_values(parts: Vec<BranchValues>, name: &str) -> Result<BranchValues> {
    use BranchValues::*;
    let mut it = parts.into_iter();
    let Some(first) = it.next() else {
        return Err(Error::Format(format!(
            "chain has no trees to read branch {name:?} from"
        )));
    };
    macro_rules! cat {
        ($variant:ident, $acc:ident) => {{
            for p in it {
                match p {
                    $variant(more) => $acc.extend(more),
                    other => {
                        return Err(Error::Format(format!(
                            "branch {name:?} has inconsistent types across the chain \
                             (got {:?})",
                            other.leaf_type()
                        )))
                    }
                }
            }
            $variant($acc)
        }};
    }
    Ok(match first {
        Bool(mut v) => cat!(Bool, v),
        I8(mut v) => cat!(I8, v),
        U8(mut v) => cat!(U8, v),
        I16(mut v) => cat!(I16, v),
        U16(mut v) => cat!(U16, v),
        I32(mut v) => cat!(I32, v),
        U32(mut v) => cat!(U32, v),
        I64(mut v) => cat!(I64, v),
        U64(mut v) => cat!(U64, v),
        F32(mut v) => cat!(F32, v),
        F64(mut v) => cat!(F64, v),
        VecBool(mut v) => cat!(VecBool, v),
        VecI8(mut v) => cat!(VecI8, v),
        VecU8(mut v) => cat!(VecU8, v),
        VecI16(mut v) => cat!(VecI16, v),
        VecU16(mut v) => cat!(VecU16, v),
        VecI32(mut v) => cat!(VecI32, v),
        VecU32(mut v) => cat!(VecU32, v),
        VecI64(mut v) => cat!(VecI64, v),
        VecU64(mut v) => cat!(VecU64, v),
        VecF32(mut v) => cat!(VecF32, v),
        VecF64(mut v) => cat!(VecF64, v),
        Str(mut v) => cat!(Str, v),
        VecStr(mut v) => cat!(VecStr, v),
        Nested {
            offsets: mut acc_off,
            items,
        } => {
            // Concatenate offsets (rebased onto the running total) and recurse to
            // concatenate the flattened inner collections.
            let mut acc_items = vec![*items];
            for p in it {
                match p {
                    Nested { offsets, items } => {
                        let base = acc_off.last().copied().unwrap_or(0);
                        acc_off.extend(offsets.iter().skip(1).map(|&o| o + base));
                        acc_items.push(*items);
                    }
                    other => {
                        return Err(Error::Format(format!(
                            "branch {name:?} has inconsistent types across the chain \
                             (got {:?})",
                            other.leaf_type()
                        )))
                    }
                }
            }
            Nested {
                offsets: acc_off,
                items: Box::new(concat_values(acc_items, name)?),
            }
        }
    })
}
