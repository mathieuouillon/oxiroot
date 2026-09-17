//! Classic ROOT histograms read (and, later, write).
//!
//! These histograms serialize through ROOT's `TStreamerInfo` mechanism and are
//! the histogram objects actually stored in ROOT files. (ROOT 7 `RHist` has no
//! persistable on-disk format — its `Streamer` throws — so it is intentionally
//! out of scope.)
//!
//! Supported for reading: `TH1D`/`TH1F`, `TH2D`/`TH2F`, `TH3D`/`TH3F`, and
//! `TProfile`. Bin contents are widened to `f64` regardless of their on-disk
//! type; the exact class is preserved in `class_name`.

mod base;
mod collections;
mod compare;
mod from_member;
mod naming;
mod ops;
mod quick;
mod read;

mod axis;
mod derive;
#[cfg(feature = "fit")]
mod fit;
mod graph;
mod graph2d;
mod graphmultierrors;
mod sample;
mod stats;
mod tefficiency;
mod tf;
mod th1;
mod th2;
mod th2poly;
mod th3;
mod thnsparse;
mod threaded;
mod tprofile;
mod tprofile2d;
mod tprofile3d;
mod write;

pub use oxiroot_io_core::Compression;

pub use axis::TAxis;
pub use base::BinContentType;
pub use collections::{THStack, TMultiGraph};
pub use compare::{Chi2TestKind, Chi2TestResult, KsTestResult};
#[cfg(feature = "fit")]
pub use fit::{FitData, FitExt, FitMethod, FitOptions, FitResult, Minimizer, Model, Point, Points};
pub use graph::{GraphErrors, GraphFunction, TGraph};
pub use graph2d::TGraph2D;
pub use graphmultierrors::TGraphMultiErrors;
// The generic objects live in `oxiroot-io-core`; re-exported so these paths keep
// resolving.
pub use ops::Histogram;
pub use oxiroot_io_core::{
    FromMember, ListKind, ObjList, ParamValue, TMap, TObjString, TParameter,
};
pub use quick::{Hist, H1, H2, H3};
pub use read::ReadRoot;
pub use sample::Random;
pub use tefficiency::TEfficiency;
pub use tf::{TF1, TF2, TF3};
pub use th1::TH1;
pub use th2::TH2;
pub use th2poly::{PolyBin, TH2Poly};
pub use th3::TH3;
pub use thnsparse::{SparseBin, THnSparse};
#[cfg(feature = "rayon")]
pub use threaded::fill_par;
pub use threaded::{Mergeable, ThreadedHist};
pub use tprofile::{ErrorMode, TProfile};
pub use tprofile2d::TProfile2D;
pub use tprofile3d::TProfile3D;
pub use write::{RootFile, SubdirBuilder, WriteRoot};
