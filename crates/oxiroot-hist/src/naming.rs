//! Fluent `named()` / `titled()` setters shared by every named ROOT object.
//!
//! A histogram (or graph, profile, …) is just data — a binning and its contents.
//! Unlike ROOT, oxiroot never forces a name at construction and keeps no global
//! name registry, so you can build as many anonymous histograms as you like. A
//! name is only needed when you *persist* the object: it becomes the file key.
//! Set it then (or whenever) with [`named`](Hist1D::named); writing an unnamed
//! object, or two with the same name in one directory, is a loud error.

use crate::{
    Efficiency, Graph, Graph2D, Hist1D, Hist2D, Hist3D, MultiErrorGraph, PolyHist, Profile1D,
    Profile2D, Profile3D, SparseHist,
};

macro_rules! impl_named {
    ($($t:ty),+ $(,)?) => {$(
        impl $t {
            #[doc = "Set the object's name — the key it is written under in a ROOT"]
            #[doc = "file. Chainable: `Hist1D::new(100, 0.0, 1.0).named(\"pt\")`."]
            #[must_use]
            pub fn named(mut self, name: impl Into<String>) -> Self {
                self.name = name.into();
                self
            }

            #[doc = "Set the object's title (ROOT's `fTitle`)."]
            #[must_use]
            pub fn titled(mut self, title: impl Into<String>) -> Self {
                self.title = title.into();
                self
            }
        }
    )+};
}

impl_named!(
    Hist1D,
    Hist2D,
    Hist3D,
    Profile1D,
    Profile2D,
    Profile3D,
    Efficiency,
    SparseHist,
    PolyHist,
    Graph,
    Graph2D,
    MultiErrorGraph,
);
