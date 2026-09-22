//! Concatenating trees — the `TTree` half of a `hadd`-style file merge.
//!
//! [`concat_trees`] reads the same branches from several [`TreeReader`]s and appends
//! their entries into one writable [`Tree`], reconstructing each branch's kind
//! (scalar, fixed array, jagged, `std::vector`, or string) so the result writes
//! back the way it was read. [`append_trees`] streams the same merge into a
//! [`TreeWriter`], holding one input's entries at a time; the `oxiroot`
//! facade's file merger uses it for tree keys.

use std::io::{Seek, Write};

use oxiroot_io_core::{Error, FileReader, Result};

use crate::reader::{BranchMetaLite, TreeReader};
use crate::value::BranchValues;
use crate::writer::{Branch, Tree, TreeWriter};

/// Concatenate several `TTree`s entry-wise into one writable [`Tree`].
///
/// Every input must hold the same branches — same names, element types, and
/// kinds — as the first tree; their entries are appended in the given order
/// (as ROOT's `hadd` does). Each `(file, tree)` pair is a tree together with the
/// [`FileReader`] it was opened from, since baskets are read on demand. The output
/// tree's name is taken from the first input.
///
/// # Errors
///
/// Returns an error if the inputs disagree on which branches exist or on their
/// types; if a branch uses a layout this crate cannot write back (a
/// multidimensional fixed array, a `std::vector<std::string>`, a
/// `std::vector<std::vector<T>>`, a `std::vector<bool>`, a multi-leaf
/// "leaflist" branch, or a split-object member column); or if a tree has
/// branches that could not be read at all (see
/// [`TreeReader::unsupported_branches`]) — in which case the merge would silently
/// drop data, so it is refused.
pub fn concat_trees(inputs: &[(&FileReader, &TreeReader)]) -> Result<Tree> {
    let &(first_file, first) = inputs
        .first()
        .ok_or_else(|| Error::Format("concat_trees: no input trees".into()))?;

    // Refuse a tree with branches we cannot even read — the output would
    // silently drop them.
    if let Some((name, reason)) = first.unsupported_branches().first() {
        return Err(Error::Format(format!(
            "concat_trees: tree {:?} has an unreadable branch {name:?} ({reason}); \
             cannot merge it without losing data",
            first.name(),
        )));
    }

    let names = first.branch_names();
    let mut branches = Vec::with_capacity(names.len());

    for &name in &names {
        let meta = first
            .branch_meta(name)
            .expect("a branch listed by branch_names has metadata");

        // Read and concatenate this branch's values across every input tree.
        let mut values = first.read_branch(first_file, name)?;
        for (i, &(file, tree)) in inputs.iter().enumerate().skip(1) {
            if tree.branch_meta(name).is_none() {
                return Err(Error::Format(format!(
                    "concat_trees: input #{i} ({:?}) is missing branch {name:?} \
                     present in the first tree",
                    tree.name(),
                )));
            }
            let more = tree.read_branch(file, name)?;
            values
                .append(more)
                .map_err(|e| Error::Format(format!("concat_trees: branch {name:?}: {e}")))?;
        }

        branches.push(build_branch(name, &meta, values)?);
    }

    Ok(Tree::new(first.name(), branches))
}

/// Append every entry of `inputs` to `writer`, one input at a time: each input's
/// branches are read, written as one batch (one basket per branch), and dropped
/// before the next input is read. Returns the number of entries appended.
///
/// The inputs must agree on their branches as for [`concat_trees`]; the first
/// batch the writer receives fixes the schema.
///
/// # Errors
///
/// As [`concat_trees`], plus any error from [`TreeWriter::write_batch`].
pub fn append_trees<W: Write + Seek>(
    writer: &mut TreeWriter<W>,
    inputs: &[(&FileReader, &TreeReader)],
) -> Result<u64> {
    let &(_, first) = inputs
        .first()
        .ok_or_else(|| Error::Format("append_trees: no input trees".into()))?;
    let names = first.branch_names();
    let mut appended = 0;
    for (i, &(file, tree)) in inputs.iter().enumerate() {
        if let Some((name, reason)) = tree.unsupported_branches().first() {
            return Err(Error::Format(format!(
                "append_trees: input #{i} ({:?}) has an unreadable branch {name:?} ({reason}); \
                 cannot merge it without losing data",
                tree.name(),
            )));
        }
        let mut batch = Vec::with_capacity(names.len());
        for &name in &names {
            let meta = tree.branch_meta(name).ok_or_else(|| {
                Error::Format(format!(
                    "append_trees: input #{i} ({:?}) is missing branch {name:?} \
                     present in the first tree",
                    tree.name(),
                ))
            })?;
            batch.push(build_branch(name, &meta, tree.read_branch(file, name)?)?);
        }
        writer.write_batch(&batch)?;
        appended += tree.num_entries();
    }
    Ok(appended)
}

/// Rebuild a writable [`Branch`] of the correct kind from a branch's read
/// metadata and its concatenated values.
fn build_branch(name: &str, meta: &BranchMetaLite, values: BranchValues) -> Result<Branch> {
    // Layouts this crate can read but not yet write back.
    let reject = |what: &str| -> Result<Branch> {
        Err(Error::Format(format!(
            "concat_trees: branch {name:?} is a {what}, which oxiroot cannot write yet",
        )))
    };
    if meta.has_object_member {
        return reject("split-object member column");
    }
    if meta.has_leaflist {
        return reject("multi-leaf (leaflist) branch");
    }
    if meta.has_nested {
        return reject("std::vector<std::vector<T>> branch");
    }
    if meta.dims_len > 1 {
        return reject("multidimensional fixed array");
    }

    // Which flavour a `Vec*` value writes back as: a single fixed dimension
    // (`x[N]`) → fixed array; else a `std::vector<T>` (marked by a per-entry
    // streamer header) → STL vector; else a jagged `x[n]`.
    enum Flavor {
        Fixed,
        Stl,
        Jagged,
    }
    let flavor = if meta.dims_len == 1 {
        Flavor::Fixed
    } else if meta.elem_header > 0 {
        Flavor::Stl
    } else {
        Flavor::Jagged
    };

    use BranchValues::*;
    // A `Vec*` variant with all three flavours available.
    macro_rules! vec3 {
        ($v:expr, $fixed:ident, $jagged:ident, $stl:ident) => {
            match flavor {
                Flavor::Fixed => Ok(Branch::$fixed(name, $v)),
                Flavor::Jagged => Ok(Branch::$jagged(name, $v)),
                Flavor::Stl => Ok(Branch::$stl(name, $v)),
            }
        };
    }

    match values {
        Bool(v) => Ok(Branch::bools(name, v)),
        I8(v) => Ok(Branch::i8(name, v)),
        U8(v) => Ok(Branch::u8(name, v)),
        I16(v) => Ok(Branch::i16(name, v)),
        U16(v) => Ok(Branch::u16(name, v)),
        I32(v) => Ok(Branch::i32(name, v)),
        U32(v) => Ok(Branch::u32(name, v)),
        I64(v) => Ok(Branch::i64(name, v)),
        U64(v) => Ok(Branch::u64(name, v)),
        F32(v) => Ok(Branch::f32(name, v)),
        F64(v) => Ok(Branch::f64(name, v)),
        Str(v) => Ok(Branch::strings(name, v)),
        // `std::vector<bool>` has no writer constructor (bit-packed); the fixed
        // and jagged flavours do.
        VecBool(v) => match flavor {
            Flavor::Fixed => Ok(Branch::vec_bool(name, v)),
            Flavor::Jagged => Ok(Branch::jagged_bool(name, v)),
            Flavor::Stl => reject("std::vector<bool> branch"),
        },
        VecI8(v) => vec3!(v, vec_i8, jagged_i8, vector_i8),
        VecU8(v) => vec3!(v, vec_u8, jagged_u8, vector_u8),
        VecI16(v) => vec3!(v, vec_i16, jagged_i16, vector_i16),
        VecU16(v) => vec3!(v, vec_u16, jagged_u16, vector_u16),
        VecI32(v) => vec3!(v, vec_i32, jagged_i32, vector_i32),
        VecU32(v) => vec3!(v, vec_u32, jagged_u32, vector_u32),
        VecI64(v) => vec3!(v, vec_i64, jagged_i64, vector_i64),
        VecU64(v) => vec3!(v, vec_u64, jagged_u64, vector_u64),
        VecF32(v) => vec3!(v, vec_f32, jagged_f32, vector_f32),
        VecF64(v) => vec3!(v, vec_f64, jagged_f64, vector_f64),
        VecStr(_) => reject("std::vector<std::string> branch"),
        Nested { .. } => reject("std::vector<std::vector<T>> branch"),
    }
}
