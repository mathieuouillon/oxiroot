//! `TProfile` — a 1-D profile histogram.
//!
//! Streamed layout: `TProfile{ TH1D{ … }, fBinEntries(TArrayD), fErrorMode,
//! fYmin, fYmax, fTsumwy, fTsumwy2, fBinSumw2(TArrayD) }`. The `TH1D` base's
//! bin contents are the per-bin sums of y; `fBinEntries` is the per-bin count.
//! The profiled value of a bin is `sum / entries`.

use oxiroot_io_core::buffer::RBuffer;
use oxiroot_io_core::error::Result;
use oxiroot_io_core::RFile;

use crate::axis::TAxis;
use crate::base::{
    cell_count, check_cells, object_bytes, object_bytes_in, read_tarray, read_th1_object, Precision,
};

/// How a profile's per-bin error bar is computed (ROOT's `fErrorMode`). Shared
/// by [`TProfile`], [`crate::TProfile2D`], and [`crate::TProfile3D`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[non_exhaustive]
pub enum ErrorMode {
    /// `kERRORMEAN` (0): error on the mean, `spread / sqrt(N_eff)` (the default).
    #[default]
    Mean,
    /// `kERRORSPREAD` (1): the RMS spread itself.
    Spread,
    /// `kERRORSPREADI` (2): like `Mean`, but an empty spread uses the
    /// uniform-bin estimate `1 / sqrt(12 N_eff)`.
    SpreadI,
    /// `kERRORSPREADG` (3): the weighted error `1 / sqrt(Σw)`.
    SpreadG,
}

impl ErrorMode {
    /// Map a ROOT `fErrorMode` code; an unrecognized code falls back to
    /// [`Mean`](ErrorMode::Mean) (ROOT only ever writes `0..=3`).
    #[must_use]
    pub fn from_code(code: i32) -> ErrorMode {
        match code {
            1 => ErrorMode::Spread,
            2 => ErrorMode::SpreadI,
            3 => ErrorMode::SpreadG,
            _ => ErrorMode::Mean,
        }
    }

    /// The ROOT `fErrorMode` code written to disk.
    #[must_use]
    pub fn to_code(self) -> i32 {
        match self {
            ErrorMode::Mean => 0,
            ErrorMode::Spread => 1,
            ErrorMode::SpreadI => 2,
            ErrorMode::SpreadG => 3,
        }
    }
}

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
    /// Sum of weights (`fTsumw`).
    pub tsumw: f64,
    /// Sum of weight^2 (`fTsumw2`).
    pub tsumw2: f64,
    /// Sum of weight*x (`fTsumwx`).
    pub tsumwx: f64,
    /// Sum of weight*x^2 (`fTsumwx2`).
    pub tsumwx2: f64,
    /// Per-bin sums of weight*y (the `TH1D` base contents, length `ncells`).
    pub sums: Vec<f64>,
    /// Per-bin sums of weight*y^2 (the `TH1` base `fSumw2`, length `ncells`).
    pub sumy2: Vec<f64>,
    /// Per-bin entry counts / sums of weight (`fBinEntries`, length `ncells`).
    pub bin_entries: Vec<f64>,
    /// Error computation mode (`fErrorMode`).
    pub error_mode: ErrorMode,
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
        let error_mode = ErrorMode::from_code(r.be_i32()?);
        let ymin = r.be_f64()?;
        let ymax = r.be_f64()?;
        let tsumwy = r.be_f64()?;
        let tsumwy2 = r.be_f64()?;
        let bin_sumw2 = read_tarray(r, Precision::Double)?;

        if let Some(end) = tprofile.end {
            r.seek(end)?;
        }

        let cells = cell_count(&[core.xaxis.nbins])?;
        check_cells("TProfile sums", sums.len(), cells, false)?;
        check_cells("TProfile fBinEntries", bin_entries.len(), cells, false)?;
        check_cells("TProfile fSumw2", core.sumw2.len(), cells, true)?;
        check_cells("TProfile fBinSumw2", bin_sumw2.len(), cells, true)?;

        Ok(TProfile {
            name: core.name,
            title: core.title,
            xaxis: core.xaxis,
            ncells: core.ncells,
            entries: core.entries,
            tsumw: core.tsumw,
            tsumw2: core.tsumw2,
            tsumwx: core.tsumwx,
            tsumwx2: core.tsumwx2,
            sums,
            sumy2: core.sumw2,
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

    /// Effective number of entries in a bin, `(Σw)² / Σw²` (equal to the entry
    /// count for unweighted fills). `bin` includes flow.
    pub fn effective_entries(&self, bin: usize) -> f64 {
        let sumw = self.bin_entries.get(bin).copied().unwrap_or(0.0);
        let sumw2 = self
            .bin_sumw2
            .get(bin)
            .copied()
            .filter(|&s| s > 0.0)
            .unwrap_or(sumw);
        if sumw2 > 0.0 {
            sumw * sumw / sumw2
        } else {
            0.0
        }
    }

    /// Per-bin error of the profiled value, following ROOT's
    /// `TProfile::GetBinError` for this profile's `fErrorMode`. `bin` includes
    /// flow (0 = underflow).
    ///
    /// With weight sum `sumw = Σw`, profiled value `mean = Σwy / Σw`, spread
    /// `v = |Σwy²/Σw − mean²|`, and effective entries
    /// `neff = (Σw)² / Σw²`:
    /// - mean (mode 0, the default): `sqrt(v / neff)` — the error on the mean;
    /// - spread (mode 1): `sqrt(v)` — the RMS spread itself;
    /// - spread-i (mode 2): `sqrt(v / neff)`, or `1/sqrt(12·neff)` when `v == 0`;
    /// - spread-g (mode 3): `1/sqrt(Σw)`.
    pub fn bin_error(&self, bin: usize) -> f64 {
        let sumw = self.bin_entries.get(bin).copied().unwrap_or(0.0);
        if sumw == 0.0 {
            return 0.0;
        }
        let sum = self.sums.get(bin).copied().unwrap_or(0.0);
        let sumy2 = self.sumy2.get(bin).copied().unwrap_or(0.0);
        let mean = sum / sumw;
        let variance = (sumy2 / sumw - mean * mean).abs();
        let neff = self.effective_entries(bin);
        match self.error_mode {
            // The RMS spread itself.
            ErrorMode::Spread => variance.sqrt(),
            // SpreadI with an empty spread: the uniform-bin estimate.
            ErrorMode::SpreadI if variance <= 0.0 => {
                if neff > 0.0 {
                    1.0 / (12.0 * neff).sqrt()
                } else {
                    0.0
                }
            }
            // The weighted error 1/sqrt(Σw).
            ErrorMode::SpreadG => 1.0 / sumw.sqrt(),
            // Mean (default) and SpreadI with a non-zero spread.
            _ => {
                if neff > 0.0 {
                    (variance / neff).sqrt()
                } else {
                    0.0
                }
            }
        }
    }

    /// Create an empty `TProfile` with `nbins` uniform x bins over `[xlo, xhi)`
    /// and no y restriction. Internal primitive behind the public builder:
    /// [`Hist::reg`](crate::Hist::reg)`(nbins, xlo, xhi).profile()`.
    pub(crate) fn new(nbins: i32, xlo: f64, xhi: f64) -> TProfile {
        let ncells = (nbins.max(0) + 2) as usize;
        TProfile {
            name: String::new(),
            title: String::new(),
            xaxis: TAxis::new("xaxis", nbins, xlo, xhi),
            ncells: ncells as i32,
            entries: 0.0,
            tsumw: 0.0,
            tsumw2: 0.0,
            tsumwx: 0.0,
            tsumwx2: 0.0,
            sums: vec![0.0; ncells],
            sumy2: vec![0.0; ncells],
            bin_entries: vec![0.0; ncells],
            error_mode: ErrorMode::Mean,
            ymin: 0.0,
            ymax: 0.0,
            tsumwy: 0.0,
            tsumwy2: 0.0,
            bin_sumw2: Vec::new(),
        }
    }

    /// Profile a point `(x, y)` with unit weight.
    pub fn fill(&mut self, x: f64, y: f64) {
        self.fill_weight(x, y, 1.0);
    }

    /// Profile a point `(x, y)` with weight `w`, matching ROOT's `TProfile::Fill`:
    /// accumulate the per-bin sums of `w*y` and `w*y^2` and the per-bin weight,
    /// plus the x/y moment sums (the latter only when x is in range). A `y` range
    /// (`ymin != ymax`) rejects out-of-range points before they are counted.
    pub fn fill_weight(&mut self, x: f64, y: f64, w: f64) {
        if self.ymin != self.ymax && (y < self.ymin || y > self.ymax || y.is_nan()) {
            return;
        }
        let nbins = self.xaxis.nbins.max(0) as usize;
        let bin = self.xaxis.find_bin(x);
        if let Some(s) = self.sums.get_mut(bin) {
            *s += w * y;
        }
        if let Some(s) = self.sumy2.get_mut(bin) {
            *s += w * y * y;
        }
        if let Some(e) = self.bin_entries.get_mut(bin) {
            *e += w;
        }
        self.entries += 1.0;

        if (1..=nbins).contains(&bin) {
            self.tsumw += w;
            self.tsumw2 += w * w;
            self.tsumwx += w * x;
            self.tsumwx2 += w * x * x;
            self.tsumwy += w * y;
            self.tsumwy2 += w * y * y;
        }
    }
}

/// Read a `TProfile` from an open ROOT file.
pub(crate) fn read_tprofile(file: &RFile, name: &str) -> Result<TProfile> {
    TProfile::read(&mut RBuffer::new(&object_bytes(file, name, "TProfile")?))
}

/// Read a `TProfile` from subdirectory `subdir`.
pub(crate) fn read_tprofile_in(file: &RFile, subdir: &str, name: &str) -> Result<TProfile> {
    TProfile::read(&mut RBuffer::new(&object_bytes_in(
        file, subdir, name, "TProfile",
    )?))
}
