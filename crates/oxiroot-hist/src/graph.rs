//! `TGraph` and its error-bar variants `TGraphErrors` / `TGraphAsymmErrors`.
//!
//! A `TGraph` is an (x, y) scatter of points. `TGraphErrors` adds symmetric x/y
//! error bars and `TGraphAsymmErrors` adds independent low/high errors on each
//! axis; both are a `TGraph` base followed by inline `double* //[fNpoints]`
//! error arrays. One [`TGraph`] type covers all three ROOT classes, the variant
//! recorded in [`errors`](TGraph::errors).
//!
//! On disk: `TGraph` (v5) is `TNamed`, `TAttLine`, `TAttFill`, `TAttMarker`,
//! `fNpoints`, `fX`, `fY`, then a trailer (`fFunctions`, `fHistogram`,
//! `fMinimum`, `fMaximum`, `fOption`) that we skip on read and write empty.

use oxiroot_io_core::buffer::RBuffer;
use oxiroot_io_core::error::{Error, Result};
use oxiroot_io_core::streamer::{read_tnamed, skip_versioned};
use oxiroot_io_core::RFile;

use crate::base::{object_bytes_any, precision_of, Precision};
use crate::th1::TH1;

/// Error bars attached to a graph's points.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum GraphErrors {
    /// No error bars — a plain `TGraph`.
    None,
    /// Symmetric per-point x and y errors — a `TGraphErrors`.
    Symmetric {
        /// x error (`fEX`).
        ex: Vec<f64>,
        /// y error (`fEY`).
        ey: Vec<f64>,
    },
    /// Independent low/high errors on each axis — a `TGraphAsymmErrors`.
    Asymmetric {
        /// Low x error (`fEXlow`).
        ex_low: Vec<f64>,
        /// High x error (`fEXhigh`).
        ex_high: Vec<f64>,
        /// Low y error (`fEYlow`).
        ey_low: Vec<f64>,
        /// High y error (`fEYhigh`).
        ey_high: Vec<f64>,
    },
}

/// An (x, y) graph, optionally with error bars (ROOT `TGraph` /
/// `TGraphErrors` / `TGraphAsymmErrors`).
#[derive(Debug, Clone, PartialEq)]
pub struct TGraph {
    /// Graph name (`fName`).
    pub name: String,
    /// Graph title (`fTitle`).
    pub title: String,
    /// Point x coordinates (`fX`).
    pub x: Vec<f64>,
    /// Point y coordinates (`fY`).
    pub y: Vec<f64>,
    /// Error bars, selecting the concrete ROOT class.
    pub errors: GraphErrors,
    /// Optional display frame (`fHistogram`, a `TH1F`): the axis frame ROOT
    /// builds when a graph is drawn. `None` (the default) writes a null pointer,
    /// matching a freshly-created ROOT graph; set one with
    /// [`with_histogram`](TGraph::with_histogram) to persist axis ranges/titles.
    pub histogram: Option<TH1>,
}

impl TGraph {
    /// Create a plain `TGraph` from paired `x`/`y` points (truncated to the
    /// shorter length).
    pub fn new(x: Vec<f64>, y: Vec<f64>) -> TGraph {
        TGraph {
            name: String::new(),
            title: String::new(),
            x,
            y,
            errors: GraphErrors::None,
            histogram: None,
        }
    }

    /// Create a `TGraphErrors` with symmetric x/y errors.
    pub fn with_errors(x: Vec<f64>, y: Vec<f64>, ex: Vec<f64>, ey: Vec<f64>) -> TGraph {
        TGraph {
            name: String::new(),
            title: String::new(),
            x,
            y,
            errors: GraphErrors::Symmetric { ex, ey },
            histogram: None,
        }
    }

    /// Create a `TGraphAsymmErrors` with independent low/high errors per axis.
    #[allow(clippy::too_many_arguments)]
    pub fn with_asymm_errors(
        x: Vec<f64>,
        y: Vec<f64>,
        ex_low: Vec<f64>,
        ex_high: Vec<f64>,
        ey_low: Vec<f64>,
        ey_high: Vec<f64>,
    ) -> TGraph {
        TGraph {
            name: String::new(),
            title: String::new(),
            x,
            y,
            errors: GraphErrors::Asymmetric {
                ex_low,
                ex_high,
                ey_low,
                ey_high,
            },
            histogram: None,
        }
    }

    /// Attach a display frame (`fHistogram`) — the axis-frame ROOT would build on
    /// draw. Stored (and persisted) as a `TH1F`, ROOT's declared type for
    /// `fHistogram`, so the precision is coerced to `Float`. Chainable.
    #[must_use]
    pub fn with_histogram(mut self, histogram: TH1) -> Self {
        self.histogram = Some(histogram.with_precision(Precision::Float));
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

    /// The ROOT class this graph serializes as.
    pub fn class_name(&self) -> &'static str {
        match self.errors {
            GraphErrors::None => "TGraph",
            GraphErrors::Symmetric { .. } => "TGraphErrors",
            GraphErrors::Asymmetric { .. } => "TGraphAsymmErrors",
        }
    }
}

/// Read the `TGraph` base (`fName`/`fTitle`/`fX`/`fY`), then seek past the
/// trailer (`fFunctions`/`fHistogram`/…) to the base object's end.
fn read_tgraph_base(r: &mut RBuffer) -> Result<TGraph> {
    let base = r.read_version()?; // TGraph v5
    let named = read_tnamed(r)?;
    skip_versioned(r)?; // TAttLine
    skip_versioned(r)?; // TAttFill
    skip_versioned(r)?; // TAttMarker
    let npoints = r.be_i32()?.max(0) as usize;
    let x = read_basic_array(r, npoints)?;
    let y = read_basic_array(r, npoints)?;
    skip_object_ptr(r)?; // fFunctions (TList) — display/fit objects, not kept here
    let histogram = read_opt_th1(r)?; // fHistogram (TH1F*, or null)
    if let Some(end) = base.end {
        r.seek(end)?; // skip fMinimum/fMaximum/fOption
    }
    Ok(TGraph {
        name: named.name,
        title: named.title,
        x,
        y,
        errors: GraphErrors::None,
        histogram,
    })
}

/// Skip a byte-counted object pointer (`{kByteCountMask | len}{len bytes}`),
/// used for the `fFunctions` `TList`.
fn skip_object_ptr(r: &mut RBuffer) -> Result<()> {
    let bc = r.be_i32()? as u32;
    r.skip((bc & 0x3fff_ffff) as usize)
}

/// Read an optional embedded `TH1*` (the `fHistogram` display frame). A null
/// pointer is a 4-byte zero; otherwise `{byte count}{class tag}{TH1 object}`.
fn read_opt_th1(r: &mut RBuffer) -> Result<Option<TH1>> {
    let bc = r.be_i32()? as u32;
    if bc == 0 {
        return Ok(None); // null pointer
    }
    let tag = r.be_i32()? as u32;
    let precision = if tag == 0xFFFF_FFFF {
        // kNewClassTag: a NUL-terminated class name follows (e.g. "TH1F").
        let mut class = String::new();
        loop {
            let b = r.u8()?;
            if b == 0 {
                break;
            }
            class.push(b as char);
        }
        precision_of(&class)?
    } else {
        Precision::Float // a back-reference: fHistogram is always a TH1F
    };
    Ok(Some(TH1::read(r, precision)?))
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

/// Read a `TGraph`, `TGraphErrors`, or `TGraphAsymmErrors` named `name`.
pub(crate) fn read_tgraph(file: &RFile, name: &str) -> Result<TGraph> {
    let (class, object) = object_bytes_any(file, name)?;
    decode_tgraph(name, &class, &object)
}

/// Read a graph from subdirectory `subdir`.
pub(crate) fn read_tgraph_in(file: &RFile, subdir: &str, name: &str) -> Result<TGraph> {
    let (class, object) = file.object_in(subdir, name)?;
    decode_tgraph(name, &class, &object)
}

fn decode_tgraph(name: &str, class: &str, object: &[u8]) -> Result<TGraph> {
    let mut r = RBuffer::new(object);
    match class {
        "TGraph" => read_tgraph_base(&mut r),
        "TGraphErrors" => {
            let _wrapper = r.read_version()?; // TGraphErrors v3
            let mut g = read_tgraph_base(&mut r)?;
            let n = g.len();
            let ex = read_basic_array(&mut r, n)?;
            let ey = read_basic_array(&mut r, n)?;
            g.errors = GraphErrors::Symmetric { ex, ey };
            Ok(g)
        }
        "TGraphAsymmErrors" => {
            let _wrapper = r.read_version()?; // TGraphAsymmErrors v3
            let mut g = read_tgraph_base(&mut r)?;
            let n = g.len();
            let ex_low = read_basic_array(&mut r, n)?;
            let ex_high = read_basic_array(&mut r, n)?;
            let ey_low = read_basic_array(&mut r, n)?;
            let ey_high = read_basic_array(&mut r, n)?;
            g.errors = GraphErrors::Asymmetric {
                ex_low,
                ex_high,
                ey_low,
                ey_high,
            };
            Ok(g)
        }
        other => Err(Error::Format(format!(
            "key {name:?} is a {other}, not a TGraph/TGraphErrors/TGraphAsymmErrors"
        ))),
    }
}
