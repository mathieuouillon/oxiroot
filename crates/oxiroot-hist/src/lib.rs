//! Classic ROOT histograms, profiles and graphs, read and written byte-for-byte
//! as ROOT 6 does.
//!
//! - Histograms: [`Hist1D`], [`Hist2D`], [`Hist3D`] in every bin content type
//!   ([`BinContentType`]; the exact class is preserved in `class_name`), built
//!   with [`Hist`].
//! - Profiles: [`Profile1D`], [`Profile2D`], [`Profile3D`].
//! - [`Efficiency`], [`SparseHist`] and [`PolyHist`].
//! - Graphs: [`Graph`] (with its error variants), [`Graph2D`] and
//!   [`MultiErrorGraph`]; the collections [`HistStack`] and [`GraphStack`].
//!
//! The parametric functions (`Func1D`/`Func2D`/`Func3D`) live in `oxiroot-hist-func`,
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
mod efficiency;
#[cfg(feature = "fit")]
mod fit;
mod func_record;
mod graph;
mod graph2d;
mod hist1d;
mod hist2d;
mod hist3d;
mod multierrorgraph;
mod polyhist;
mod profile1d;
mod profile2d;
mod profile3d;
mod sample;
mod sparsehist;
mod stats;
mod threaded;
mod write;

pub use oxiroot_io_core::Compression;

pub use axis::Axis;
pub use base::BinContentType;
pub use collections::{GraphStack, HistStack};
pub use compare::{Chi2TestKind, Chi2TestResult, KsTestResult};
#[cfg(feature = "fit")]
pub use fit::{FitData, FitExt, FitMethod, FitOptions, FitResult, Minimizer, Model, Point, Points};
pub use graph::{Graph, GraphErrors, GraphFunction};
pub use graph2d::Graph2D;
pub use multierrorgraph::MultiErrorGraph;
// The generic objects live in `oxiroot-io-core`; re-exported so these paths keep
// resolving.
pub use efficiency::Efficiency;
pub use hist1d::Hist1D;
pub use hist2d::Hist2D;
pub use hist3d::Hist3D;
pub use ops::Histogram;
pub use oxiroot_io_core::{
    FromMember, ListKind, ObjList, ObjMap, ObjString, ParamValue, Parameter,
};
pub use polyhist::{PolyBin, PolyHist};
pub use profile1d::{ErrorMode, Profile1D};
pub use profile2d::Profile2D;
pub use profile3d::Profile3D;
pub use quick::{Build1D, Build2D, Build3D, Hist};
pub use read::ReadRoot;
pub use sample::Random;
pub use sparsehist::{SparseBin, SparseHist};
#[cfg(feature = "rayon")]
pub use threaded::fill_par;
pub use threaded::{Mergeable, ThreadedHist};
pub use write::{hist_streamer_classes, FileWriter, SubdirWriter, WriteRoot};
