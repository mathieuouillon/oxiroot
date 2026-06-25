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
pub mod th1;
pub mod th2;
pub mod th3;
pub mod tprofile;
pub mod write;

pub use oxiroot_io_core::Compression;

pub use axis::TAxis;
pub use th1::{read_th1, read_th1d, read_th1d_in, read_th1f, TH1};
pub use th2::{read_th2, read_th2d, read_th2f, TH2};
pub use th3::{read_th3, read_th3d, read_th3f, TH3};
pub use tprofile::{read_tprofile, TProfile};
pub use write::{
    append_histograms_file, th1d_to_bytes, th1f_to_bytes, th2d_to_bytes, th2f_to_bytes,
    th3d_to_bytes, th3f_to_bytes, tprofile_to_bytes, write_histograms_dirs, write_histograms_file,
    write_th1d, write_th1d_file, write_th1f, write_th1f_file, write_th2d, write_th2d_file,
    write_th2f, write_th2f_file, write_th3d, write_th3d_file, write_th3f, write_th3f_file,
    write_tprofile, write_tprofile_file, Hist,
};
