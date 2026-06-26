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
mod ops;

pub mod axis;
mod derive;
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
pub use tefficiency::{read_tefficiency, TEfficiency};
pub use th1::{read_th1, read_th1d, read_th1d_in, read_th1f, TH1};
pub use th2::{read_th2, read_th2d, read_th2f, TH2};
pub use th2poly::{read_th2poly, PolyBin, TH2Poly};
pub use th3::{read_th3, read_th3d, read_th3f, TH3};
pub use thnsparse::{read_thnsparse, SparseBin, THnSparse};
#[cfg(feature = "rayon")]
pub use threaded::fill_par;
pub use threaded::{merge_all, Merge, ThreadedHist};
pub use tprofile::{read_tprofile, TProfile};
pub use tprofile2d::{read_tprofile2d, TProfile2D};
pub use tprofile3d::{read_tprofile3d, TProfile3D};
pub use write::{
    append_histograms_file, tefficiency_to_bytes, th1c_to_bytes, th1d_to_bytes, th1f_to_bytes,
    th1i_to_bytes, th1l_to_bytes, th1s_to_bytes, th2c_to_bytes, th2d_to_bytes, th2f_to_bytes,
    th2i_to_bytes, th2l_to_bytes, th2poly_to_bytes, th2s_to_bytes, th3c_to_bytes, th3d_to_bytes,
    th3f_to_bytes, th3i_to_bytes, th3l_to_bytes, th3s_to_bytes, thnsparse_to_bytes,
    tprofile2d_to_bytes, tprofile3d_to_bytes, tprofile_to_bytes, write_histograms_dirs,
    write_histograms_file, write_tefficiency, write_tefficiency_file, write_th1c, write_th1c_file,
    write_th1d, write_th1d_file, write_th1f, write_th1f_file, write_th1i, write_th1i_file,
    write_th1l, write_th1l_file, write_th1s, write_th1s_file, write_th2c, write_th2c_file,
    write_th2d, write_th2d_file, write_th2f, write_th2f_file, write_th2i, write_th2i_file,
    write_th2l, write_th2l_file, write_th2poly, write_th2poly_file, write_th2s, write_th2s_file,
    write_th3c, write_th3c_file, write_th3d, write_th3d_file, write_th3f, write_th3f_file,
    write_th3i, write_th3i_file, write_th3l, write_th3l_file, write_th3s, write_th3s_file,
    write_thnsparse, write_thnsparse_file, write_tprofile, write_tprofile2d, write_tprofile2d_file,
    write_tprofile3d, write_tprofile3d_file, write_tprofile_file, Hist,
};
