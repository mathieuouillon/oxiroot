//! `TGraphMultiErrors` — an (x, y) graph with asymmetric x errors and *several*
//! independent layers of asymmetric y errors (e.g. statistical + systematic).
//!
//! On disk (v1): a full `TGraph` base (v5), then `fNYErrors`, `fSumErrorsMode`,
//! the `fExL`/`fExH` `double* //[fNpoints]` arrays, the `fEyL`/`fEyH`
//! `vector<TArrayD>` (one `TArrayD` per y-error layer, streamed *objectwise*),
//! and finally `fAttFill`/`fAttLine` (`vector<TAttFill>`/`vector<TAttLine>`,
//! streamed *memberwise*) holding the per-layer draw attributes. We keep the
//! point + error data and write ROOT's default attributes; the attribute vectors
//! are display-only and skipped on read.
//!
//! Note: uproot cannot decode the memberwise attribute vectors, so this class is
//! cross-checked against compiled ROOT C++ only.

use oxiroot_io_core::buffer::RBuffer;
use oxiroot_io_core::error::{Error, Result};
use oxiroot_io_core::streamer::{read_tnamed, skip_versioned};
use oxiroot_io_core::RFile;

use crate::base::object_bytes_any;

/// An (x, y) graph with asymmetric x errors and one or more layers of asymmetric
/// y errors (ROOT `TGraphMultiErrors`).
#[derive(Debug, Clone, PartialEq)]
pub struct TGraphMultiErrors {
    /// Graph name (`fName`).
    pub name: String,
    /// Graph title (`fTitle`).
    pub title: String,
    /// Point x coordinates (`fX`).
    pub x: Vec<f64>,
    /// Point y coordinates (`fY`).
    pub y: Vec<f64>,
    /// Low x errors (`fExL`).
    pub ex_low: Vec<f64>,
    /// High x errors (`fExH`).
    pub ex_high: Vec<f64>,
    /// Low y errors, one `Vec` per error layer (`fEyL`).
    pub ey_low: Vec<Vec<f64>>,
    /// High y errors, one `Vec` per error layer (`fEyH`).
    pub ey_high: Vec<Vec<f64>>,
    /// How the y-error layers combine when summed (`fSumErrorsMode`; 0 =
    /// `kOnlyFirst`, 1 = `kSquareSum`, 2 = `kSum`).
    pub sum_errors_mode: i32,
}

impl TGraphMultiErrors {
    /// Create a `TGraphMultiErrors` with x errors and a first y-error layer.
    /// Add further y-error layers with [`add_y_error`](Self::add_y_error).
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        x: Vec<f64>,
        y: Vec<f64>,
        ex_low: Vec<f64>,
        ex_high: Vec<f64>,
        ey_low: Vec<f64>,
        ey_high: Vec<f64>,
    ) -> TGraphMultiErrors {
        TGraphMultiErrors {
            name: String::new(),
            title: String::new(),
            x,
            y,
            ex_low,
            ex_high,
            ey_low: vec![ey_low],
            ey_high: vec![ey_high],
            sum_errors_mode: 0,
        }
    }

    /// Add another independent layer of asymmetric y errors. Chainable.
    #[must_use]
    pub fn add_y_error(mut self, ey_low: Vec<f64>, ey_high: Vec<f64>) -> Self {
        self.ey_low.push(ey_low);
        self.ey_high.push(ey_high);
        self
    }

    /// Number of points (`fNpoints`).
    pub fn len(&self) -> usize {
        self.x.len().min(self.y.len())
    }

    /// Whether the graph has no points.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Number of independent y-error layers (`fNYErrors`).
    pub fn n_y_errors(&self) -> usize {
        self.ey_low.len()
    }
}

fn read_basic_array(r: &mut RBuffer, n: usize) -> Result<Vec<f64>> {
    let present = r.u8()?;
    if present == 0 {
        return Ok(Vec::new());
    }
    (0..n).map(|_| r.be_f64()).collect()
}

/// Read an *objectwise* `vector<TArrayD>`: `[bc][ver]`, a count, then one
/// `TArrayD` (`fN` + `fN` doubles) per layer.
fn read_vector_tarrayd(r: &mut RBuffer) -> Result<Vec<Vec<f64>>> {
    let v = r.read_version()?;
    let count = r.be_i32()?.max(0) as usize;
    let mut layers = Vec::with_capacity(count);
    for _ in 0..count {
        let fnn = r.be_i32()?.max(0) as usize;
        let arr: Vec<f64> = (0..fnn).map(|_| r.be_f64()).collect::<Result<_>>()?;
        layers.push(arr);
    }
    if let Some(end) = v.end {
        r.seek(end)?;
    }
    Ok(layers)
}

fn decode_tgraphmultierrors(name: &str, class: &str, object: &[u8]) -> Result<TGraphMultiErrors> {
    if class != "TGraphMultiErrors" {
        return Err(Error::Format(format!(
            "key {name:?} is a {class}, not a TGraphMultiErrors"
        )));
    }
    let mut r = RBuffer::new(object);
    let outer = r.read_version()?; // TGraphMultiErrors v1
    let base = r.read_version()?; // TGraph v5 base
    let named = read_tnamed(&mut r)?;
    skip_versioned(&mut r)?; // TAttLine
    skip_versioned(&mut r)?; // TAttFill
    skip_versioned(&mut r)?; // TAttMarker
    let npoints = r.be_i32()?.max(0) as usize;
    let x = read_basic_array(&mut r, npoints)?;
    let y = read_basic_array(&mut r, npoints)?;
    if let Some(end) = base.end {
        r.seek(end)?; // skip the TGraph base trailer (fFunctions/fHistogram/…)
    }
    let _n_y_errors = r.be_i32()?;
    let sum_errors_mode = r.be_i32()?;
    let ex_low = read_basic_array(&mut r, npoints)?;
    let ex_high = read_basic_array(&mut r, npoints)?;
    let ey_low = read_vector_tarrayd(&mut r)?;
    let ey_high = read_vector_tarrayd(&mut r)?;
    if let Some(end) = outer.end {
        r.seek(end)?; // skip fAttFill/fAttLine (display attributes)
    }
    Ok(TGraphMultiErrors {
        name: named.name,
        title: named.title,
        x,
        y,
        ex_low,
        ex_high,
        ey_low,
        ey_high,
        sum_errors_mode,
    })
}

/// Read a `TGraphMultiErrors` named `name`.
pub(crate) fn read_tgraphmultierrors(file: &RFile, name: &str) -> Result<TGraphMultiErrors> {
    let (class, object) = object_bytes_any(file, name)?;
    decode_tgraphmultierrors(name, &class, &object)
}

/// Read a `TGraphMultiErrors` from subdirectory `subdir`.
pub(crate) fn read_tgraphmultierrors_in(
    file: &RFile,
    subdir: &str,
    name: &str,
) -> Result<TGraphMultiErrors> {
    let (class, object) = file.object_in(subdir, name)?;
    decode_tgraphmultierrors(name, &class, &object)
}
