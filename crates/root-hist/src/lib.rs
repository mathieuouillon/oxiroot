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

pub mod axis;
pub mod th1;
pub mod th2;
pub mod th3;
pub mod tprofile;

pub use axis::TAxis;
pub use th1::{read_th1, read_th1d, read_th1f, TH1};
pub use th2::{read_th2, read_th2d, read_th2f, TH2};
pub use th3::{read_th3, read_th3d, read_th3f, TH3};
pub use tprofile::{read_tprofile, TProfile};
