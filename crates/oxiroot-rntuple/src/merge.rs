//! Concatenating RNTuples — the `RNTuple` half of a `hadd`-style file merge.
//!
//! [`concat_ntuples`] reads the same fields from several [`NtupleReader`]s and
//! appends their entries into one writable [`Ntuple`]. Unlike a `TTree` branch,
//! an RNTuple field's type is fully determined by its [`FieldValues`] variant,
//! so the mapping back to a [`Field`] is direct. [`append_ntuples`] streams the
//! same merge into an [`NtupleWriter`], one cluster per input; the `oxiroot`
//! facade's file merger uses it for RNTuple keys.

use std::io::{Seek, Write};

use oxiroot_io_core::{Error, FileReader, Result};

use crate::field::FieldValues;
use crate::reader::NtupleReader;
use crate::writer::{Field, Ntuple, NtupleWriter};

/// Concatenate several `RNTuple`s entry-wise into one writable [`Ntuple`] named
/// `name`.
///
/// Every input must hold the same fields — same names and types — as the first
/// RNTuple; their entries are appended in the given order (as ROOT's `hadd`
/// does). Each `(file, ntuple)` pair is an RNTuple together with the [`FileReader`]
/// it was opened from, since pages are read on demand.
///
/// # Errors
///
/// Returns an error if the inputs disagree on which fields exist or on their
/// types, or if a field uses a type this crate cannot write back: a record /
/// struct, a nested collection (`std::vector<std::vector<T>>` or
/// `std::vector<MyStruct>`), an `std::variant`, an optional / `unique_ptr`, or a
/// `std::vector<uint32_t>` / `std::vector<uint64_t>` (which the writer does not
/// encode).
pub fn concat_ntuples(name: &str, inputs: &[(&FileReader, &NtupleReader)]) -> Result<Ntuple> {
    let &(first_file, first) = inputs
        .first()
        .ok_or_else(|| Error::InvalidInput("concat_ntuples: no input RNTuples".into()))?;

    let field_names = first.field_names();
    let mut fields = Vec::with_capacity(field_names.len());

    for &field in &field_names {
        // Read and concatenate this field's values across every input. A later
        // RNTuple missing the field surfaces as a read error here.
        let mut values = first.read_field(first_file, field)?;
        for (i, &(file, ntuple)) in inputs.iter().enumerate().skip(1) {
            let more = ntuple.read_field(file, field).map_err(|e| {
                e.context(format_args!("concat_ntuples: input #{i} field {field:?}"))
            })?;
            values
                .append(more)
                .map_err(|e| e.context(format_args!("concat_ntuples: field {field:?}")))?;
        }
        fields.push(build_field(field, values)?);
    }

    Ok(Ntuple::new(name, fields))
}

/// Append every entry of `inputs` to `writer`, one input at a time: each input's
/// fields are read, written as one cluster, and dropped before the next input is
/// read. Returns the number of entries appended.
///
/// The inputs must agree on their fields as for [`concat_ntuples`]; the first
/// cluster the writer receives fixes the schema.
///
/// # Errors
///
/// As [`concat_ntuples`], plus any error from [`NtupleWriter::write_batch`].
pub fn append_ntuples<W: Write + Seek>(
    writer: &mut NtupleWriter<W>,
    inputs: &[(&FileReader, &NtupleReader)],
) -> Result<u64> {
    let &(_, first) = inputs
        .first()
        .ok_or_else(|| Error::InvalidInput("append_ntuples: no input RNTuples".into()))?;
    let field_names = first.field_names();
    let mut appended = 0;
    for (i, &(file, ntuple)) in inputs.iter().enumerate() {
        let mut batch = Vec::with_capacity(field_names.len());
        for &field in &field_names {
            let values = ntuple.read_field(file, field).map_err(|e| {
                e.context(format_args!("append_ntuples: input #{i} field {field:?}"))
            })?;
            batch.push(build_field(field, values)?);
        }
        writer.write_batch(&batch)?;
        appended += ntuple.num_entries();
    }
    Ok(appended)
}

/// Rebuild a writable [`Field`] from a field's concatenated values.
fn build_field(name: &str, values: FieldValues) -> Result<Field> {
    use FieldValues::*;
    let reject = |what: &str| -> Result<Field> {
        Err(Error::Unsupported(format!(
            "concat_ntuples: field {name:?} is a {what}, which oxiroot cannot write yet",
        )))
    };
    match values {
        Bool(v) => Ok(Field::bools(name, v)),
        I8(v) => Ok(Field::i8(name, v)),
        U8(v) => Ok(Field::u8(name, v)),
        I16(v) => Ok(Field::i16(name, v)),
        U16(v) => Ok(Field::u16(name, v)),
        I32(v) => Ok(Field::i32(name, v)),
        I64(v) => Ok(Field::i64(name, v)),
        U32(v) => Ok(Field::u32(name, v)),
        U64(v) => Ok(Field::u64(name, v)),
        F32(v) => Ok(Field::f32(name, v)),
        F64(v) => Ok(Field::f64(name, v)),
        Str(v) => Ok(Field::strings(name, v)),
        VecBool(v) => Ok(Field::vec_bool(name, v)),
        VecI8(v) => Ok(Field::vec_i8(name, v)),
        VecU8(v) => Ok(Field::vec_u8(name, v)),
        VecI16(v) => Ok(Field::vec_i16(name, v)),
        VecU16(v) => Ok(Field::vec_u16(name, v)),
        VecI32(v) => Ok(Field::vec_i32(name, v)),
        VecI64(v) => Ok(Field::vec_i64(name, v)),
        VecF32(v) => Ok(Field::vec_f32(name, v)),
        VecF64(v) => Ok(Field::vec_f64(name, v)),
        VecStr(v) => Ok(Field::vec_str(name, v)),
        // The writer has no column encoding for these vector element types.
        VecU32(_) => reject("std::vector<uint32_t> field"),
        VecU64(_) => reject("std::vector<uint64_t> field"),
        Record(_) => reject("record / struct field"),
        Nested { .. } => reject("nested collection field (vector<vector> or vector<struct>)"),
        Variant { .. } => reject("std::variant field"),
        Opt { .. } => reject("optional / unique_ptr field"),
    }
}
