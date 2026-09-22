//! The branches a caller builds: [`Branch`] and its constructors, and the
//! members of a split `std::vector<MyStruct>` branch ([`SplitMember`]).

use oxiroot_io_core::streamer_gen::{basic, Cls};
use oxiroot_io_core::{Error, Result};

use crate::value::BranchValues;

/// One named branch to write. Use the typed constructors: [`Branch::i32`] … for
/// scalars, [`Branch::vec_f64`] … for fixed-size arrays, [`Branch::jagged_f64`] …
/// for variable-length arrays, [`Branch::strings`] for strings.
pub struct Branch {
    /// Branch (and leaf) name.
    pub name: String,
    /// Branch values (a [`BranchValues`] variant — scalar, array, or string).
    /// Private: a branch is built only through the typed constructors
    /// ([`Branch::i32`], [`Branch::jagged_f64`], …) so the payload can never
    /// drift from the `kind` that decides how it is serialized.
    pub(super) values: BranchValues,
    /// How the branch is serialized; one private enum instead of three
    /// independent flags, so an invalid combination is unrepresentable.
    pub(super) kind: BranchKind,
}

/// How a [`Branch`] is written, replacing the old `jagged`/`stl_vector`/`split`
/// flag triple. The constructors pick the variant; the payload type is the
/// `Branch`'s `values`.
pub(super) enum BranchKind {
    /// A scalar, fixed-size array (`x[N]`), or string (`TLeafC`) branch.
    Plain,
    /// The `n<name>` count branch of a jagged array: a plain `Int_t` scalar
    /// whose leaf, like ROOT's, is a range (`fIsRange`) whose `fMaximum` is the
    /// largest count.
    Count,
    /// A variable-length (jagged) array: rows may differ in length, written
    /// with a paired `n<name>` count branch and an `fLeafCount` reference.
    Jagged,
    /// A `std::vector<T>` branch, written as a `TBranchElement` (each basket
    /// entry carries a 10-byte streamer header instead of a count branch).
    StlVector,
    /// A split `std::vector<MyStruct>` branch — a parent `TBranchElement` whose
    /// per-member data lives in sub-branches; the `Branch`'s `values` is unused.
    Split(SplitSpec),
}

/// A split `std::vector<MyStruct>` branch: the struct's class name and one
/// member per sub-branch.
pub(super) struct SplitSpec {
    pub(super) class_name: String,
    pub(super) members: Vec<SplitMember>,
}

impl Branch {
    pub(super) fn jagged(&self) -> bool {
        matches!(self.kind, BranchKind::Jagged)
    }
    pub(super) fn stl_vector(&self) -> bool {
        matches!(self.kind, BranchKind::StlVector)
    }
    pub(super) fn split(&self) -> Option<&SplitSpec> {
        match &self.kind {
            BranchKind::Split(spec) => Some(spec),
            _ => None,
        }
    }
    /// The kind a chunk/copy of this branch takes — the same kind unless it is a
    /// split branch (which is never chunked), in which case [`BranchKind::Plain`].
    pub(super) fn chunk_kind(&self) -> BranchKind {
        match self.kind {
            BranchKind::Jagged => BranchKind::Jagged,
            BranchKind::StlVector => BranchKind::StlVector,
            BranchKind::Count => BranchKind::Count,
            _ => BranchKind::Plain,
        }
    }
}

/// One member of a split `std::vector<MyStruct>` branch: its name and the
/// per-entry jagged values (`Vec<Vec<T>>` via a `VecXxx` [`BranchValues`]).
pub struct SplitMember {
    pub(super) name: String,
    pub(super) values: BranchValues,
}

macro_rules! split_member_ctors {
    ($($method:ident => $variant:ident($elem:ty)),* $(,)?) => {
        impl SplitMember {
            $(
                #[doc = concat!("A `", stringify!($elem), "` member of a split struct branch.")]
                pub fn $method(name: impl Into<String>, values: Vec<Vec<$elem>>) -> SplitMember {
                    SplitMember { name: name.into(), values: BranchValues::$variant(values) }
                }
            )*
        }
    };
}
split_member_ctors! {
    i8 => VecI8(i8), u8 => VecU8(u8), i16 => VecI16(i16), u16 => VecU16(u16),
    i32 => VecI32(i32), u32 => VecU32(u32), i64 => VecI64(i64), u64 => VecU64(u64),
    f32 => VecF32(f32), f64 => VecF64(f64),
}

macro_rules! branch_ctors {
    ($($method:ident => $variant:ident($elem:ty)),* $(,)?) => {
        impl Branch {
            $(
                #[doc = concat!("A branch holding `", stringify!($variant), "` values.")]
                pub fn $method(name: impl Into<String>, values: Vec<$elem>) -> Branch {
                    Branch { name: name.into(), values: BranchValues::$variant(values), kind: BranchKind::Plain }
                }
            )*
        }
    };
}
branch_ctors! {
    bools => Bool(bool), i8 => I8(i8), u8 => U8(u8), i16 => I16(i16), u16 => U16(u16),
    i32 => I32(i32), u32 => U32(u32), i64 => I64(i64), u64 => U64(u64),
    f32 => F32(f32), f64 => F64(f64),
}

/// Generate `Branch::vec_<name>` shortcuts for fixed-size array branches (each
/// inner vector must have the same length `N`, written as `x[N]`).
macro_rules! vec_ctors {
    ($($method:ident => $variant:ident($elem:ty)),* $(,)?) => {
        impl Branch {
            $(
                #[doc = concat!("A fixed-size array branch holding `", stringify!($variant), "` rows.")]
                pub fn $method(name: impl Into<String>, values: Vec<Vec<$elem>>) -> Branch {
                    Branch { name: name.into(), values: BranchValues::$variant(values), kind: BranchKind::Plain }
                }
            )*
        }
    };
}
vec_ctors! {
    vec_bool => VecBool(bool), vec_i8 => VecI8(i8), vec_u8 => VecU8(u8),
    vec_i16 => VecI16(i16), vec_u16 => VecU16(u16), vec_i32 => VecI32(i32),
    vec_u32 => VecU32(u32), vec_i64 => VecI64(i64), vec_u64 => VecU64(u64),
    vec_f32 => VecF32(f32), vec_f64 => VecF64(f64),
}

/// Generate `Branch::jagged_<name>` shortcuts for variable-length array branches
/// (rows may differ in length; written as `y[n<name>]` with a paired count
/// branch). Same backing variants as the fixed-array constructors.
macro_rules! jagged_ctors {
    ($($method:ident => $variant:ident($elem:ty)),* $(,)?) => {
        impl Branch {
            $(
                #[doc = concat!("A variable-length array branch holding `", stringify!($variant), "` rows.")]
                pub fn $method(name: impl Into<String>, values: Vec<Vec<$elem>>) -> Branch {
                    Branch { name: name.into(), values: BranchValues::$variant(values), kind: BranchKind::Jagged }
                }
            )*
        }
    };
}
jagged_ctors! {
    jagged_bool => VecBool(bool), jagged_i8 => VecI8(i8), jagged_u8 => VecU8(u8),
    jagged_i16 => VecI16(i16), jagged_u16 => VecU16(u16), jagged_i32 => VecI32(i32),
    jagged_u32 => VecU32(u32), jagged_i64 => VecI64(i64), jagged_u64 => VecU64(u64),
    jagged_f32 => VecF32(f32), jagged_f64 => VecF64(f64),
}

/// Generate `Branch::vector_<name>` shortcuts for `std::vector<T>` branches,
/// written as `TBranchElement`s (one per inner vector, variable length).
macro_rules! vector_ctors {
    ($($method:ident => $variant:ident($elem:ty)),* $(,)?) => {
        impl Branch {
            $(
                #[doc = concat!("A `std::vector<", stringify!($elem), ">` branch (a `TBranchElement`).")]
                pub fn $method(name: impl Into<String>, values: Vec<Vec<$elem>>) -> Branch {
                    Branch { name: name.into(), values: BranchValues::$variant(values), kind: BranchKind::StlVector }
                }
            )*
        }
    };
}
vector_ctors! {
    vector_i8 => VecI8(i8), vector_u8 => VecU8(u8), vector_i16 => VecI16(i16),
    vector_u16 => VecU16(u16), vector_i32 => VecI32(i32), vector_u32 => VecU32(u32),
    vector_i64 => VecI64(i64), vector_u64 => VecU64(u64),
    vector_f32 => VecF32(f32), vector_f64 => VecF64(f64),
}

impl Branch {
    /// A string branch (`TLeafC`).
    pub fn strings(name: impl Into<String>, values: Vec<String>) -> Branch {
        Branch {
            name: name.into(),
            values: BranchValues::Str(values),
            kind: BranchKind::Plain,
        }
    }

    /// A split `std::vector<MyStruct>` branch: `class_name` is the struct's C++
    /// class name and `members` its fields (each a jagged sub-branch, all sharing
    /// per-entry lengths). Written as a parent `TBranchElement` (`fSplitLevel>0`)
    /// with one sub-branch per member and the struct's generated `TStreamerInfo`.
    pub fn split_vector(
        name: impl Into<String>,
        class_name: impl Into<String>,
        members: Vec<SplitMember>,
    ) -> Branch {
        Branch {
            name: name.into(),
            values: BranchValues::I32(Vec::new()),
            kind: BranchKind::Split(SplitSpec {
                class_name: class_name.into(),
                members,
            }),
        }
    }
}

/// `(fStreamerType code, C++ type name, element byte size)` for a split-vector
/// member, from its jagged `BranchValues` variant.
pub(super) fn member_type_info(values: &BranchValues) -> (i32, &'static str, i32) {
    use BranchValues::*;
    match values {
        VecBool(_) => (18, "bool", 1),
        VecI8(_) => (1, "char", 1),
        VecU8(_) => (11, "unsigned char", 1),
        VecI16(_) => (2, "short", 2),
        VecU16(_) => (12, "unsigned short", 2),
        VecI32(_) => (3, "int", 4),
        VecU32(_) => (13, "unsigned int", 4),
        VecI64(_) => (16, "Long64_t", 8),
        VecU64(_) => (17, "ULong64_t", 8),
        VecF32(_) => (5, "float", 4),
        VecF64(_) => (8, "double", 8),
        _ => (0, "", 0),
    }
}

/// Per-entry row counts of a jagged `BranchValues` (the element count per entry).
pub(super) fn vec_row_lengths(values: &BranchValues) -> Vec<i32> {
    use BranchValues::*;
    macro_rules! lens {
        ($r:expr) => {
            $r.iter().map(|x| x.len() as i32).collect()
        };
    }
    match values {
        VecBool(r) => lens!(r),
        VecI8(r) => lens!(r),
        VecU8(r) => lens!(r),
        VecI16(r) => lens!(r),
        VecU16(r) => lens!(r),
        VecI32(r) => lens!(r),
        VecU32(r) => lens!(r),
        VecI64(r) => lens!(r),
        VecU64(r) => lens!(r),
        VecF32(r) => lens!(r),
        VecF64(r) => lens!(r),
        _ => Vec::new(),
    }
}

/// A split `std::vector<Struct>` branch stores one element per struct in every
/// member, and the writer takes the per-entry element counts from the first
/// member. So there must be at least one member, and every member must have the
/// same number of entries and the same element count in each entry; otherwise
/// the file would claim entries some members do not hold.
pub(super) fn check_split_members(branch: &str, spec: &SplitSpec) -> Result<()> {
    let Some(first) = spec.members.first() else {
        return Err(Error::InvalidInput(format!(
            "branch {branch:?}: a split {:?} branch needs at least one member; \
             add them with Branch::split_vector(.., vec![SplitMember::..])",
            spec.class_name
        )));
    };
    let counts = vec_row_lengths(&first.values);
    for m in &spec.members[1..] {
        let other = vec_row_lengths(&m.values);
        // Every member needs one value per struct: the first member's shape.
        if other.len() != counts.len() {
            return Err(Error::LengthMismatch {
                what: format!("branch {branch:?} split member {:?}", m.name),
                expected: counts.len(),
                found: other.len(),
            });
        }
        if let Some(entry) = other.iter().zip(&counts).position(|(a, b)| a != b) {
            return Err(Error::LengthMismatch {
                what: format!(
                    "branch {branch:?} split member {:?} in entry {entry}",
                    m.name
                ),
                expected: counts[entry] as usize,
                found: other[entry] as usize,
            });
        }
    }
    Ok(())
}

/// ROOT's class checksum: `id = id*3 + ch` over the class name, then each
/// member's name and type-name characters. Matches `TClass::GetCheckSum` for a
/// struct of plain members. (ROOT's split reader ignores it, but we match it.)
pub(super) fn class_checksum(class_name: &str, members: &[SplitMember]) -> u32 {
    let mut id: u32 = 0;
    let mut feed = |s: &str| {
        for ch in s.bytes() {
            id = id.wrapping_mul(3).wrapping_add(u32::from(ch));
        }
    };
    feed(class_name);
    for m in members {
        feed(&m.name);
        feed(member_type_info(&m.values).1);
    }
    id
}

/// The `TStreamerInfo` entry for a split branch's struct: version 1, one basic
/// member per struct member, as ROOT writes it for a class without `ClassDef`.
pub(super) fn split_class(spec: &SplitSpec) -> Cls<'_> {
    Cls {
        name: spec.class_name.as_str().into(),
        version: 1,
        checksum: class_checksum(&spec.class_name, &spec.members),
        elements: spec
            .members
            .iter()
            .map(|m| {
                let (ty, type_name, size) = member_type_info(&m.values);
                basic(&m.name, ty, size, type_name)
            })
            .collect(),
    }
}
