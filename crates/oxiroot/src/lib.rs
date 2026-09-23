//! `oxiroot`: pure-Rust IO for the CERN ROOT file format.
//!
//! Read and write [RNTuple](oxiroot_rntuple) (ROOT's columnar event-data
//! format), classic [`TTree`](oxiroot_tree), the [histogram](oxiroot_hist)
//! family (`TH1`/`TH2`/`TH3`, `TProfile`/`2D`/`3D`, `TEfficiency`, `THnSparse`,
//! `TH2Poly`), and [graphs](oxiroot_hist::TGraph) (`TGraph`/`TGraphErrors`/
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
//! let f = FileReader::open("data.root")?;
//! let ntpl = NtupleReader::open(&f, "events")?;
//! assert_eq!(ntpl.num_entries(), 2);
//! # Ok::<(), oxiroot::Error>(())
//! ```
//!
//! # Naming
//!
//! Types that read a file end in `Reader` ([`FileReader`],
//! [`TreeReader`](tree::TreeReader), [`NtupleReader`](ntuple::NtupleReader),
//! [`ChainReader`](tree::ChainReader)), and types that write one end in
//! `Writer` ([`FileWriter`](file::FileWriter), [`TreeWriter`](tree::TreeWriter),
//! [`NtupleWriter`](ntuple::NtupleWriter)). The rest is in-memory data:
//! histograms and graphs keep their ROOT class names (`TH1`, `TGraph`, …), and
//! a [`Tree`](tree::Tree) or [`Ntuple`](ntuple::Ntuple) holds a whole tree or
//! RNTuple to write in one go. ROOT's names for the readers and writers
//! (`TFile`, `TTree`, `RNTuple`, `TChain`) are doc aliases, so searching the
//! docs for them finds these types.
//!
//! The flat [`prelude`] covers the common read/write surface; the [`hist`],
//! [`ntuple`], [`tree`], [`compress`], and [`file`](mod@file) modules expose
//! everything else.

#[doc(inline)]
pub use oxiroot_io_core::{read_object, Compression, Error, FileReader, Result, Value};

/// The ROOT file container and the object framework: reading and writing files,
/// keys and directories, streamer info, and the traits objects implement
/// (re-exported from `oxiroot-io-core`).
pub mod file {
    pub use oxiroot_io_core::*;
}

pub mod hadd;

/// ROOT compression framing and codecs (re-exported from `oxiroot-compress`).
pub mod compress {
    pub use oxiroot_compress::*;
}

/// Classic ROOT histograms, profiles, graphs and functions —
/// `TH1`/`TH2`/`TH3`/`TProfile`/`TGraph`/`TF1`… (from `oxiroot-hist` and
/// `oxiroot-hist-func`).
pub mod hist {
    pub use oxiroot_hist::*;
    pub use oxiroot_hist_func::{TF1, TF2, TF3};
}

/// ROOT linear-algebra objects — `TVectorD`/`TMatrixD`/`TMatrixDSym` (from
/// `oxiroot-linalg`), with byte-exact ROOT read/write.
pub mod linalg {
    pub use oxiroot_linalg::*;
}

/// RNTuple, ROOT's columnar event-data format (from `oxiroot-rntuple`).
pub mod ntuple {
    pub use oxiroot_rntuple::*;
}

/// Classic `TTree` columnar storage (from `oxiroot-tree`).
pub mod tree {
    pub use oxiroot_tree::*;
}

/// Pure-Rust statistics — special functions (`erf`, `gammainc`, `betainc`,
/// `ndtri`), distributions (`Normal`/`StudentT`/`ChiSquared`/`FisherF`),
/// descriptive stats (`skew`, `kurtosis`, `sem`, …), correlation (`pearsonr`,
/// `spearmanr`), and hypothesis tests (`ttest_ind`, `normaltest`) — from
/// `oxiroot-stat`, verified against `scipy.stats`.
pub mod stat {
    pub use oxiroot_stat::*;
}

/// PDG particle data — the [`PdgId`](oxiroot_particle::PdgId) Monte Carlo
/// numbering-scheme decoder (is it a lepton? meson? baryon? its charge/spin/quark
/// content) plus a bundled [`Particle`](oxiroot_particle::Particle) table (mass,
/// width, lifetime, quantum numbers). From `oxiroot-particle`, a Rust take on
/// scikit-hep `particle` and verified against it.
pub mod particle {
    pub use oxiroot_particle::*;
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
/// `use oxiroot::prelude::*;` brings in the file reader and writer
/// ([`FileReader`], [`FileWriter`](crate::file::FileWriter)), [`Compression`],
/// the histogram types with their `read_*`/`write_*` helpers, and the tree and
/// RNTuple readers and writers.
pub mod prelude {
    // `Error` and `Result` are deliberately not here: a glob import would shadow
    // `std::result::Result`. Name them as `oxiroot::Error` / `oxiroot::Result`.
    pub use oxiroot_io_core::{
        Compression, FileReader, FileWriter, FromMember, ListKind, ObjList, ParamValue, ReadRoot,
        SubdirWriter, TMap, TObjString, TParameter, WriteInto, WriteRoot,
    };

    pub use crate::hadd::{merge_files, MergeKind, MergeReport, Merger};

    #[cfg(feature = "fit")]
    pub use oxiroot_fit::{
        FitData, FitExt, FitMethod, FitOptions, FitResult, Loss, Minimizer, Model, Point, Points,
    };
    #[cfg(feature = "rayon")]
    pub use oxiroot_hist::fill_par;
    pub use oxiroot_hist::{
        BinContentType, Chi2TestKind, Chi2TestResult, ErrorMode, GraphErrors, GraphFunction, Hist,
        Histogram, KsTestResult, Mergeable, PolyBin, Random, SparseBin, TAxis, TEfficiency, TGraph,
        TGraph2D, TGraphMultiErrors, TH2Poly, THStack, THnSparse, TMultiGraph, TProfile,
        TProfile2D, TProfile3D, ThreadedHist, TH1, TH2, TH3,
    };
    pub use oxiroot_hist_func::{TF1, TF2, TF3};
    pub use oxiroot_linalg::{TMatrixD, TMatrixDSym, TVectorD};

    pub use oxiroot_rntuple::{
        write_rntuple_file, Column, Field, FieldValues, Ntuple, NtupleReader, NtupleWriter,
    };

    pub use oxiroot_tree::{
        write_tree_file, write_tree_file_baskets, Branch, BranchValues, ChainReader, Friend,
        Jagged, LeafType, SplitMember, TEntryList, Tree, TreeIndex, TreeReader, TreeWriter,
    };
}
