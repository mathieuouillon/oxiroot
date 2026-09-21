//! Reading and writing ROOT `TTree` — the classic columnar store.
//!
//! A `TTree` holds `TBranch`es, each storing its values in `TBasket`s (`TKey`s)
//! described by `TLeaf`s. This crate reads and writes: primitive scalars
//! (`bool`/`int`/`float`/… and unsigned variants), fixed-size arrays (`x[N]`),
//! variable-length arrays (`x[n]`, via `fEntryOffset`), `TLeafC` strings,
//! unsplit `std::vector<T>` (`TBranchElement`), and split
//! `std::vector<MyStruct>` exposed as per-member sub-branches (`hits.x`, …) —
//! across any number of baskets, compressed or not. Split writing generates a
//! `TStreamerInfo` for the element struct so the file is self-describing.

mod basket;
mod chain;
mod entrylist;
mod merge;
mod reader;
mod streamer_gen;
mod value;
mod writer;

pub use chain::ChainReader;
pub use entrylist::TEntryList;
pub use merge::{append_trees, concat_trees};
pub use reader::{Friend, TreeReader};
pub use value::{BranchValues, Jagged, LeafType};
pub use writer::{
    tree_file_bytes, write_tree_file, write_tree_file_baskets, Branch, SplitMember, Tree,
    TreeWriter,
};
