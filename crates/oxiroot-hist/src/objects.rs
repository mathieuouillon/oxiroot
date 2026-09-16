//! Small persistable ROOT objects stored as top-level keys alongside histograms:
//! [`TObjString`] (ROOT's "collectable string") and [`TParameter`] (a named
//! scalar — a luminosity, an event count, …). Both read and write, byte-for-byte
//! as ROOT serializes them, so ROOT and uproot read what oxiroot writes and vice
//! versa.

use oxiroot_io_core::buffer::{RBuffer, WBuffer};
use oxiroot_io_core::error::{Error, Result};
use oxiroot_io_core::streamer::read_tobject;
use oxiroot_io_core::streamer_gen::{any, base, basic, objanyptr, objptr, stl, strf, Cls};
use oxiroot_io_core::RFile;

use crate::base::object_bytes_any;
use crate::write::WriteRoot;

/// `fBits` ROOT writes for a `TParameter`'s embedded `TObject` (`TObjString`'s is
/// `0`). Cosmetic, but matched so written files equal ROOT's byte-for-byte.
const PARAM_BITS: u32 = 0x0020_0000;

/// Write a `TObject` base: a 2-byte version, `fUniqueID` (`0`), and `fBits`. No
/// byte count (ROOT writes `TObject` inline without one).
fn write_tobject(w: &mut WBuffer, bits: u32) {
    w.be_u16(1); // TObject version
    w.be_u32(0); // fUniqueID
    w.be_u32(bits);
}

// --- TObjString -------------------------------------------------------------

/// A `TObjString` — ROOT's wrapper for a single `TString`, stored under a key
/// (e.g. a metadata label). Build with [`TObjString::new`] then
/// [`named`](TObjString::named); write it through [`RootFile`](crate::RootFile)
/// or [`write_root`](crate::WriteRoot::write_root).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TObjString {
    name: String,
    value: String,
}

impl TObjString {
    /// A `TObjString` holding `value` (give it a key name with
    /// [`named`](Self::named) before writing).
    pub fn new(value: impl Into<String>) -> TObjString {
        TObjString {
            name: String::new(),
            value: value.into(),
        }
    }

    /// Set the key name this string is stored under.
    #[must_use]
    pub fn named(mut self, name: impl Into<String>) -> TObjString {
        self.name = name.into();
        self
    }

    /// The key name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The stored string.
    pub fn value(&self) -> &str {
        &self.value
    }
}

impl WriteRoot for TObjString {
    fn root_class(&self) -> String {
        "TObjString".to_string()
    }
    fn root_name(&self) -> &str {
        &self.name
    }
    fn root_title(&self) -> &str {
        // ROOT records the TObjString class description as the key title.
        "Collectable string class"
    }
    fn to_root_bytes(&self) -> Vec<u8> {
        let mut w = WBuffer::new();
        let obj = w.begin_object(1); // TObjString version 1
        write_tobject(&mut w, 0);
        w.string(&self.value); // fString
        w.end_object(obj);
        w.into_vec()
    }
    fn streamer_classes(&self) -> Vec<Cls<'static>> {
        vec![tobjstring_class()]
    }
}

pub(crate) fn decode_tobjstring(name: &str, class: &str, object: &[u8]) -> Result<TObjString> {
    if class != "TObjString" {
        return Err(Error::Format(format!(
            "key {name:?} is a {class}, not a TObjString"
        )));
    }
    let mut r = RBuffer::new(object);
    r.read_version()?; // TObjString version
    read_tobject(&mut r)?; // TObject base
    let value = r.string()?; // fString
    Ok(TObjString {
        name: name.to_string(),
        value,
    })
}

pub(crate) fn read_tobjstring(file: &RFile, name: &str) -> Result<TObjString> {
    let (class, object) = object_bytes_any(file, name)?;
    decode_tobjstring(name, &class, &object)
}

pub(crate) fn read_tobjstring_in(file: &RFile, subdir: &str, name: &str) -> Result<TObjString> {
    let (class, object) = file.object_in(subdir, name)?;
    decode_tobjstring(name, &class, &object)
}

// --- TParameter<T> ----------------------------------------------------------

/// The scalar a [`TParameter`] holds, tagged with its C++ type (which selects the
/// `TParameter<…>` class name and the value's on-disk width).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ParamValue {
    /// `TParameter<double>`.
    Double(f64),
    /// `TParameter<float>`.
    Float(f32),
    /// `TParameter<int>`.
    Int(i32),
    /// `TParameter<long long>` (a 64-bit integer; ROOT's `Long64_t`).
    Long64(i64),
}

impl ParamValue {
    /// The C++ type name ROOT uses in the `TParameter<…>` class name. `Long64_t`
    /// demangles to `long long` on disk (matching ROOT's own key class and
    /// streamer-info names, so uproot resolves the class).
    fn type_name(&self) -> &'static str {
        match self {
            ParamValue::Double(_) => "double",
            ParamValue::Float(_) => "float",
            ParamValue::Int(_) => "int",
            ParamValue::Long64(_) => "long long",
        }
    }
    fn write(&self, w: &mut WBuffer) {
        match *self {
            ParamValue::Double(v) => w.be_f64(v),
            ParamValue::Float(v) => w.be_f32(v),
            ParamValue::Int(v) => w.be_i32(v),
            ParamValue::Long64(v) => w.be_i64(v),
        }
    }
    /// The value as an `f64` (integers and floats widen losslessly except for a
    /// very large `Long64`).
    pub fn as_f64(&self) -> f64 {
        match *self {
            ParamValue::Double(v) => v,
            ParamValue::Float(v) => v as f64,
            ParamValue::Int(v) => v as f64,
            ParamValue::Long64(v) => v as f64,
        }
    }
}

/// A `TParameter<T>` — a named scalar value stored under a key, the way ROOT
/// stashes a luminosity, an event count, or a cut threshold alongside histograms.
#[derive(Debug, Clone, PartialEq)]
pub struct TParameter {
    name: String,
    value: ParamValue,
}

impl TParameter {
    /// A `TParameter<double>` named `name`.
    pub fn f64(name: impl Into<String>, value: f64) -> TParameter {
        TParameter::new(name, ParamValue::Double(value))
    }
    /// A `TParameter<float>` named `name`.
    pub fn f32(name: impl Into<String>, value: f32) -> TParameter {
        TParameter::new(name, ParamValue::Float(value))
    }
    /// A `TParameter<int>` named `name`.
    pub fn i32(name: impl Into<String>, value: i32) -> TParameter {
        TParameter::new(name, ParamValue::Int(value))
    }
    /// A `TParameter<Long64_t>` (64-bit integer) named `name`.
    pub fn i64(name: impl Into<String>, value: i64) -> TParameter {
        TParameter::new(name, ParamValue::Long64(value))
    }

    fn new(name: impl Into<String>, value: ParamValue) -> TParameter {
        TParameter {
            name: name.into(),
            value,
        }
    }

    /// The parameter name.
    pub fn name(&self) -> &str {
        &self.name
    }
    /// The stored value (typed).
    pub fn value(&self) -> ParamValue {
        self.value
    }
}

impl WriteRoot for TParameter {
    fn root_class(&self) -> String {
        format!("TParameter<{}>", self.value.type_name())
    }
    fn root_name(&self) -> &str {
        &self.name
    }
    fn root_title(&self) -> &str {
        ""
    }
    fn to_root_bytes(&self) -> Vec<u8> {
        // [version 2][TObject][fName][fVal] — ROOT's TParameter omits the TNamed
        // version header and fTitle.
        let mut w = WBuffer::new();
        let obj = w.begin_object(2); // TParameter version 2
        write_tobject(&mut w, PARAM_BITS);
        w.string(&self.name); // fName (TNamed)
        self.value.write(&mut w); // fVal
        w.end_object(obj);
        w.into_vec()
    }
    fn streamer_classes(&self) -> Vec<Cls<'static>> {
        vec![tparameter_class(self.value)]
    }
}

pub(crate) fn decode_tparameter(name: &str, class: &str, object: &[u8]) -> Result<TParameter> {
    let type_name = class
        .strip_prefix("TParameter<")
        .and_then(|s| s.strip_suffix('>'))
        .ok_or_else(|| Error::Format(format!("key {name:?} is a {class}, not a TParameter")))?;
    let mut r = RBuffer::new(object);
    r.read_version()?; // TParameter version
    read_tobject(&mut r)?; // TObject base
    let _fname = r.string()?; // fName (use the key name for consistency)
    let value = match type_name {
        "double" => ParamValue::Double(r.be_f64()?),
        "float" => ParamValue::Float(r.be_f32()?),
        "int" => ParamValue::Int(r.be_i32()?),
        "Long64_t" | "long" | "long long" => ParamValue::Long64(r.be_i64()?),
        other => {
            return Err(Error::Format(format!(
                "TParameter element type {other:?} is not supported"
            )))
        }
    };
    Ok(TParameter {
        name: name.to_string(),
        value,
    })
}

// --- streamer info -----------------------------------------------------------
//
// ROOT C++ has these classes compiled in, but uproot models a templated
// `TParameter<…>`/`TVectorT<…>`/`TMatrixT<…>` (or a `THStack`/`TMultiGraph`)
// only from its streamer, so files that store them embed these entries. The
// common bases (`TObject`, `TString`, `TNamed`, `TList`) and the histogram and
// graph classes are in the captured histogram list. Checksums and versions are
// ROOT's own (see the `scripts/gen_*.cpp`).

/// The `TStreamerInfo` of `TObjString`.
fn tobjstring_class() -> Cls<'static> {
    Cls {
        name: "TObjString".into(),
        version: 1,
        checksum: 2_626_570_240,
        elements: vec![base("TObject", 1), strf("fString")],
    }
}

/// The `TStreamerInfo` of the `TParameter<…>` holding `value`'s type.
fn tparameter_class(value: ParamValue) -> Cls<'static> {
    let (checksum, ty, size) = match value {
        ParamValue::Double(_) => (1_968_899_544, 8, 8),
        ParamValue::Float(_) => (1_396_280_242, 5, 4),
        ParamValue::Int(_) => (4_270_151_672, 3, 4),
        ParamValue::Long64(_) => (3_647_805_264, 16, 8),
    };
    let type_name = value.type_name();
    Cls {
        name: format!("TParameter<{type_name}>").into(),
        version: 2,
        checksum,
        elements: vec![
            base("TObject", 1),
            strf("fName"),
            basic("fVal", ty, size, type_name),
        ],
    }
}

/// The `TStreamerInfo` of `THStack`.
pub(crate) fn thstack_class() -> Cls<'static> {
    Cls {
        name: "THStack".into(),
        version: 2,
        checksum: 1_918_797_077,
        elements: vec![
            base("TNamed", 1),
            objptr("fHists", "TList*"),
            objptr("fHistogram", "TH1*"),
            basic("fMaximum", 8, 8, "double"),
            basic("fMinimum", 8, 8, "double"),
        ],
    }
}

/// The `TStreamerInfo` of `TMultiGraph`.
pub(crate) fn tmultigraph_class() -> Cls<'static> {
    Cls {
        name: "TMultiGraph".into(),
        version: 2,
        checksum: 3_767_090_389,
        elements: vec![
            base("TNamed", 1),
            objptr("fGraphs", "TList*"),
            objptr("fFunctions", "TList*"),
            objptr("fHistogram", "TH1F*"),
            basic("fMaximum", 8, 8, "double"),
            basic("fMinimum", 8, 8, "double"),
        ],
    }
}

/// The `TStreamerInfo`s a `TF1` (`dim` 1), `TF2` or `TF3` needs: its formula,
/// then its base classes deepest first, then itself.
pub(crate) fn tf_classes(dim: usize) -> Vec<Cls<'static>> {
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

/// The `TStreamerInfo`s for a collection member known only by its class name (a
/// collection read from a file keeps just its members' bytes).
pub(crate) fn member_classes(class: &str) -> Vec<Cls<'static>> {
    let param = |value| vec![tparameter_class(value)];
    match class {
        "TObjString" => vec![tobjstring_class()],
        "TParameter<double>" => param(ParamValue::Double(0.0)),
        "TParameter<float>" => param(ParamValue::Float(0.0)),
        "TParameter<int>" => param(ParamValue::Int(0)),
        "TParameter<long long>" => param(ParamValue::Long64(0)),
        "THStack" => vec![thstack_class()],
        "TMultiGraph" => vec![tmultigraph_class()],
        "TF1" => tf_classes(1),
        "TF2" => tf_classes(2),
        "TF3" => tf_classes(3),
        _ => oxiroot_linalg::streamer_classes(class),
    }
}

pub(crate) fn read_tparameter(file: &RFile, name: &str) -> Result<TParameter> {
    let (class, object) = object_bytes_any(file, name)?;
    decode_tparameter(name, &class, &object)
}

pub(crate) fn read_tparameter_in(file: &RFile, subdir: &str, name: &str) -> Result<TParameter> {
    let (class, object) = file.object_in(subdir, name)?;
    decode_tparameter(name, &class, &object)
}
