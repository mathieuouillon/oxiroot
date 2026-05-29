//! Shared building blocks for the classic histogram hierarchy.
//!
//! `TH1` and its `ClassDef` bases each carry a `{byte-count, version}` header,
//! so we read the members we need and seek to `TH1`'s end. `TArray*` bin
//! contents are streamed inline — just a count and the values, no header.

use root_io_core::buffer::RBuffer;
use root_io_core::error::{Error, Result};
use root_io_core::streamer::{read_tnamed, skip_versioned};
use root_io_core::RFile;

use crate::axis::TAxis;

/// Bin-content array element type, named by the histogram class suffix.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Precision {
    /// `TArrayD` (`f64`).
    Double,
    /// `TArrayF` (`f32`).
    Float,
    /// `TArrayI` (`i32`).
    Int,
    /// `TArrayS` (`i16`).
    Short,
    /// `TArrayC` (`i8`).
    Char,
    /// `TArrayL64` (`i64`).
    Long,
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
    })
}

/// Read an inline `TArray` of `n` values at the given precision (a count
/// followed by that many values, widened to `f64`).
pub(crate) fn read_tarray(r: &mut RBuffer, precision: Precision) -> Result<Vec<f64>> {
    let n = r.be_i32()?.max(0) as usize;
    let mut v = Vec::with_capacity(n);
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
    let payload = &file.data()[key.payload_range()];
    root_compress::decompress(payload, key.obj_len as usize)
        .map_err(|e| Error::Format(format!("decompressing {name:?}: {e}")))
}

/// Return a key's class name together with its decompressed object bytes,
/// without checking the class.
pub(crate) fn object_bytes_any(file: &RFile, name: &str) -> Result<(String, Vec<u8>)> {
    let key = file
        .key(name)
        .ok_or_else(|| Error::Format(format!("no key named {name:?}")))?;
    let payload = &file.data()[key.payload_range()];
    let object = root_compress::decompress(payload, key.obj_len as usize)
        .map_err(|e| Error::Format(format!("decompressing {name:?}: {e}")))?;
    Ok((key.class_name.clone(), object))
}

/// Fetch a histogram object, requiring a 4-character class with the given
/// dimension prefix (e.g. `"TH1"`), so a `read_th1` cannot accept a `TH2`.
pub(crate) fn histogram_object(
    file: &RFile,
    name: &str,
    dim_prefix: &str,
) -> Result<(String, Vec<u8>)> {
    let (class, object) = object_bytes_any(file, name)?;
    if class.len() == 4 && class.starts_with(dim_prefix) {
        Ok((class, object))
    } else {
        Err(Error::Format(format!(
            "key {name:?} is a {class}, not a {dim_prefix} histogram"
        )))
    }
}
