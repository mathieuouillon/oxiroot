//! Reading (and, later, writing) ROOT `TTree` — the classic columnar store.
//!
//! A `TTree` holds `TBranch`es, each storing its values in `TBasket`s (`TKey`s)
//! described by `TLeaf`s. This crate reads branches with a single leaf:
//! primitive scalars (`bool`/`int`/`float`/… and unsigned variants),
//! fixed-size arrays (`x[N]`), variable-length arrays (`x[n]`, via
//! `fEntryOffset`), and `TLeafC` strings — across any number of baskets,
//! compressed or not. Split / `std::vector` (`TBranchElement`) branches and
//! writing arrive later.

mod basket;
mod reader;
mod value;

pub use reader::TTree;
pub use value::{BranchValues, LeafType};
