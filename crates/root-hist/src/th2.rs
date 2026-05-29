//! 2-D histograms (`TH2D`, `TH2F`).
//!
//! Streamed layout: `TH2x{ TH2{ TH1{ … }, fScalefactor, fTsumwy, fTsumwy2,
//! fTsumwxy }, TArray }`. The inline `TArray` holds the `(nx+2)*(ny+2)` cells
//! with the x index varying fastest.

use root_io_core::buffer::RBuffer;
use root_io_core::error::{Error, Result};
use root_io_core::RFile;

use crate::axis::TAxis;
use crate::base::{
    histogram_object, object_bytes, precision_of, read_tarray, read_th1_base, Precision,
};

/// A 2-D classic histogram (`TH2D` or `TH2F`); contents are widened to `f64`.
#[derive(Debug, Clone, PartialEq)]
pub struct TH2 {
    /// The exact ROOT class (`"TH2D"` or `"TH2F"`).
    pub class_name: String,
    /// Histogram name (`fName`).
    pub name: String,
    /// Histogram title (`fTitle`).
    pub title: String,
    /// X axis.
    pub xaxis: TAxis,
    /// Y axis.
    pub yaxis: TAxis,
    /// Z axis (degenerate for 2-D).
    pub zaxis: TAxis,
    /// Total cells, including flow (`fNcells = (nx+2)*(ny+2)`).
    pub ncells: i32,
    /// Number of entries (`fEntries`).
    pub entries: f64,
    /// Sum of weights (`fTsumw`).
    pub tsumw: f64,
    /// Sum of weight*y (`fTsumwy`).
    pub tsumwy: f64,
    /// Sum of weight*y^2 (`fTsumwy2`).
    pub tsumwy2: f64,
    /// Sum of weight*x*y (`fTsumwxy`).
    pub tsumwxy: f64,
    /// Bin contents including flow (length `ncells`, x fastest).
    pub contents: Vec<f64>,
}

impl TH2 {
    pub(crate) fn read(r: &mut RBuffer, class_name: &str, precision: Precision) -> Result<TH2> {
        let _th2x = r.read_version()?; // TH2x wrapper
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
        let contents = read_tarray(r, precision)?;

        Ok(TH2 {
            class_name: class_name.to_string(),
            name: c.name,
            title: c.title,
            xaxis: c.xaxis,
            yaxis: c.yaxis,
            zaxis: c.zaxis,
            ncells: c.ncells,
            entries: c.entries,
            tsumw: c.tsumw,
            tsumwy,
            tsumwy2,
            tsumwxy,
            contents,
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
}

/// Read any 2-D histogram (`TH2D/F/I/S/C/L`), detecting the precision from the
/// stored class.
pub fn read_th2(file: &RFile, name: &str) -> Result<TH2> {
    let (class, object) = histogram_object(file, name, "TH2")?;
    TH2::read(&mut RBuffer::new(&object), &class, precision_of(&class)?)
}

/// Read a `TH2D` (2-D double histogram) from an open ROOT file.
pub fn read_th2d(file: &RFile, name: &str) -> Result<TH2> {
    read_th2_named(file, name, "TH2D")
}

/// Read a `TH2F` (2-D float histogram) from an open ROOT file.
pub fn read_th2f(file: &RFile, name: &str) -> Result<TH2> {
    read_th2_named(file, name, "TH2F")
}

fn read_th2_named(file: &RFile, name: &str, class: &str) -> Result<TH2> {
    let object = object_bytes(file, name, class)?;
    TH2::read(&mut RBuffer::new(&object), class, precision_of(class)?)
}
