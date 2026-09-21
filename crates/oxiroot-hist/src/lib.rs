//! Classic ROOT histograms, profiles and graphs, read and written byte-for-byte
//! as ROOT 6 does.
//!
//! - Histograms: [`TH1`], [`TH2`], [`TH3`] in every bin content type
//!   ([`BinContentType`]; the exact class is preserved in `class_name`), built
//!   with [`Hist`].
//! - Profiles: [`TProfile`], [`TProfile2D`], [`TProfile3D`].
//! - [`TEfficiency`], [`THnSparse`] and [`TH2Poly`].
//! - Graphs: [`TGraph`] (with its error variants), [`TGraph2D`] and
//!   [`TGraphMultiErrors`]; the collections [`THStack`] and [`TMultiGraph`].
//!
//! The parametric functions (`TF1`/`TF2`/`TF3`) live in `oxiroot-hist-func`,
//! so a histogram-only build does not compile the formula engine; a graph's
//! attached functions are plain [`GraphFunction`] records.
//!
//! These objects serialize through ROOT's `TStreamerInfo` mechanism. (ROOT 7
//! `RHist` has no persistable on-disk format — its `Streamer` throws — so it is
//! out of scope.)

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
mod tf1_record;
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
pub use write::{hist_streamer_blob, FileWriter, SubdirWriter, WriteRoot};
