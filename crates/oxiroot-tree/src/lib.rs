//! Reading (and, later, writing) ROOT `TTree` — the classic columnar store.
//!
//! A `TTree` holds `TBranch`es, each storing its values in `TBasket`s (`TKey`s)
//! described by `TLeaf`s. This crate currently reads **Tier 1** trees: flat
//! branches with a single primitive leaf (`bool`/`int`/`float`/… and unsigned
//! variants), across any number of baskets. Arrays, `std::string`, and split /
//! `std::vector` branches arrive later.

mod basket;
mod reader;
mod value;

pub use reader::TTree;
pub use value::{BranchValues, LeafType};
