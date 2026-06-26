//! `TProfile2D` — a 2-D profile histogram. For each `(x, y)` cell it stores the
//! running sums needed to recover the mean (and spread) of a third quantity `z`:
//! `Σw` (`bin_entries`), `Σ(w·z)` (`sums`, the TH2 contents), and `Σ(w·z²)`
//! (`sumz2`, the TH2 `fSumw2`). On disk it is a `TH2D` plus the profile members,
//! exactly as `TProfile` is a `TH1D` plus profile members.

use oxiroot_io_core::buffer::RBuffer;
use oxiroot_io_core::error::{Error, Result};
use oxiroot_io_core::RFile;

use crate::axis::TAxis;
use crate::base::{cell_count, check_cells, object_bytes, read_tarray, read_th1_base, Precision};

/// A 2-D profile histogram (ROOT `TProfile2D`).
#[derive(Debug, Clone, PartialEq)]
pub struct TProfile2D {
    /// Histogram name (`fName`).
    pub name: String,
    /// Histogram title (`fTitle`).
    pub title: String,
    /// X axis.
    pub xaxis: TAxis,
    /// Y axis.
    pub yaxis: TAxis,
    /// Total cells, including flow (`fNcells = (nx+2)*(ny+2)`).
    pub ncells: i32,
    /// Number of entries (`fEntries`).
    pub entries: f64,
    /// Sum of weights (`fTsumw`).
    pub tsumw: f64,
    /// Sum of squared weights (`fTsumw2`).
    pub tsumw2: f64,
    /// Sum of `w·x` (`fTsumwx`).
    pub tsumwx: f64,
    /// Sum of `w·x²` (`fTsumwx2`).
    pub tsumwx2: f64,
    /// Sum of `w·y` (`fTsumwy`).
    pub tsumwy: f64,
    /// Sum of `w·y²` (`fTsumwy2`).
    pub tsumwy2: f64,
    /// Sum of `w·x·y` (`fTsumwxy`).
    pub tsumwxy: f64,
    /// Per-cell `Σ(w·z)` (the TH2 contents, `fArray`); length `ncells`.
    pub sums: Vec<f64>,
    /// Per-cell `Σ(w·z²)` (the TH2 `fSumw2`); length `ncells`.
    pub sumz2: Vec<f64>,
    /// Per-cell `Σw` (`fBinEntries`); length `ncells`.
    pub bin_entries: Vec<f64>,
    /// Error computation mode (`fErrorMode`: 0=mean, 1=spread, 2=spread-i, 3=spread-g).
    pub error_mode: i32,
    /// Lower `z` accept bound (`fZmin`; `0` = no restriction when `zmin == zmax`).
    pub zmin: f64,
    /// Upper `z` accept bound (`fZmax`).
    pub zmax: f64,
    /// Sum of `w·z` over in-range fills (`fTsumwz`).
    pub tsumwz: f64,
    /// Sum of `w·z²` over in-range fills (`fTsumwz2`).
    pub tsumwz2: f64,
    /// Per-cell `Σw²` (`fBinSumw2`); empty unless weighted-error tracking is on.
    pub bin_sumw2: Vec<f64>,
}

impl TProfile2D {
    /// Create an empty `TProfile2D` with uniform x and y bins and no z restriction.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        name: &str,
        title: &str,
        nx: i32,
        xlo: f64,
        xhi: f64,
        ny: i32,
        ylo: f64,
        yhi: f64,
    ) -> TProfile2D {
        let ncells = ((nx.max(0) + 2) * (ny.max(0) + 2)) as usize;
        TProfile2D {
            name: name.to_string(),
            title: title.to_string(),
            xaxis: TAxis::new("xaxis", nx, xlo, xhi),
            yaxis: TAxis::new("yaxis", ny, ylo, yhi),
            ncells: ncells as i32,
            entries: 0.0,
            tsumw: 0.0,
            tsumw2: 0.0,
            tsumwx: 0.0,
            tsumwx2: 0.0,
            tsumwy: 0.0,
            tsumwy2: 0.0,
            tsumwxy: 0.0,
            sums: vec![0.0; ncells],
            sumz2: vec![0.0; ncells],
            bin_entries: vec![0.0; ncells],
            error_mode: 0,
            zmin: 0.0,
            zmax: 0.0,
            tsumwz: 0.0,
            tsumwz2: 0.0,
            bin_sumw2: Vec::new(),
        }
    }

    /// Number of x bins (excluding flow).
    pub fn nx(&self) -> usize {
        self.xaxis.nbins.max(0) as usize
    }

    /// Number of y bins (excluding flow).
    pub fn ny(&self) -> usize {
        self.yaxis.nbins.max(0) as usize
    }

    /// Profile a point `(x, y, z)` with unit weight.
    pub fn fill(&mut self, x: f64, y: f64, z: f64) {
        self.fill_weight(x, y, z, 1.0);
    }

    /// Profile a point `(x, y, z)` with weight `w`, matching `TProfile2D::Fill`:
    /// accumulate the per-cell sums of `w·z` and `w·z²` and the per-cell weight,
    /// plus the moment sums (the latter only when both x and y are in range). A
    /// `z` range (`zmin != zmax`) rejects out-of-range points first.
    pub fn fill_weight(&mut self, x: f64, y: f64, z: f64, w: f64) {
        if self.zmin != self.zmax && (z < self.zmin || z > self.zmax || z.is_nan()) {
            return;
        }
        let stride = self.nx() + 2;
        let (bx, by) = (self.xaxis.find_bin(x), self.yaxis.find_bin(y));
        let cell = bx + stride * by;
        if let Some(s) = self.sums.get_mut(cell) {
            *s += w * z;
        }
        if let Some(s) = self.sumz2.get_mut(cell) {
            *s += w * z * z;
        }
        if let Some(e) = self.bin_entries.get_mut(cell) {
            *e += w;
        }
        self.entries += 1.0;

        let in_range = (1..=self.nx()).contains(&bx) && (1..=self.ny()).contains(&by);
        if in_range {
            self.tsumw += w;
            self.tsumw2 += w * w;
            self.tsumwx += w * x;
            self.tsumwx2 += w * x * x;
            self.tsumwy += w * y;
            self.tsumwy2 += w * y * y;
            self.tsumwxy += w * x * y;
            self.tsumwz += w * z;
            self.tsumwz2 += w * z * z;
        }
    }

    /// Profiled `z` value per in-range cell as `values[ix][iy]`: `Σ(w·z) / Σw`,
    /// or `0` where a cell has no entries. Matches ROOT/uproot.
    pub fn values(&self) -> Vec<Vec<f64>> {
        let (nx, ny) = (self.nx(), self.ny());
        let stride = nx + 2;
        (1..=nx)
            .map(|ix| {
                (1..=ny)
                    .map(|iy| {
                        let cell = ix + stride * iy;
                        let e = self.bin_entries.get(cell).copied().unwrap_or(0.0);
                        if e != 0.0 {
                            self.sums[cell] / e
                        } else {
                            0.0
                        }
                    })
                    .collect()
            })
            .collect()
    }

    /// Per-cell error of the profiled value, following ROOT's `GetBinError` for
    /// this profile's `fErrorMode` (see [`crate::TProfile::bin_error`] for the
    /// formula). `cell` is the global index (flow included).
    pub fn bin_error(&self, cell: usize) -> f64 {
        let sumw = self.bin_entries.get(cell).copied().unwrap_or(0.0);
        if sumw == 0.0 {
            return 0.0;
        }
        let sum = self.sums.get(cell).copied().unwrap_or(0.0);
        let sum2 = self.sumz2.get(cell).copied().unwrap_or(0.0);
        let sumw2 = self
            .bin_sumw2
            .get(cell)
            .copied()
            .filter(|&s| s > 0.0)
            .unwrap_or(sumw);
        let neff = if sumw2 > 0.0 {
            sumw * sumw / sumw2
        } else {
            0.0
        };
        let mean = sum / sumw;
        let var = (sum2 / sumw - mean * mean).abs();
        match self.error_mode {
            1 => var.sqrt(),
            3 => 1.0 / sumw.sqrt(),
            2 => {
                if var > 0.0 {
                    (var / neff).sqrt()
                } else if neff > 0.0 {
                    1.0 / (12.0 * neff).sqrt()
                } else {
                    0.0
                }
            }
            _ => {
                if neff > 0.0 {
                    (var / neff).sqrt()
                } else {
                    0.0
                }
            }
        }
    }

    pub(crate) fn read(r: &mut RBuffer) -> Result<TProfile2D> {
        let tp = r.read_version()?; // TProfile2D wrapper
        let _th2d = r.read_version()?; // TH2D wrapper
        let th2 = r.read_version()?; // TH2 wrapper (TH1 base + TH2 members)

        let c = read_th1_base(r)?;
        let _scalefactor = r.be_f64()?;
        let tsumwy = r.be_f64()?;
        let tsumwy2 = r.be_f64()?;
        let tsumwxy = r.be_f64()?;
        let end = th2
            .end
            .ok_or_else(|| Error::Format("TH2 record has no byte count".into()))?;
        r.seek(end)?;

        let sums = read_tarray(r, Precision::Double)?; // TH2D TArrayD = Σ(w·z)
        let bin_entries = read_tarray(r, Precision::Double)?;
        let error_mode = r.be_i32()?;
        let zmin = r.be_f64()?;
        let zmax = r.be_f64()?;
        let tsumwz = r.be_f64()?;
        let tsumwz2 = r.be_f64()?;
        let bin_sumw2 = read_tarray(r, Precision::Double)?;

        if let Some(end) = tp.end {
            r.seek(end)?;
        }

        let cells = cell_count(&[c.xaxis.nbins, c.yaxis.nbins])?;
        check_cells("TProfile2D sums", sums.len(), cells, false)?;
        check_cells("TProfile2D fBinEntries", bin_entries.len(), cells, false)?;
        check_cells("TProfile2D fSumw2", c.sumw2.len(), cells, true)?;
        check_cells("TProfile2D fBinSumw2", bin_sumw2.len(), cells, true)?;

        Ok(TProfile2D {
            name: c.name,
            title: c.title,
            xaxis: c.xaxis,
            yaxis: c.yaxis,
            ncells: c.ncells,
            entries: c.entries,
            tsumw: c.tsumw,
            tsumw2: c.tsumw2,
            tsumwx: c.tsumwx,
            tsumwx2: c.tsumwx2,
            tsumwy,
            tsumwy2,
            tsumwxy,
            sums,
            sumz2: c.sumw2,
            bin_entries,
            error_mode,
            zmin,
            zmax,
            tsumwz,
            tsumwz2,
            bin_sumw2,
        })
    }
}

/// Read a `TProfile2D` named `name` from `file`.
pub fn read_tprofile2d(file: &RFile, name: &str) -> Result<TProfile2D> {
    let object = object_bytes(file, name, "TProfile2D")?;
    TProfile2D::read(&mut RBuffer::new(&object))
}
