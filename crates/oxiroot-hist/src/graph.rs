//! `TGraph` and its error-bar variants `TGraphErrors` / `TGraphAsymmErrors`.
//!
//! A `TGraph` is an (x, y) scatter of points. `TGraphErrors` adds symmetric x/y
//! error bars and `TGraphAsymmErrors` adds independent low/high errors on each
//! axis; both are a `TGraph` base followed by inline `double* //[fNpoints]`
//! error arrays. One [`TGraph`] type covers all three ROOT classes, the variant
//! recorded in [`errors`](TGraph::errors).
//!
//! On disk: `TGraph` (v5) is `TNamed`, `TAttLine`, `TAttFill`, `TAttMarker`,
//! `fNpoints`, `fX`, `fY`, then a trailer: `fFunctions` (a `TList` of attached
//! `TF1`s — fitted functions, parsed into [`functions`](TGraph::functions)),
//! `fHistogram` (an optional display frame), `fMinimum`, `fMaximum`, `fOption`.

use oxiroot_io_core::{
    read_tnamed, read_tobject, skip_versioned, Error, FileReader, RBuffer, Result,
};

use crate::base::{bin_content_type_of, check_len, object_bytes_any, BinContentType};
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

/// A function (ROOT `TF1`) attached to a graph — e.g. a fitted curve stored in
/// the graph's `fFunctions` list. Formula-based: the formula references
/// parameters as `[p0]`, `[p1]`, … (ROOT also accepts `[0]`, `[1]`, which
/// [`GraphFunction::new`] normalizes), with values in [`params`](Self::params).
#[derive(Debug, Clone, PartialEq)]
pub struct GraphFunction {
    /// Function name (`fName`).
    pub name: String,
    /// Function title — ROOT's convention is the `[0]`-form formula (`fTitle`).
    pub title: String,
    /// The formula in `[p0]`/`[p1]` form (`TFormula::fFormula`).
    pub formula: String,
    /// Current parameter values (`TFormula::fClingParameters`).
    pub params: Vec<f64>,
    /// Per-parameter fit errors (`TF1::fParErrors`).
    pub par_errors: Vec<f64>,
    /// Per-parameter lower limits (`TF1::fParMin`).
    pub par_min: Vec<f64>,
    /// Per-parameter upper limits (`TF1::fParMax`).
    pub par_max: Vec<f64>,
    /// Lower bound of the function's range (`TF1::fXmin`).
    pub xmin: f64,
    /// Upper bound of the function's range (`TF1::fXmax`).
    pub xmax: f64,
    /// Fit chi-square (`TF1::fChisquare`).
    pub chi2: f64,
    /// Fit degrees of freedom (`TF1::fNDF`).
    pub ndf: i32,
}

impl GraphFunction {
    /// Build a formula function over `[xmin, xmax]`. `formula` may use either
    /// `[0]`/`[1]` or `[p0]`/`[p1]` for parameters; it is stored in `[pN]` form.
    /// Errors/limits default to zero, sized to `params`.
    pub fn new(
        name: impl Into<String>,
        formula: impl Into<String>,
        params: Vec<f64>,
        xmin: f64,
        xmax: f64,
    ) -> GraphFunction {
        let title = formula.into();
        let n = params.len();
        GraphFunction {
            name: name.into(),
            formula: normalize_formula(&title),
            title,
            params,
            par_errors: vec![0.0; n],
            par_min: vec![0.0; n],
            par_max: vec![0.0; n],
            xmin,
            xmax,
            chi2: 0.0,
            ndf: 0,
        }
    }

    /// Number of parameters (`fNpar`).
    pub fn npar(&self) -> usize {
        self.params.len()
    }
}

/// Rewrite bare `[N]` parameter references to ROOT's `[pN]` form (leaving an
/// already-`[pN]` reference, or any non-numeric `[…]`, untouched).
fn normalize_formula(s: &str) -> String {
    let mut out = String::new();
    let mut rest = s;
    while let Some(open) = rest.find('[') {
        out.push_str(&rest[..open]);
        rest = &rest[open..];
        if let Some(close) = rest.find(']') {
            let inner = &rest[1..close];
            if !inner.is_empty() && inner.bytes().all(|b| b.is_ascii_digit()) {
                out.push_str("[p");
                out.push_str(inner);
                out.push(']');
            } else {
                out.push_str(&rest[..=close]);
            }
            rest = &rest[close + 1..];
        } else {
            out.push_str(rest);
            rest = "";
        }
    }
    out.push_str(rest);
    out
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
    /// Functions attached to the graph (`fFunctions`) — typically the `TF1`s
    /// produced by fitting it. Empty (the default) writes an empty list, matching
    /// a freshly-created ROOT graph; attach one with
    /// [`with_function`](TGraph::with_function).
    pub functions: Vec<GraphFunction>,
}

impl TGraph {
    /// Create a plain `TGraph` from paired `x`/`y` points.
    ///
    /// # Errors
    /// [`Error::LengthMismatch`] if `y` is not as long as `x`.
    pub fn new(x: Vec<f64>, y: Vec<f64>) -> Result<TGraph> {
        check_len("TGraph y", x.len(), y.len())?;
        Ok(TGraph {
            name: String::new(),
            title: String::new(),
            x,
            y,
            errors: GraphErrors::None,
            histogram: None,
            functions: Vec::new(),
        })
    }

    /// Create a `TGraphErrors` with symmetric x/y errors.
    ///
    /// # Errors
    /// [`Error::LengthMismatch`] if `y`, `ex` or `ey` is not as long as `x`.
    pub fn with_errors(x: Vec<f64>, y: Vec<f64>, ex: Vec<f64>, ey: Vec<f64>) -> Result<TGraph> {
        let n = x.len();
        check_len("TGraphErrors y", n, y.len())?;
        check_len("TGraphErrors ex", n, ex.len())?;
        check_len("TGraphErrors ey", n, ey.len())?;
        Ok(TGraph {
            name: String::new(),
            title: String::new(),
            x,
            y,
            errors: GraphErrors::Symmetric { ex, ey },
            histogram: None,
            functions: Vec::new(),
        })
    }

    /// Create a `TGraphAsymmErrors` with independent low/high errors per axis.
    ///
    /// # Errors
    /// [`Error::LengthMismatch`] if `y` or an error vector is not as long as `x`.
    #[allow(clippy::too_many_arguments)]
    pub fn with_asymm_errors(
        x: Vec<f64>,
        y: Vec<f64>,
        ex_low: Vec<f64>,
        ex_high: Vec<f64>,
        ey_low: Vec<f64>,
        ey_high: Vec<f64>,
    ) -> Result<TGraph> {
        let n = x.len();
        check_len("TGraphAsymmErrors y", n, y.len())?;
        check_len("TGraphAsymmErrors ex_low", n, ex_low.len())?;
        check_len("TGraphAsymmErrors ex_high", n, ex_high.len())?;
        check_len("TGraphAsymmErrors ey_low", n, ey_low.len())?;
        check_len("TGraphAsymmErrors ey_high", n, ey_high.len())?;
        Ok(TGraph {
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
            functions: Vec::new(),
        })
    }

    /// Attach a display frame (`fHistogram`) — the axis-frame ROOT would build on
    /// draw. Stored (and persisted) as a `TH1F`, ROOT's declared type for
    /// `fHistogram`, so the bin content type is coerced to `F32`. Chainable.
    #[must_use]
    pub fn with_histogram(mut self, histogram: TH1) -> Self {
        self.histogram = Some(histogram.with_bin_content_type(BinContentType::F32));
        self
    }

    /// Attach a function (`fFunctions`) — e.g. a fitted `TF1`. Persisted as a
    /// `TF1`/`TFormula` inside the graph's function list. Chainable; call more
    /// than once to attach several.
    #[must_use]
    pub fn with_function(mut self, function: GraphFunction) -> Self {
        self.functions.push(function);
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
    let functions = read_functions(r)?; // fFunctions (TList<TF1>)
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
        functions,
    })
}

/// Read the `fFunctions` `TList` object pointer, decoding the `TF1` elements we
/// understand (formula functions) and silently dropping any other object kinds.
/// A null pointer or empty list yields an empty `Vec`.
fn read_functions(r: &mut RBuffer) -> Result<Vec<GraphFunction>> {
    let bc = r.be_i32()? as u32;
    if bc == 0 {
        return Ok(Vec::new()); // null pointer
    }
    let end = r.pos() + (bc & 0x3fff_ffff) as usize;
    let tag = r.be_i32()? as u32;
    if tag == 0xFFFF_FFFF {
        skip_cstring(r)?; // "TList\0"
    }
    let _ver = r.read_version()?; // TList v5
    let _obj = read_tobject(r)?; // TObject base
    let _name = r.string()?; // fName (empty)
    let nfns = r.be_i32()?.max(0) as usize;
    let mut functions = Vec::new();
    for _ in 0..nfns {
        if let Some(f) = read_function_element(r)? {
            functions.push(f);
        }
        // Each element carries an option TString trailer in the TList.
        let _opt = r.string()?;
    }
    r.seek(end)?; // skip any remaining TList payload
    Ok(functions)
}

/// Read one `fFunctions` element: an object pointer that is a `TF1` (decoded) or
/// some other class (skipped, returning `None`).
fn read_function_element(r: &mut RBuffer) -> Result<Option<GraphFunction>> {
    let bc = r.be_i32()? as u32;
    if bc == 0 {
        return Ok(None); // null element
    }
    let end = r.pos() + (bc & 0x3fff_ffff) as usize;
    let tag = r.be_i32()? as u32;
    let class = if tag == 0xFFFF_FFFF {
        read_cstring(r)?
    } else {
        String::new() // back-reference; we cannot resolve it, so skip
    };
    let function = if class == "TF1" {
        Some(GraphFunction::read_tf1_body(r)?)
    } else {
        None
    };
    r.seek(end)?;
    Ok(function)
}

/// Read a NUL-terminated class name (after a `kNewClassTag`).
fn read_cstring(r: &mut RBuffer) -> Result<String> {
    let mut s = String::new();
    loop {
        let b = r.u8()?;
        if b == 0 {
            break;
        }
        s.push(b as char);
    }
    Ok(s)
}

/// Skip a NUL-terminated class name.
fn skip_cstring(r: &mut RBuffer) -> Result<()> {
    while r.u8()? != 0 {}
    Ok(())
}

/// Read an optional embedded `TH1*` (the `fHistogram` display frame). A null
/// pointer is a 4-byte zero; otherwise `{byte count}{class tag}{TH1 object}`.
fn read_opt_th1(r: &mut RBuffer) -> Result<Option<TH1>> {
    let bc = r.be_i32()? as u32;
    if bc == 0 {
        return Ok(None); // null pointer
    }
    let tag = r.be_i32()? as u32;
    let bin_content_type = if tag == 0xFFFF_FFFF {
        // kNewClassTag: a NUL-terminated class name follows (e.g. "TH1F").
        let mut class = String::new();
        loop {
            let b = r.u8()?;
            if b == 0 {
                break;
            }
            class.push(b as char);
        }
        bin_content_type_of(&class)?
    } else {
        BinContentType::F32 // a back-reference: fHistogram is always a TH1F
    };
    Ok(Some(TH1::read(r, bin_content_type)?))
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
pub(crate) fn read_tgraph(file: &FileReader, name: &str) -> Result<TGraph> {
    let (class, object) = object_bytes_any(file, name)?;
    decode_tgraph(name, &class, &object)
}

/// Read a graph from subdirectory `subdir`.
pub(crate) fn read_tgraph_in(file: &FileReader, subdir: &str, name: &str) -> Result<TGraph> {
    let (class, object) = file.object_in(subdir, name)?;
    decode_tgraph(name, &class, &object)
}

pub(crate) fn decode_tgraph(name: &str, class: &str, object: &[u8]) -> Result<TGraph> {
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
        other => Err(Error::WrongClass {
            name: name.to_string(),
            found: other.to_string(),
            expected: "TGraph, TGraphErrors or TGraphAsymmErrors".to_string(),
        }),
    }
}
