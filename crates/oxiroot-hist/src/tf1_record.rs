//! The `TF1` object body codec: one reader and one writer for the record a
//! graph's `fFunctions` entries and the standalone `TF1`/`TF2`/`TF3` keys (in
//! `oxiroot-hist-func`) share.

use oxiroot_io_core::buffer::{RBuffer, WBuffer};
use oxiroot_io_core::error::Result;
use oxiroot_io_core::streamer::{read_tnamed, skip_versioned, write_tnamed};

use crate::graph::GraphFunction;
use crate::write::write_object_ptr;

impl GraphFunction {
    /// Decode a streamed `TF1` object body (version 12), starting at its
    /// byte-count/version header (no class tag). The `TFormula` it embeds gives
    /// [`formula`](Self::formula) and [`params`](Self::params).
    ///
    /// # Errors
    /// Returns an error if the buffer ends early or a header is malformed.
    pub fn read_tf1_body(r: &mut RBuffer) -> Result<GraphFunction> {
        let tf1 = r.read_version()?; // TF1 v12
        let named = read_tnamed(r)?;
        skip_versioned(r)?; // TAttLine
        skip_versioned(r)?; // TAttFill
        skip_versioned(r)?; // TAttMarker
        let xmin = r.be_f64()?;
        let xmax = r.be_f64()?;
        let _npar = r.be_i32()?;
        let _ndim = r.be_i32()?;
        let _npx = r.be_i32()?;
        let _ftype = r.be_i32()?;
        let _npfits = r.be_i32()?;
        let ndf = r.be_i32()?;
        let chi2 = r.be_f64()?;
        let _min = r.be_f64()?;
        let _max = r.be_f64()?;
        let par_errors = read_vector_f64(r)?;
        let par_min = read_vector_f64(r)?;
        let par_max = read_vector_f64(r)?;
        let _save = read_vector_f64(r)?;
        let _normalized = r.u8()?;
        let _norm_integral = r.be_f64()?;
        let (formula, params) = read_tformula_ptr(r)?;
        if let Some(end) = tf1.end {
            r.seek(end)?; // skip fParams + fComposition
        }
        Ok(GraphFunction {
            name: named.name,
            title: named.title,
            formula,
            params,
            par_errors,
            par_min,
            par_max,
            xmin,
            xmax,
            chi2,
            ndf,
        })
    }

    /// Encode this function as a `TF1` object body (version 12), byte-for-byte as
    /// ROOT writes it: the `TNamed`/`TAtt*` bases, the scalar members, the
    /// parameter `vector<double>`s, the `fFormula` `TFormula*`, and null
    /// `fParams`/`fComposition` pointers.
    ///
    /// `ndim` is `fNdim` (1 for a `TF1`, 2 or 3 for a `TF2`/`TF3` base) and `npx`
    /// is `fNpx` (ROOT writes 100 for a `TF1` and 30 for a `TF2`/`TF3` base).
    pub fn write_tf1_body(&self, w: &mut WBuffer, ndim: i32, npx: i32) {
        let npar = self.params.len();
        let obj = w.begin_object(12); // TF1 version 12
        write_tnamed(w, 0, &self.name, &self.title);

        let line = w.begin_object(2); // TAttLine
        w.be_i16(2); // fLineColor (ROOT's TF1 default)
        w.be_i16(1); // fLineStyle
        w.be_i16(2); // fLineWidth (ROOT's TF1 default)
        w.end_object(line);
        let fill = w.begin_object(2); // TAttFill
        w.be_i16(19); // fFillColor (ROOT's TF1 default)
        w.be_i16(0); // fFillStyle
        w.end_object(fill);
        let marker = w.begin_object(3); // TAttMarker
        w.be_i16(1); // fMarkerColor
        w.be_i16(1); // fMarkerStyle
        w.be_f32(1.0); // fMarkerSize
        w.end_object(marker);

        w.be_f64(self.xmin); // fXmin
        w.be_f64(self.xmax); // fXmax
        w.be_i32(npar as i32); // fNpar
        w.be_i32(ndim); // fNdim
        w.be_i32(npx); // fNpx
        w.be_i32(0); // fType (kFormula)
        w.be_i32(0); // fNpfits
        w.be_i32(self.ndf); // fNDF
        w.be_f64(self.chi2); // fChisquare
        w.be_f64(-1111.0); // fMinimum
        w.be_f64(-1111.0); // fMaximum
        write_vector_f64(w, &self.par_errors); // fParErrors
        write_vector_f64(w, &self.par_min); // fParMin
        write_vector_f64(w, &self.par_max); // fParMax
        write_vector_f64(w, &[]); // fSave (empty)
        w.u8(0); // fNormalized
        w.be_f64(0.0); // fNormIntegral
        write_object_ptr(w, "TFormula", |w| write_tformula_body(w, self, ndim)); // fFormula
        w.be_u32(0); // fParams (TF1Parameters*) = null
        w.be_u32(0); // fComposition (TF1AbsComposition*) = null
        w.end_object(obj);
    }
}

// --- read -------------------------------------------------------------------

/// Read an objectwise `vector<double>` (`[bc][ver][count][count×f64]`).
fn read_vector_f64(r: &mut RBuffer) -> Result<Vec<f64>> {
    let _bc = r.be_i32()?;
    let _ver = r.be_i16()?;
    let count = r.be_i32()?.max(0) as usize;
    (0..count).map(|_| r.be_f64()).collect()
}

/// Read the `fFormula` (`TFormula*`) object pointer, returning
/// `(fFormula string, fClingParameters)`.
fn read_tformula_ptr(r: &mut RBuffer) -> Result<(String, Vec<f64>)> {
    let bc = r.be_i32()? as u32;
    if bc == 0 {
        return Ok((String::new(), Vec::new()));
    }
    let end = r.pos() + (bc & 0x3fff_ffff) as usize;
    let tag = r.be_i32()? as u32;
    if tag == 0xFFFF_FFFF {
        // NUL-terminated class name "TFormula\0".
        while r.u8()? != 0 {}
    }
    let _ver = r.read_version()?; // TFormula v14
    let _named = read_tnamed(r)?;
    let params = read_vector_f64(r)?; // fClingParameters
    let _all_set = r.u8()?;
    skip_param_map(r)?; // fParams
    let formula = r.string()?; // fFormula ([pN] form)
    r.seek(end)?;
    Ok((formula, params))
}

/// Skip a `map<TString,int>` (`[bc][ver][count]` then `count` `{TString}{i32}`).
fn skip_param_map(r: &mut RBuffer) -> Result<()> {
    let _bc = r.be_i32()?;
    let _ver = r.be_i16()?;
    let count = r.be_i32()?.max(0) as usize;
    for _ in 0..count {
        let _key = r.string()?;
        let _val = r.be_i32()?;
    }
    Ok(())
}

// --- write ------------------------------------------------------------------

/// `TFormula::kNotGlobal` (`BIT(10)`): set on a formula owned by another object so
/// that, on read, ROOT does *not* register it in `gROOT`'s global function list
/// (which would re-JIT it and crash in a headless/JIT-less context). ROOT sets
/// this on every embedded `TFormula`; omitting it makes ROOT segfault on read.
const FORMULA_NOT_GLOBAL: u32 = 0x0000_0400;

/// Write a `TFormula` object body (version 14): `TNamed`, `fClingParameters`,
/// `fAllParametersSetted`, the `fParams` name→index map, the `[pN]`-form formula
/// string, and the trailing scalars/empty `fLinearParts`.
fn write_tformula_body(w: &mut WBuffer, f: &GraphFunction, ndim: i32) {
    let npar = f.params.len();
    let obj = w.begin_object(14); // TFormula version 14
    write_tnamed(w, FORMULA_NOT_GLOBAL, &f.name, &f.title);
    write_vector_f64(w, &f.params); // fClingParameters
    w.u8(1); // fAllParametersSetted
    write_param_map(w, npar); // fParams (map<TString,int>)
    w.string(&f.formula); // fFormula (in [pN] form)
    w.be_i32(ndim); // fNdim
    w.be_i32(0); // fNumber
                 // fLinearParts: an empty objectwise vector<TObject*>.
    let lp = w.reserve(4);
    let start = w.len();
    w.be_i16(0x000a); // streamer version
    w.be_i32(0); // count
    let len = (w.len() - start) as u32;
    w.patch_be_u32(lp, 0x4000_0000 | len);
    w.u8(0); // fVectorized
    w.end_object(obj);
}

/// Write an objectwise `vector<double>`: `{byte count}{ver 0x000a}{count}{f64s}`.
fn write_vector_f64(w: &mut WBuffer, data: &[f64]) {
    let bc = w.reserve(4);
    let start = w.len();
    w.be_i16(0x000a); // streamer version
    w.be_i32(data.len() as i32);
    for &d in data {
        w.be_f64(d);
    }
    let len = (w.len() - start) as u32;
    w.patch_be_u32(bc, 0x4000_0000 | len);
}

/// Write a `TFormula::fParams` `map<TString,int>` for `n` parameters: the entries
/// `p0→0, p1→1, …` in index order (ROOT re-sorts on read by its own comparator).
fn write_param_map(w: &mut WBuffer, n: usize) {
    let bc = w.reserve(4);
    let start = w.len();
    w.be_i16(0x000a); // streamer version
    w.be_i32(n as i32); // count
    for i in 0..n {
        w.string(&format!("p{i}"));
        w.be_i32(i as i32);
    }
    let len = (w.len() - start) as u32;
    w.patch_be_u32(bc, 0x4000_0000 | len);
}
