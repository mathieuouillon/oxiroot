//! 3-D histograms (`TH3D`, `TH3F`).
//!
//! Streamed layout: `TH3x{ TH3{ TH1{ … }, TAtt3D, fTsumwy, fTsumwy2, fTsumwxy,
//! fTsumwz, fTsumwz2, fTsumwxz, fTsumwyz }, TArray }`. `TAtt3D` is an empty base
//! (skipped via its byte count); the inline `TArray` holds the
//! `(nx+2)*(ny+2)*(nz+2)` cells with x fastest, then y, then z.

use root_io_core::buffer::RBuffer;
use root_io_core::error::{Error, Result};
use root_io_core::streamer::skip_versioned;
use root_io_core::RFile;

use crate::axis::TAxis;
use crate::base::{
    histogram_object, object_bytes, precision_of, read_tarray, read_th1_base, Precision,
};

/// A 3-D classic histogram (`TH3D` or `TH3F`); contents are widened to `f64`.
#[derive(Debug, Clone, PartialEq)]
pub struct TH3 {
    /// The exact ROOT class (`"TH3D"` or `"TH3F"`).
    pub class_name: String,
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
    /// Total cells, including flow (`fNcells = (nx+2)*(ny+2)*(nz+2)`).
    pub ncells: i32,
    /// Number of entries (`fEntries`).
    pub entries: f64,
    /// Sum of weights (`fTsumw`).
    pub tsumw: f64,
    /// Bin contents including flow (length `ncells`, x fastest then y then z).
    pub contents: Vec<f64>,
}

impl TH3 {
    pub(crate) fn read(r: &mut RBuffer, class_name: &str, precision: Precision) -> Result<TH3> {
        let _th3x = r.read_version()?; // TH3x wrapper
        let th3 = r.read_version()?; // TH3 wrapper

        let c = read_th1_base(r)?;
        skip_versioned(r)?; // TAtt3D base (empty)
        for _ in 0..7 {
            // fTsumwy, fTsumwy2, fTsumwxy, fTsumwz, fTsumwz2, fTsumwxz, fTsumwyz
            let _stat = r.be_f64()?;
        }

        let end = th3
            .end
            .ok_or_else(|| Error::Format("TH3 record has no byte count".into()))?;
        r.seek(end)?;
        let contents = read_tarray(r, precision)?;

        Ok(TH3 {
            class_name: class_name.to_string(),
            name: c.name,
            title: c.title,
            xaxis: c.xaxis,
            yaxis: c.yaxis,
            zaxis: c.zaxis,
            ncells: c.ncells,
            entries: c.entries,
            tsumw: c.tsumw,
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
}

/// Read any 3-D histogram (`TH3D/F/I/S/C/L`), detecting the precision from the
/// stored class.
pub fn read_th3(file: &RFile, name: &str) -> Result<TH3> {
    let (class, object) = histogram_object(file, name, "TH3")?;
    TH3::read(&mut RBuffer::new(&object), &class, precision_of(&class)?)
}

/// Read a `TH3D` (3-D double histogram) from an open ROOT file.
pub fn read_th3d(file: &RFile, name: &str) -> Result<TH3> {
    read_th3_named(file, name, "TH3D")
}

/// Read a `TH3F` (3-D float histogram) from an open ROOT file.
pub fn read_th3f(file: &RFile, name: &str) -> Result<TH3> {
    read_th3_named(file, name, "TH3F")
}

fn read_th3_named(file: &RFile, name: &str, class: &str) -> Result<TH3> {
    let object = object_bytes(file, name, class)?;
    TH3::read(&mut RBuffer::new(&object), class, precision_of(class)?)
}
