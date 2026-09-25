//! 2-D histograms (`TH2D`, `TH2F`).
//!
//! Streamed layout: `TH2x{ Hist2D{ Hist1D{ … }, fScalefactor, fTsumwy, fTsumwy2,
//! fTsumwxy }, TArray }`. The inline `TArray` holds the `(nx+2)*(ny+2)` cells
//! with the x index varying fastest.

use oxiroot_io_core::{Error, FileReader, RBuffer, Result};

use crate::axis::Axis;
use crate::base::{
    bin_content_type_of, cell_count, check_cells, histogram_object, histogram_object_in,
    read_tarray, read_th1_base, unsupported_version, BinContentType,
};

/// A 2-D classic histogram (`TH2D` or `TH2F`); contents are widened to `f64`.
#[derive(Debug, Clone, PartialEq)]
#[doc(alias = "TH2", alias = "TH2D", alias = "TH2F")]
pub struct Hist2D {
    /// On-disk [`BinContentType`] (the class suffix); read the class name via
    /// [`class_name`](Hist2D::class_name).
    pub(crate) bin_content_type: BinContentType,
    /// Histogram name (`fName`).
    pub name: String,
    /// Histogram title (`fTitle`).
    pub title: String,
    /// X axis.
    pub xaxis: Axis,
    /// Y axis.
    pub yaxis: Axis,
    /// Z axis (degenerate for 2-D).
    pub zaxis: Axis,
    /// Total cells, including flow (`fNcells = (nx+2)*(ny+2)`). Read via
    /// [`ncells`](Hist2D::ncells); `pub(crate)` so it cannot drift from `contents`.
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
    /// Bin contents including flow (length `ncells`, x fastest).
    pub contents: Vec<f64>,
    /// Per-bin sum of squared weights (`fSumw2`); empty until error tracking is
    /// turned on by [`Hist2D::sumw2`], [`Hist2D::scale`], or a weighted fill (see
    /// [`Hist1D::fill_weight`](crate::Hist1D::fill_weight)).
    pub sumw2: Vec<f64>,
}

impl Hist2D {
    pub(crate) fn read(r: &mut RBuffer, bin_content_type: BinContentType) -> Result<Hist2D> {
        let th2x = r.read_version()?; // TH2x wrapper
                                      // Class version 1 (ROOT 1) streamed the Hist1D base, the bin array, then the
                                      // Hist2D members, with no Hist2D record of their own.
        if th2x.version < 2 {
            return Err(unsupported_version("TH2", th2x.version));
        }
        let th2 = r.read_version()?; // Hist2D wrapper (Hist1D base + Hist2D members)

        let c = read_th1_base(r)?;
        let _scalefactor = r.be_f64()?;
        let tsumwy = r.be_f64()?;
        let tsumwy2 = r.be_f64()?;
        let tsumwxy = r.be_f64()?;

        let end = th2
            .end
            .ok_or_else(|| Error::Format("TH2 record has no byte count".into()))?;
        r.seek(end)?;
        let contents = read_tarray(r, bin_content_type)?;

        let cells = cell_count(&[c.xaxis.nbins, c.yaxis.nbins])?;
        check_cells("TH2 contents", contents.len(), cells, false)?;
        check_cells("TH2 fSumw2", c.sumw2.len(), cells, true)?;

        Ok(Hist2D {
            bin_content_type,
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

    /// Bin contents excluding flow as `values[ix][iy]` (`nx` rows, `ny` cols),
    /// matching uproot's `values(flow=False)`. Cell `(ix, iy)` is stored at
    /// `ix + (nx + 2) * iy` (indices include the underflow bin at 0).
    pub fn values(&self) -> Vec<Vec<f64>> {
        let (nx, ny) = (self.nx(), self.ny());
        let stride = nx + 2;
        (1..=nx)
            .map(|ix| (1..=ny).map(|iy| self.contents[ix + stride * iy]).collect())
            .collect()
    }

    /// Create an empty `TH2D` with uniform axes: `nx` bins over `[xlo, xhi)`
    /// and `ny` bins over `[ylo, yhi)`. Internal primitive behind the public
    /// builder: [`Hist::reg`](crate::Hist::reg)`(nx, xlo, xhi).reg(ny, ylo, yhi).double()`.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(nx: i32, xlo: f64, xhi: f64, ny: i32, ylo: f64, yhi: f64) -> Hist2D {
        let ncells = (nx.max(0) + 2) * (ny.max(0) + 2);
        Hist2D {
            bin_content_type: BinContentType::F64,
            name: String::new(),
            title: String::new(),
            xaxis: Axis::new("xaxis", nx, xlo, xhi),
            yaxis: Axis::new("yaxis", ny, ylo, yhi),
            zaxis: Axis::new("zaxis", 1, 0.0, 1.0),
            ncells,
            entries: 0.0,
            tsumw: 0.0,
            tsumw2: 0.0,
            tsumwx: 0.0,
            tsumwx2: 0.0,
            tsumwy: 0.0,
            tsumwy2: 0.0,
            tsumwxy: 0.0,
            contents: vec![0.0; ncells.max(0) as usize],
            sumw2: Vec::new(),
        }
    }

    /// Create an empty `TH2D` with variable bin edges on each axis. Internal
    /// primitive behind [`Hist::var`](crate::Hist::var)`(xedges).var(yedges).double()`.
    pub(crate) fn new_variable(xedges: &[f64], yedges: &[f64]) -> Hist2D {
        let ncells = (xedges.len() as i32 + 1) * (yedges.len() as i32 + 1);
        Hist2D {
            bin_content_type: BinContentType::F64,
            name: String::new(),
            title: String::new(),
            xaxis: Axis::variable("xaxis", xedges),
            yaxis: Axis::variable("yaxis", yedges),
            zaxis: Axis::new("zaxis", 1, 0.0, 1.0),
            ncells,
            entries: 0.0,
            tsumw: 0.0,
            tsumw2: 0.0,
            tsumwx: 0.0,
            tsumwx2: 0.0,
            tsumwy: 0.0,
            tsumwy2: 0.0,
            tsumwxy: 0.0,
            contents: vec![0.0; ncells.max(0) as usize],
            sumw2: Vec::new(),
        }
    }

    /// Enable per-bin error tracking (ROOT's `Sumw2`); see [`crate::Hist1D::sumw2`].
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

    /// The exact ROOT class name (`"TH2D"`/`"TH2F"`/…), derived from the stored
    /// [`bin_content_type`](Hist2D::bin_content_type).
    #[must_use]
    pub fn class_name(&self) -> String {
        self.bin_content_type.class_name("TH2")
    }

    /// This histogram's on-disk [`BinContentType`] (the class suffix);
    /// [`BinContentType::F64`] by default. See [`crate::Hist1D::bin_content_type`].
    #[must_use]
    pub fn bin_content_type(&self) -> BinContentType {
        self.bin_content_type
    }

    /// Change the on-disk bin content type of an existing histogram — the
    /// post-construction counterpart of the builder's storage finalizers (build
    /// with a given type via [`Hist::reg(...).reg(...).float()`](crate::Hist) → `TH2F`, …).
    #[must_use]
    pub fn with_bin_content_type(mut self, bin_content_type: BinContentType) -> Self {
        self.bin_content_type = bin_content_type;
        self
    }

    /// Per-bin error: `sqrt(sumw2[bin])` when error tracking is on, else
    /// `sqrt(content)`. `bin` is the global cell index (x fastest).
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

    /// Fill `(x, y)` with unit weight, as in an analysis loop.
    pub fn fill(&mut self, x: f64, y: f64) {
        self.fill_weight(x, y, 1.0);
    }

    /// Fill `(x, y)` with weight `w`, matching ROOT's `Hist2D::Fill` semantics:
    /// every fill counts toward `fEntries`, the cell (including flow) is
    /// incremented, but the statistical moment sums accumulate only when both
    /// coordinates land in range (`fgStatOverflows` defaults to off).
    pub fn fill_weight(&mut self, x: f64, y: f64, w: f64) {
        // Before the contents change; see `Hist1D::fill_weight`.
        if w != 1.0 && self.sumw2.is_empty() {
            self.sumw2();
        }
        let (nx, ny) = (self.nx(), self.ny());
        let binx = self.xaxis.find_bin(x);
        let biny = self.yaxis.find_bin(y);
        let bin = binx + (nx + 2) * biny;
        if let Some(c) = self.contents.get_mut(bin) {
            *c += w;
        }
        if let Some(s) = self.sumw2.get_mut(bin) {
            *s += w * w;
        }
        self.entries += 1.0;

        let in_range = (1..=nx).contains(&binx) && (1..=ny).contains(&biny);
        if in_range {
            self.tsumw += w;
            self.tsumw2 += w * w;
            self.tsumwx += w * x;
            self.tsumwx2 += w * x * x;
            self.tsumwy += w * y;
            self.tsumwy2 += w * y * y;
            self.tsumwxy += w * x * y;
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
}

/// Read any 2-D histogram (`TH2D/F/I/S/C/L`), detecting the bin content type from the
/// stored class.
pub(crate) fn read_th2(file: &FileReader, name: &str) -> Result<Hist2D> {
    decode_th2(histogram_object(file, name, "TH2")?)
}

/// Read any 2-D histogram from subdirectory `subdir`.
pub(crate) fn read_th2_in(file: &FileReader, subdir: &str, name: &str) -> Result<Hist2D> {
    decode_th2(histogram_object_in(file, subdir, name, "TH2")?)
}

pub(crate) fn decode_th2((class, object): (String, Vec<u8>)) -> Result<Hist2D> {
    Hist2D::read(&mut RBuffer::new(&object), bin_content_type_of(&class)?)
}
