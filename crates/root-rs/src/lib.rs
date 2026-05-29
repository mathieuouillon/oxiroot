//! `root-rs`: pure-Rust IO for the CERN ROOT file format.
//!
//! Read and write [RNTuple](root_rntuple) (ROOT's columnar event-data format)
//! and classic [histograms](root_hist) — `TH1`/`TH2`/`TH3`/`TProfile` — in the
//! ROOT (`TFile`) container, with no C++/libROOT dependency. Files written here
//! are read by official ROOT and uproot, and vice versa.
//!
//! # Quick start
//!
//! ```no_run
//! use root_rs::prelude::*;
//!
//! // Fill and save a histogram.
//! let mut h = TH1::new("pt", "transverse momentum", 50, 0.0, 100.0);
//! h.sumw2();
//! h.fill_weight(42.0, 1.5);
//! write_th1d_file("out.root".as_ref(), &h, Compression::Zstd(5))?;
//!
//! // Write a columnar dataset, then read it back.
//! let fields = vec![Field::f64("mass", vec![91.2, 125.0])];
//! write_rntuple_file("data.root".as_ref(), "events", &fields, Compression::None)?;
//! let f = RFile::open("data.root")?;
//! let ntpl = RNTuple::open(&f, "events")?;
//! assert_eq!(ntpl.num_entries(), 2);
//! # Ok::<(), root_rs::Error>(())
//! ```
//!
//! The flat [`prelude`] covers the common read/write surface; the [`hist`],
//! [`ntuple`], [`compress`], and [`file`] modules expose everything else.

#[doc(inline)]
pub use root_io_core::{buffer, error, file, Compression, Error, RFile, Result};

/// ROOT compression framing and codecs (re-exported from `root-compress`).
pub mod compress {
    pub use root_compress::*;
}

/// Classic ROOT histograms — `TH1`/`TH2`/`TH3`/`TProfile` (from `root-hist`).
pub mod hist {
    pub use root_hist::*;
}

/// RNTuple, ROOT's columnar event-data format (from `root-rntuple`).
pub mod ntuple {
    pub use root_rntuple::*;
}

/// The common types and functions for reading and writing ROOT files.
///
/// `use root_rs::prelude::*;` brings in the container ([`RFile`],
/// [`Compression`]), the histogram types with their `read_*`/`write_*` helpers,
/// and the RNTuple reader/writer.
pub mod prelude {
    pub use root_io_core::{Compression, Error, RFile, Result};

    pub use root_hist::{
        append_histograms_file, read_th1, read_th1d, read_th1d_in, read_th1f, read_th2, read_th2d,
        read_th2f, read_th3, read_th3d, read_th3f, read_tprofile, write_histograms_dirs,
        write_histograms_file, write_th1d_file, write_th2d_file, write_th3d_file,
        write_tprofile_file, Hist, TAxis, TProfile, TH1, TH2, TH3,
    };

    pub use root_rntuple::{
        write_rntuple_file, Column, Field, FieldValues, RNTuple, RNTupleWriter,
    };
}
