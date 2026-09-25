//! ROOT I/O for the function types: the `WriteRoot`/`ReadRoot` impls, the
//! `Func2D`/`Func3D` object bodies around the shared `Func1D` record, and the
//! `TStreamerInfo`s a file storing them embeds.

use oxiroot_hist::{hist_streamer_classes, GraphFunction};
use oxiroot_io_core::streamer_gen::{any, base, basic, objanyptr, objptr, stl, strf, Cls};
use oxiroot_io_core::{
    object_bytes_any, Error, FileReader, RBuffer, ReadRoot, Result, WBuffer, WriteRoot,
};

use crate::func::{Func1D, Func2D, Func3D, FuncCore};

// --- write ------------------------------------------------------------------

impl WriteRoot for Func1D {
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
    fn streamer_classes(&self) -> Vec<Cls<'static>> {
        function_classes(1)
    }
}

impl WriteRoot for Func2D {
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
    fn streamer_classes(&self) -> Vec<Cls<'static>> {
        function_classes(2)
    }
}

impl WriteRoot for Func3D {
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
    fn streamer_classes(&self) -> Vec<Cls<'static>> {
        function_classes(3)
    }
}

/// Write a `Func2D` object body (version 4): the `Func1D` base (`fNdim` = `ndim`,
/// `fNpx` = 30), then `fYmin`/`fYmax`, `fNpy`, and an empty `fContour`
/// (`TArrayD`).
fn write_tf2_body(w: &mut WBuffer, f: &GraphFunction, ndim: i32, ymin: f64, ymax: f64) {
    let obj = w.begin_object(4); // Func2D version 4
    f.write_tf1_body(w, ndim, 30); // Func1D base
    w.be_f64(ymin); // fYmin
    w.be_f64(ymax); // fYmax
    w.be_i32(30); // fNpy (ROOT keeps npx == npy by default)
    w.be_i32(0); // fContour: empty TArrayD (fN = 0)
    w.end_object(obj);
}

/// Write a `Func3D` object body (version 3): the `Func2D` base (`fNdim` = 3), then
/// `fZmin`/`fZmax` and `fNpz`.
fn write_tf3_body(w: &mut WBuffer, f: &GraphFunction, ymin: f64, ymax: f64, zmin: f64, zmax: f64) {
    let obj = w.begin_object(3); // Func3D version 3
    write_tf2_body(w, f, 3, ymin, ymax); // Func2D base
    w.be_f64(zmin); // fZmin
    w.be_f64(zmax); // fZmax
    w.be_i32(30); // fNpz
    w.end_object(obj);
}

// ROOT C++ has these classes compiled in; uproot builds a function model from
// its streamer, so a file storing a Func1D/Func2D/Func3D embeds them (versions and
// checksums as ROOT writes them).

/// The `TStreamerInfo`s a `Func1D` (`dim` 1), `Func2D` or `Func3D` needs: its formula,
/// then its base classes deepest first, then itself.
/// The classes a `dim`-dimensional function needs: ROOT's captured `TF1`, with
/// its `TFormula`, `TF1Parameters` and bases, then the generated `TF2`/`TF3`.
/// The generated `TFormula` and `TF1` are the same class versions as the
/// captured ones, which a file describes once.
fn function_classes(dim: usize) -> Vec<Cls<'static>> {
    let mut classes = hist_streamer_classes(&["TF1"]);
    for class in tf_classes(dim) {
        if !classes
            .iter()
            .any(|c| c.name == class.name && c.version == class.version)
        {
            classes.push(class);
        }
    }
    classes
}

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

/// Read a `Func2D` object body (version 4): the `Func1D` base then `fYmin`/`fYmax`.
fn read_tf2_body(r: &mut RBuffer) -> Result<(GraphFunction, f64, f64)> {
    let tf2 = r.read_version()?; // Func2D v4
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

fn decode_tf1(name: &str, class: &str, object: &[u8]) -> Result<Func1D> {
    if class != "TF1" {
        return Err(Error::WrongClass {
            name: name.to_string(),
            found: class.to_string(),
            expected: "TF1".to_string(),
        });
    }
    let mut r = RBuffer::new(object);
    Func1D::from_graph_function(GraphFunction::read_tf1_body(&mut r)?)
}

fn decode_tf2(name: &str, class: &str, object: &[u8]) -> Result<Func2D> {
    if class != "TF2" {
        return Err(Error::WrongClass {
            name: name.to_string(),
            found: class.to_string(),
            expected: "TF2".to_string(),
        });
    }
    let mut r = RBuffer::new(object);
    let (d, ymin, ymax) = read_tf2_body(&mut r)?;
    let (xmin, xmax) = (d.xmin, d.xmax);
    Ok(Func2D {
        core: FuncCore::from_record(d)?,
        xmin,
        xmax,
        ymin,
        ymax,
    })
}

fn decode_tf3(name: &str, class: &str, object: &[u8]) -> Result<Func3D> {
    if class != "TF3" {
        return Err(Error::WrongClass {
            name: name.to_string(),
            found: class.to_string(),
            expected: "TF3".to_string(),
        });
    }
    let mut r = RBuffer::new(object);
    let _tf3 = r.read_version()?; // Func3D v3
    let (d, ymin, ymax) = read_tf2_body(&mut r)?;
    let zmin = r.be_f64()?;
    let zmax = r.be_f64()?;
    let _npz = r.be_i32()?;
    let (xmin, xmax) = (d.xmin, d.xmax);
    Ok(Func3D {
        core: FuncCore::from_record(d)?,
        xmin,
        xmax,
        ymin,
        ymax,
        zmin,
        zmax,
    })
}

impl ReadRoot for Func1D {
    fn read_root(file: &FileReader, name: &str) -> Result<Self> {
        let (class, object) = object_bytes_any(file, name)?;
        decode_tf1(name, &class, &object)
    }
    fn read_root_in(file: &FileReader, dir: &str, name: &str) -> Result<Self> {
        let (class, object) = file.object_in(dir, name)?;
        decode_tf1(name, &class, &object)
    }
}

impl ReadRoot for Func2D {
    fn read_root(file: &FileReader, name: &str) -> Result<Self> {
        let (class, object) = object_bytes_any(file, name)?;
        decode_tf2(name, &class, &object)
    }
    fn read_root_in(file: &FileReader, dir: &str, name: &str) -> Result<Self> {
        let (class, object) = file.object_in(dir, name)?;
        decode_tf2(name, &class, &object)
    }
}

impl ReadRoot for Func3D {
    fn read_root(file: &FileReader, name: &str) -> Result<Self> {
        let (class, object) = object_bytes_any(file, name)?;
        decode_tf3(name, &class, &object)
    }
    fn read_root_in(file: &FileReader, dir: &str, name: &str) -> Result<Self> {
        let (class, object) = file.object_in(dir, name)?;
        decode_tf3(name, &class, &object)
    }
}
