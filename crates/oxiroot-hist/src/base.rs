//! Shared building blocks for the classic histogram hierarchy.
//!
//! `Hist1D` and its `ClassDef` bases each carry a `{byte-count, version}` header,
//! so we read the members we need and seek to `Hist1D`'s end. `TArray*` bin
//! contents are streamed inline — just a count and the values, no header.

use oxiroot_io_core::{
    read_named, skip_versioned, Error, FileReader, RBuffer, Result, VersionHeader,
};
// The generic object-byte readers now live in `oxiroot-io-core`; re-export them
// here so the histogram modules keep addressing them as `crate::base::…`.
pub(crate) use oxiroot_io_core::{object_bytes_any, object_bytes_any_keyed};

use crate::axis::Axis;

/// The on-disk type of a histogram's bin contents, named by the class suffix
/// (`Hist1D**D**`, `Hist2D**F**`, …). Contents are always held in memory as `f64`;
/// this only selects the `TArray*` element type written to (and read from) the
/// file. The default is [`BinContentType::F64`] (ROOT's `TH1D`/`TH2D`/`TH3D`).
#[doc(alias = "Precision")]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[non_exhaustive]
pub enum BinContentType {
    /// `TArrayD` (`f64`), the `D` classes (`TH1D`/`TH2D`/`TH3D`).
    #[default]
    F64,
    /// `TArrayF` (`f32`), the `F` classes (`TH1F`/`TH2F`/`TH3F`).
    F32,
    /// `TArrayI` (`i32`), the `I` classes (`TH1I`/`TH2I`/`TH3I`).
    I32,
    /// `TArrayS` (`i16`), the `S` classes (`TH1S`/`TH2S`/`TH3S`).
    I16,
    /// `TArrayC` (`i8`), the `C` classes (`TH1C`/`TH2C`/`TH3C`).
    I8,
    /// `TArrayL64` (`i64`), the `L` classes (`TH1L`/`TH2L`/`TH3L`).
    I64,
}

impl BinContentType {
    /// The class-name suffix character for this type (`'D'`, `'F'`, …).
    #[must_use]
    pub fn code(self) -> char {
        match self {
            BinContentType::F64 => 'D',
            BinContentType::F32 => 'F',
            BinContentType::I32 => 'I',
            BinContentType::I16 => 'S',
            BinContentType::I8 => 'C',
            BinContentType::I64 => 'L',
        }
    }

    /// The full ROOT class name for a histogram of dimension `dim` (`"Hist1D"`,
    /// `"Hist2D"`, `"Hist3D"`) with this bin content type, e.g.
    /// `BinContentType::F32.class_name("Hist1D") == "TH1F"`.
    #[must_use]
    pub fn class_name(self, dim: &str) -> String {
        let mut s = String::with_capacity(dim.len() + 1);
        s.push_str(dim);
        s.push(self.code());
        s
    }
}

/// Determine the bin-content type from a histogram class name's suffix
/// (`TH1D`/`TH2F`/`TH1I`/…). `Profile1D` and similar are handled by their own
/// readers.
pub(crate) fn bin_content_type_of(class: &str) -> Result<BinContentType> {
    match class.chars().last() {
        Some('D') => Ok(BinContentType::F64),
        Some('F') => Ok(BinContentType::F32),
        Some('I') => Ok(BinContentType::I32),
        Some('S') => Ok(BinContentType::I16),
        Some('C') => Ok(BinContentType::I8),
        Some('L') => Ok(BinContentType::I64),
        _ => Err(Error::Unsupported(format!(
            "unsupported histogram type: {class}"
        ))),
    }
}

/// Check that an input has the length of the input it is paired with, so a
/// constructor or fill never silently truncates or pads.
pub(crate) fn check_len(what: &str, expected: usize, found: usize) -> Result<()> {
    if found == expected {
        Ok(())
    } else {
        Err(Error::LengthMismatch {
            what: what.to_string(),
            expected,
            found,
        })
    }
}

/// The members shared by every `Hist1D`-derived histogram.
#[derive(Debug, Clone, PartialEq)]
pub struct HistBase {
    /// Histogram name (`fName`).
    pub name: String,
    /// Histogram title (`fTitle`).
    pub title: String,
    /// X axis.
    pub xaxis: Axis,
    /// Y axis.
    pub yaxis: Axis,
    /// Z axis.
    pub zaxis: Axis,
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
    /// histogram, but used by `Profile1D` to store the per-bin sum of `y^2`.
    pub sumw2: Vec<f64>,
}

/// An object written by a ROOT release older than this crate can read: `class`
/// at `version` still used a hand-written streamer.
pub(crate) fn unsupported_version(class: &str, version: u16) -> Error {
    Error::UnsupportedVersion {
        class: class.to_string(),
        version: i32::from(version),
    }
}

/// Move past the rest of `header`'s record: the members a newer class version
/// added. It is an error if the members read ran past the record's end.
pub(crate) fn end_record(r: &mut RBuffer, header: &VersionHeader, class: &str) -> Result<()> {
    if let Some(end) = header.end {
        if r.pos() > end {
            return Err(Error::Format(format!(
                "{class} (class version {}) is shorter than its members",
                header.version
            )));
        }
        r.seek(end)?;
    }
    Ok(())
}

/// The sum of `values` over the in-range cells of a histogram with the given
/// per-axis bin counts (flow cells excluded), as ROOT's `GetStats` sums them.
/// `values` has one entry per cell, flow included; an empty slice sums to 0.
pub(crate) fn in_range_sum(values: &[f64], axis_nbins: &[i32]) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    let sizes: Vec<usize> = axis_nbins.iter().map(|&n| n.max(0) as usize).collect();
    if sizes.contains(&0) {
        return 0.0;
    }
    let mut total = 0.0;
    let mut index = vec![1usize; sizes.len()];
    loop {
        // Global cell index: x + (nx+2)*(y + (ny+2)*z).
        let mut cell = 0;
        for (i, &n) in sizes.iter().enumerate().rev() {
            cell = cell * (n + 2) + index[i];
        }
        total += values.get(cell).copied().unwrap_or(0.0);
        // Advance the multi-index over 1..=n per axis, x fastest.
        let mut axis = 0;
        loop {
            if axis == sizes.len() {
                return total;
            }
            if index[axis] < sizes[axis] {
                index[axis] += 1;
                break;
            }
            index[axis] = 1;
            axis += 1;
        }
    }
}

/// Read a `Hist1D` base object (its header, the `Named`/`TAtt*` bases, and the
/// members up to the core statistics), then seek to the `Hist1D` record's end.
pub(crate) fn read_th1_base(r: &mut RBuffer) -> Result<HistBase> {
    let th1 = r.read_version()?;
    // Class version 1 (ROOT 1) stored fMaximum, fMinimum, fNormFactor and
    // fContour as floats; every later version reads the same up to fSumw2.
    if th1.version < 2 {
        return Err(unsupported_version("TH1", th1.version));
    }

    let named = read_named(r)?;
    skip_versioned(r)?; // TAttLine
    skip_versioned(r)?; // TAttFill
    skip_versioned(r)?; // TAttMarker

    let ncells = r.be_i32()?;
    let xaxis = Axis::read(r)?;
    let yaxis = Axis::read(r)?;
    let zaxis = Axis::read(r)?;
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
    let _contour = read_tarray(r, BinContentType::F64)?; // fContour
    let sumw2 = read_tarray(r, BinContentType::F64)?; // fSumw2

    let end = th1
        .end
        .ok_or_else(|| Error::Format("TH1 record has no byte count".into()))?;
    r.seek(end)?;

    Ok(HistBase {
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

/// Read an inline `TArray` of `n` values of the given element type (a count
/// followed by that many values, widened to `f64`).
pub(crate) fn read_tarray(r: &mut RBuffer, bin_content_type: BinContentType) -> Result<Vec<f64>> {
    let n = r.be_i32()?.max(0) as usize;
    // Cap the up-front reservation at what the buffer could possibly hold, so a
    // forged count can't drive a huge allocation before the read fails.
    let mut v = Vec::with_capacity(n.min(r.remaining()));
    for _ in 0..n {
        let value = match bin_content_type {
            BinContentType::F64 => r.be_f64()?,
            BinContentType::F32 => r.be_f32()? as f64,
            BinContentType::I32 => r.be_i32()? as f64,
            BinContentType::I16 => r.be_i16()? as f64,
            BinContentType::I8 => r.i8()? as f64,
            BinContentType::I64 => r.be_i64()? as f64,
        };
        v.push(value);
    }
    Ok(v)
}

/// Read a standalone `TH1x` object: its wrapper, the `Hist1D` base, and the inline
/// `TArray` bin contents; seek to the wrapper's end. Used both for a top-level
/// `TH1D`/`TH1F` and for the `TH1D` base inside a `Profile1D`.
pub(crate) fn read_th1_object(
    r: &mut RBuffer,
    bin_content_type: BinContentType,
) -> Result<(HistBase, Vec<f64>)> {
    let wrapper = r.read_version()?;
    let core = read_th1_base(r)?;
    let contents = read_tarray(r, bin_content_type)?;
    if let Some(end) = wrapper.end {
        r.seek(end)?;
    }
    Ok((core, contents))
}

/// Check that key `name` exists and holds a `class`, before its payload is read.
fn check_key_class(file: &FileReader, name: &str, class: &str) -> Result<()> {
    let key = file.key(name).ok_or_else(|| Error::NotFound {
        what: "key",
        name: name.to_string(),
    })?;
    if key.class_name != class {
        return Err(Error::WrongClass {
            name: name.to_string(),
            found: key.class_name.clone(),
            expected: class.to_string(),
        });
    }
    Ok(())
}

/// Locate a key, verify its class, and return its decompressed object bytes.
pub(crate) fn object_bytes(file: &FileReader, name: &str, class: &str) -> Result<Vec<u8>> {
    check_key_class(file, name, class)?;
    Ok(object_bytes_any(file, name)?.1)
}

/// Like [`object_bytes`], but also return the key's header length (`fKeyLen`).
///
/// ROOT keys objects relative to `-fKeyLen`, so the object-reference map (see
/// [`oxiroot_io_core::TagReader`]) needs the key length to resolve the
/// class/object back-references inside a streamed object (e.g. `PolyHist`'s bins).
pub(crate) fn object_bytes_keyed(
    file: &FileReader,
    name: &str,
    class: &str,
) -> Result<(Vec<u8>, usize)> {
    check_key_class(file, name, class)?;
    let (_, object, key_len) = object_bytes_any_keyed(file, name)?;
    Ok((object, key_len))
}

/// Fetch a histogram object, requiring a 4-character class with the given
/// dimension prefix (e.g. `"Hist1D"`), so a `read_th1` cannot accept a `Hist2D`.
pub(crate) fn histogram_object(
    file: &FileReader,
    name: &str,
    dim_prefix: &str,
) -> Result<(String, Vec<u8>)> {
    check_dim(name, object_bytes_any(file, name)?, dim_prefix)
}

/// Like [`histogram_object`] but from subdirectory `subdir`.
pub(crate) fn histogram_object_in(
    file: &FileReader,
    subdir: &str,
    name: &str,
    dim_prefix: &str,
) -> Result<(String, Vec<u8>)> {
    check_dim(name, file.object_in(subdir, name)?, dim_prefix)
}

/// Require a looked-up `(class, object)` to be a 4-character histogram class with
/// the given dimension prefix (e.g. `"Hist1D"`).
fn check_dim(
    name: &str,
    (class, object): (String, Vec<u8>),
    dim_prefix: &str,
) -> Result<(String, Vec<u8>)> {
    if class.len() == 4 && class.starts_with(dim_prefix) {
        Ok((class, object))
    } else {
        Err(Error::WrongClass {
            name: name.to_string(),
            found: class,
            expected: format!("{dim_prefix} histogram"),
        })
    }
}

/// Like [`object_bytes`] but from subdirectory `subdir` (validates the class).
pub(crate) fn object_bytes_in(
    file: &FileReader,
    subdir: &str,
    name: &str,
    class: &str,
) -> Result<Vec<u8>> {
    let (got, object) = file.object_in(subdir, name)?;
    if got == class {
        Ok(object)
    } else {
        Err(Error::WrongClass {
            name: format!("{}/{name}", subdir.trim_end_matches('/')),
            found: got,
            expected: class.to_string(),
        })
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
