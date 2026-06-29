//! `TGraph2D` — an (x, y, z) scatter of points for 3-D surface/scatter display.
//!
//! On disk (v1): `TNamed`, `TAttLine`, `TAttFill`, `TAttMarker`, then the scalars
//! `fNpoints`/`fNpx`/`fNpy`/`fMaxIter`, the `fX`/`fY`/`fZ` `double* //[fNpoints]`
//! arrays, `fMinimum`/`fMaximum`/`fMargin`/`fZout`, an `fFunctions` list, and the
//! `fUserHisto` flag. We keep the point data and write ROOT's display defaults
//! for the rest (an empty `fFunctions`, like [`TGraph`](crate::TGraph)); the
//! `fHistogram` display frame is transient in ROOT and not persisted.

use oxiroot_io_core::buffer::RBuffer;
use oxiroot_io_core::error::{Error, Result};
use oxiroot_io_core::streamer::{read_tnamed, skip_versioned};
use oxiroot_io_core::RFile;

use crate::base::object_bytes_any;

/// An (x, y, z) graph (ROOT `TGraph2D`).
#[derive(Debug, Clone, PartialEq)]
pub struct TGraph2D {
    /// Graph name (`fName`).
    pub name: String,
    /// Graph title (`fTitle`).
    pub title: String,
    /// Point x coordinates (`fX`).
    pub x: Vec<f64>,
    /// Point y coordinates (`fY`).
    pub y: Vec<f64>,
    /// Point z coordinates (`fZ`).
    pub z: Vec<f64>,
}

impl TGraph2D {
    /// Create a `TGraph2D` from paired `x`/`y`/`z` points (truncated to the
    /// shortest length).
    pub fn new(x: Vec<f64>, y: Vec<f64>, z: Vec<f64>) -> TGraph2D {
        TGraph2D {
            name: String::new(),
            title: String::new(),
            x,
            y,
            z,
        }
    }

    /// Number of points (`fNpoints`).
    pub fn len(&self) -> usize {
        self.x.len().min(self.y.len()).min(self.z.len())
    }

    /// Whether the graph has no points.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// Read a `Double_t* //[n]` member: a presence-marker byte then (if present)
/// `n` big-endian doubles.
fn read_basic_array(r: &mut RBuffer, n: usize) -> Result<Vec<f64>> {
    let present = r.u8()?;
    if present == 0 {
        return Ok(Vec::new());
    }
    (0..n).map(|_| r.be_f64()).collect()
}

fn decode_tgraph2d(name: &str, class: &str, object: &[u8]) -> Result<TGraph2D> {
    if class != "TGraph2D" {
        return Err(Error::Format(format!(
            "key {name:?} is a {class}, not a TGraph2D"
        )));
    }
    let mut r = RBuffer::new(object);
    let base = r.read_version()?; // TGraph2D v1
    let named = read_tnamed(&mut r)?;
    skip_versioned(&mut r)?; // TAttLine
    skip_versioned(&mut r)?; // TAttFill
    skip_versioned(&mut r)?; // TAttMarker
    let npoints = r.be_i32()?.max(0) as usize;
    let _fnpx = r.be_i32()?;
    let _fnpy = r.be_i32()?;
    let _fmaxiter = r.be_i32()?;
    let x = read_basic_array(&mut r, npoints)?;
    let y = read_basic_array(&mut r, npoints)?;
    let z = read_basic_array(&mut r, npoints)?;
    if let Some(end) = base.end {
        r.seek(end)?; // skip fMinimum/fMaximum/fMargin/fZout/fFunctions/fUserHisto
    }
    Ok(TGraph2D {
        name: named.name,
        title: named.title,
        x,
        y,
        z,
    })
}

/// Read a `TGraph2D` named `name`.
pub(crate) fn read_tgraph2d(file: &RFile, name: &str) -> Result<TGraph2D> {
    let (class, object) = object_bytes_any(file, name)?;
    decode_tgraph2d(name, &class, &object)
}

/// Read a `TGraph2D` from subdirectory `subdir`.
pub(crate) fn read_tgraph2d_in(file: &RFile, subdir: &str, name: &str) -> Result<TGraph2D> {
    let (class, object) = file.object_in(subdir, name)?;
    decode_tgraph2d(name, &class, &object)
}
