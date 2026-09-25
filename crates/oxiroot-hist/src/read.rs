//! [`ReadRoot`] — read a ROOT object from a file with an associated function.

use oxiroot_io_core::{FileReader, Result};

use crate::collections::{GraphStack, HistStack};
use crate::efficiency::Efficiency;
use crate::graph::Graph;
use crate::graph2d::Graph2D;
use crate::hist1d::Hist1D;
use crate::hist2d::Hist2D;
use crate::hist3d::Hist3D;
use crate::multierrorgraph::MultiErrorGraph;
use crate::polyhist::PolyHist;
use crate::profile1d::Profile1D;
use crate::profile2d::Profile2D;
use crate::profile3d::Profile3D;
use crate::sparsehist::SparseHist;

// The `ReadRoot` trait now lives in `oxiroot-io-core`; re-export it so
// `oxiroot_hist::ReadRoot` keeps resolving. This module registers the histogram
// family's implementations.
pub use oxiroot_io_core::ReadRoot;

macro_rules! impl_read_root {
    ($ty:ty, $read:path, $read_in:path) => {
        impl ReadRoot for $ty {
            fn read_root(file: &FileReader, name: &str) -> Result<Self> {
                $read(file, name)
            }
            fn read_root_in(file: &FileReader, dir: &str, name: &str) -> Result<Self> {
                $read_in(file, dir, name)
            }
        }
    };
}

impl_read_root!(Hist1D, crate::hist1d::read_th1, crate::hist1d::read_th1_in);
impl_read_root!(Hist2D, crate::hist2d::read_th2, crate::hist2d::read_th2_in);
impl_read_root!(Hist3D, crate::hist3d::read_th3, crate::hist3d::read_th3_in);
impl_read_root!(
    Profile1D,
    crate::profile1d::read_tprofile,
    crate::profile1d::read_tprofile_in
);
impl_read_root!(
    Profile2D,
    crate::profile2d::read_tprofile2d,
    crate::profile2d::read_tprofile2d_in
);
impl_read_root!(
    Profile3D,
    crate::profile3d::read_tprofile3d,
    crate::profile3d::read_tprofile3d_in
);
impl_read_root!(
    Efficiency,
    crate::efficiency::read_tefficiency,
    crate::efficiency::read_tefficiency_in
);
impl_read_root!(
    SparseHist,
    crate::sparsehist::read_thnsparse,
    crate::sparsehist::read_thnsparse_in
);
impl_read_root!(
    PolyHist,
    crate::polyhist::read_th2poly,
    crate::polyhist::read_th2poly_in
);
impl_read_root!(
    Graph,
    crate::graph::read_tgraph,
    crate::graph::read_tgraph_in
);
impl_read_root!(
    Graph2D,
    crate::graph2d::read_tgraph2d,
    crate::graph2d::read_tgraph2d_in
);
impl_read_root!(
    MultiErrorGraph,
    crate::multierrorgraph::read_tgraphmultierrors,
    crate::multierrorgraph::read_tgraphmultierrors_in
);
impl_read_root!(
    HistStack,
    crate::collections::read_thstack,
    crate::collections::read_thstack_in
);
impl_read_root!(
    GraphStack,
    crate::collections::read_tmultigraph,
    crate::collections::read_tmultigraph_in
);
