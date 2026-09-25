//! Generic ROOT objects that belong to no data format: the scalar objects
//! [`ObjString`] and [`Parameter`], and the collections [`ObjList`] (a `TList`
//! or `TObjArray`) and [`ObjMap`], which hold any writable objects.
//!
//! They live here, below the format crates, so any crate can store a label, a
//! parameter or a list of its own objects without depending on the histogram
//! crate.

mod collection;
mod scalars;

pub use collection::{FromMember, ListKind, ObjList, ObjMap};
pub use scalars::{ObjString, ParamValue, Parameter};
