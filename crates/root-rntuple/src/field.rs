//! A typed, per-entry view over RNTuple fields.
//!
//! [`FieldValues`] reconstructs a top-level field's values from its physical
//! column(s): scalar leaves map straight from a column, `std::string` combines
//! an index column with a char column, and `std::vector<T>` combines an index
//! column with the element field's column.

use root_io_core::error::{Error, Result};

use crate::page::ColumnValues;

/// One top-level field's values, one element per entry.
#[derive(Debug, Clone, PartialEq)]
pub enum FieldValues {
    /// `bool`.
    Bool(Vec<bool>),
    /// 32-bit signed integer.
    I32(Vec<i32>),
    /// 64-bit signed integer.
    I64(Vec<i64>),
    /// Unsigned 64-bit integer.
    U64(Vec<u64>),
    /// 32-bit float.
    F32(Vec<f32>),
    /// 64-bit float.
    F64(Vec<f64>),
    /// `std::string`.
    Str(Vec<String>),
    /// `std::vector<bool>`.
    VecBool(Vec<Vec<bool>>),
    /// `std::vector<int32_t>`.
    VecI32(Vec<Vec<i32>>),
    /// `std::vector<int64_t>`.
    VecI64(Vec<Vec<i64>>),
    /// `std::vector<float>`.
    VecF32(Vec<Vec<f32>>),
    /// `std::vector<double>`.
    VecF64(Vec<Vec<f64>>),
}

/// Map a scalar leaf column to per-entry field values.
pub(crate) fn scalar(values: ColumnValues) -> Result<FieldValues> {
    Ok(match values {
        ColumnValues::Bits(v) => FieldValues::Bool(v),
        ColumnValues::I32(v) => FieldValues::I32(v),
        ColumnValues::I64(v) => FieldValues::I64(v),
        ColumnValues::U64(v) => FieldValues::U64(v),
        ColumnValues::F32(v) => FieldValues::F32(v),
        ColumnValues::F64(v) => FieldValues::F64(v),
        ColumnValues::Bytes(_) => {
            return Err(Error::Format(
                "byte-typed scalar fields are not supported".into(),
            ))
        }
    })
}

/// Reconstruct `std::string` values from cumulative offsets and char bytes.
pub(crate) fn strings(offsets: &[u64], bytes: &[u8]) -> Result<FieldValues> {
    let mut start = 0usize;
    let mut out = Vec::with_capacity(offsets.len());
    for &end in offsets {
        let end = end as usize;
        let slice = bytes
            .get(start..end)
            .ok_or_else(|| Error::Format("string offset out of range".into()))?;
        out.push(String::from_utf8(slice.to_vec()).map_err(|_| Error::InvalidUtf8)?);
        start = end;
    }
    Ok(FieldValues::Str(out))
}

/// Reconstruct `std::vector<T>` values from cumulative offsets and flat element
/// data.
pub(crate) fn collection(offsets: &[u64], data: ColumnValues) -> Result<FieldValues> {
    Ok(match data {
        ColumnValues::Bits(v) => FieldValues::VecBool(group(offsets, &v)?),
        ColumnValues::I32(v) => FieldValues::VecI32(group(offsets, &v)?),
        ColumnValues::I64(v) => FieldValues::VecI64(group(offsets, &v)?),
        ColumnValues::F32(v) => FieldValues::VecF32(group(offsets, &v)?),
        ColumnValues::F64(v) => FieldValues::VecF64(group(offsets, &v)?),
        other => {
            return Err(Error::Format(format!(
                "collections of {other:?} are not supported yet"
            )))
        }
    })
}

fn group<T: Clone>(offsets: &[u64], data: &[T]) -> Result<Vec<Vec<T>>> {
    let mut start = 0usize;
    let mut out = Vec::with_capacity(offsets.len());
    for &end in offsets {
        let end = end as usize;
        let slice = data
            .get(start..end)
            .ok_or_else(|| Error::Format("collection offset out of range".into()))?;
        out.push(slice.to_vec());
        start = end;
    }
    Ok(out)
}
