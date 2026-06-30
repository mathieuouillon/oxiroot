//! Classic ROOT histograms read (and, later, write).
//!
//! These histograms serialize through ROOT's `TStreamerInfo` mechanism and are
//! the histogram objects actually stored in ROOT files. (ROOT 7 `RHist` has no
//! persistable on-disk format — its `Streamer` throws — so it is intentionally
//! out of scope.)
//!
//! Supported for reading: `TH1D`/`TH1F`, `TH2D`/`TH2F`, `TH3D`/`TH3F`, and
//! `TProfile`. Bin contents are widened to `f64` regardless of on-disk
//! precision; the exact class is preserved in `class_name`.

mod base;
mod collections;
mod compare;
mod naming;
mod objects;
mod ops;
mod read;

pub mod axis;
mod derive;
#[cfg(feature = "fit")]
pub mod fit;
pub mod graph;
pub mod graph2d;
pub mod graphmultierrors;
mod stats;
pub mod tefficiency;
pub mod th1;
pub mod th2;
pub mod th2poly;
pub mod th3;
pub mod thnsparse;
pub mod threaded;
pub mod tprofile;
pub mod tprofile2d;
pub mod tprofile3d;
pub mod write;

pub use oxiroot_io_core::Compression;

pub use axis::TAxis;
pub use base::Precision;
pub use collections::{THStack, TMultiGraph};
pub use compare::{Chi2TestKind, Chi2TestResult, KsTestResult};
#[cfg(feature = "fit")]
pub use fit::{
    FitData, FitExt, FitMethod, FitOptions, FitResult, Minimizer, Model, Point, Points, TF1,
};
pub use graph::{GraphErrors, GraphFunction, TGraph};
pub use graph2d::TGraph2D;
pub use graphmultierrors::TGraphMultiErrors;
pub use objects::{ParamValue, TObjString, TParameter};
pub use ops::Histogram;
pub use read::ReadRoot;
pub use tefficiency::TEfficiency;
pub use th1::TH1;
pub use th2::TH2;
pub use th2poly::{PolyBin, TH2Poly};
pub use th3::TH3;
pub use thnsparse::{SparseBin, THnSparse};
#[cfg(feature = "rayon")]
pub use threaded::fill_par;
pub use threaded::{merge_all, Merge, ThreadedHist};
pub use tprofile::{ErrorMode, TProfile};
pub use tprofile2d::TProfile2D;
pub use tprofile3d::TProfile3D;
pub use write::{Dir, RootFile, WriteRoot};
