//! [`ReadRoot`] — read a ROOT object from a file with an associated function.

use oxiroot_io_core::error::Result;
use oxiroot_io_core::RFile;

use crate::collections::{THStack, TMultiGraph};
use crate::graph::TGraph;
use crate::graph2d::TGraph2D;
use crate::graphmultierrors::TGraphMultiErrors;
use crate::linalg::{TMatrixD, TMatrixDSym, TVectorD};
use crate::objects::{TObjString, TParameter};
use crate::objlist::{ObjList, TMap};
use crate::tefficiency::TEfficiency;
use crate::th1::TH1;
use crate::th2::TH2;
use crate::th2poly::TH2Poly;
use crate::th3::TH3;
use crate::thnsparse::THnSparse;
use crate::tprofile::TProfile;
use crate::tprofile2d::TProfile2D;
use crate::tprofile3d::TProfile3D;

/// Read a ROOT object of this type from an open file by key name, auto-detecting
/// the on-disk precision where one applies (`TH1D`/`F`/`I`/`S`/`C`/`L` all read
/// into a [`TH1`]). This is the way to read any single object:
///
/// ```no_run
/// use oxiroot_hist::{ReadRoot, TH1};
/// use oxiroot_io_core::RFile;
/// let f = RFile::open("in.root")?;
/// let h = TH1::read_root(&f, "h")?; // any of TH1D/F/I/S/C/L
/// let s = TH1::read_root_in(&f, "by_region", "sig")?; // from a subdirectory
/// # Ok::<(), oxiroot_io_core::Error>(())
/// ```
pub trait ReadRoot: Sized {
    /// Read the object stored under key `name` in the file's top directory.
    fn read_root(file: &RFile, name: &str) -> Result<Self>;
    /// Read the object stored under key `name` inside subdirectory `dir` (written
    /// via [`RootFile::dir`](crate::RootFile::dir)).
    fn read_root_in(file: &RFile, dir: &str, name: &str) -> Result<Self>;
}

macro_rules! impl_read_root {
    ($ty:ty, $read:path, $read_in:path) => {
        impl ReadRoot for $ty {
            fn read_root(file: &RFile, name: &str) -> Result<Self> {
                $read(file, name)
            }
            fn read_root_in(file: &RFile, dir: &str, name: &str) -> Result<Self> {
                $read_in(file, dir, name)
            }
        }
    };
}

impl_read_root!(TH1, crate::th1::read_th1, crate::th1::read_th1_in);
impl_read_root!(TH2, crate::th2::read_th2, crate::th2::read_th2_in);
impl_read_root!(TH3, crate::th3::read_th3, crate::th3::read_th3_in);
impl_read_root!(
    TProfile,
    crate::tprofile::read_tprofile,
    crate::tprofile::read_tprofile_in
);
impl_read_root!(
    TProfile2D,
    crate::tprofile2d::read_tprofile2d,
    crate::tprofile2d::read_tprofile2d_in
);
impl_read_root!(
    TProfile3D,
    crate::tprofile3d::read_tprofile3d,
    crate::tprofile3d::read_tprofile3d_in
);
impl_read_root!(
    TEfficiency,
    crate::tefficiency::read_tefficiency,
    crate::tefficiency::read_tefficiency_in
);
impl_read_root!(
    THnSparse,
    crate::thnsparse::read_thnsparse,
    crate::thnsparse::read_thnsparse_in
);
impl_read_root!(
    TH2Poly,
    crate::th2poly::read_th2poly,
    crate::th2poly::read_th2poly_in
);
impl_read_root!(
    TGraph,
    crate::graph::read_tgraph,
    crate::graph::read_tgraph_in
);
impl_read_root!(
    TGraph2D,
    crate::graph2d::read_tgraph2d,
    crate::graph2d::read_tgraph2d_in
);
impl_read_root!(
    TGraphMultiErrors,
    crate::graphmultierrors::read_tgraphmultierrors,
    crate::graphmultierrors::read_tgraphmultierrors_in
);
impl_read_root!(
    TObjString,
    crate::objects::read_tobjstring,
    crate::objects::read_tobjstring_in
);
impl_read_root!(
    TParameter,
    crate::objects::read_tparameter,
    crate::objects::read_tparameter_in
);
impl_read_root!(
    THStack,
    crate::collections::read_thstack,
    crate::collections::read_thstack_in
);
impl_read_root!(
    TMultiGraph,
    crate::collections::read_tmultigraph,
    crate::collections::read_tmultigraph_in
);
impl_read_root!(
    TVectorD,
    crate::linalg::read_tvectord,
    crate::linalg::read_tvectord_in
);
impl_read_root!(
    TMatrixD,
    crate::linalg::read_tmatrixd,
    crate::linalg::read_tmatrixd_in
);
impl_read_root!(
    TMatrixDSym,
    crate::linalg::read_tmatrixdsym,
    crate::linalg::read_tmatrixdsym_in
);
impl_read_root!(
    ObjList,
    crate::objlist::read_objlist,
    crate::objlist::read_objlist_in
);
impl_read_root!(
    TMap,
    crate::objlist::read_tmap,
    crate::objlist::read_tmap_in
);
