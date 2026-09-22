//! The streamer classes an RNTuple's record fields need, and their checksums.

use oxiroot_io_core::streamer_gen::{basic, streamer_info_list, Cls};

use super::fields::{Column, Field};
use crate::anchor::anchor_streamer_class;

/// ROOT's class checksum (`TClass::GetCheckSum`) for a flat record: fold each
/// character of the class name, then of every member's name and C++ type, into
/// `id = id * 3 + ch`. Matches `TClass::GetCheckSum()` for plain-member classes,
/// which the RNTuple field record stores so ROOT can validate the on-disk schema.
pub(super) fn class_checksum(class_name: &str, members: &[(String, Column)]) -> u32 {
    let fold = |id: u32, s: &str| {
        s.bytes()
            .fold(id, |acc, b| acc.wrapping_mul(3).wrapping_add(u32::from(b)))
    };
    let mut id = fold(0, class_name);
    for (name, col) in members {
        id = fold(id, name);
        id = fold(id, checksum_type_name(col));
    }
    id
}

/// How ROOT describes a class member of a plain-number column kind: the C++
/// spelling its class checksum folds in, and the `(fType, fSize, fTypeName)` of
/// its streamer element. `None` for any other column kind.
fn scalar_member(col: &Column) -> Option<(&'static str, i32, i32, &'static str)> {
    Some(match col {
        Column::Bool(_) => ("bool", 18, 1, "bool"),
        Column::I8(_) => ("char", 1, 1, "char"),
        Column::U8(_) => ("unsigned char", 11, 1, "unsigned char"),
        Column::I16(_) => ("short", 2, 2, "short"),
        Column::U16(_) => ("unsigned short", 12, 2, "unsigned short"),
        Column::I32(_) => ("int", 3, 4, "int"),
        Column::U32(_) => ("unsigned int", 13, 4, "unsigned int"),
        Column::I64(_) => ("long long", 16, 8, "Long64_t"),
        Column::U64(_) => ("unsigned long long", 17, 8, "ULong64_t"),
        Column::F32(_) | Column::HalfF32(_) | Column::TruncF32 { .. } | Column::QuantF32 { .. } => {
            ("float", 5, 4, "float")
        }
        Column::F64(_) => ("double", 8, 8, "double"),
        _ => return None,
    })
}

/// The C++ type spelling ROOT uses for a member when computing a class checksum
/// (the fundamental-type keyword, e.g. `int`/`double`); `void` for a member that
/// is not a plain number, which the supported checksum does not cover.
fn checksum_type_name(col: &Column) -> &'static str {
    scalar_member(col).map_or("void", |(name, ..)| name)
}

/// Add the `TStreamerInfo` entries for the user classes inside `col` to
/// `classes`, once per class name. A class is described when every member is a
/// plain number, as ROOT writes it for a struct without `ClassDef` (version 1);
/// its checksum is the one the field record carries.
fn collect_classes<'a>(col: &'a Column, classes: &mut Vec<Cls<'a>>) {
    match col {
        Column::Object { type_name, members } => {
            for (_, member) in members {
                collect_classes(member, classes);
            }
            let elements: Option<Vec<_>> = members
                .iter()
                .map(|(name, member)| {
                    scalar_member(member)
                        .map(|(_, ty, size, type_name)| basic(name, ty, size, type_name))
                })
                .collect();
            if let Some(elements) = elements {
                if !classes.iter().any(|c| c.name == type_name.as_str()) {
                    classes.push(Cls {
                        name: type_name.into(),
                        version: 1,
                        checksum: class_checksum(type_name, members),
                        elements,
                    });
                }
            }
        }
        Column::Record(members) => {
            for (_, member) in members {
                collect_classes(member, classes);
            }
        }
        Column::Variant { alternatives, .. } => {
            for alt in alternatives {
                collect_classes(alt, classes);
            }
        }
        Column::Nested { items, .. }
        | Column::Array { items, .. }
        | Column::Assoc { items, .. } => {
            collect_classes(items, classes);
        }
        Column::Optional { values, .. } => collect_classes(values, classes),
        Column::Atomic(inner) => collect_classes(inner, classes),
        _ => {}
    }
}

/// The `TStreamerInfo` entries a file holding RNTuples with these fields needs:
/// the anchor class, which ROOT describes in every RNTuple file, then the user
/// classes of `fields`.
pub(super) fn ntuple_classes<'a>(fields: impl IntoIterator<Item = &'a Field>) -> Vec<Cls<'a>> {
    let mut classes = vec![anchor_streamer_class()];
    for field in fields {
        collect_classes(&field.data, &mut classes);
    }
    classes
}

/// [`ntuple_classes`] as a serialized `TList<TStreamerInfo>`.
pub(super) fn ntuple_streamer_info<'a>(fields: impl IntoIterator<Item = &'a Field>) -> Vec<u8> {
    streamer_info_list(&ntuple_classes(fields))
}
