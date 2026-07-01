//! `TProfile3D` — a 3-D profile histogram. For each `(x, y, z)` cell it stores
//! the running sums to recover the mean of a fourth quantity `t`: `Σw`
//! (`bin_entries`), `Σ(w·t)` (`sums`, the TH3 contents), and `Σ(w·t²)` (`sumt2`,
//! the TH3 `fSumw2`). On disk it is a `TH3D` plus the profile members, exactly as
//! `TProfile`/`TProfile2D` extend `TH1D`/`TH2D`.

use oxiroot_io_core::buffer::RBuffer;
use oxiroot_io_core::error::{Error, Result};
use oxiroot_io_core::streamer::skip_versioned;
use oxiroot_io_core::RFile;

use crate::axis::TAxis;
use crate::base::{
    cell_count, check_cells, object_bytes, object_bytes_in, read_tarray, read_th1_base, Precision,
};
use crate::tprofile::ErrorMode;

/// A 3-D profile histogram (ROOT `TProfile3D`).
#[derive(Debug, Clone, PartialEq)]
pub struct TProfile3D {
    /// Histogram name (`fName`).
    pub name: String,
    /// Histogram title (`fTitle`).
    pub title: String,
    /// X axis.
    pub xaxis: TAxis,
    /// Y axis.
    pub yaxis: TAxis,
    /// Z axis.
    pub zaxis: TAxis,
    /// Total cells, including flow (`fNcells = (nx+2)*(ny+2)*(nz+2)`). Read via
    /// [`ncells`](TProfile3D::ncells); `pub(crate)` so it cannot drift from the
    /// per-bin vectors.
    pub(crate) ncells: i32,
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
    /// Sum of `w·z` (`fTsumwz`).
    pub tsumwz: f64,
    /// Sum of `w·z²` (`fTsumwz2`).
    pub tsumwz2: f64,
    /// Sum of `w·x·z` (`fTsumwxz`).
    pub tsumwxz: f64,
    /// Sum of `w·y·z` (`fTsumwyz`).
    pub tsumwyz: f64,
    /// Per-cell `Σ(w·t)` (the TH3 contents, `fArray`); length `ncells`.
    pub sums: Vec<f64>,
    /// Per-cell `Σ(w·t²)` (the TH3 `fSumw2`); length `ncells`.
    pub sumt2: Vec<f64>,
    /// Per-cell `Σw` (`fBinEntries`); length `ncells`.
    pub bin_entries: Vec<f64>,
    /// Error computation mode (`fErrorMode`).
    pub error_mode: ErrorMode,
    /// Lower `t` accept bound (`fTmin`; `0` = no restriction when `tmin == tmax`).
    pub tmin: f64,
    /// Upper `t` accept bound (`fTmax`).
    pub tmax: f64,
    /// Sum of `w·t` over in-range fills (`fTsumwt`).
    pub tsumwt: f64,
    /// Sum of `w·t²` over in-range fills (`fTsumwt2`).
    pub tsumwt2: f64,
    /// Per-cell `Σw²` (`fBinSumw2`); empty unless weighted-error tracking is on.
    pub bin_sumw2: Vec<f64>,
}

impl TProfile3D {
    /// Total cells including the flow bins (`fNcells`), derived from the per-bin
    /// vectors so it cannot disagree with them.
    #[must_use]
    pub fn ncells(&self) -> i32 {
        self.bin_entries.len() as i32
    }

    /// Create an empty `TProfile3D` with uniform x/y/z bins and no t
    /// restriction. Internal primitive behind the public builder:
    /// [`Hist::reg`](crate::Hist::reg)`(nx, ..).reg(ny, ..).reg(nz, ..).profile()`.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        nx: i32,
        xlo: f64,
        xhi: f64,
        ny: i32,
        ylo: f64,
        yhi: f64,
        nz: i32,
        zlo: f64,
        zhi: f64,
    ) -> TProfile3D {
        let ncells = ((nx.max(0) + 2) * (ny.max(0) + 2) * (nz.max(0) + 2)) as usize;
        TProfile3D {
            name: String::new(),
            title: String::new(),
            xaxis: TAxis::new("xaxis", nx, xlo, xhi),
            yaxis: TAxis::new("yaxis", ny, ylo, yhi),
            zaxis: TAxis::new("zaxis", nz, zlo, zhi),
            ncells: ncells as i32,
            entries: 0.0,
            tsumw: 0.0,
            tsumw2: 0.0,
            tsumwx: 0.0,
            tsumwx2: 0.0,
            tsumwy: 0.0,
            tsumwy2: 0.0,
            tsumwxy: 0.0,
            tsumwz: 0.0,
            tsumwz2: 0.0,
            tsumwxz: 0.0,
            tsumwyz: 0.0,
            sums: vec![0.0; ncells],
            sumt2: vec![0.0; ncells],
            bin_entries: vec![0.0; ncells],
            error_mode: ErrorMode::Mean,
            tmin: 0.0,
            tmax: 0.0,
            tsumwt: 0.0,
            tsumwt2: 0.0,
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
    /// Number of z bins (excluding flow).
    pub fn nz(&self) -> usize {
        self.zaxis.nbins.max(0) as usize
    }

    /// Profile a point `(x, y, z, t)` with unit weight.
    pub fn fill(&mut self, x: f64, y: f64, z: f64, t: f64) {
        self.fill_weight(x, y, z, t, 1.0);
    }

    /// Profile a point `(x, y, z, t)` with weight `w`, matching `TProfile3D::Fill`.
    pub fn fill_weight(&mut self, x: f64, y: f64, z: f64, t: f64, w: f64) {
        if self.tmin != self.tmax && (t < self.tmin || t > self.tmax || t.is_nan()) {
            return;
        }
        let (sx, sy) = (self.nx() + 2, self.ny() + 2);
        let (bx, by, bz) = (
            self.xaxis.find_bin(x),
            self.yaxis.find_bin(y),
            self.zaxis.find_bin(z),
        );
        let cell = bx + sx * (by + sy * bz);
        if let Some(s) = self.sums.get_mut(cell) {
            *s += w * t;
        }
        if let Some(s) = self.sumt2.get_mut(cell) {
            *s += w * t * t;
        }
        if let Some(e) = self.bin_entries.get_mut(cell) {
            *e += w;
        }
        self.entries += 1.0;

        let in_range = (1..=self.nx()).contains(&bx)
            && (1..=self.ny()).contains(&by)
            && (1..=self.nz()).contains(&bz);
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
            self.tsumwxz += w * x * z;
            self.tsumwyz += w * y * z;
            self.tsumwt += w * t;
            self.tsumwt2 += w * t * t;
        }
    }

    /// Profiled `t` value per in-range cell as `values[ix][iy][iz]`:
    /// `Σ(w·t) / Σw`, or `0` where a cell has no entries.
    pub fn values(&self) -> Vec<Vec<Vec<f64>>> {
        let (nx, ny, nz) = (self.nx(), self.ny(), self.nz());
        let (sx, sy) = (nx + 2, ny + 2);
        (1..=nx)
            .map(|ix| {
                (1..=ny)
                    .map(|iy| {
                        (1..=nz)
                            .map(|iz| {
                                let cell = ix + sx * (iy + sy * iz);
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
            })
            .collect()
    }

    pub(crate) fn read(r: &mut RBuffer) -> Result<TProfile3D> {
        let tp = r.read_version()?; // TProfile3D wrapper
        let _th3d = r.read_version()?; // TH3D wrapper
        let th3 = r.read_version()?; // TH3 wrapper

        let c = read_th1_base(r)?;
        skip_versioned(r)?; // TAtt3D base (empty)
        let tsumwy = r.be_f64()?;
        let tsumwy2 = r.be_f64()?;
        let tsumwxy = r.be_f64()?;
        let tsumwz = r.be_f64()?;
        let tsumwz2 = r.be_f64()?;
        let tsumwxz = r.be_f64()?;
        let tsumwyz = r.be_f64()?;
        let end = th3
            .end
            .ok_or_else(|| Error::Format("TH3 record has no byte count".into()))?;
        r.seek(end)?;

        let sums = read_tarray(r, Precision::Double)?; // TH3D TArrayD = Σ(w·t)
        let bin_entries = read_tarray(r, Precision::Double)?;
        let error_mode = ErrorMode::from_code(r.be_i32()?);
        let tmin = r.be_f64()?;
        let tmax = r.be_f64()?;
        let tsumwt = r.be_f64()?;
        let tsumwt2 = r.be_f64()?;
        let bin_sumw2 = read_tarray(r, Precision::Double)?;

        if let Some(end) = tp.end {
            r.seek(end)?;
        }

        let cells = cell_count(&[c.xaxis.nbins, c.yaxis.nbins, c.zaxis.nbins])?;
        check_cells("TProfile3D sums", sums.len(), cells, false)?;
        check_cells("TProfile3D fBinEntries", bin_entries.len(), cells, false)?;
        check_cells("TProfile3D fSumw2", c.sumw2.len(), cells, true)?;
        check_cells("TProfile3D fBinSumw2", bin_sumw2.len(), cells, true)?;

        Ok(TProfile3D {
            name: c.name,
            title: c.title,
            xaxis: c.xaxis,
            yaxis: c.yaxis,
            zaxis: c.zaxis,
            ncells: c.ncells,
            entries: c.entries,
            tsumw: c.tsumw,
            tsumw2: c.tsumw2,
            tsumwx: c.tsumwx,
            tsumwx2: c.tsumwx2,
            tsumwy,
            tsumwy2,
            tsumwxy,
            tsumwz,
            tsumwz2,
            tsumwxz,
            tsumwyz,
            sums,
            sumt2: c.sumw2,
            bin_entries,
            error_mode,
            tmin,
            tmax,
            tsumwt,
            tsumwt2,
            bin_sumw2,
        })
    }
}

/// Read a `TProfile3D` named `name` from `file`.
pub(crate) fn read_tprofile3d(file: &RFile, name: &str) -> Result<TProfile3D> {
    TProfile3D::read(&mut RBuffer::new(&object_bytes(file, name, "TProfile3D")?))
}

/// Read a `TProfile3D` from subdirectory `subdir`.
pub(crate) fn read_tprofile3d_in(file: &RFile, subdir: &str, name: &str) -> Result<TProfile3D> {
    TProfile3D::read(&mut RBuffer::new(&object_bytes_in(
        file,
        subdir,
        name,
        "TProfile3D",
    )?))
}
