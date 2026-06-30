//! Linear-algebra objects: [`TVectorD`] (a vector of doubles), [`TMatrixD`] (a
//! dense matrix), and [`TMatrixDSym`] (a symmetric matrix — the shape a fit's
//! covariance takes). All read and write byte-for-byte as ROOT serializes them
//! (the `TVectorT<double>` / `TMatrixT<double>` / `TMatrixTSym<double>` template
//! instantiations), so ROOT and uproot read what oxiroot writes and vice versa.

use oxiroot_io_core::buffer::{RBuffer, WBuffer};
use oxiroot_io_core::error::{Error, Result};
use oxiroot_io_core::streamer::{read_tobject, write_tobject};
use oxiroot_io_core::RFile;

use crate::base::object_bytes_any;
use crate::write::WriteRoot;

/// `fTol` ROOT stores in a matrix base (`TMatrixTBase::fTol`), its default
/// `DBL_EPSILON`. Matched so written files equal ROOT's byte-for-byte.
const MATRIX_TOL: f64 = f64::EPSILON;

/// Write the seven `TMatrixTBase<double>` dimension fields (a byte-counted
/// `TObject` + dims + `fTol`) for an `nrows`×`ncols` matrix.
fn write_matrix_base(w: &mut WBuffer, nrows: usize, ncols: usize) {
    let base = w.begin_object(5); // TMatrixTBase<double> version 5
    write_tobject(w, 0);
    w.be_i32(nrows as i32); // fNrows
    w.be_i32(ncols as i32); // fNcols
    w.be_i32(0); // fRowLwb
    w.be_i32(0); // fColLwb
    w.be_i32((nrows * ncols) as i32); // fNelems
    w.be_i32(0); // fNrowIndex
    w.be_f64(MATRIX_TOL); // fTol
    w.end_object(base);
}

/// Read the `TMatrixTBase<double>` dimension fields, returning `(nrows, ncols)`.
/// The cursor must sit at the base's version header.
fn read_matrix_base(r: &mut RBuffer) -> Result<(usize, usize)> {
    r.read_version()?; // TMatrixTBase version
    read_tobject(r)?;
    let nrows = r.be_i32()?.max(0) as usize;
    let ncols = r.be_i32()?.max(0) as usize;
    r.be_i32()?; // fRowLwb
    r.be_i32()?; // fColLwb
    r.be_i32()?; // fNelems
    r.be_i32()?; // fNrowIndex
    r.be_f64()?; // fTol
    Ok((nrows, ncols))
}

// --- TVectorD ---------------------------------------------------------------

/// A `TVectorD` — a dense vector of `f64` (ROOT's `TVectorT<double>`). Build with
/// [`TVectorD::new`] and name it with [`named`](TVectorD::named).
#[derive(Debug, Clone, PartialEq)]
pub struct TVectorD {
    name: String,
    elements: Vec<f64>,
}

impl TVectorD {
    /// A vector holding `elements` (give it a key name with [`named`](Self::named)).
    pub fn new(elements: impl Into<Vec<f64>>) -> TVectorD {
        TVectorD {
            name: String::new(),
            elements: elements.into(),
        }
    }

    /// Set the key name this vector is stored under.
    #[must_use]
    pub fn named(mut self, name: impl Into<String>) -> TVectorD {
        self.name = name.into();
        self
    }

    /// The key name.
    pub fn name(&self) -> &str {
        &self.name
    }
    /// The elements.
    pub fn elements(&self) -> &[f64] {
        &self.elements
    }
    /// The number of elements.
    pub fn len(&self) -> usize {
        self.elements.len()
    }
    /// Whether the vector is empty.
    pub fn is_empty(&self) -> bool {
        self.elements.is_empty()
    }
}

impl WriteRoot for TVectorD {
    fn root_class(&self) -> String {
        "TVectorT<double>".to_string()
    }
    fn root_name(&self) -> &str {
        &self.name
    }
    fn root_title(&self) -> &str {
        ""
    }
    fn to_root_bytes(&self) -> Vec<u8> {
        let mut w = WBuffer::new();
        let obj = w.begin_object(4); // TVectorT<double> version 4
        write_tobject(&mut w, 0);
        w.be_i32(self.elements.len() as i32); // fNrows
        w.be_i32(0); // fRowLwb
        w.u8(1); // fElements is-array flag
        for &e in &self.elements {
            w.be_f64(e);
        }
        w.end_object(obj);
        w.into_vec()
    }
}

fn decode_tvectord(name: &str, class: &str, object: &[u8]) -> Result<TVectorD> {
    if class != "TVectorT<double>" {
        return Err(Error::Format(format!(
            "key {name:?} is a {class}, not a TVectorD"
        )));
    }
    let mut r = RBuffer::new(object);
    r.read_version()?; // TVectorT version
    read_tobject(&mut r)?;
    let nrows = r.be_i32()?.max(0) as usize;
    r.be_i32()?; // fRowLwb
    r.u8()?; // is-array flag
    let elements = (0..nrows).map(|_| r.be_f64()).collect::<Result<_>>()?;
    Ok(TVectorD {
        name: name.to_string(),
        elements,
    })
}

pub(crate) fn read_tvectord(file: &RFile, name: &str) -> Result<TVectorD> {
    let (class, object) = object_bytes_any(file, name)?;
    decode_tvectord(name, &class, &object)
}

pub(crate) fn read_tvectord_in(file: &RFile, subdir: &str, name: &str) -> Result<TVectorD> {
    let (class, object) = file.object_in(subdir, name)?;
    decode_tvectord(name, &class, &object)
}

// --- TMatrixD ---------------------------------------------------------------

/// A `TMatrixD` — a dense `nrows`×`ncols` matrix of `f64` (ROOT's
/// `TMatrixT<double>`), stored row-major.
#[derive(Debug, Clone, PartialEq)]
pub struct TMatrixD {
    name: String,
    nrows: usize,
    ncols: usize,
    elements: Vec<f64>,
}

impl TMatrixD {
    /// A matrix from `elements` in row-major order (`nrows * ncols` of them).
    ///
    /// # Panics
    /// If `elements.len() != nrows * ncols`.
    pub fn new(nrows: usize, ncols: usize, elements: impl Into<Vec<f64>>) -> TMatrixD {
        let elements = elements.into();
        assert_eq!(
            elements.len(),
            nrows * ncols,
            "TMatrixD: {nrows}x{ncols} needs {} elements, got {}",
            nrows * ncols,
            elements.len()
        );
        TMatrixD {
            name: String::new(),
            nrows,
            ncols,
            elements,
        }
    }

    /// Set the key name this matrix is stored under.
    #[must_use]
    pub fn named(mut self, name: impl Into<String>) -> TMatrixD {
        self.name = name.into();
        self
    }

    /// The key name.
    pub fn name(&self) -> &str {
        &self.name
    }
    /// The number of rows.
    pub fn rows(&self) -> usize {
        self.nrows
    }
    /// The number of columns.
    pub fn cols(&self) -> usize {
        self.ncols
    }
    /// The element at row `i`, column `j`.
    pub fn get(&self, i: usize, j: usize) -> f64 {
        self.elements[i * self.ncols + j]
    }
    /// The elements, row-major.
    pub fn elements(&self) -> &[f64] {
        &self.elements
    }
}

impl WriteRoot for TMatrixD {
    fn root_class(&self) -> String {
        "TMatrixT<double>".to_string()
    }
    fn root_name(&self) -> &str {
        &self.name
    }
    fn root_title(&self) -> &str {
        ""
    }
    fn to_root_bytes(&self) -> Vec<u8> {
        let mut w = WBuffer::new();
        let obj = w.begin_object(4); // TMatrixT<double> version 4
        write_matrix_base(&mut w, self.nrows, self.ncols);
        w.u8(1); // fElements is-array flag
        for &e in &self.elements {
            w.be_f64(e);
        }
        w.end_object(obj);
        w.into_vec()
    }
}

fn decode_tmatrixd(name: &str, class: &str, object: &[u8]) -> Result<TMatrixD> {
    if class != "TMatrixT<double>" {
        return Err(Error::Format(format!(
            "key {name:?} is a {class}, not a TMatrixD"
        )));
    }
    let mut r = RBuffer::new(object);
    r.read_version()?; // TMatrixT version (outer)
    let (nrows, ncols) = read_matrix_base(&mut r)?;
    r.u8()?; // is-array flag
    let elements = (0..nrows * ncols)
        .map(|_| r.be_f64())
        .collect::<Result<_>>()?;
    Ok(TMatrixD {
        name: name.to_string(),
        nrows,
        ncols,
        elements,
    })
}

pub(crate) fn read_tmatrixd(file: &RFile, name: &str) -> Result<TMatrixD> {
    let (class, object) = object_bytes_any(file, name)?;
    decode_tmatrixd(name, &class, &object)
}

pub(crate) fn read_tmatrixd_in(file: &RFile, subdir: &str, name: &str) -> Result<TMatrixD> {
    let (class, object) = file.object_in(subdir, name)?;
    decode_tmatrixd(name, &class, &object)
}

// --- TMatrixDSym ------------------------------------------------------------

/// A `TMatrixDSym` — a symmetric `n`×`n` matrix of `f64` (ROOT's
/// `TMatrixTSym<double>`), the shape a fit's covariance matrix takes. Stored as
/// the full `n*n` row-major matrix in memory; on disk ROOT writes only the upper
/// triangle, which this type expands and re-packs.
#[derive(Debug, Clone, PartialEq)]
pub struct TMatrixDSym {
    name: String,
    n: usize,
    /// The full `n*n` matrix, row-major (`elements[i*n + j] == elements[j*n + i]`).
    elements: Vec<f64>,
}

impl TMatrixDSym {
    /// A symmetric matrix from the full `n*n` row-major `elements`. The matrix is
    /// assumed symmetric; only the upper triangle is written.
    ///
    /// # Panics
    /// If `elements.len() != n * n`.
    pub fn new(n: usize, elements: impl Into<Vec<f64>>) -> TMatrixDSym {
        let elements = elements.into();
        assert_eq!(
            elements.len(),
            n * n,
            "TMatrixDSym: {n}x{n} needs {} elements, got {}",
            n * n,
            elements.len()
        );
        TMatrixDSym {
            name: String::new(),
            n,
            elements,
        }
    }

    /// Set the key name this matrix is stored under.
    #[must_use]
    pub fn named(mut self, name: impl Into<String>) -> TMatrixDSym {
        self.name = name.into();
        self
    }

    /// The key name.
    pub fn name(&self) -> &str {
        &self.name
    }
    /// The dimension `n` (the matrix is `n`×`n`).
    pub fn dim(&self) -> usize {
        self.n
    }
    /// The element at row `i`, column `j`.
    pub fn get(&self, i: usize, j: usize) -> f64 {
        self.elements[i * self.n + j]
    }
    /// The full `n*n` elements, row-major.
    pub fn elements(&self) -> &[f64] {
        &self.elements
    }
}

impl WriteRoot for TMatrixDSym {
    fn root_class(&self) -> String {
        "TMatrixTSym<double>".to_string()
    }
    fn root_name(&self) -> &str {
        &self.name
    }
    fn root_title(&self) -> &str {
        ""
    }
    fn to_root_bytes(&self) -> Vec<u8> {
        // ROOT's TMatrixTSym Streamer: the base (with its own version header),
        // then the upper triangle row-major — no outer version header, no
        // is-array flag.
        let mut w = WBuffer::new();
        write_matrix_base(&mut w, self.n, self.n);
        for i in 0..self.n {
            for j in i..self.n {
                w.be_f64(self.elements[i * self.n + j]);
            }
        }
        w.into_vec()
    }
}

fn decode_tmatrixdsym(name: &str, class: &str, object: &[u8]) -> Result<TMatrixDSym> {
    if class != "TMatrixTSym<double>" {
        return Err(Error::Format(format!(
            "key {name:?} is a {class}, not a TMatrixDSym"
        )));
    }
    let mut r = RBuffer::new(object);
    let (n, _ncols) = read_matrix_base(&mut r)?;
    // The upper triangle, row-major (n(n+1)/2 elements), expanded to the full
    // symmetric matrix.
    let mut elements = vec![0.0; n * n];
    for i in 0..n {
        for j in i..n {
            let v = r.be_f64()?;
            elements[i * n + j] = v;
            elements[j * n + i] = v;
        }
    }
    Ok(TMatrixDSym {
        name: name.to_string(),
        n,
        elements,
    })
}

pub(crate) fn read_tmatrixdsym(file: &RFile, name: &str) -> Result<TMatrixDSym> {
    let (class, object) = object_bytes_any(file, name)?;
    decode_tmatrixdsym(name, &class, &object)
}

pub(crate) fn read_tmatrixdsym_in(file: &RFile, subdir: &str, name: &str) -> Result<TMatrixDSym> {
    let (class, object) = file.object_in(subdir, name)?;
    decode_tmatrixdsym(name, &class, &object)
}
