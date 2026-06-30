//! Shared building blocks for the classic histogram hierarchy.
//!
//! `TH1` and its `ClassDef` bases each carry a `{byte-count, version}` header,
//! so we read the members we need and seek to `TH1`'s end. `TArray*` bin
//! contents are streamed inline — just a count and the values, no header.

use oxiroot_io_core::buffer::RBuffer;
use oxiroot_io_core::error::{Error, Result};
use oxiroot_io_core::streamer::{read_tnamed, skip_versioned};
use oxiroot_io_core::RFile;

use crate::axis::TAxis;

/// On-disk bin-content precision, named by a histogram class suffix
/// (`TH1**D**`, `TH2**F**`, …). Contents are always held in memory as `f64`;
/// this only selects the `TArray*` element type written to (and read from) the
/// file. The default is [`Precision::Double`] (ROOT's `TH1D`/`TH2D`/`TH3D`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[non_exhaustive]
pub enum Precision {
    /// `TArrayD` (`f64`) — the `D` classes.
    #[default]
    Double,
    /// `TArrayF` (`f32`) — the `F` classes.
    Float,
    /// `TArrayI` (`i32`) — the `I` classes.
    Int,
    /// `TArrayS` (`i16`) — the `S` classes.
    Short,
    /// `TArrayC` (`i8`) — the `C` classes.
    Char,
    /// `TArrayL64` (`i64`) — the `L` classes.
    Long,
}

impl Precision {
    /// The class-name suffix character for this precision (`'D'`, `'F'`, …).
    #[must_use]
    pub fn code(self) -> char {
        match self {
            Precision::Double => 'D',
            Precision::Float => 'F',
            Precision::Int => 'I',
            Precision::Short => 'S',
            Precision::Char => 'C',
            Precision::Long => 'L',
        }
    }

    /// The full ROOT class name for a histogram of dimension `dim` (`"TH1"`,
    /// `"TH2"`, `"TH3"`) at this precision, e.g. `Precision::Float.class_name("TH1") == "TH1F"`.
    #[must_use]
    pub fn class_name(self, dim: &str) -> String {
        let mut s = String::with_capacity(dim.len() + 1);
        s.push_str(dim);
        s.push(self.code());
        s
    }
}

/// Determine the bin-content type from a histogram class name's suffix
/// (`TH1D`/`TH2F`/`TH1I`/…). `TProfile` and similar are handled by their own
/// readers.
pub(crate) fn precision_of(class: &str) -> Result<Precision> {
    match class.chars().last() {
        Some('D') => Ok(Precision::Double),
        Some('F') => Ok(Precision::Float),
        Some('I') => Ok(Precision::Int),
        Some('S') => Ok(Precision::Short),
        Some('C') => Ok(Precision::Char),
        Some('L') => Ok(Precision::Long),
        _ => Err(Error::Format(format!(
            "unsupported histogram type: {class}"
        ))),
    }
}

/// The members shared by every `TH1`-derived histogram.
#[derive(Debug, Clone, PartialEq)]
pub struct TH1Core {
    /// Histogram name (`fName`).
    pub name: String,
    /// Histogram title (`fTitle`).
    pub title: String,
    /// X axis.
    pub xaxis: TAxis,
    /// Y axis.
    pub yaxis: TAxis,
    /// Z axis.
    pub zaxis: TAxis,
    /// Total number of cells, including flow (`fNcells`).
    pub ncells: i32,
    /// Number of entries (`fEntries`).
    pub entries: f64,
    /// Sum of weights (`fTsumw`).
    pub tsumw: f64,
    /// Sum of squared weights (`fTsumw2`).
    pub tsumw2: f64,
    /// Sum of weight*x (`fTsumwx`).
    pub tsumwx: f64,
    /// Sum of weight*x^2 (`fTsumwx2`).
    pub tsumwx2: f64,
    /// Per-bin sum of squared weights (`fSumw2`); empty for an unweighted
    /// histogram, but used by `TProfile` to store the per-bin sum of `y^2`.
    pub sumw2: Vec<f64>,
}

/// Read a `TH1` base object (its header, the `TNamed`/`TAtt*` bases, and the
/// members up to the core statistics), then seek to the `TH1` record's end.
pub(crate) fn read_th1_base(r: &mut RBuffer) -> Result<TH1Core> {
    let th1 = r.read_version()?;

    let named = read_tnamed(r)?;
    skip_versioned(r)?; // TAttLine
    skip_versioned(r)?; // TAttFill
    skip_versioned(r)?; // TAttMarker

    let ncells = r.be_i32()?;
    let xaxis = TAxis::read(r)?;
    let yaxis = TAxis::read(r)?;
    let zaxis = TAxis::read(r)?;
    let _bar_offset = r.be_i16()?;
    let _bar_width = r.be_i16()?;
    let entries = r.be_f64()?;
    let tsumw = r.be_f64()?;
    let tsumw2 = r.be_f64()?;
    let tsumwx = r.be_f64()?;
    let tsumwx2 = r.be_f64()?;
    let _maximum = r.be_f64()?;
    let _minimum = r.be_f64()?;
    let _norm_factor = r.be_f64()?;
    let _contour = read_tarray(r, Precision::Double)?; // fContour
    let sumw2 = read_tarray(r, Precision::Double)?; // fSumw2

    let end = th1
        .end
        .ok_or_else(|| Error::Format("TH1 record has no byte count".into()))?;
    r.seek(end)?;

    Ok(TH1Core {
        name: named.name,
        title: named.title,
        xaxis,
        yaxis,
        zaxis,
        ncells,
        entries,
        tsumw,
        tsumw2,
        tsumwx,
        tsumwx2,
        sumw2,
    })
}

/// The number of cells (flow-inclusive) for the given per-axis bin counts:
/// `Π (nbins_i + 2)`, computed with overflow checking. A malformed file can
/// carry absurd `fNbins`, so this returns `Err` rather than wrapping.
pub(crate) fn cell_count(axis_nbins: &[i32]) -> Result<usize> {
    let mut total: usize = 1;
    for &n in axis_nbins {
        let cells = (n.max(0) as usize)
            .checked_add(2)
            .and_then(|c| total.checked_mul(c))
            .ok_or_else(|| Error::Format("histogram cell count overflows usize".into()))?;
        total = cells;
    }
    Ok(total)
}

/// Reject a histogram array whose length disagrees with its axis cell count,
/// so later flow-bin indexing (`contents[ix + stride*iy]`, etc.) is provably
/// in range. `optional` arrays (e.g. `fSumw2`) may also be empty.
pub(crate) fn check_cells(name: &str, len: usize, cells: usize, optional: bool) -> Result<()> {
    if len == cells || (optional && len == 0) {
        Ok(())
    } else {
        Err(Error::Format(format!(
            "histogram {name} length {len} does not match {cells} cells"
        )))
    }
}

/// Read an inline `TArray` of `n` values at the given precision (a count
/// followed by that many values, widened to `f64`).
pub(crate) fn read_tarray(r: &mut RBuffer, precision: Precision) -> Result<Vec<f64>> {
    let n = r.be_i32()?.max(0) as usize;
    // Cap the up-front reservation at what the buffer could possibly hold, so a
    // forged count can't drive a huge allocation before the read fails.
    let mut v = Vec::with_capacity(n.min(r.remaining()));
    for _ in 0..n {
        let value = match precision {
            Precision::Double => r.be_f64()?,
            Precision::Float => r.be_f32()? as f64,
            Precision::Int => r.be_i32()? as f64,
            Precision::Short => r.be_i16()? as f64,
            Precision::Char => r.i8()? as f64,
            Precision::Long => r.be_i64()? as f64,
        };
        v.push(value);
    }
    Ok(v)
}

/// Read a standalone `TH1x` object: its wrapper, the `TH1` base, and the inline
/// `TArray` bin contents; seek to the wrapper's end. Used both for a top-level
/// `TH1D`/`TH1F` and for the `TH1D` base inside a `TProfile`.
pub(crate) fn read_th1_object(
    r: &mut RBuffer,
    precision: Precision,
) -> Result<(TH1Core, Vec<f64>)> {
    let wrapper = r.read_version()?;
    let core = read_th1_base(r)?;
    let contents = read_tarray(r, precision)?;
    if let Some(end) = wrapper.end {
        r.seek(end)?;
    }
    Ok((core, contents))
}

/// Locate a key, verify its class, and return its decompressed object bytes.
pub(crate) fn object_bytes(file: &RFile, name: &str, class: &str) -> Result<Vec<u8>> {
    let key = file
        .key(name)
        .ok_or_else(|| Error::Format(format!("no key named {name:?}")))?;
    if key.class_name != class {
        return Err(Error::Format(format!(
            "key {name:?} is a {}, not {class}",
            key.class_name
        )));
    }
    let payload = key.payload(file.data())?;
    oxiroot_compress::decompress(payload, key.obj_len as usize)
        .map_err(|e| Error::Format(format!("decompressing {name:?}: {e}")))
}

/// Like [`object_bytes`], but also return the key's header length (`fKeyLen`).
///
/// ROOT keys objects relative to `-fKeyLen`, so the object-reference map (see
/// [`oxiroot_io_core::object::TagReader`]) needs the key length to resolve the
/// class/object back-references inside a streamed object (e.g. `TH2Poly`'s bins).
pub(crate) fn object_bytes_keyed(
    file: &RFile,
    name: &str,
    class: &str,
) -> Result<(Vec<u8>, usize)> {
    let key = file
        .key(name)
        .ok_or_else(|| Error::Format(format!("no key named {name:?}")))?;
    if key.class_name != class {
        return Err(Error::Format(format!(
            "key {name:?} is a {}, not {class}",
            key.class_name
        )));
    }
    let keylen = key.key_len as usize;
    let payload = key.payload(file.data())?;
    let object = oxiroot_compress::decompress(payload, key.obj_len as usize)
        .map_err(|e| Error::Format(format!("decompressing {name:?}: {e}")))?;
    Ok((object, keylen))
}

/// Return a key's class name together with its decompressed object bytes,
/// without checking the class.
pub(crate) fn object_bytes_any(file: &RFile, name: &str) -> Result<(String, Vec<u8>)> {
    let key = file
        .key(name)
        .ok_or_else(|| Error::Format(format!("no key named {name:?}")))?;
    let payload = key.payload(file.data())?;
    let object = oxiroot_compress::decompress(payload, key.obj_len as usize)
        .map_err(|e| Error::Format(format!("decompressing {name:?}: {e}")))?;
    Ok((key.class_name.clone(), object))
}

/// Like [`object_bytes_any`], but also return the key's header length, needed by
/// the object-reference map ([`oxiroot_io_core::object::TagReader`]) to resolve
/// the class back-references inside a collection (a `THStack`'s `TList` of
/// histograms, a `TMultiGraph`'s `TList` of graphs).
pub(crate) fn object_bytes_any_keyed(file: &RFile, name: &str) -> Result<(String, Vec<u8>, usize)> {
    let key = file
        .key(name)
        .ok_or_else(|| Error::Format(format!("no key named {name:?}")))?;
    let payload = key.payload(file.data())?;
    let object = oxiroot_compress::decompress(payload, key.obj_len as usize)
        .map_err(|e| Error::Format(format!("decompressing {name:?}: {e}")))?;
    Ok((key.class_name.clone(), object, key.key_len as usize))
}

/// Fetch a histogram object, requiring a 4-character class with the given
/// dimension prefix (e.g. `"TH1"`), so a `read_th1` cannot accept a `TH2`.
pub(crate) fn histogram_object(
    file: &RFile,
    name: &str,
    dim_prefix: &str,
) -> Result<(String, Vec<u8>)> {
    check_dim(name, object_bytes_any(file, name)?, dim_prefix)
}

/// Like [`histogram_object`] but from subdirectory `subdir`.
pub(crate) fn histogram_object_in(
    file: &RFile,
    subdir: &str,
    name: &str,
    dim_prefix: &str,
) -> Result<(String, Vec<u8>)> {
    check_dim(name, file.object_in(subdir, name)?, dim_prefix)
}

/// Require a looked-up `(class, object)` to be a 4-character histogram class with
/// the given dimension prefix (e.g. `"TH1"`).
fn check_dim(
    name: &str,
    (class, object): (String, Vec<u8>),
    dim_prefix: &str,
) -> Result<(String, Vec<u8>)> {
    if class.len() == 4 && class.starts_with(dim_prefix) {
        Ok((class, object))
    } else {
        Err(Error::Format(format!(
            "key {name:?} is a {class}, not a {dim_prefix} histogram"
        )))
    }
}

/// Like [`object_bytes`] but from subdirectory `subdir` (validates the class).
pub(crate) fn object_bytes_in(
    file: &RFile,
    subdir: &str,
    name: &str,
    class: &str,
) -> Result<Vec<u8>> {
    let (got, object) = file.object_in(subdir, name)?;
    if got == class {
        Ok(object)
    } else {
        Err(Error::Format(format!(
            "key {name:?} in {subdir:?} is a {got}, not {class}"
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::{cell_count, check_cells};

    #[test]
    fn cell_count_products_and_overflow() {
        assert_eq!(cell_count(&[5]).unwrap(), 7); // nx + 2
        assert_eq!(cell_count(&[3, 2]).unwrap(), 5 * 4);
        assert_eq!(cell_count(&[3, 2, 1]).unwrap(), 5 * 4 * 3);
        // A forged axis count must error, not wrap.
        assert!(cell_count(&[i32::MAX, i32::MAX, i32::MAX]).is_err());
    }

    #[test]
    fn check_cells_accepts_match_and_optional_empty() {
        assert!(check_cells("c", 7, 7, false).is_ok());
        assert!(check_cells("c", 6, 7, false).is_err()); // wrong length rejected
        assert!(check_cells("sumw2", 0, 7, true).is_ok()); // optional empty ok
        assert!(check_cells("sumw2", 3, 7, true).is_err()); // wrong non-empty rejected
    }
}
