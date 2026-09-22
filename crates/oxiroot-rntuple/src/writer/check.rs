//! Checking fields before anything is written: entry counts, offsets and
//! nesting.

use oxiroot_io_core::error::{Error, Result};

use super::fields::{Column, Field};

/// Reject fields the writer would lay out inconsistently: fields whose entry
/// counts differ (a cluster has one entry count) and composite columns whose
/// parts disagree, such as record members of different lengths or offsets that
/// do not end at their items' length.
pub(super) fn check_fields(fields: &[Field]) -> Result<()> {
    let mut expected = None;
    for f in fields {
        let what = format!("field {:?}", f.name);
        check_column(&what, &f.data)?;
        if let Some(n) = entry_count(&f.data) {
            match expected {
                None => expected = Some(n),
                Some(e) if e != n => return Err(mismatch(format!("{what} entries"), e, n)),
                Some(_) => {}
            }
        }
    }
    Ok(())
}

/// The number of entries `col` holds, or `None` for a zero-size array or bitset,
/// whose data does not tell (it holds no values for any number of entries).
pub(super) fn entry_count(col: &Column) -> Option<usize> {
    match col {
        Column::Array { len: 0, .. } | Column::Bitset { len: 0, .. } => None,
        Column::Atomic(inner) => entry_count(inner),
        Column::Record(members) | Column::Object { members, .. } => {
            members.iter().find_map(|(_, c)| entry_count(c))
        }
        _ => Some(col.len()),
    }
}

/// Check that a composite column's parts agree with each other.
fn check_column(what: &str, col: &Column) -> Result<()> {
    match col {
        Column::Record(members) | Column::Object { members, .. } => {
            let mut expected = None;
            for (name, member) in members {
                let member_what = format!("{what} member {name:?}");
                check_column(&member_what, member)?;
                if let Some(n) = entry_count(member) {
                    match expected {
                        None => expected = Some(n),
                        Some(e) if e != n => return Err(mismatch(member_what, e, n)),
                        Some(_) => {}
                    }
                }
            }
            Ok(())
        }
        Column::Nested { offsets, items } | Column::Assoc { offsets, items, .. } => {
            if let Some(i) = offsets.windows(2).position(|w| w[1] < w[0]) {
                return Err(Error::Format(format!(
                    "{what}: collection offsets decrease at entry {}",
                    i + 1
                )));
            }
            let items_what = format!("{what} items");
            check_column(&items_what, items)?;
            let end = offsets.last().map_or(0, |&o| o as usize);
            match entry_count(items) {
                Some(n) if n != end => Err(mismatch(items_what, end, n)),
                _ => Ok(()),
            }
        }
        Column::Variant { alternatives, tags } => {
            let mut counts = vec![0usize; alternatives.len()];
            for &tag in tags {
                match tag as usize {
                    0 => {}
                    k if k <= alternatives.len() => counts[k - 1] += 1,
                    k => {
                        return Err(Error::Format(format!(
                            "{what}: variant tag {k} but only {} alternatives",
                            alternatives.len()
                        )))
                    }
                }
            }
            for (k, (alternative, &n)) in alternatives.iter().zip(&counts).enumerate() {
                let alt_what = format!("{what} alternative {k}");
                check_column(&alt_what, alternative)?;
                match entry_count(alternative) {
                    Some(len) if len != n => return Err(mismatch(alt_what, n, len)),
                    _ => {}
                }
            }
            Ok(())
        }
        Column::Array { len, items } => {
            let items_what = format!("{what} items");
            check_column(&items_what, items)?;
            match (entry_count(items), *len) {
                (Some(n), 0) if n != 0 => Err(mismatch(items_what, 0, n)),
                (Some(n), len) if len != 0 && n % len != 0 => Err(Error::Format(format!(
                    "{what}: {n} array items do not divide into arrays of {len}"
                ))),
                _ => Ok(()),
            }
        }
        Column::Bitset { len, bits } => match (bits.len(), *len) {
            (n, 0) if n != 0 => Err(mismatch(format!("{what} bits"), 0, n)),
            (n, len) if len != 0 && n % len != 0 => Err(Error::Format(format!(
                "{what}: {n} bits do not divide into bitsets of {len}"
            ))),
            _ => Ok(()),
        },
        Column::Optional {
            present, values, ..
        } => {
            let values_what = format!("{what} values");
            check_column(&values_what, values)?;
            let n_present = present.iter().filter(|&&p| p).count();
            match entry_count(values) {
                Some(n) if n != n_present => Err(mismatch(values_what, n_present, n)),
                _ => Ok(()),
            }
        }
        Column::Atomic(inner) => check_column(what, inner),
        _ => Ok(()),
    }
}

fn mismatch(what: String, expected: usize, found: usize) -> Error {
    Error::LengthMismatch {
        what,
        expected,
        found,
    }
}
