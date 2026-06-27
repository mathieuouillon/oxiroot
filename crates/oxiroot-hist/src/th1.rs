//! 1-D histograms (`TH1D`, `TH1F`).
//!
//! Streamed layout: `TH1x{ TH1{ … }, TArray }`. The `TH1` base is shared via
//! the crate's `base` module; the inline `TArray` holds the bin contents.

use oxiroot_io_core::buffer::RBuffer;
use oxiroot_io_core::error::Result;
use oxiroot_io_core::RFile;

use crate::axis::TAxis;
use crate::base::{
    cell_count, check_cells, histogram_object, histogram_object_in, precision_of, read_th1_object,
    Precision,
};

/// A 1-D classic histogram (`TH1D` or `TH1F`); contents are widened to `f64`.
#[derive(Debug, Clone, PartialEq)]
pub struct TH1 {
    /// On-disk [`Precision`] (the class suffix). Read the class name via
    /// [`class_name`](TH1::class_name); `pub(crate)` so the precision stays a
    /// typed value rather than a free-form string.
    pub(crate) precision: Precision,
    /// Histogram name (`fName`).
    pub name: String,
    /// Histogram title (`fTitle`).
    pub title: String,
    /// X axis.
    pub xaxis: TAxis,
    /// Y axis (degenerate for 1-D).
    pub yaxis: TAxis,
    /// Z axis (degenerate for 1-D).
    pub zaxis: TAxis,
    /// Total cells, including under/overflow (`fNcells = nbins + 2`). Read it
    /// through [`ncells`](TH1::ncells); `pub(crate)` so it cannot drift from
    /// `contents` via outside mutation.
    pub(crate) ncells: i32,
    /// Number of entries (`fEntries`).
    pub entries: f64,
    /// Sum of weights (`fTsumw`).
    pub tsumw: f64,
    /// Sum of squared weights (`fTsumw2`).
    pub tsumw2: f64,
    /// Sum of weight*x (`fTsumwx`).
    pub tsumwx: f64,
    /// Sum of weight*x^2 (`fTsumwx2`).
    pub tsumwx2: f64,
    /// Bin contents including under/overflow (length `ncells`).
    pub contents: Vec<f64>,
    /// Per-bin sum of squared weights (`fSumw2`); empty unless error tracking is
    /// enabled via [`TH1::sumw2`]. When present, `bin_error = sqrt(sumw2[bin])`.
    pub sumw2: Vec<f64>,
}

impl TH1 {
    /// Create an empty `TH1D` with `nbins` uniform bins over `[xmin, xmax)`,
    /// ready to be filled. The histogram is anonymous; name it with
    /// [`named`](TH1::named) when you write it to a file.
    pub fn new(nbins: i32, xmin: f64, xmax: f64) -> TH1 {
        let cells = (nbins.max(0) as usize) + 2;
        TH1 {
            precision: Precision::Double,
            name: String::new(),
            title: String::new(),
            xaxis: TAxis::new("xaxis", nbins, xmin, xmax),
            yaxis: TAxis::new("yaxis", 1, 0.0, 1.0),
            zaxis: TAxis::new("zaxis", 1, 0.0, 1.0),
            ncells: cells as i32,
            entries: 0.0,
            tsumw: 0.0,
            tsumw2: 0.0,
            tsumwx: 0.0,
            tsumwx2: 0.0,
            contents: vec![0.0; cells],
            sumw2: Vec::new(),
        }
    }

    /// Create an empty `TH1D` with variable bin edges (`edges` = the `nbins + 1`
    /// boundaries, ascending). Anonymous; name it with [`named`](TH1::named).
    pub fn new_variable(edges: &[f64]) -> TH1 {
        let cells = edges.len() + 1; // (edges.len() - 1) bins + 2 flow
        TH1 {
            precision: Precision::Double,
            name: String::new(),
            title: String::new(),
            xaxis: TAxis::variable("xaxis", edges),
            yaxis: TAxis::new("yaxis", 1, 0.0, 1.0),
            zaxis: TAxis::new("zaxis", 1, 0.0, 1.0),
            ncells: cells as i32,
            entries: 0.0,
            tsumw: 0.0,
            tsumw2: 0.0,
            tsumwx: 0.0,
            tsumwx2: 0.0,
            contents: vec![0.0; cells],
            sumw2: Vec::new(),
        }
    }

    /// Enable per-bin error tracking (ROOT's `Sumw2`): allocate the `fSumw2`
    /// array and seed it from the current contents, after which every fill also
    /// accumulates `weight^2`. Call before filling for correct weighted errors.
    /// Returns `&mut self` so it can chain (`h.sumw2().fill(x)`).
    pub fn sumw2(&mut self) -> &mut Self {
        if self.sumw2.len() != self.contents.len() {
            self.sumw2 = self.contents.iter().map(|c| c.abs()).collect();
        }
        self
    }

    /// Total cells including the under/overflow bins (`fNcells`), derived from
    /// the contents so it can never disagree with them.
    #[must_use]
    pub fn ncells(&self) -> i32 {
        self.contents.len() as i32
    }

    /// The exact ROOT class name (`"TH1D"`/`"TH1F"`/…), derived from the stored
    /// [`precision`](TH1::precision).
    #[must_use]
    pub fn class_name(&self) -> String {
        self.precision.class_name("TH1")
    }

    /// This histogram's on-disk [`Precision`] — the class suffix
    /// (`TH1`**`D`**/`F`/`I`/`S`/`C`/`L`). [`Precision::Double`] by default.
    #[must_use]
    pub fn precision(&self) -> Precision {
        self.precision
    }

    /// Set the on-disk precision (the `TArray*` element type the writer emits):
    /// `TH1::new(...).with_precision(Precision::Float)` writes a `TH1F`. Bin
    /// contents stay `f64` in memory and are narrowed only at write time.
    #[must_use]
    pub fn with_precision(mut self, precision: Precision) -> Self {
        self.precision = precision;
        self
    }

    /// Fill the histogram with `x` (weight 1).
    pub fn fill(&mut self, x: f64) {
        self.fill_weight(x, 1.0);
    }

    /// Fill the histogram with `x` and weight `w`, updating bin contents,
    /// entry count, and the running statistics (ROOT `Fill` semantics: every
    /// fill increments `fEntries`; the moment sums accumulate for in-range
    /// fills only).
    pub fn fill_weight(&mut self, x: f64, w: f64) {
        let nbins = self.xaxis.nbins.max(0) as usize;
        let bin = self.xaxis.find_bin(x);
        if let Some(c) = self.contents.get_mut(bin) {
            *c += w;
        }
        if let Some(s) = self.sumw2.get_mut(bin) {
            *s += w * w;
        }
        self.entries += 1.0;
        if (1..=nbins).contains(&bin) {
            self.tsumw += w;
            self.tsumw2 += w * w;
            self.tsumwx += w * x;
            self.tsumwx2 += w * x * x;
        }
    }

    /// Mean of the in-range fills (`fTsumwx / fTsumw`), or 0 if empty.
    pub fn mean(&self) -> f64 {
        if self.tsumw != 0.0 {
            self.tsumwx / self.tsumw
        } else {
            0.0
        }
    }

    pub(crate) fn read(r: &mut RBuffer, precision: Precision) -> Result<TH1> {
        let (c, contents) = read_th1_object(r, precision)?;
        let cells = cell_count(&[c.xaxis.nbins])?;
        check_cells("TH1 contents", contents.len(), cells, false)?;
        check_cells("TH1 fSumw2", c.sumw2.len(), cells, true)?;
        Ok(TH1 {
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
            contents,
            sumw2: c.sumw2,
        })
    }

    /// Per-bin error: `sqrt(sumw2[bin])` when error tracking is on, otherwise the
    /// Poisson default `sqrt(content)`. `bin` includes flow (0 = underflow).
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

    /// Bin contents excluding the under/overflow bins.
    pub fn values(&self) -> &[f64] {
        let n = self.contents.len();
        if n >= 2 {
            &self.contents[1..n - 1]
        } else {
            &self.contents
        }
    }

    /// The X-axis bin edges (`nbins + 1` values).
    pub fn edges(&self) -> Vec<f64> {
        self.xaxis.edges()
    }
}

/// Read any 1-D histogram (`TH1D/F/I/S/C/L`), detecting the precision from the
/// stored class.
pub(crate) fn read_th1(file: &RFile, name: &str) -> Result<TH1> {
    decode_th1(histogram_object(file, name, "TH1")?)
}

/// Read any 1-D histogram from subdirectory `subdir`.
pub(crate) fn read_th1_in(file: &RFile, subdir: &str, name: &str) -> Result<TH1> {
    decode_th1(histogram_object_in(file, subdir, name, "TH1")?)
}

fn decode_th1((class, object): (String, Vec<u8>)) -> Result<TH1> {
    TH1::read(&mut RBuffer::new(&object), precision_of(&class)?)
}
