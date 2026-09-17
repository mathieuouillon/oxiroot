//! Standalone function objects: [`TF1`], [`TF2`], and [`TF3`] — ROOT's
//! parametric functions, backed by the [`oxiroot_formula`] expression engine.
//!
//! A function is a formula (`"[0]*sin([1]*x)"`, `"gaus"`, `"expo"`, `"pol2"`, …),
//! its parameter values, and a range. It evaluates in pure Rust ([`eval`](TF1::eval),
//! [`integral`](TF1::integral), [`derivative`](TF1::derivative)) and reads/writes
//! as an ordinary ROOT `TF1`/`TF2`/`TF3` key (embedding a `TFormula`), so ROOT
//! C++ and uproot read what oxiroot writes and vice versa.
//!
//! ```
//! use oxiroot_hist::TF1;
//! let f = TF1::new("f", "[0]*sin([1]*x) + [2]", 0.0, 6.283).unwrap()
//!     .with_params(vec![2.0, 1.5, 0.5]);
//! assert!((f.eval(1.0) - 2.494_990).abs() < 1e-6);
//! ```

use oxiroot_formula::{derivative, integrate, Formula};
use std::borrow::Cow;

use oxiroot_io_core::buffer::RBuffer;
use oxiroot_io_core::error::{Error, Result};
use oxiroot_io_core::streamer_gen::{any, base, basic, objanyptr, objptr, stl, strf, Cls};
use oxiroot_io_core::RFile;

use oxiroot_io_core::buffer::WBuffer;

use crate::base::object_bytes_any;
use crate::graph::GraphFunction;
use crate::write::WriteRoot;

/// The data shared by [`TF1`]/[`TF2`]/[`TF3`]: a name and title, the parsed
/// formula, the parameter values, and the fit-result metadata ROOT stores
/// (`fParErrors`/`fParMin`/`fParMax`/`fChisquare`/`fNDF`).
#[derive(Debug, Clone, PartialEq)]
struct FuncCore {
    name: String,
    /// ROOT's `fTitle` — the formula as written by the user (`[0]` form).
    title: String,
    formula: Formula,
    params: Vec<f64>,
    par_errors: Vec<f64>,
    par_min: Vec<f64>,
    par_max: Vec<f64>,
    chi2: f64,
    ndf: i32,
}

impl FuncCore {
    fn build(name: &str, source: &str) -> Result<FuncCore> {
        let formula = Formula::parse(source)
            .map_err(|e| Error::Format(format!("bad formula {source:?}: {e}")))?;
        let npar = formula.npar();
        Ok(FuncCore {
            name: name.to_string(),
            title: source.to_string(),
            params: vec![0.0; npar],
            par_errors: vec![0.0; npar],
            par_min: vec![0.0; npar],
            par_max: vec![0.0; npar],
            chi2: 0.0,
            ndf: 0,
            formula,
        })
    }

    /// The `TF1` record ROOT stores for this function over `[xmin, xmax]`.
    fn record(&self, xmin: f64, xmax: f64) -> GraphFunction {
        GraphFunction {
            name: self.name.clone(),
            title: self.title.clone(),
            formula: self.formula.root_formula().to_owned(),
            params: self.params.clone(),
            par_errors: self.par_errors.clone(),
            par_min: self.par_min.clone(),
            par_max: self.par_max.clone(),
            xmin,
            xmax,
            chi2: self.chi2,
            ndf: self.ndf,
        }
    }

    /// Parse a `TF1` record's formula back into a function core.
    fn from_record(f: GraphFunction) -> Result<FuncCore> {
        let formula = Formula::parse(&f.formula)
            .map_err(|e| Error::Format(format!("bad formula {:?}: {e}", f.formula)))?;
        Ok(FuncCore {
            name: f.name,
            title: f.title,
            formula,
            params: f.params,
            par_errors: f.par_errors,
            par_min: f.par_min,
            par_max: f.par_max,
            chi2: f.chi2,
            ndf: f.ndf,
        })
    }
}

/// The accessors and parameter mutators common to `TF1`/`TF2`/`TF3`.
macro_rules! accessors {
    () => {
        /// The key name (`fName`).
        #[must_use]
        pub fn name(&self) -> &str {
            &self.core.name
        }
        /// The formula expression as written (`fTitle`).
        #[must_use]
        pub fn title(&self) -> &str {
            &self.core.title
        }
        /// The formula in ROOT's canonical `[pN]` form.
        #[must_use]
        pub fn formula(&self) -> &str {
            self.core.formula.root_formula()
        }
        /// The number of parameters.
        #[must_use]
        pub fn npar(&self) -> usize {
            self.core.params.len()
        }
        /// The current parameter values.
        #[must_use]
        pub fn params(&self) -> &[f64] {
            &self.core.params
        }
        /// Parameter `i` (or `0.0` if out of range).
        #[must_use]
        pub fn param(&self, i: usize) -> f64 {
            self.core.params.get(i).copied().unwrap_or(0.0)
        }
        /// Set parameter `i` (ignored if out of range).
        pub fn set_param(&mut self, i: usize, value: f64) {
            if let Some(p) = self.core.params.get_mut(i) {
                *p = value;
            }
        }
        /// Set all parameter values (truncated/zero-padded to the parameter count).
        pub fn set_params(&mut self, params: &[f64]) {
            let n = self.core.params.len();
            self.core.params = params
                .iter()
                .copied()
                .chain(std::iter::repeat(0.0))
                .take(n)
                .collect();
        }
        /// The fit χ² stored on the function (`fChisquare`), `0.0` if unfitted.
        #[must_use]
        pub fn chi2(&self) -> f64 {
            self.core.chi2
        }
        /// The fit degrees of freedom (`fNDF`).
        #[must_use]
        pub fn ndf(&self) -> i32 {
            self.core.ndf
        }
    };
}

/// A `TF1` — a 1-D parametric function `f(x; p)`.
#[derive(Debug, Clone, PartialEq)]
pub struct TF1 {
    core: FuncCore,
    xmin: f64,
    xmax: f64,
}

impl TF1 {
    /// Build a `TF1` named `name` from a ROOT `formula` over `[xmin, xmax]`, with
    /// all parameters initialised to zero.
    ///
    /// # Errors
    /// Returns an error if `formula` is not a valid expression.
    pub fn new(name: &str, formula: &str, xmin: f64, xmax: f64) -> Result<TF1> {
        Ok(TF1 {
            core: FuncCore::build(name, formula)?,
            xmin,
            xmax,
        })
    }

    /// Set all parameter values (a builder; `[0], [1], …`).
    #[must_use]
    pub fn with_params(mut self, params: Vec<f64>) -> TF1 {
        self.set_params(&params);
        self
    }

    /// Set the key name this function is stored under.
    #[must_use]
    pub fn named(mut self, name: impl Into<String>) -> TF1 {
        self.core.name = name.into();
        self
    }

    /// Evaluate `f(x)` with the current parameters.
    #[must_use]
    pub fn eval(&self, x: f64) -> f64 {
        self.core.formula.eval1(x, &self.core.params)
    }

    /// The definite integral `∫ₐᵇ f(x) dx` (adaptive Gauss–Kronrod), as
    /// `TF1::Integral`.
    #[must_use]
    pub fn integral(&self, a: f64, b: f64) -> f64 {
        integrate(|x| self.eval(x), a, b)
    }

    /// The derivative `f'(x)` (Richardson central difference), as
    /// `TF1::Derivative`.
    #[must_use]
    pub fn derivative(&self, x: f64) -> f64 {
        derivative(|x| self.eval(x), x)
    }

    accessors!();

    /// The lower/upper bounds of the function's range (`fXmin`, `fXmax`).
    #[must_use]
    pub fn range(&self) -> (f64, f64) {
        (self.xmin, self.xmax)
    }
}

#[cfg(feature = "fit")]
impl TF1 {
    /// Convert to a fittable [`Model`](oxiroot_fit::Model), seeded with this
    /// function's current parameters — so `data.fit(&tf1.to_model())` fits data
    /// to this function's shape. Requires the `fit` feature.
    #[must_use]
    pub fn to_model(&self) -> oxiroot_fit::Model {
        oxiroot_fit::Model::from_formula(self.name(), self.title())
            .expect("a TF1 always holds an already-parsed formula")
            .with_params(self.params().to_vec())
    }
}

/// A `TF2` — a 2-D parametric function `f(x, y; p)`.
#[derive(Debug, Clone, PartialEq)]
pub struct TF2 {
    core: FuncCore,
    xmin: f64,
    xmax: f64,
    ymin: f64,
    ymax: f64,
}

impl TF2 {
    /// Build a `TF2` from a formula in `x` and `y` over `[xmin, xmax] × [ymin, ymax]`.
    ///
    /// # Errors
    /// Returns an error if `formula` is not a valid expression.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        name: &str,
        formula: &str,
        xmin: f64,
        xmax: f64,
        ymin: f64,
        ymax: f64,
    ) -> Result<TF2> {
        Ok(TF2 {
            core: FuncCore::build(name, formula)?,
            xmin,
            xmax,
            ymin,
            ymax,
        })
    }

    /// Set all parameter values (a builder).
    #[must_use]
    pub fn with_params(mut self, params: Vec<f64>) -> TF2 {
        self.set_params(&params);
        self
    }
    /// Set the key name this function is stored under.
    #[must_use]
    pub fn named(mut self, name: impl Into<String>) -> TF2 {
        self.core.name = name.into();
        self
    }

    /// Evaluate `f(x, y)` with the current parameters.
    #[must_use]
    pub fn eval(&self, x: f64, y: f64) -> f64 {
        self.core.formula.eval(&[x, y], &self.core.params)
    }

    accessors!();

    /// The `x` and `y` ranges (`fXmin`/`fXmax`, `fYmin`/`fYmax`).
    #[must_use]
    pub fn range(&self) -> ((f64, f64), (f64, f64)) {
        ((self.xmin, self.xmax), (self.ymin, self.ymax))
    }
}

/// A `TF3` — a 3-D parametric function `f(x, y, z; p)`.
#[derive(Debug, Clone, PartialEq)]
pub struct TF3 {
    core: FuncCore,
    xmin: f64,
    xmax: f64,
    ymin: f64,
    ymax: f64,
    zmin: f64,
    zmax: f64,
}

impl TF3 {
    /// Build a `TF3` from a formula in `x`, `y`, `z` over the given box.
    ///
    /// # Errors
    /// Returns an error if `formula` is not a valid expression.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        name: &str,
        formula: &str,
        xmin: f64,
        xmax: f64,
        ymin: f64,
        ymax: f64,
        zmin: f64,
        zmax: f64,
    ) -> Result<TF3> {
        Ok(TF3 {
            core: FuncCore::build(name, formula)?,
            xmin,
            xmax,
            ymin,
            ymax,
            zmin,
            zmax,
        })
    }

    /// Set all parameter values (a builder).
    #[must_use]
    pub fn with_params(mut self, params: Vec<f64>) -> TF3 {
        self.set_params(&params);
        self
    }
    /// Set the key name this function is stored under.
    #[must_use]
    pub fn named(mut self, name: impl Into<String>) -> TF3 {
        self.core.name = name.into();
        self
    }

    /// Evaluate `f(x, y, z)` with the current parameters.
    #[must_use]
    pub fn eval(&self, x: f64, y: f64, z: f64) -> f64 {
        self.core.formula.eval(&[x, y, z], &self.core.params)
    }

    accessors!();

    /// The `x`, `y`, and `z` ranges.
    #[must_use]
    pub fn range(&self) -> ((f64, f64), (f64, f64), (f64, f64)) {
        (
            (self.xmin, self.xmax),
            (self.ymin, self.ymax),
            (self.zmin, self.zmax),
        )
    }
}

// --- write ------------------------------------------------------------------

impl WriteRoot for TF1 {
    fn root_class(&self) -> String {
        "TF1".to_string()
    }
    fn root_name(&self) -> &str {
        &self.core.name
    }
    fn root_title(&self) -> &str {
        &self.core.title
    }
    fn to_root_bytes(&self) -> Vec<u8> {
        let mut w = WBuffer::new();
        self.core
            .record(self.xmin, self.xmax)
            .write_tf1_body(&mut w, 1, 100);
        w.into_vec()
    }
    fn streamer_blob(&self) -> Cow<'static, [u8]> {
        crate::write::hist_streamer_list()
    }
    fn streamer_classes(&self) -> Vec<Cls<'static>> {
        tf_classes(1)
    }
}

impl WriteRoot for TF2 {
    fn root_class(&self) -> String {
        "TF2".to_string()
    }
    fn root_name(&self) -> &str {
        &self.core.name
    }
    fn root_title(&self) -> &str {
        &self.core.title
    }
    fn to_root_bytes(&self) -> Vec<u8> {
        let mut w = WBuffer::new();
        let record = self.core.record(self.xmin, self.xmax);
        write_tf2_body(&mut w, &record, 2, self.ymin, self.ymax);
        w.into_vec()
    }
    fn streamer_blob(&self) -> Cow<'static, [u8]> {
        crate::write::hist_streamer_list()
    }
    fn streamer_classes(&self) -> Vec<Cls<'static>> {
        tf_classes(2)
    }
}

impl WriteRoot for TF3 {
    fn root_class(&self) -> String {
        "TF3".to_string()
    }
    fn root_name(&self) -> &str {
        &self.core.name
    }
    fn root_title(&self) -> &str {
        &self.core.title
    }
    fn to_root_bytes(&self) -> Vec<u8> {
        let mut w = WBuffer::new();
        let record = self.core.record(self.xmin, self.xmax);
        write_tf3_body(&mut w, &record, self.ymin, self.ymax, self.zmin, self.zmax);
        w.into_vec()
    }
    fn streamer_blob(&self) -> Cow<'static, [u8]> {
        crate::write::hist_streamer_list()
    }
    fn streamer_classes(&self) -> Vec<Cls<'static>> {
        tf_classes(3)
    }
}

/// Write a `TF2` object body (version 4): the `TF1` base (`fNdim` = `ndim`,
/// `fNpx` = 30), then `fYmin`/`fYmax`, `fNpy`, and an empty `fContour`
/// (`TArrayD`).
fn write_tf2_body(w: &mut WBuffer, f: &GraphFunction, ndim: i32, ymin: f64, ymax: f64) {
    let obj = w.begin_object(4); // TF2 version 4
    f.write_tf1_body(w, ndim, 30); // TF1 base
    w.be_f64(ymin); // fYmin
    w.be_f64(ymax); // fYmax
    w.be_i32(30); // fNpy (ROOT keeps npx == npy by default)
    w.be_i32(0); // fContour: empty TArrayD (fN = 0)
    w.end_object(obj);
}

/// Write a `TF3` object body (version 3): the `TF2` base (`fNdim` = 3), then
/// `fZmin`/`fZmax` and `fNpz`.
fn write_tf3_body(w: &mut WBuffer, f: &GraphFunction, ymin: f64, ymax: f64, zmin: f64, zmax: f64) {
    let obj = w.begin_object(3); // TF3 version 3
    write_tf2_body(w, f, 3, ymin, ymax); // TF2 base
    w.be_f64(zmin); // fZmin
    w.be_f64(zmax); // fZmax
    w.be_i32(30); // fNpz
    w.end_object(obj);
}

// ROOT C++ has these classes compiled in; uproot builds a function model from
// its streamer, so a file storing a TF1/TF2/TF3 embeds them (versions and
// checksums as ROOT writes them).

/// The `TStreamerInfo`s a `TF1` (`dim` 1), `TF2` or `TF3` needs: its formula,
/// then its base classes deepest first, then itself.
fn tf_classes(dim: usize) -> Vec<Cls<'static>> {
    let tformula = Cls {
        name: "TFormula".into(),
        version: 14,
        checksum: 3_342_972_029,
        elements: vec![
            base("TNamed", 1),
            stl("fClingParameters", "vector<double>", 1, 8),
            basic("fAllParametersSetted", 18, 1, "bool"),
            stl("fParams", "map<TString,int,TFormulaParamOrder>", 4, 61),
            strf("fFormula"),
            basic("fNdim", 3, 4, "int"),
            basic("fNumber", 3, 4, "int"),
            stl("fLinearParts", "vector<TObject*>", 1, 63),
            basic("fVectorized", 18, 1, "bool"),
        ],
    };
    let tf1 = Cls {
        name: "TF1".into(),
        version: 12,
        checksum: 1_914_961_880,
        elements: vec![
            base("TNamed", 1),
            base("TAttLine", 2),
            base("TAttFill", 2),
            base("TAttMarker", 3),
            basic("fXmin", 8, 8, "double"),
            basic("fXmax", 8, 8, "double"),
            basic("fNpar", 3, 4, "int"),
            basic("fNdim", 3, 4, "int"),
            basic("fNpx", 3, 4, "int"),
            basic("fType", 3, 4, "TF1::EFType"),
            basic("fNpfits", 3, 4, "int"),
            basic("fNDF", 3, 4, "int"),
            basic("fChisquare", 8, 8, "double"),
            basic("fMinimum", 8, 8, "double"),
            basic("fMaximum", 8, 8, "double"),
            stl("fParErrors", "vector<double>", 1, 8),
            stl("fParMin", "vector<double>", 1, 8),
            stl("fParMax", "vector<double>", 1, 8),
            stl("fSave", "vector<double>", 1, 8),
            basic("fNormalized", 18, 1, "bool"),
            basic("fNormIntegral", 8, 8, "double"),
            objptr("fFormula", "TFormula*"),
            objanyptr("fParams", "TF1Parameters*"),
            objptr("fComposition", "TF1AbsComposition*"),
        ],
    };
    let tf2 = Cls {
        name: "TF2".into(),
        version: 4,
        checksum: 3_115_609_752,
        elements: vec![
            base("TF1", 12),
            basic("fYmin", 8, 8, "double"),
            basic("fYmax", 8, 8, "double"),
            basic("fNpy", 3, 4, "int"),
            any("fContour", 24, "TArrayD"),
        ],
    };
    let tf3 = Cls {
        name: "TF3".into(),
        version: 3,
        checksum: 3_522_165_386,
        elements: vec![
            base("TF2", 4),
            basic("fZmin", 8, 8, "double"),
            basic("fZmax", 8, 8, "double"),
            basic("fNpz", 3, 4, "int"),
        ],
    };
    let mut classes = vec![tformula, tf1, tf2, tf3];
    classes.truncate(dim + 1);
    classes
}

// --- read -------------------------------------------------------------------

/// Read a `TF2` object body (version 4): the `TF1` base then `fYmin`/`fYmax`.
fn read_tf2_body(r: &mut RBuffer) -> Result<(GraphFunction, f64, f64)> {
    let tf2 = r.read_version()?; // TF2 v4
    let base = GraphFunction::read_tf1_body(r)?;
    let ymin = r.be_f64()?;
    let ymax = r.be_f64()?;
    let _npy = r.be_i32()?;
    let ncontour = r.be_i32()?.max(0) as usize; // fContour: TArrayD (fN then fN f64s)
    for _ in 0..ncontour {
        r.be_f64()?;
    }
    if let Some(end) = tf2.end {
        r.seek(end)?;
    }
    Ok((base, ymin, ymax))
}

pub(crate) fn decode_tf1(name: &str, class: &str, object: &[u8]) -> Result<TF1> {
    if class != "TF1" {
        return Err(Error::Format(format!(
            "key {name:?} is a {class}, not a TF1"
        )));
    }
    let mut r = RBuffer::new(object);
    let d = GraphFunction::read_tf1_body(&mut r)?;
    let (xmin, xmax) = (d.xmin, d.xmax);
    Ok(TF1 {
        core: FuncCore::from_record(d)?,
        xmin,
        xmax,
    })
}

pub(crate) fn decode_tf2(name: &str, class: &str, object: &[u8]) -> Result<TF2> {
    if class != "TF2" {
        return Err(Error::Format(format!(
            "key {name:?} is a {class}, not a TF2"
        )));
    }
    let mut r = RBuffer::new(object);
    let (d, ymin, ymax) = read_tf2_body(&mut r)?;
    let (xmin, xmax) = (d.xmin, d.xmax);
    Ok(TF2 {
        core: FuncCore::from_record(d)?,
        xmin,
        xmax,
        ymin,
        ymax,
    })
}

pub(crate) fn decode_tf3(name: &str, class: &str, object: &[u8]) -> Result<TF3> {
    if class != "TF3" {
        return Err(Error::Format(format!(
            "key {name:?} is a {class}, not a TF3"
        )));
    }
    let mut r = RBuffer::new(object);
    let _tf3 = r.read_version()?; // TF3 v3
    let (d, ymin, ymax) = read_tf2_body(&mut r)?;
    let zmin = r.be_f64()?;
    let zmax = r.be_f64()?;
    let _npz = r.be_i32()?;
    let (xmin, xmax) = (d.xmin, d.xmax);
    Ok(TF3 {
        core: FuncCore::from_record(d)?,
        xmin,
        xmax,
        ymin,
        ymax,
        zmin,
        zmax,
    })
}

pub(crate) fn read_tf1(file: &RFile, name: &str) -> Result<TF1> {
    let (class, object) = object_bytes_any(file, name)?;
    decode_tf1(name, &class, &object)
}
pub(crate) fn read_tf1_in(file: &RFile, dir: &str, name: &str) -> Result<TF1> {
    let (class, object) = file.object_in(dir, name)?;
    decode_tf1(name, &class, &object)
}
pub(crate) fn read_tf2(file: &RFile, name: &str) -> Result<TF2> {
    let (class, object) = object_bytes_any(file, name)?;
    decode_tf2(name, &class, &object)
}
pub(crate) fn read_tf2_in(file: &RFile, dir: &str, name: &str) -> Result<TF2> {
    let (class, object) = file.object_in(dir, name)?;
    decode_tf2(name, &class, &object)
}
pub(crate) fn read_tf3(file: &RFile, name: &str) -> Result<TF3> {
    let (class, object) = object_bytes_any(file, name)?;
    decode_tf3(name, &class, &object)
}
pub(crate) fn read_tf3_in(file: &RFile, dir: &str, name: &str) -> Result<TF3> {
    let (class, object) = file.object_in(dir, name)?;
    decode_tf3(name, &class, &object)
}
