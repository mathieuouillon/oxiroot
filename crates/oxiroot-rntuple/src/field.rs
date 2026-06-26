//! A typed, per-entry view over RNTuple fields.
//!
//! [`FieldValues`] reconstructs a top-level field's values from its physical
//! column(s): scalar leaves map straight from a column, `std::string` combines
//! an index column with a char column, and `std::vector<T>` combines an index
//! column with the element field's column.

use oxiroot_io_core::error::{Error, Result};

use crate::page::ColumnValues;

/// One top-level field's values, one element per entry.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum FieldValues {
    /// `bool`.
    Bool(Vec<bool>),
    /// 32-bit signed integer.
    I32(Vec<i32>),
    /// 64-bit signed integer.
    I64(Vec<i64>),
    /// Unsigned 32-bit integer.
    U32(Vec<u32>),
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
    /// `std::vector<uint32_t>`.
    VecU32(Vec<Vec<u32>>),
    /// `std::vector<uint64_t>`.
    VecU64(Vec<Vec<u64>>),
    /// `std::vector<float>`.
    VecF32(Vec<Vec<f32>>),
    /// `std::vector<double>`.
    VecF64(Vec<Vec<f64>>),
    /// `std::vector<std::string>` — one inner `Vec<String>` per entry.
    VecStr(Vec<Vec<String>>),
    /// A record / struct: its sub-fields in declaration order, each holding one
    /// value per record instance (a struct-of-arrays). At top level every
    /// sub-field has one element per entry; inside a [`Nested`](Self::Nested)
    /// collection they hold the flattened record instances.
    Record(Vec<(String, FieldValues)>),
    /// A collection whose element is itself a collection or a record — e.g.
    /// `std::vector<std::vector<T>>` or `std::vector<MyStruct>`. The cumulative
    /// `offsets` (one per element of the enclosing level) partition the flattened
    /// child `items`: element `k` spans `items[offsets[k-1]..offsets[k]]` (with
    /// `offsets[-1] = 0`).
    Nested {
        /// Cumulative element boundaries, one per element of the enclosing level.
        offsets: Vec<u64>,
        /// The flattened child values, partitioned by `offsets`.
        items: Box<FieldValues>,
    },
}

impl FieldValues {
    /// The number of elements at this level — entries, for a top-level field.
    #[must_use]
    pub fn len(&self) -> usize {
        use FieldValues::*;
        match self {
            Bool(v) => v.len(),
            I32(v) => v.len(),
            I64(v) => v.len(),
            U32(v) => v.len(),
            U64(v) => v.len(),
            F32(v) => v.len(),
            F64(v) => v.len(),
            Str(v) => v.len(),
            VecBool(v) => v.len(),
            VecI32(v) => v.len(),
            VecI64(v) => v.len(),
            VecU32(v) => v.len(),
            VecU64(v) => v.len(),
            VecF32(v) => v.len(),
            VecF64(v) => v.len(),
            VecStr(v) => v.len(),
            Record(fields) => fields.first().map_or(0, |(_, f)| f.len()),
            Nested { offsets, .. } => offsets.len(),
        }
    }

    /// Whether there are no elements at this level.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// Map a scalar leaf column to per-entry field values.
pub(crate) fn scalar(values: ColumnValues) -> Result<FieldValues> {
    Ok(match values {
        ColumnValues::Bits(v) => FieldValues::Bool(v),
        ColumnValues::I32(v) => FieldValues::I32(v),
        ColumnValues::I64(v) => FieldValues::I64(v),
        ColumnValues::U32(v) => FieldValues::U32(v),
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

/// Group a collection's flattened child `items` by its cumulative `offsets`.
/// Scalar and string children materialize into the ergonomic flat `Vec*`
/// variants; a collection- or record-valued child is wrapped in
/// [`FieldValues::Nested`] (so arbitrarily deep nesting stays representable).
pub(crate) fn collect(offsets: Vec<u64>, items: FieldValues) -> Result<FieldValues> {
    use FieldValues::*;
    Ok(match items {
        Bool(v) => VecBool(group(&offsets, &v)?),
        I32(v) => VecI32(group(&offsets, &v)?),
        I64(v) => VecI64(group(&offsets, &v)?),
        U32(v) => VecU32(group(&offsets, &v)?),
        U64(v) => VecU64(group(&offsets, &v)?),
        F32(v) => VecF32(group(&offsets, &v)?),
        F64(v) => VecF64(group(&offsets, &v)?),
        Str(v) => VecStr(group(&offsets, &v)?),
        other => Nested {
            offsets,
            items: Box::new(other),
        },
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
