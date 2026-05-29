//! 1-D histograms (`TH1D`, `TH1F`).
//!
//! Streamed layout: `TH1x{ TH1{ … }, TArray }`. The `TH1` base is shared via
//! [`crate::base`]; the inline `TArray` holds the bin contents.

use root_io_core::buffer::RBuffer;
use root_io_core::error::Result;
use root_io_core::RFile;

use crate::axis::TAxis;
use crate::base::{histogram_object, object_bytes, precision_of, read_th1_object, Precision};

/// A 1-D classic histogram (`TH1D` or `TH1F`); contents are widened to `f64`.
#[derive(Debug, Clone, PartialEq)]
pub struct TH1 {
    /// The exact ROOT class (`"TH1D"` or `"TH1F"`).
    pub class_name: String,
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
    /// Total cells, including under/overflow (`fNcells = nbins + 2`).
    pub ncells: i32,
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
}

impl TH1 {
    pub(crate) fn read(r: &mut RBuffer, class_name: &str, precision: Precision) -> Result<TH1> {
        let (c, contents) = read_th1_object(r, precision)?;
        Ok(TH1 {
            class_name: class_name.to_string(),
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
        })
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
pub fn read_th1(file: &RFile, name: &str) -> Result<TH1> {
    let (class, object) = histogram_object(file, name, "TH1")?;
    TH1::read(&mut RBuffer::new(&object), &class, precision_of(&class)?)
}

/// Read a `TH1D` (1-D double histogram) from an open ROOT file.
pub fn read_th1d(file: &RFile, name: &str) -> Result<TH1> {
    read_th1_named(file, name, "TH1D")
}

/// Read a `TH1F` (1-D float histogram) from an open ROOT file.
pub fn read_th1f(file: &RFile, name: &str) -> Result<TH1> {
    read_th1_named(file, name, "TH1F")
}

fn read_th1_named(file: &RFile, name: &str, class: &str) -> Result<TH1> {
    let object = object_bytes(file, name, class)?;
    TH1::read(&mut RBuffer::new(&object), class, precision_of(class)?)
}
