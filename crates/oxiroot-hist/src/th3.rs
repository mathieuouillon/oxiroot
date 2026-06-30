//! 3-D histograms (`TH3D`, `TH3F`).
//!
//! Streamed layout: `TH3x{ TH3{ TH1{ … }, TAtt3D, fTsumwy, fTsumwy2, fTsumwxy,
//! fTsumwz, fTsumwz2, fTsumwxz, fTsumwyz }, TArray }`. `TAtt3D` is an empty base
//! (skipped via its byte count); the inline `TArray` holds the
//! `(nx+2)*(ny+2)*(nz+2)` cells with x fastest, then y, then z.

use oxiroot_io_core::buffer::RBuffer;
use oxiroot_io_core::error::{Error, Result};
use oxiroot_io_core::streamer::skip_versioned;
use oxiroot_io_core::RFile;

use crate::axis::TAxis;
use crate::base::{
    cell_count, check_cells, histogram_object, histogram_object_in, precision_of, read_tarray,
    read_th1_base, Precision,
};

/// A 3-D classic histogram (`TH3D` or `TH3F`); contents are widened to `f64`.
#[derive(Debug, Clone, PartialEq)]
pub struct TH3 {
    /// On-disk [`Precision`] (the class suffix); read the class name via
    /// [`class_name`](TH3::class_name).
    pub(crate) precision: Precision,
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
    /// [`ncells`](TH3::ncells); `pub(crate)` so it cannot drift from `contents`.
    pub(crate) ncells: i32,
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
    /// Sum of weight*y (`fTsumwy`).
    pub tsumwy: f64,
    /// Sum of weight*y^2 (`fTsumwy2`).
    pub tsumwy2: f64,
    /// Sum of weight*x*y (`fTsumwxy`).
    pub tsumwxy: f64,
    /// Sum of weight*z (`fTsumwz`).
    pub tsumwz: f64,
    /// Sum of weight*z^2 (`fTsumwz2`).
    pub tsumwz2: f64,
    /// Sum of weight*x*z (`fTsumwxz`).
    pub tsumwxz: f64,
    /// Sum of weight*y*z (`fTsumwyz`).
    pub tsumwyz: f64,
    /// Bin contents including flow (length `ncells`, x fastest then y then z).
    pub contents: Vec<f64>,
    /// Per-bin sum of squared weights (`fSumw2`); empty unless enabled via
    /// [`TH3::sumw2`].
    pub sumw2: Vec<f64>,
}

impl TH3 {
    pub(crate) fn read(r: &mut RBuffer, precision: Precision) -> Result<TH3> {
        let _th3x = r.read_version()?; // TH3x wrapper
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
        let contents = read_tarray(r, precision)?;

        let cells = cell_count(&[c.xaxis.nbins, c.yaxis.nbins, c.zaxis.nbins])?;
        check_cells("TH3 contents", contents.len(), cells, false)?;
        check_cells("TH3 fSumw2", c.sumw2.len(), cells, true)?;

        Ok(TH3 {
            precision,
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
            contents,
            sumw2: c.sumw2,
        })
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

    /// Bin contents excluding flow as `values[ix][iy][iz]`, matching uproot's
    /// `values(flow=False)`. Cell `(ix, iy, iz)` is stored at
    /// `ix + (nx+2)*(iy + (ny+2)*iz)` (indices include the underflow bin at 0).
    pub fn values(&self) -> Vec<Vec<Vec<f64>>> {
        let (nx, ny, nz) = (self.nx(), self.ny(), self.nz());
        let (sx, sy) = (nx + 2, ny + 2);
        (1..=nx)
            .map(|ix| {
                (1..=ny)
                    .map(|iy| {
                        (1..=nz)
                            .map(|iz| self.contents[ix + sx * (iy + sy * iz)])
                            .collect()
                    })
                    .collect()
            })
            .collect()
    }

    /// Create an empty `TH3D` with uniform axes. Mirrors ROOT's `TH3D`
    /// constructor: `nx` bins over `[xlo, xhi)`, etc.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        nx: i32,
        xlo: f64,
        xhi: f64,
        ny: i32,
        ylo: f64,
        yhi: f64,
        nz: i32,
        zlo: f64,
        zhi: f64,
    ) -> TH3 {
        let ncells = (nx.max(0) + 2) * (ny.max(0) + 2) * (nz.max(0) + 2);
        TH3 {
            precision: Precision::Double,
            name: String::new(),
            title: String::new(),
            xaxis: TAxis::new("xaxis", nx, xlo, xhi),
            yaxis: TAxis::new("yaxis", ny, ylo, yhi),
            zaxis: TAxis::new("zaxis", nz, zlo, zhi),
            ncells,
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
            contents: vec![0.0; ncells.max(0) as usize],
            sumw2: Vec::new(),
        }
    }

    /// Enable per-bin error tracking (ROOT's `Sumw2`); see [`crate::TH1::sumw2`].
    /// Returns `&mut self` so it can chain.
    pub fn sumw2(&mut self) -> &mut Self {
        if self.sumw2.len() != self.contents.len() {
            self.sumw2 = self.contents.iter().map(|c| c.abs()).collect();
        }
        self
    }

    /// Total cells including the flow bins (`fNcells`), derived from `contents`.
    #[must_use]
    pub fn ncells(&self) -> i32 {
        self.contents.len() as i32
    }

    /// The exact ROOT class name (`"TH3D"`/`"TH3F"`/…), derived from the stored
    /// [`precision`](TH3::precision).
    #[must_use]
    pub fn class_name(&self) -> String {
        self.precision.class_name("TH3")
    }

    /// This histogram's on-disk [`Precision`] (the class suffix);
    /// [`Precision::Double`] by default. See [`crate::TH1::precision`].
    #[must_use]
    pub fn precision(&self) -> Precision {
        self.precision
    }

    /// Set the on-disk precision (e.g. `Precision::Float` writes a `TH3F`).
    #[must_use]
    pub fn with_precision(mut self, precision: Precision) -> Self {
        self.precision = precision;
        self
    }

    /// Per-bin error: `sqrt(sumw2[bin])` when error tracking is on, else
    /// `sqrt(content)`. `bin` is the global cell index (x fastest, then y, z).
    pub fn bin_error(&self, bin: usize) -> f64 {
        if let Some(&s) = self.sumw2.get(bin) {
            s.max(0.0).sqrt()
        } else {
            self.contents
                .get(bin)
                .copied()
                .unwrap_or(0.0)
                .max(0.0)
                .sqrt()
        }
    }

    /// Fill `(x, y, z)` with unit weight.
    pub fn fill(&mut self, x: f64, y: f64, z: f64) {
        self.fill_weight(x, y, z, 1.0);
    }

    /// Fill `(x, y, z)` with weight `w`, matching ROOT's `TH3::Fill`: every fill
    /// counts toward `fEntries`, the cell (including flow) is incremented, and
    /// the moment sums accumulate only when all three coordinates are in range.
    pub fn fill_weight(&mut self, x: f64, y: f64, z: f64, w: f64) {
        let (nx, ny, nz) = (self.nx(), self.ny(), self.nz());
        let binx = self.xaxis.find_bin(x);
        let biny = self.yaxis.find_bin(y);
        let binz = self.zaxis.find_bin(z);
        let bin = binx + (nx + 2) * (biny + (ny + 2) * binz);
        if let Some(c) = self.contents.get_mut(bin) {
            *c += w;
        }
        if let Some(s) = self.sumw2.get_mut(bin) {
            *s += w * w;
        }
        self.entries += 1.0;

        let in_range =
            (1..=nx).contains(&binx) && (1..=ny).contains(&biny) && (1..=nz).contains(&binz);
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
        }
    }

    /// Mean of the x projection (`fTsumwx / fTsumw`), 0 when empty.
    pub fn mean_x(&self) -> f64 {
        if self.tsumw == 0.0 {
            0.0
        } else {
            self.tsumwx / self.tsumw
        }
    }

    /// Mean of the y projection (`fTsumwy / fTsumw`), 0 when empty.
    pub fn mean_y(&self) -> f64 {
        if self.tsumw == 0.0 {
            0.0
        } else {
            self.tsumwy / self.tsumw
        }
    }

    /// Mean of the z projection (`fTsumwz / fTsumw`), 0 when empty.
    pub fn mean_z(&self) -> f64 {
        if self.tsumw == 0.0 {
            0.0
        } else {
            self.tsumwz / self.tsumw
        }
    }
}

/// Read any 3-D histogram (`TH3D/F/I/S/C/L`), detecting the precision from the
/// stored class.
pub(crate) fn read_th3(file: &RFile, name: &str) -> Result<TH3> {
    decode_th3(histogram_object(file, name, "TH3")?)
}

/// Read any 3-D histogram from subdirectory `subdir`.
pub(crate) fn read_th3_in(file: &RFile, subdir: &str, name: &str) -> Result<TH3> {
    decode_th3(histogram_object_in(file, subdir, name, "TH3")?)
}

pub(crate) fn decode_th3((class, object): (String, Vec<u8>)) -> Result<TH3> {
    TH3::read(&mut RBuffer::new(&object), precision_of(&class)?)
}
