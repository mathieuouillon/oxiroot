//! `oxiroot`: pure-Rust IO for the CERN ROOT file format.
//!
//! Read and write [RNTuple](oxiroot_rntuple) (ROOT's columnar event-data
//! format), classic [`TTree`](oxiroot_tree), the [histogram](oxiroot_hist)
//! family (`TH1`/`TH2`/`TH3`, `TProfile`/`2D`/`3D`, `TEfficiency`, `THnSparse`,
//! `TH2Poly`), and [graphs](oxiroot_hist::graph) (`TGraph`/`TGraphErrors`/
//! `TGraphAsymmErrors`, plus `TGraph2D` and `TGraphMultiErrors`) in the ROOT (`TFile`) container, with no C++/libROOT
//! dependency. Files written here are read by official ROOT and uproot, and
//! vice versa.
//!
//! # Quick start
//!
//! ```no_run
//! use oxiroot::prelude::*;
//!
//! // Fill and save a histogram (the Hist builder; `weight()` tracks Sumw2).
//! let mut h = Hist::reg(50, 0.0, 100.0).name("pt").title("transverse momentum").weight();
//! h.fill_weight(42.0, 1.5);
//! h.write_root("out.root", Compression::Zstd(5))?; // WriteRoot trait
//!
//! // Write a columnar dataset, then read it back.
//! let fields = vec![Field::f64("mass", vec![91.2, 125.0])];
//! write_rntuple_file("data.root", "events", &fields, Compression::None)?;
//! let f = RFile::open("data.root")?;
//! let ntpl = RNTuple::open(&f, "events")?;
//! assert_eq!(ntpl.num_entries(), 2);
//! # Ok::<(), oxiroot::Error>(())
//! ```
//!
//! The flat [`prelude`] covers the common read/write surface; the [`hist`],
//! [`ntuple`], [`tree`], [`compress`], and [`file`](mod@file) modules expose
//! everything else.

#[doc(inline)]
pub use oxiroot_io_core::{buffer, error, file, Compression, Error, RFile, Result};

/// ROOT compression framing and codecs (re-exported from `oxiroot-compress`).
pub mod compress {
    pub use oxiroot_compress::*;
}

/// Classic ROOT histograms — `TH1`/`TH2`/`TH3`/`TProfile` (from `oxiroot-hist`).
pub mod hist {
    pub use oxiroot_hist::*;
}

/// RNTuple, ROOT's columnar event-data format (from `oxiroot-rntuple`).
pub mod ntuple {
    pub use oxiroot_rntuple::*;
}

/// Classic `TTree` columnar storage (from `oxiroot-tree`).
pub mod tree {
    pub use oxiroot_tree::*;
}

/// Curve fitting for any 1-D data — histograms, graphs, or custom points (from
/// `oxiroot-fit`). The [`FitData`](oxiroot_fit::FitData) trait + the blanket
/// [`FitExt`](oxiroot_fit::FitExt) give `data.fit(&model)` to every dataset;
/// `hist`'s `TH1`/`TGraph` implement `FitData` (under the `fit` feature).
#[cfg(feature = "fit")]
pub mod fit {
    pub use oxiroot_fit::*;
}

/// Plotting — render histograms and graphs to SVG/PNG with a matplotlib-like
/// API and an mplhep histogram style (from `oxiroot-plot`). `Axes::hist`/
/// `errorbar`/`hist2d` draw `TH1`/`TGraph`/`TH2`; `$…$` labels are typeset as
/// LaTeX math. Enabled by the `plot` feature.
#[cfg(feature = "plot")]
pub mod plot {
    pub use oxiroot_plot::*;
}

/// The common types and functions for reading and writing ROOT files.
///
/// `use oxiroot::prelude::*;` brings in the container ([`RFile`],
/// [`Compression`]), the histogram types with their `read_*`/`write_*` helpers,
/// and the RNTuple reader/writer.
pub mod prelude {
    pub use oxiroot_io_core::{Compression, Error, RFile, Result};

    #[cfg(feature = "fit")]
    pub use oxiroot_fit::{
        FitData, FitExt, FitMethod, FitOptions, FitResult, Minimizer, Model, Point, Points, TF1,
    };
    #[cfg(feature = "rayon")]
    pub use oxiroot_hist::fill_par;
    pub use oxiroot_hist::{
        merge_all, Chi2TestKind, Chi2TestResult, Dir, ErrorMode, FromMember, GraphErrors,
        GraphFunction, Hist, Histogram, KsTestResult, ListKind, Merge, ObjList, ParamValue,
        PolyBin, Precision, ReadRoot, RootFile, SparseBin, TAxis, TEfficiency, TGraph, TGraph2D,
        TGraphMultiErrors, TH2Poly, THStack, THnSparse, TMap, TMatrixD, TMatrixDSym, TMultiGraph,
        TObjString, TParameter, TProfile, TProfile2D, TProfile3D, TVectorD, ThreadedHist,
        WriteRoot, TH1, TH2, TH3,
    };

    pub use oxiroot_rntuple::{
        write_rntuple_file, Column, Field, FieldValues, Ntuple, NtupleDir, NtupleFile, RNTuple,
        RNTupleWriter,
    };

    pub use oxiroot_tree::{
        write_tree_file, write_tree_file_baskets, Branch, BranchValues, Friend, Jagged, LeafType,
        SplitMember, TChain, TEntryList, TTree, TTreeWriter, Tree,
    };
}
