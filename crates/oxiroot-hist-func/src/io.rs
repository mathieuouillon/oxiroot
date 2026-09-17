//! ROOT I/O for the function types: the `WriteRoot`/`ReadRoot` impls, the
//! `TF2`/`TF3` object bodies around the shared `TF1` record, and the
//! `TStreamerInfo`s a file storing them embeds.

use std::borrow::Cow;

use oxiroot_hist::{hist_streamer_blob, GraphFunction};
use oxiroot_io_core::buffer::{RBuffer, WBuffer};
use oxiroot_io_core::error::{Error, Result};
use oxiroot_io_core::streamer_gen::{any, base, basic, objanyptr, objptr, stl, strf, Cls};
use oxiroot_io_core::{object_bytes_any, RFile, ReadRoot, WriteRoot};

use crate::tf::{FuncCore, TF1, TF2, TF3};

// --- write ------------------------------------------------------------------

impl WriteRoot for TF1 {
    fn root_class(&self) -> String {
        "TF1".to_string()
    }
    fn root_name(&self) -> &str {
        self.name()
    }
    fn root_title(&self) -> &str {
        self.title()
    }
    fn to_root_bytes(&self) -> Vec<u8> {
        let mut w = WBuffer::new();
        self.core
            .record(self.xmin, self.xmax)
            .write_tf1_body(&mut w, 1, 100);
        w.into_vec()
    }
    fn streamer_blob(&self) -> Cow<'static, [u8]> {
        hist_streamer_blob()
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
        self.name()
    }
    fn root_title(&self) -> &str {
        self.title()
    }
    fn to_root_bytes(&self) -> Vec<u8> {
        let mut w = WBuffer::new();
        let record = self.core.record(self.xmin, self.xmax);
        write_tf2_body(&mut w, &record, 2, self.ymin, self.ymax);
        w.into_vec()
    }
    fn streamer_blob(&self) -> Cow<'static, [u8]> {
        hist_streamer_blob()
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
        self.name()
    }
    fn root_title(&self) -> &str {
        self.title()
    }
    fn to_root_bytes(&self) -> Vec<u8> {
        let mut w = WBuffer::new();
        let record = self.core.record(self.xmin, self.xmax);
        write_tf3_body(&mut w, &record, self.ymin, self.ymax, self.zmin, self.zmax);
        w.into_vec()
    }
    fn streamer_blob(&self) -> Cow<'static, [u8]> {
        hist_streamer_blob()
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

fn decode_tf1(name: &str, class: &str, object: &[u8]) -> Result<TF1> {
    if class != "TF1" {
        return Err(Error::Format(format!(
            "key {name:?} is a {class}, not a TF1"
        )));
    }
    let mut r = RBuffer::new(object);
    TF1::from_graph_function(GraphFunction::read_tf1_body(&mut r)?)
}

fn decode_tf2(name: &str, class: &str, object: &[u8]) -> Result<TF2> {
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

fn decode_tf3(name: &str, class: &str, object: &[u8]) -> Result<TF3> {
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

impl ReadRoot for TF1 {
    fn read_root(file: &RFile, name: &str) -> Result<Self> {
        let (class, object) = object_bytes_any(file, name)?;
        decode_tf1(name, &class, &object)
    }
    fn read_root_in(file: &RFile, dir: &str, name: &str) -> Result<Self> {
        let (class, object) = file.object_in(dir, name)?;
        decode_tf1(name, &class, &object)
    }
}

impl ReadRoot for TF2 {
    fn read_root(file: &RFile, name: &str) -> Result<Self> {
        let (class, object) = object_bytes_any(file, name)?;
        decode_tf2(name, &class, &object)
    }
    fn read_root_in(file: &RFile, dir: &str, name: &str) -> Result<Self> {
        let (class, object) = file.object_in(dir, name)?;
        decode_tf2(name, &class, &object)
    }
}

impl ReadRoot for TF3 {
    fn read_root(file: &RFile, name: &str) -> Result<Self> {
        let (class, object) = object_bytes_any(file, name)?;
        decode_tf3(name, &class, &object)
    }
    fn read_root_in(file: &RFile, dir: &str, name: &str) -> Result<Self> {
        let (class, object) = file.object_in(dir, name)?;
        decode_tf3(name, &class, &object)
    }
}
