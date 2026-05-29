//! `root-rs`: pure-Rust IO for the CERN ROOT file format.
//!
//! Read and write [RNTuple](root-rntuple) and classic
//! [histograms](root-hist) in the ROOT (TFile) container — no C++/libROOT
//! dependency. This facade re-exports the workspace crates; see the crate-level
//! docs of each for details.
//!
//! The high-level `RFile` Put/Get/List API (mirroring
//! `ROOT::Experimental::RFile`) is added as the container layer comes online in
//! milestone M1+.

#[doc(inline)]
pub use root_io_core::{buffer, error, file, Error, RFile, Result};

/// ROOT compression framing and codecs (re-exported from `root-compress`).
pub mod compress {
    pub use root_compress::*;
}

/// Classic ROOT histograms (re-exported from `root-hist`).
pub mod hist {
    pub use root_hist::*;
}
