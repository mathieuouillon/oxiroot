//! `TProfile` — a 1-D profile histogram.
//!
//! Streamed layout: `TProfile{ TH1D{ … }, fBinEntries(TArrayD), fErrorMode,
//! fYmin, fYmax, fTsumwy, fTsumwy2, fBinSumw2(TArrayD) }`. The `TH1D` base's
//! bin contents are the per-bin sums of y; `fBinEntries` is the per-bin count.
//! The profiled value of a bin is `sum / entries`.

use root_io_core::buffer::RBuffer;
use root_io_core::error::Result;
use root_io_core::RFile;

use crate::axis::TAxis;
use crate::base::{object_bytes, read_tarray, read_th1_object, Precision};

/// A 1-D profile histogram (`TProfile`).
#[derive(Debug, Clone, PartialEq)]
pub struct TProfile {
    /// Histogram name (`fName`).
    pub name: String,
    /// Histogram title (`fTitle`).
    pub title: String,
    /// X axis.
    pub xaxis: TAxis,
    /// Total cells, including flow (`fNcells = nbins + 2`).
    pub ncells: i32,
    /// Number of entries (`fEntries`).
    pub entries: f64,
    /// Per-bin sums of y (the `TH1D` base contents, length `ncells`).
    pub sums: Vec<f64>,
    /// Per-bin entry counts (`fBinEntries`, length `ncells`).
    pub bin_entries: Vec<f64>,
    /// Error computation mode (`fErrorMode`).
    pub error_mode: i32,
    /// Lower y limit (`fYmin`).
    pub ymin: f64,
    /// Upper y limit (`fYmax`).
    pub ymax: f64,
    /// Sum of weight*y (`fTsumwy`).
    pub tsumwy: f64,
    /// Sum of weight*y^2 (`fTsumwy2`).
    pub tsumwy2: f64,
    /// Per-bin sum of squared weights (`fBinSumw2`), possibly empty.
    pub bin_sumw2: Vec<f64>,
}

impl TProfile {
    pub(crate) fn read(r: &mut RBuffer) -> Result<TProfile> {
        let tprofile = r.read_version()?; // TProfile wrapper

        // The TH1D base: its own wrapper, the TH1 base, and the TArrayD sums.
        let (core, sums) = read_th1_object(r, Precision::Double)?;

        let bin_entries = read_tarray(r, Precision::Double)?;
        let error_mode = r.be_i32()?;
        let ymin = r.be_f64()?;
        let ymax = r.be_f64()?;
        let tsumwy = r.be_f64()?;
        let tsumwy2 = r.be_f64()?;
        let bin_sumw2 = read_tarray(r, Precision::Double)?;

        if let Some(end) = tprofile.end {
            r.seek(end)?;
        }

        Ok(TProfile {
            name: core.name,
            title: core.title,
            xaxis: core.xaxis,
            ncells: core.ncells,
            entries: core.entries,
            sums,
            bin_entries,
            error_mode,
            ymin,
            ymax,
            tsumwy,
            tsumwy2,
            bin_sumw2,
        })
    }

    /// The profiled value per bin (excluding flow): `sum / entries`, or 0 where
    /// a bin has no entries. Matches ROOT/uproot `TProfile::values()`.
    pub fn values(&self) -> Vec<f64> {
        let n = self.sums.len();
        if n < 2 {
            return Vec::new();
        }
        (1..n - 1)
            .map(|i| {
                let entries = self.bin_entries.get(i).copied().unwrap_or(0.0);
                if entries != 0.0 {
                    self.sums[i] / entries
                } else {
                    0.0
                }
            })
            .collect()
    }

    /// The X-axis bin edges (`nbins + 1` values).
    pub fn edges(&self) -> Vec<f64> {
        self.xaxis.edges()
    }
}

/// Read a `TProfile` from an open ROOT file.
pub fn read_tprofile(file: &RFile, name: &str) -> Result<TProfile> {
    let object = object_bytes(file, name, "TProfile")?;
    TProfile::read(&mut RBuffer::new(&object))
}
