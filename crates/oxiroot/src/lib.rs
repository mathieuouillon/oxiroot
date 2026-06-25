//! `oxiroot`: pure-Rust IO for the CERN ROOT file format.
//!
//! Read and write [RNTuple](oxiroot_rntuple) (ROOT's columnar event-data format)
//! and classic [histograms](oxiroot_hist) — `TH1`/`TH2`/`TH3`/`TProfile` — in the
//! ROOT (`TFile`) container, with no C++/libROOT dependency. Files written here
//! are read by official ROOT and uproot, and vice versa.
//!
//! # Quick start
//!
//! ```no_run
//! use oxiroot::prelude::*;
//!
//! // Fill and save a histogram.
//! let mut h = TH1::new("pt", "transverse momentum", 50, 0.0, 100.0);
//! h.sumw2();
//! h.fill_weight(42.0, 1.5);
//! write_th1d_file("out.root", &h, Compression::Zstd(5))?;
//!
//! // Write a columnar dataset, then read it back.
//! let fields = vec![Field::f64("mass", vec![91.2, 125.0])];
//! write_rntuple_file("data.root", "events", &fields, Compression::None)?;
//! let f = RFile::open("data.root")?;
//! let ntpl = RNTuple::open(&f, "events")?;
//! assert_eq!(ntpl.num_entries(), 2);
//! # Ok::<(), oxiroot::Error>(())
//! ```
//!
//! The flat [`prelude`] covers the common read/write surface; the [`hist`],
//! [`ntuple`], [`compress`], and [`file`] modules expose everything else.

#[doc(inline)]
pub use oxiroot_io_core::{buffer, error, file, Compression, Error, RFile, Result};

/// ROOT compression framing and codecs (re-exported from `oxiroot-compress`).
pub mod compress {
    pub use oxiroot_compress::*;
}

/// Classic ROOT histograms — `TH1`/`TH2`/`TH3`/`TProfile` (from `oxiroot-hist`).
pub mod hist {
    pub use oxiroot_hist::*;
}

/// RNTuple, ROOT's columnar event-data format (from `oxiroot-rntuple`).
pub mod ntuple {
    pub use oxiroot_rntuple::*;
}

/// Classic `TTree` columnar storage (from `oxiroot-tree`).
pub mod tree {
    pub use oxiroot_tree::*;
}

/// The common types and functions for reading and writing ROOT files.
///
/// `use oxiroot::prelude::*;` brings in the container ([`RFile`],
/// [`Compression`]), the histogram types with their `read_*`/`write_*` helpers,
/// and the RNTuple reader/writer.
pub mod prelude {
    pub use oxiroot_io_core::{Compression, Error, RFile, Result};

    #[cfg(feature = "rayon")]
    pub use oxiroot_hist::fill_par;
    pub use oxiroot_hist::{
        append_histograms_file, merge_all, read_th1, read_th1d, read_th1d_in, read_th1f, read_th2,
        read_th2d, read_th2f, read_th3, read_th3d, read_th3f, read_tprofile, write_histograms_dirs,
        write_histograms_file, write_th1c_file, write_th1d_file, write_th1f_file, write_th1i_file,
        write_th1l_file, write_th1s_file, write_th2c_file, write_th2d_file, write_th2f_file,
        write_th2i_file, write_th2l_file, write_th2s_file, write_th3c_file, write_th3d_file,
        write_th3f_file, write_th3i_file, write_th3l_file, write_th3s_file, write_tprofile_file,
        Hist, Merge, TAxis, TProfile, ThreadedHist, TH1, TH2, TH3,
    };

    pub use oxiroot_rntuple::{
        write_rntuple_file, Column, Field, FieldValues, RNTuple, RNTupleWriter,
    };

    pub use oxiroot_tree::{write_tree_file, Branch, BranchValues, SplitMember, TTree};
}
