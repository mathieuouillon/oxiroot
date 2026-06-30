//! Small persistable ROOT objects stored as top-level keys alongside histograms:
//! [`TObjString`] (ROOT's "collectable string") and [`TParameter`] (a named
//! scalar — a luminosity, an event count, …). Both read and write, byte-for-byte
//! as ROOT serializes them, so ROOT and uproot read what oxiroot writes and vice
//! versa.

use oxiroot_io_core::buffer::{RBuffer, WBuffer};
use oxiroot_io_core::error::{Error, Result};
use oxiroot_io_core::streamer::read_tobject;
use oxiroot_io_core::streamer_gen::{base, basic, basicptr, basicptr_in, objptr, strf, Cls};
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

/// The `TStreamerInfo`s ROOT writes for `class` (the class plus any of its base
/// classes not already in the histogram streamer info), if it is one of the
/// persistable-object, collection, or linear-algebra classes oxiroot serializes
/// outside the histogram family. The written file embeds these (merged into the
/// histogram streamer info) so uproot can model the class — ROOT C++ has them
/// compiled in and does not need them, but uproot reads a templated
/// `TParameter<…>`/`TVectorT<…>`/`TMatrixT<…>` (or a `THStack`/`TMultiGraph`)
/// only from its streamer. The common bases (`TObject`, `TString`, `TNamed`,
/// `TList`) and the histogram/graph members are already covered. Returns an empty
/// vector for anything else (e.g. a histogram, already described).
/// Checksums/versions are ROOT's own values (see the `scripts/gen_*.cpp`).
pub(crate) fn streamer_classes(class: &str) -> Vec<Cls> {
    let param = |name, checksum, ty, size, type_name| Cls {
        name,
        version: 2,
        checksum,
        elements: vec![
            base("TObject", 1),
            strf("fName"),
            basic("fVal", ty, size, type_name),
        ],
    };
    // `TMatrixTBase<double>` — the dimensions base shared by the matrix classes.
    let matrix_base = || Cls {
        name: "TMatrixTBase<double>",
        version: 5,
        checksum: 2_333_786_657,
        elements: vec![
            base("TObject", 1),
            basic("fNrows", 3, 4, "int"),
            basic("fNcols", 3, 4, "int"),
            basic("fRowLwb", 3, 4, "int"),
            basic("fColLwb", 3, 4, "int"),
            basic("fNelems", 6, 4, "int"),
            basic("fNrowIndex", 3, 4, "int"),
            basic("fTol", 8, 8, "double"),
        ],
    };
    match class {
        "TObjString" => vec![Cls {
            name: "TObjString",
            version: 1,
            checksum: 2_626_570_240,
            elements: vec![base("TObject", 1), strf("fString")],
        }],
        "TParameter<double>" => vec![param("TParameter<double>", 1_968_899_544, 8, 8, "double")],
        "TParameter<float>" => vec![param("TParameter<float>", 1_396_280_242, 5, 4, "float")],
        "TParameter<int>" => vec![param("TParameter<int>", 4_270_151_672, 3, 4, "int")],
        "TParameter<long long>" => vec![param(
            "TParameter<long long>",
            3_647_805_264,
            16,
            8,
            "long long",
        )],
        "THStack" => vec![Cls {
            name: "THStack",
            version: 2,
            checksum: 1_918_797_077,
            elements: vec![
                base("TNamed", 1),
                objptr("fHists", "TList*"),
                objptr("fHistogram", "TH1*"),
                basic("fMaximum", 8, 8, "double"),
                basic("fMinimum", 8, 8, "double"),
            ],
        }],
        "TMultiGraph" => vec![Cls {
            name: "TMultiGraph",
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
        }],
        "TVectorT<double>" => vec![Cls {
            name: "TVectorT<double>",
            version: 4,
            checksum: 1_779_256_495,
            elements: vec![
                base("TObject", 1),
                basic("fNrows", 6, 4, "int"),
                basic("fRowLwb", 3, 4, "int"),
                basicptr("fElements", 48, 8, "double*", "fNrows"),
            ],
        }],
        "TMatrixT<double>" => vec![
            matrix_base(),
            Cls {
                name: "TMatrixT<double>",
                version: 4,
                checksum: 135_074_716,
                elements: vec![
                    base("TMatrixTBase<double>", 5),
                    // fNelems lives in the TMatrixTBase<double> base, not here.
                    basicptr_in(
                        "fElements",
                        48,
                        8,
                        "double*",
                        "fNelems",
                        "TMatrixTBase<double>",
                        5,
                    ),
                ],
            },
        ],
        // ROOT emits no `TMatrixTSym<double>` streamer — its custom Streamer
        // writes the base then the triangle, and uproot models it natively — so
        // only the shared base is needed.
        "TMatrixTSym<double>" => vec![matrix_base()],
        _ => Vec::new(),
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
