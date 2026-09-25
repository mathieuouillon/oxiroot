//! `Graph2D` — an (x, y, z) scatter of points for 3-D surface/scatter display.
//!
//! On disk (v1): `Named`, `TAttLine`, `TAttFill`, `TAttMarker`, then the scalars
//! `fNpoints`/`fNpx`/`fNpy`/`fMaxIter`, the `fX`/`fY`/`fZ` `double* //[fNpoints]`
//! arrays, `fMinimum`/`fMaximum`/`fMargin`/`fZout`, an `fFunctions` list, and the
//! `fUserHisto` flag. We keep the point data and write ROOT's display defaults
//! for the rest (an empty `fFunctions`, like [`Graph`](crate::Graph)); the
//! `fHistogram` display frame is transient in ROOT and not persisted.

use oxiroot_io_core::{read_named, skip_versioned, Error, FileReader, RBuffer, Result};

use crate::base::{check_len, object_bytes_any};

/// An (x, y, z) graph (ROOT `TGraph2D`).
#[derive(Debug, Clone, PartialEq)]
#[doc(alias = "TGraph2D")]
pub struct Graph2D {
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

impl Graph2D {
    /// Create a `Graph2D` from paired `x`/`y`/`z` points.
    ///
    /// # Errors
    /// [`Error::LengthMismatch`] if `y` or `z` is not as long as `x`.
    pub fn new(x: Vec<f64>, y: Vec<f64>, z: Vec<f64>) -> Result<Graph2D> {
        check_len("TGraph2D y", x.len(), y.len())?;
        check_len("TGraph2D z", x.len(), z.len())?;
        Ok(Graph2D {
            name: String::new(),
            title: String::new(),
            x,
            y,
            z,
        })
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

fn decode_tgraph2d(name: &str, class: &str, object: &[u8]) -> Result<Graph2D> {
    if class != "TGraph2D" {
        return Err(Error::WrongClass {
            name: name.to_string(),
            found: class.to_string(),
            expected: "TGraph2D".to_string(),
        });
    }
    let mut r = RBuffer::new(object);
    let base = r.read_version()?; // Graph2D v1
    let named = read_named(&mut r)?;
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
    Ok(Graph2D {
        name: named.name,
        title: named.title,
        x,
        y,
        z,
    })
}

/// Read a `Graph2D` named `name`.
pub(crate) fn read_tgraph2d(file: &FileReader, name: &str) -> Result<Graph2D> {
    let (class, object) = object_bytes_any(file, name)?;
    decode_tgraph2d(name, &class, &object)
}

/// Read a `Graph2D` from subdirectory `subdir`.
pub(crate) fn read_tgraph2d_in(file: &FileReader, subdir: &str, name: &str) -> Result<Graph2D> {
    let (class, object) = file.object_in(subdir, name)?;
    decode_tgraph2d(name, &class, &object)
}
