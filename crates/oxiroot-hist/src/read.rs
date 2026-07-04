//! [`ReadRoot`] — read a ROOT object from a file with an associated function.

use oxiroot_io_core::error::Result;
use oxiroot_io_core::RFile;

use crate::collections::{THStack, TMultiGraph};
use crate::graph::TGraph;
use crate::graph2d::TGraph2D;
use crate::graphmultierrors::TGraphMultiErrors;
use crate::objects::{TObjString, TParameter};
use crate::objlist::{ObjList, TMap};
use crate::tefficiency::TEfficiency;
use crate::tf::{TF1, TF2, TF3};
use crate::th1::TH1;
use crate::th2::TH2;
use crate::th2poly::TH2Poly;
use crate::th3::TH3;
use crate::thnsparse::THnSparse;
use crate::tprofile::TProfile;
use crate::tprofile2d::TProfile2D;
use crate::tprofile3d::TProfile3D;

// The `ReadRoot` trait now lives in `oxiroot-io-core`; re-export it so
// `oxiroot_hist::ReadRoot` keeps resolving. This module registers the histogram
// family's implementations.
pub use oxiroot_io_core::ReadRoot;

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
impl_read_root!(TF1, crate::tf::read_tf1, crate::tf::read_tf1_in);
impl_read_root!(TF2, crate::tf::read_tf2, crate::tf::read_tf2_in);
impl_read_root!(TF3, crate::tf::read_tf3, crate::tf::read_tf3_in);
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
    ObjList,
    crate::objlist::read_objlist,
    crate::objlist::read_objlist_in
);
impl_read_root!(
    TMap,
    crate::objlist::read_tmap,
    crate::objlist::read_tmap_in
);
