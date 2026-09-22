//! ROOT linear-algebra objects: [`TVectorD`] (a vector of doubles), [`TMatrixD`]
//! (a dense matrix), and [`TMatrixDSym`] (a symmetric matrix — the shape a fit's
//! covariance takes). All read and write byte-for-byte as ROOT serializes them
//! (the `TVectorT<double>` / `TMatrixT<double>` / `TMatrixTSym<double>` template
//! instantiations), so ROOT and uproot read what oxiroot writes and vice versa.
//!
//! This is a leaf crate: it owns these three types and their ROOT persistence,
//! and depends only on [`oxiroot_io_core`]. Read one back with
//! [`ReadRoot::read_root`]; write one with [`WriteRoot::write_root`] (or put
//! several objects in a file via the builder in `oxiroot-hist` / the `oxiroot`
//! facade).

use oxiroot_io_core::streamer_gen::{base, basic, basicptr, basicptr_in, Cls};
use oxiroot_io_core::{
    object_bytes_any, read_tobject, write_tobject, Error, FileReader, FromMember, RBuffer,
    ReadRoot, Result, WBuffer, WriteRoot,
};

/// `fTol` ROOT stores in a matrix base (`TMatrixTBase::fTol`), its default
/// `DBL_EPSILON`. Matched so written files equal ROOT's byte-for-byte.
const MATRIX_TOL: f64 = f64::EPSILON;

/// The most rows, columns or elements a matrix can have: ROOT stores `fNrows`,
/// `fNcols` and `fNelems` as `Int_t`.
const MAX_DIM: usize = i32::MAX as usize;

/// The element count of an `nrows`×`ncols` matrix, or an error if ROOT cannot
/// store a matrix that large.
fn element_count(what: &str, nrows: usize, ncols: usize) -> Result<usize> {
    nrows
        .checked_mul(ncols)
        .filter(|&count| nrows <= MAX_DIM && ncols <= MAX_DIM && count <= MAX_DIM)
        .ok_or_else(|| {
            Error::InvalidInput(format!(
                "{what}: a {nrows}x{ncols} matrix is larger than ROOT can store \
                 (at most {MAX_DIM} rows, columns and elements)"
            ))
        })
}

/// `Ok` if `elements` has the `expected` length, else [`Error::LengthMismatch`].
fn check_len(what: String, expected: usize, found: usize) -> Result<()> {
    if found == expected {
        Ok(())
    } else {
        Err(Error::LengthMismatch {
            what,
            expected,
            found,
        })
    }
}

/// A dimension read from a file: ROOT's `Int_t`, which must not be negative.
fn read_dim(r: &mut RBuffer, field: &str) -> Result<usize> {
    let v = r.be_i32()?;
    usize::try_from(v).map_err(|_| Error::Format(format!("negative matrix {field} ({v})")))
}

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
    let nrows = read_dim(r, "fNrows")?;
    let ncols = read_dim(r, "fNcols")?;
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
    fn streamer_classes(&self) -> Vec<Cls<'static>> {
        streamer_classes("TVectorT<double>")
    }
}

impl ReadRoot for TVectorD {
    fn read_root(file: &FileReader, name: &str) -> Result<TVectorD> {
        let (class, object) = object_bytes_any(file, name)?;
        decode_tvectord(name, &class, &object)
    }
    fn read_root_in(file: &FileReader, dir: &str, name: &str) -> Result<TVectorD> {
        let (class, object) = file.object_in(dir, name)?;
        decode_tvectord(name, &class, &object)
    }
}

/// Decode a `TVectorT<double>` object body (as stored under a key) into a
/// [`TVectorD`]. `class` is checked; `object` is the decompressed payload.
pub fn decode_tvectord(name: &str, class: &str, object: &[u8]) -> Result<TVectorD> {
    if class != "TVectorT<double>" {
        return Err(Error::WrongClass {
            name: name.to_string(),
            found: class.to_string(),
            expected: "TVectorD".to_string(),
        });
    }
    let mut r = RBuffer::new(object);
    r.read_version()?; // TVectorT version
    read_tobject(&mut r)?;
    let nrows = read_dim(&mut r, "fNrows")?;
    r.be_i32()?; // fRowLwb
    r.u8()?; // is-array flag
    let elements = (0..nrows).map(|_| r.be_f64()).collect::<Result<_>>()?;
    Ok(TVectorD {
        name: name.to_string(),
        elements,
    })
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
    /// # Errors
    /// [`Error::LengthMismatch`] if `elements.len() != nrows * ncols`, and
    /// [`Error::Format`] if the matrix has more rows, columns or elements than
    /// ROOT can store (`i32::MAX`).
    pub fn new(nrows: usize, ncols: usize, elements: impl Into<Vec<f64>>) -> Result<TMatrixD> {
        let elements = elements.into();
        let expected = element_count("TMatrixD", nrows, ncols)?;
        check_len(
            format!("TMatrixD {nrows}x{ncols} elements"),
            expected,
            elements.len(),
        )?;
        Ok(TMatrixD {
            name: String::new(),
            nrows,
            ncols,
            elements,
        })
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
    ///
    /// # Panics
    /// If `i` or `j` is out of range.
    pub fn get(&self, i: usize, j: usize) -> f64 {
        assert!(
            i < self.nrows && j < self.ncols,
            "TMatrixD::get({i}, {j}) out of range for a {}x{} matrix",
            self.nrows,
            self.ncols
        );
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
    fn streamer_classes(&self) -> Vec<Cls<'static>> {
        streamer_classes("TMatrixT<double>")
    }
}

impl ReadRoot for TMatrixD {
    fn read_root(file: &FileReader, name: &str) -> Result<TMatrixD> {
        let (class, object) = object_bytes_any(file, name)?;
        decode_tmatrixd(name, &class, &object)
    }
    fn read_root_in(file: &FileReader, dir: &str, name: &str) -> Result<TMatrixD> {
        let (class, object) = file.object_in(dir, name)?;
        decode_tmatrixd(name, &class, &object)
    }
}

/// Decode a `TMatrixT<double>` object body into a [`TMatrixD`].
pub fn decode_tmatrixd(name: &str, class: &str, object: &[u8]) -> Result<TMatrixD> {
    if class != "TMatrixT<double>" {
        return Err(Error::WrongClass {
            name: name.to_string(),
            found: class.to_string(),
            expected: "TMatrixD".to_string(),
        });
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

// --- TMatrixDSym ------------------------------------------------------------

/// A `TMatrixDSym` — a symmetric `n`×`n` matrix of `f64` (ROOT's
/// `TMatrixTSym<double>`), the shape a fit's covariance matrix takes. Stored as
/// the full `n*n` row-major matrix in memory; on disk ROOT writes only the upper
/// triangle, which this type expands and re-packs. The lower triangle is always
/// the mirror of the upper one, so a matrix reads back exactly as built.
#[derive(Debug, Clone, PartialEq)]
pub struct TMatrixDSym {
    name: String,
    n: usize,
    /// The full `n*n` matrix, row-major (`elements[i*n + j] == elements[j*n + i]`).
    elements: Vec<f64>,
}

impl TMatrixDSym {
    /// A symmetric matrix from the full `n*n` row-major `elements`. Only the
    /// upper triangle (`j >= i`) is used, as ROOT writes only that triangle: the
    /// lower one is set to its mirror image.
    ///
    /// # Errors
    /// [`Error::LengthMismatch`] if `elements.len() != n * n`, and
    /// [`Error::Format`] if the matrix has more rows or elements than ROOT can
    /// store (`i32::MAX`).
    pub fn new(n: usize, elements: impl Into<Vec<f64>>) -> Result<TMatrixDSym> {
        let mut elements = elements.into();
        let expected = element_count("TMatrixDSym", n, n)?;
        check_len(
            format!("TMatrixDSym {n}x{n} elements"),
            expected,
            elements.len(),
        )?;
        for i in 0..n {
            for j in 0..i {
                elements[i * n + j] = elements[j * n + i];
            }
        }
        Ok(TMatrixDSym {
            name: String::new(),
            n,
            elements,
        })
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
    ///
    /// # Panics
    /// If `i` or `j` is out of range.
    pub fn get(&self, i: usize, j: usize) -> f64 {
        assert!(
            i < self.n && j < self.n,
            "TMatrixDSym::get({i}, {j}) out of range for a {n}x{n} matrix",
            n = self.n
        );
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
    fn streamer_classes(&self) -> Vec<Cls<'static>> {
        streamer_classes("TMatrixTSym<double>")
    }
}

impl ReadRoot for TMatrixDSym {
    fn read_root(file: &FileReader, name: &str) -> Result<TMatrixDSym> {
        let (class, object) = object_bytes_any(file, name)?;
        decode_tmatrixdsym(name, &class, &object)
    }
    fn read_root_in(file: &FileReader, dir: &str, name: &str) -> Result<TMatrixDSym> {
        let (class, object) = file.object_in(dir, name)?;
        decode_tmatrixdsym(name, &class, &object)
    }
}

/// Decode a `TMatrixTSym<double>` object body (upper triangle on disk) into a
/// full [`TMatrixDSym`].
pub fn decode_tmatrixdsym(name: &str, class: &str, object: &[u8]) -> Result<TMatrixDSym> {
    if class != "TMatrixTSym<double>" {
        return Err(Error::WrongClass {
            name: name.to_string(),
            found: class.to_string(),
            expected: "TMatrixDSym".to_string(),
        });
    }
    let mut r = RBuffer::new(object);
    let (n, ncols) = read_matrix_base(&mut r)?;
    if ncols != n {
        return Err(Error::Format(format!(
            "key {name:?}: a TMatrixDSym must be square, not {n}x{ncols}"
        )));
    }
    // The upper triangle, row-major (n(n+1)/2 elements), expanded to the full
    // symmetric matrix. Check the triangle is all there before allocating the
    // n*n matrix, which a corrupt header could make enormous.
    let needed = n
        .checked_mul(n + 1)
        .and_then(|twice| (twice / 2).checked_mul(8))
        .ok_or_else(|| Error::Format(format!("key {name:?}: TMatrixDSym size {n} overflows")))?;
    if r.remaining() < needed {
        return Err(Error::UnexpectedEof {
            needed,
            available: r.remaining(),
        });
    }
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

// --- Collection members -----------------------------------------------------

impl FromMember for TVectorD {
    fn from_member(class: &str, bytes: &[u8]) -> Option<Result<Self>> {
        (class == "TVectorT<double>").then(|| decode_tvectord("", class, bytes))
    }
}

impl FromMember for TMatrixD {
    fn from_member(class: &str, bytes: &[u8]) -> Option<Result<Self>> {
        (class == "TMatrixT<double>").then(|| decode_tmatrixd("", class, bytes))
    }
}

impl FromMember for TMatrixDSym {
    fn from_member(class: &str, bytes: &[u8]) -> Option<Result<Self>> {
        (class == "TMatrixTSym<double>").then(|| decode_tmatrixdsym("", class, bytes))
    }
}

// --- Streamer info ----------------------------------------------------------

/// The `TStreamerInfo` [`Cls`] entries describing a matrix/vector `class` — the
/// deepest base first — so a written file is self-describing (uproot reads it;
/// ROOT C++ uses its own compiled streamers). Returns an empty vector for a
/// non-matrix class. Each matrix/vector type's
/// [`WriteRoot::streamer_classes`] returns these.
pub fn streamer_classes(class: &str) -> Vec<Cls<'static>> {
    // `TMatrixTBase<double>` — the dimensions base shared by the matrix classes.
    let matrix_base = || Cls {
        name: "TMatrixTBase<double>".into(),
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
        "TVectorT<double>" => vec![Cls {
            name: "TVectorT<double>".into(),
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
                name: "TMatrixT<double>".into(),
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
        "TMatrixTSym<double>" | "TMatrixTBase<double>" => vec![matrix_base()],
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_wrong_element_count_is_an_error() {
        let Err(Error::LengthMismatch {
            expected, found, ..
        }) = TMatrixD::new(2, 3, vec![1.0; 5])
        else {
            panic!("TMatrixD::new accepted 5 elements for a 2x3 matrix");
        };
        assert_eq!((expected, found), (6, 5));

        let Err(Error::LengthMismatch {
            expected, found, ..
        }) = TMatrixDSym::new(2, vec![1.0; 3])
        else {
            panic!("TMatrixDSym::new accepted 3 elements for a 2x2 matrix");
        };
        assert_eq!((expected, found), (4, 3));
    }

    #[test]
    fn a_matrix_too_large_for_root_is_an_error() {
        // fNrows/fNcols/fNelems are Int_t: one more than i32::MAX must not be
        // truncated on write, and a product that overflows must not wrap.
        let big = i32::MAX as usize + 1;
        assert!(matches!(
            TMatrixD::new(big, 0, vec![]),
            Err(Error::InvalidInput(_))
        ));
        assert!(matches!(
            TMatrixD::new(0, big, vec![]),
            Err(Error::InvalidInput(_))
        ));
        assert!(matches!(
            TMatrixD::new(1 << 16, 1 << 16, vec![]),
            Err(Error::InvalidInput(_))
        ));
        assert!(matches!(
            TMatrixD::new(usize::MAX, 2, vec![]),
            Err(Error::InvalidInput(_))
        ));
        assert!(matches!(
            TMatrixDSym::new(big, vec![]),
            Err(Error::InvalidInput(_))
        ));
    }

    #[test]
    fn empty_matrices_are_fine() {
        assert_eq!(
            TMatrixD::new(0, 0, vec![]).unwrap().elements(),
            &[] as &[f64]
        );
        assert_eq!(TMatrixD::new(3, 0, vec![]).unwrap().rows(), 3);
        assert_eq!(TMatrixDSym::new(0, vec![]).unwrap().dim(), 0);
    }

    #[test]
    fn the_lower_triangle_mirrors_the_upper_one() {
        // ROOT writes only the upper triangle, so that is the triangle used: the
        // matrix in memory is the one that reads back.
        let s = TMatrixDSym::new(2, vec![1.0, 0.5, 0.25, 2.0])
            .unwrap()
            .named("s");
        assert_eq!(s.elements(), &[1.0, 0.5, 0.5, 2.0]);
        let back = decode_tmatrixdsym("s", "TMatrixTSym<double>", &s.to_root_bytes()).unwrap();
        assert_eq!(back, s);
    }

    #[test]
    #[should_panic(expected = "out of range")]
    fn get_rejects_a_column_past_the_end() {
        // (0, 3) of a 2x3 matrix is not element (1, 0).
        TMatrixD::new(2, 3, vec![0.0; 6]).unwrap().get(0, 3);
    }

    /// A `TMatrixTSym<double>` body claiming an `n`×`ncols` matrix, followed by
    /// `payload` bytes.
    fn sym_body(n: i32, ncols: i32, payload: &[u8]) -> Vec<u8> {
        let mut w = WBuffer::new();
        let base = w.begin_object(5);
        write_tobject(&mut w, 0);
        w.be_i32(n);
        w.be_i32(ncols);
        w.be_i32(0);
        w.be_i32(0);
        w.be_i32(n.wrapping_mul(ncols));
        w.be_i32(0);
        w.be_f64(MATRIX_TOL);
        w.end_object(base);
        let mut bytes = w.into_vec();
        bytes.extend_from_slice(payload);
        bytes
    }

    #[test]
    fn a_huge_claimed_dimension_fails_before_allocating() {
        // A corrupt header claiming i32::MAX rows used to allocate the n²
        // doubles up front: a capacity-overflow panic here, and an abort for
        // a smaller n that still does not fit in memory.
        let body = sym_body(i32::MAX, i32::MAX, &[0; 16]);
        assert!(matches!(
            decode_tmatrixdsym("s", "TMatrixTSym<double>", &body),
            Err(Error::UnexpectedEof { .. })
        ));
    }

    #[test]
    fn a_malformed_symmetric_header_is_an_error() {
        let class = "TMatrixTSym<double>";
        assert!(matches!(
            decode_tmatrixdsym("s", class, &sym_body(2, 3, &[0; 48])),
            Err(Error::Format(_))
        ));
        assert!(matches!(
            decode_tmatrixdsym("s", class, &sym_body(-1, -1, &[])),
            Err(Error::Format(_))
        ));
    }
}
