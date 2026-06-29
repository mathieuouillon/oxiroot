//! Writing a `TTree` into a ROOT file.
//!
//! Supports scalar, fixed-size array (`x[N]`), variable-length / jagged
//! (`x[n<name>]`, with an auto-generated count branch and an `fLeafCount`
//! reference), string (`TLeafC`), and `std::vector<T>` (`TBranchElement`)
//! branches. Mirrors the layout ROOT/uproot write (TTree v20, TBranch v13,
//! TLeaf* v1, TBranchElement v10) so the result reads back in ROOT, uproot, and
//! this crate. The embedded `TStreamerInfo` ([`crate::streamer_gen`]) makes the
//! file self-describing.

use std::collections::HashMap;
use std::io::{self, Seek, SeekFrom, Write};
use std::path::Path;

use oxiroot_io_core::buffer::{CountToken, Patch, WBuffer, K_BYTE_COUNT_MASK};
use oxiroot_io_core::error::{Error, Result};
use oxiroot_io_core::streamer::{write_tnamed, write_tobject};
use oxiroot_io_core::{guard_small_format, key_len, key_len_fmt, write_key_header, Compression};

use crate::value::BranchValues;

/// Fixed creation/modification timestamp (`TDatime`); readers don't validate it.
const DATIME: u32 = 0x7d7a_79ca;
/// Small-format on-disk file version.
const FILE_VERSION: u32 = 62400;
/// `fBits` ROOT writes for embedded `TObject`s.
const OBJ_BITS: u32 = 0x0300_0000;
/// ROOT's object-map displacement (`kMapOffset`): a referenced object is keyed
/// at `byte_count_position + keylen + 2`. Used to point a jagged leaf's
/// `fLeafCount` at the already-written count leaf.
const K_MAP_OFFSET: u32 = 2;

/// Maps a leaf name to the object-reference value (`pos + keylen + kMapOffset`)
/// of the first place that leaf was written, so a later `fLeafCount` can point
/// back to it the way ROOT does.
type LeafRefs = HashMap<String, u32>;

/// A write [`Branch`] is only ever built through the typed constructors, none of
/// which produce a [`BranchValues::Nested`] (a read-only doubly-nested
/// collection), so the writer's value matches never see one.
const NESTED_NOT_WRITABLE: &str = "a write Branch is never built with a Nested value";

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
    values: BranchValues,
    /// How the branch is serialized; one private enum instead of three
    /// independent flags, so an invalid combination is unrepresentable.
    kind: BranchKind,
}

/// How a [`Branch`] is written, replacing the old `jagged`/`stl_vector`/`split`
/// flag triple. The constructors pick the variant; the payload type is the
/// `Branch`'s `values`.
enum BranchKind {
    /// A scalar, fixed-size array (`x[N]`), or string (`TLeafC`) branch.
    Plain,
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
struct SplitSpec {
    class_name: String,
    members: Vec<SplitMember>,
}

impl Branch {
    fn jagged(&self) -> bool {
        matches!(self.kind, BranchKind::Jagged)
    }
    fn stl_vector(&self) -> bool {
        matches!(self.kind, BranchKind::StlVector)
    }
    fn split(&self) -> Option<&SplitSpec> {
        match &self.kind {
            BranchKind::Split(spec) => Some(spec),
            _ => None,
        }
    }
    /// The kind a chunk/copy of this branch takes — the same kind unless it is a
    /// split branch (which is never chunked), in which case [`BranchKind::Plain`].
    fn chunk_kind(&self) -> BranchKind {
        match self.kind {
            BranchKind::Jagged => BranchKind::Jagged,
            BranchKind::StlVector => BranchKind::StlVector,
            _ => BranchKind::Plain,
        }
    }
}

/// One member of a split `std::vector<MyStruct>` branch: its name and the
/// per-entry jagged values (`Vec<Vec<T>>` via a `VecXxx` [`BranchValues`]).
pub struct SplitMember {
    name: String,
    values: BranchValues,
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
fn member_type_info(values: &BranchValues) -> (i32, &'static str, i32) {
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
fn vec_row_lengths(values: &BranchValues) -> Vec<i32> {
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

/// ROOT's class checksum: `id = id*3 + ch` over the class name, then each
/// member's name and type-name characters. Matches `TClass::GetCheckSum` for a
/// struct of plain members. (ROOT's split reader ignores it, but we match it.)
fn class_checksum(class_name: &str, members: &[SplitMember]) -> u32 {
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

/// Serialize a `TStreamerInfo` for a struct of primitive members (every object
/// written with `kNewClassTag`). Layout confirmed against ROOT: `TStreamerInfo`
/// v10 → `TObjArray` v3 of `TStreamerBasicType` v2 (a `TStreamerElement` v4 each).
fn write_class_streamer_info(class_name: &str, members: &[SplitMember]) -> Vec<u8> {
    let checksum = class_checksum(class_name, members);
    let mut w = WBuffer::new();
    let bc = begin_object_any(&mut w, "TStreamerInfo");
    let si = w.begin_object(10); // TStreamerInfo v10
    write_tnamed(&mut w, 0x0001_0000, class_name, "");
    w.be_u32(checksum);
    w.be_i32(1); // fClassVersion
    let oa_bc = begin_object_any(&mut w, "TObjArray");
    let oa = w.begin_object(3); // TObjArray v3
    write_tobject(&mut w, 0);
    w.string(""); // fName
    w.be_i32(members.len() as i32);
    w.be_i32(0); // fLowerBound
    for m in members {
        let (f_type, type_name, size) = member_type_info(&m.values);
        let e_bc = begin_object_any(&mut w, "TStreamerBasicType");
        let bt = w.begin_object(2); // TStreamerBasicType v2
        let se = w.begin_object(4); // TStreamerElement v4 (base)
        write_tnamed(&mut w, 0, &m.name, "");
        w.be_i32(f_type);
        w.be_i32(size);
        w.be_i32(0); // fArrayLength
        w.be_i32(0); // fArrayDim
        for _ in 0..5 {
            w.be_i32(0); // fMaxIndex[5]
        }
        w.string(type_name); // fTypeName
        w.end_object(se);
        w.end_object(bt);
        end_object_any(&mut w, e_bc);
    }
    w.end_object(oa);
    end_object_any(&mut w, oa_bc);
    w.end_object(si);
    end_object_any(&mut w, bc);
    w.into_vec()
}

/// Append a `TStreamerInfo` object to a baked `TList<TStreamerInfo>` blob (body
/// `{bcnt}{ver}{TObject}{fName}{nobjects}{(obj + option-TString)*}`, no trailer):
/// bump the outer byte count and `nobjects`, then append the object + empty option.
fn append_streamer_info(blob: &[u8], info: &[u8]) -> Vec<u8> {
    let mut out = blob.to_vec();
    let added = info.len() + 1; // object + empty option TString (0x00)

    let bcnt = u32::from_be_bytes([out[0], out[1], out[2], out[3]]);
    let new_bcnt = ((bcnt & !K_BYTE_COUNT_MASK) + added as u32) | K_BYTE_COUNT_MASK;
    out[0..4].copy_from_slice(&new_bcnt.to_be_bytes());

    // bcnt(4) ver(2) TObject{ver(2) uid(4) bits(4)} fName(TString) -> nobjects(i32)
    let mut p = 4 + 2 + 2 + 4 + 4;
    let n = out[p] as usize;
    p += 1 + if n == 255 {
        4 + u32::from_be_bytes([out[p + 1], out[p + 2], out[p + 3], out[p + 4]]) as usize
    } else {
        n
    };
    let nobjects = i32::from_be_bytes([out[p], out[p + 1], out[p + 2], out[p + 3]]);
    out[p..p + 4].copy_from_slice(&(nobjects + 1).to_be_bytes());

    out.extend_from_slice(info);
    out.push(0); // empty option TString
    out
}

/// Whether a branch is a scalar, a fixed-size array, a variable-length (jagged)
/// array, a `std::vector<T>` (`TBranchElement`), or a string.
enum Kind {
    Scalar,
    FixedArray(usize),
    Jagged,
    StlVector,
    Str,
}

/// The on-disk description of one leaf type.
struct LeafInfo {
    /// `TLeafI`/`TLeafD`/`TLeafC`/… class name.
    class: &'static str,
    /// Leaflist type code (`I`/`D`/`C`/…) used in the branch title.
    code: char,
    /// Element byte width (the data stride; 1 for a `TLeafC` char).
    size: i32,
    /// `fLenType` (the element width for numerics, 0 for `TLeafC`).
    len_type: i32,
    /// `fIsUnsigned`.
    unsigned: bool,
}

impl Branch {
    /// Number of entries (rows for arrays/strings).
    fn n_entries(&self) -> u32 {
        use BranchValues::*;
        if let Some(spec) = self.split() {
            return spec
                .members
                .first()
                .map_or(0, |m| vec_row_lengths(&m.values).len()) as u32;
        }
        let n = match &self.values {
            Bool(v) => v.len(),
            I8(v) => v.len(),
            U8(v) => v.len(),
            I16(v) => v.len(),
            U16(v) => v.len(),
            I32(v) => v.len(),
            U32(v) => v.len(),
            I64(v) => v.len(),
            U64(v) => v.len(),
            F32(v) => v.len(),
            F64(v) => v.len(),
            VecBool(r) => r.len(),
            VecI8(r) => r.len(),
            VecU8(r) => r.len(),
            VecI16(r) => r.len(),
            VecU16(r) => r.len(),
            VecI32(r) => r.len(),
            VecU32(r) => r.len(),
            VecI64(r) => r.len(),
            VecU64(r) => r.len(),
            VecF32(r) => r.len(),
            VecF64(r) => r.len(),
            Str(v) => v.len(),
            VecStr(v) => v.len(),
            Nested { .. } => unreachable!("{NESTED_NOT_WRITABLE}"),
        };
        n as u32
    }

    /// The element leaf type (the inner type for arrays). A `std::vector<T>`
    /// branch keeps the element width/sign but its leaf class is `TLeafElement`.
    fn leaf(&self) -> LeafInfo {
        use BranchValues::*;
        let (mut class, code, size, unsigned) = match &self.values {
            Bool(_) | VecBool(_) => ("TLeafO", 'O', 1, false),
            I8(_) | VecI8(_) => ("TLeafB", 'B', 1, false),
            U8(_) | VecU8(_) => ("TLeafB", 'b', 1, true),
            I16(_) | VecI16(_) => ("TLeafS", 'S', 2, false),
            U16(_) | VecU16(_) => ("TLeafS", 's', 2, true),
            I32(_) | VecI32(_) => ("TLeafI", 'I', 4, false),
            U32(_) | VecU32(_) => ("TLeafI", 'i', 4, true),
            I64(_) | VecI64(_) => ("TLeafL", 'L', 8, false),
            U64(_) | VecU64(_) => ("TLeafL", 'l', 8, true),
            F32(_) | VecF32(_) => ("TLeafF", 'F', 4, false),
            F64(_) | VecF64(_) => ("TLeafD", 'D', 8, false),
            // vector<string> is read-only; it never reaches the writer.
            Str(_) | VecStr(_) => ("TLeafC", 'C', 1, false),
            Nested { .. } => unreachable!("{NESTED_NOT_WRITABLE}"),
        };
        let len_type = if matches!(self.values, Str(_)) || self.stl_vector() {
            0
        } else {
            size
        };
        if self.stl_vector() {
            class = "TLeafElement";
        }
        LeafInfo {
            class,
            code,
            size,
            len_type,
            unsigned,
        }
    }

    /// The maximum value among integer scalar leaves — ROOT's `fMaximum`, which
    /// it uses to size the buffer when this leaf is a leaf count. 0 for
    /// non-integer leaves (where `fMaximum` is unused for reading).
    fn leaf_max(&self) -> i64 {
        use BranchValues::*;
        match &self.values {
            Bool(v) => i64::from(v.iter().any(|&b| b)),
            I8(v) => v.iter().copied().max().unwrap_or(0) as i64,
            U8(v) => v.iter().copied().max().unwrap_or(0) as i64,
            I16(v) => v.iter().copied().max().unwrap_or(0) as i64,
            U16(v) => v.iter().copied().max().unwrap_or(0) as i64,
            I32(v) => v.iter().copied().max().unwrap_or(0) as i64,
            U32(v) => v.iter().copied().max().unwrap_or(0) as i64,
            I64(v) => v.iter().copied().max().unwrap_or(0),
            U64(v) => v.iter().copied().max().unwrap_or(0) as i64,
            // A TLeafC's fMaximum is the longest string length + 1 (the buffer
            // size); ROOT uses it to size fValue and reallocates if it is 0.
            Str(v) => v.iter().map(|s| s.len() as i64).max().unwrap_or(0) + 1,
            _ => 0,
        }
    }

    /// The TLeafC `fLen` (longest string length + 1) for a string branch.
    fn str_len(&self) -> i32 {
        match &self.values {
            BranchValues::Str(v) => v.iter().map(|s| s.len()).max().unwrap_or(0) as i32 + 1,
            _ => 1,
        }
    }

    /// Elements per entry: `N` for a fixed array (from row 0), else 1 (scalar,
    /// string, and the jagged leaf — whose per-entry length is dynamic).
    fn flen(&self) -> i32 {
        use BranchValues::*;
        if self.jagged() || self.stl_vector() {
            return 1;
        }
        let n = match &self.values {
            VecBool(r) => r.first().map_or(0, Vec::len),
            VecI8(r) => r.first().map_or(0, Vec::len),
            VecU8(r) => r.first().map_or(0, Vec::len),
            VecI16(r) => r.first().map_or(0, Vec::len),
            VecU16(r) => r.first().map_or(0, Vec::len),
            VecI32(r) => r.first().map_or(0, Vec::len),
            VecU32(r) => r.first().map_or(0, Vec::len),
            VecI64(r) => r.first().map_or(0, Vec::len),
            VecU64(r) => r.first().map_or(0, Vec::len),
            VecF32(r) => r.first().map_or(0, Vec::len),
            VecF64(r) => r.first().map_or(0, Vec::len),
            _ => 1,
        };
        n as i32
    }

    /// Whether this is an array branch whose rows differ in length (not yet
    /// writable — variable-length numeric arrays need a separate count branch).
    fn is_jagged(&self) -> bool {
        use BranchValues::*;
        macro_rules! jag {
            ($r:expr) => {{
                let n = $r.first().map_or(0, Vec::len);
                $r.iter().any(|x| x.len() != n)
            }};
        }
        match &self.values {
            VecBool(r) => jag!(r),
            VecI8(r) => jag!(r),
            VecU8(r) => jag!(r),
            VecI16(r) => jag!(r),
            VecU16(r) => jag!(r),
            VecI32(r) => jag!(r),
            VecU32(r) => jag!(r),
            VecI64(r) => jag!(r),
            VecU64(r) => jag!(r),
            VecF32(r) => jag!(r),
            VecF64(r) => jag!(r),
            _ => false,
        }
    }

    fn kind(&self) -> Kind {
        use BranchValues::*;
        if self.stl_vector() {
            return Kind::StlVector;
        }
        if self.jagged() {
            return Kind::Jagged;
        }
        match &self.values {
            Str(_) => Kind::Str,
            VecBool(_) | VecI8(_) | VecU8(_) | VecI16(_) | VecU16(_) | VecI32(_) | VecU32(_)
            | VecI64(_) | VecU64(_) | VecF32(_) | VecF64(_) => {
                Kind::FixedArray(self.flen() as usize)
            }
            _ => Kind::Scalar,
        }
    }

    /// The name of the auto-generated count branch for a jagged branch (`y` →
    /// `ny`), matching uproot's convention.
    fn count_name(&self) -> String {
        format!("n{}", self.name)
    }

    /// Per-row element counts (for a jagged branch's count branch); empty for
    /// non-array branches.
    fn row_lengths(&self) -> Vec<i32> {
        use BranchValues::*;
        macro_rules! lens {
            ($r:expr) => {
                $r.iter().map(|x| x.len() as i32).collect()
            };
        }
        match &self.values {
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

    /// The paired count branch (`n<name>`, a scalar `i32` of row lengths) for a
    /// jagged branch; `None` for any other branch.
    fn count_branch(&self) -> Option<Branch> {
        self.jagged().then(|| Branch {
            name: self.count_name(),
            values: BranchValues::I32(self.row_lengths()),
            kind: BranchKind::Plain,
        })
    }

    /// The `std::vector<T>` class name and `fCheckSum` ROOT writes for the
    /// element type (used when this is a `TBranchElement`). Checksums are the
    /// fixed values ROOT computes for each `vector<T>` specialization.
    fn stl_info(&self) -> (&'static str, u32) {
        use BranchValues::*;
        match &self.values {
            VecI8(_) => ("vector<char>", 2107423027),
            VecU8(_) => ("vector<unsigned char>", 3193843768),
            VecI16(_) => ("vector<short>", 2609783071),
            VecU16(_) => ("vector<unsigned short>", 2240785856),
            VecI32(_) => ("vector<int>", 1796663354),
            VecU32(_) => ("vector<unsigned int>", 2269658365),
            VecI64(_) => ("vector<Long64_t>", 1788137638),
            VecU64(_) => ("vector<ULong64_t>", 3999597035),
            VecF32(_) => ("vector<float>", 1727547419),
            VecF64(_) => ("vector<double>", 3894200540),
            _ => ("vector<float>", 0),
        }
    }

    /// Entry data + `fEntryOffset` for a `std::vector<T>` branch: each row is a
    /// streamed collection — `[byte count | mask](4) [0x000a](2) [size n](4)`
    /// then `n` big-endian elements.
    fn stl_basket_content(&self) -> (Vec<u8>, Vec<u32>) {
        use BranchValues::*;
        let mut data = Vec::new();
        let mut offsets = vec![0u32];
        macro_rules! emit {
            ($rows:expr, $w:expr, $conv:expr) => {{
                for row in $rows {
                    let n = row.len() as u32;
                    let bc = (6 + n * $w) | 0x4000_0000;
                    data.extend_from_slice(&bc.to_be_bytes());
                    data.extend_from_slice(&0x000a_u16.to_be_bytes());
                    data.extend_from_slice(&n.to_be_bytes());
                    for x in row {
                        data.extend_from_slice(&$conv(x));
                    }
                    offsets.push(data.len() as u32);
                }
            }};
        }
        match &self.values {
            VecI8(r) => emit!(r, 1, |x: &i8| [*x as u8]),
            VecU8(r) => emit!(r, 1, |x: &u8| [*x]),
            VecI16(r) => emit!(r, 2, |x: &i16| x.to_be_bytes()),
            VecU16(r) => emit!(r, 2, |x: &u16| x.to_be_bytes()),
            VecI32(r) => emit!(r, 4, |x: &i32| x.to_be_bytes()),
            VecU32(r) => emit!(r, 4, |x: &u32| x.to_be_bytes()),
            VecI64(r) => emit!(r, 8, |x: &i64| x.to_be_bytes()),
            VecU64(r) => emit!(r, 8, |x: &u64| x.to_be_bytes()),
            VecF32(r) => emit!(r, 4, |x: &f32| x.to_be_bytes()),
            VecF64(r) => emit!(r, 8, |x: &f64| x.to_be_bytes()),
            _ => {}
        }
        (data, offsets)
    }

    /// The basket's uncompressed entry data, plus (for variable branches) the
    /// data-relative `fEntryOffset` array (`n_entries + 1` offsets).
    fn basket_content(&self) -> (Vec<u8>, Option<Vec<u32>>) {
        use BranchValues::*;
        if self.stl_vector() {
            let (data, offsets) = self.stl_basket_content();
            return (data, Some(offsets));
        }
        macro_rules! be {
            ($v:expr, $w:expr) => {{
                let mut out = Vec::with_capacity($v.len() * $w);
                for x in $v {
                    out.extend_from_slice(&x.to_be_bytes());
                }
                out
            }};
        }
        macro_rules! be_rows {
            ($r:expr, $w:expr) => {{
                let mut out = Vec::new();
                for row in $r {
                    for x in row {
                        out.extend_from_slice(&x.to_be_bytes());
                    }
                }
                out
            }};
        }
        let data = match &self.values {
            Bool(v) => v.iter().map(|&b| b as u8).collect(),
            I8(v) => v.iter().map(|&x| x as u8).collect(),
            U8(v) => v.clone(),
            I16(v) => be!(v, 2),
            U16(v) => be!(v, 2),
            I32(v) => be!(v, 4),
            U32(v) => be!(v, 4),
            I64(v) => be!(v, 8),
            U64(v) => be!(v, 8),
            F32(v) => be!(v, 4),
            F64(v) => be!(v, 8),
            VecBool(r) => r.iter().flatten().map(|&b| b as u8).collect(),
            VecI8(r) => r.iter().flatten().map(|&x| x as u8).collect(),
            VecU8(r) => r.concat(),
            VecI16(r) => be_rows!(r, 2),
            VecU16(r) => be_rows!(r, 2),
            VecI32(r) => be_rows!(r, 4),
            VecU32(r) => be_rows!(r, 4),
            VecI64(r) => be_rows!(r, 8),
            VecU64(r) => be_rows!(r, 8),
            VecF32(r) => be_rows!(r, 4),
            VecF64(r) => be_rows!(r, 8),
            Str(strings) => {
                let mut data = Vec::new();
                let mut offsets = vec![0u32];
                for s in strings {
                    let b = s.as_bytes();
                    if b.len() < 255 {
                        data.push(b.len() as u8);
                    } else {
                        data.push(255);
                        data.extend_from_slice(&(b.len() as u32).to_be_bytes());
                    }
                    data.extend_from_slice(b);
                    offsets.push(data.len() as u32);
                }
                return (data, Some(offsets));
            }
            // vector<string> is read-only; the writer never receives one.
            VecStr(_) => unreachable!("vector<string> branches cannot be written"),
            Nested { .. } => unreachable!("{NESTED_NOT_WRITABLE}"),
        };
        // A jagged numeric branch is variable-length too: emit the byte offset
        // after each row (element count × element width).
        if self.jagged() {
            let elem = self.leaf().size as u32;
            let mut offsets = Vec::with_capacity(self.n_entries() as usize + 1);
            let mut acc = 0u32;
            offsets.push(0);
            for len in self.row_lengths() {
                acc += len as u32 * elem;
                offsets.push(acc);
            }
            return (data, Some(offsets));
        }
        (data, None)
    }
}

/// One basket's recorded location, for the branch metadata.
#[derive(Clone, Copy)]
struct BasketRec {
    seek: u64,
    nbytes: u32,
    /// Number of entries this basket holds (for the cumulative `fBasketEntry`).
    n_entries: u32,
}

/// Write a single-tree ROOT file containing the flat scalar `branches`.
pub fn write_tree_file(
    path: impl AsRef<Path>,
    tree_name: &str,
    branches: &[Branch],
    compression: Compression,
) -> Result<()> {
    let path = path.as_ref();
    let file_name = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("file.root");
    std::fs::write(
        path,
        tree_file_bytes(file_name, tree_name, branches, compression)?,
    )?;
    Ok(())
}

/// Build the bytes of a single-tree ROOT file (one basket per branch).
///
/// Returns an error if a fixed-array branch ([`Branch::vec_f64`] …) was given
/// rows of differing length — use [`Branch::jagged_f64`] … for that.
pub fn tree_file_bytes(
    file_name: &str,
    tree_name: &str,
    branches: &[Branch],
    compression: Compression,
) -> Result<Vec<u8>> {
    tree_bytes(file_name, tree_name, branches, compression, 0)
}

/// Write a single-tree ROOT file, splitting each branch into baskets of at most
/// `entries_per_basket` entries (`0` = one basket per branch). Multiple baskets
/// let a large tree be stored the way ROOT writes it; split `std::vector<Struct>`
/// branches are always one basket (their per-member alignment is not chunked).
pub fn write_tree_file_baskets(
    path: impl AsRef<Path>,
    tree_name: &str,
    branches: &[Branch],
    compression: Compression,
    entries_per_basket: usize,
) -> Result<()> {
    let path = path.as_ref();
    let file_name = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("file.root");
    std::fs::write(
        path,
        tree_bytes(
            file_name,
            tree_name,
            branches,
            compression,
            entries_per_basket,
        )?,
    )?;
    Ok(())
}

/// A tree to write: a name and its [`Branch`]es. The method-based,
/// write-side counterpart to the free [`write_tree_file`] function (and to the
/// read-only [`TTree`](crate::TTree)) — build one, then call
/// [`write_root`](Tree::write_root), mirroring `hist.write_root`:
///
/// ```no_run
/// use oxiroot_tree::{Branch, Tree};
/// use oxiroot_io_core::Compression;
///
/// let branches = vec![
///     Branch::i32("event", vec![1, 2, 3]),
///     Branch::f64("energy", vec![10.5, 20.1, 5.0]),
/// ];
/// Tree::new("Events", branches).write_root("tree.root", Compression::None)?;
/// # Ok::<(), oxiroot_io_core::error::Error>(())
/// ```
pub struct Tree {
    name: String,
    branches: Vec<Branch>,
}

impl Tree {
    /// Create a writable tree from a name and its branches.
    pub fn new(name: impl Into<String>, branches: Vec<Branch>) -> Tree {
        Tree {
            name: name.into(),
            branches,
        }
    }

    /// The tree's name (the in-file `TTree` key).
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The tree's branches.
    pub fn branches(&self) -> &[Branch] {
        &self.branches
    }

    /// Write this tree as a new single-tree ROOT file (one basket per branch),
    /// readable by ROOT and uproot. The method form of [`write_tree_file`].
    pub fn write_root(&self, path: impl AsRef<Path>, compression: Compression) -> Result<()> {
        write_tree_file(path, &self.name, &self.branches, compression)
    }

    /// Like [`write_root`](Tree::write_root) but split each branch into baskets
    /// of at most `entries_per_basket` entries (`0` = one basket per branch).
    /// The method form of [`write_tree_file_baskets`].
    pub fn write_root_baskets(
        &self,
        path: impl AsRef<Path>,
        compression: Compression,
        entries_per_basket: usize,
    ) -> Result<()> {
        write_tree_file_baskets(
            path,
            &self.name,
            &self.branches,
            compression,
            entries_per_basket,
        )
    }

    /// The complete ROOT-file bytes for this tree (the method form of
    /// [`tree_file_bytes`]); `file_name` is the `TFile` name recorded in the
    /// file header.
    pub fn to_root_bytes(&self, file_name: &str, compression: Compression) -> Result<Vec<u8>> {
        tree_file_bytes(file_name, &self.name, &self.branches, compression)
    }
}

/// A column's identity, used to check that every batch shares the first batch's
/// schema: name, value-variant, and the array/`std::vector` flags. Fixed-array
/// width is folded into the variant via `flen` so a shape change is caught too.
#[derive(PartialEq)]
struct ColSig {
    name: String,
    variant: std::mem::Discriminant<BranchValues>,
    jagged: bool,
    stl_vector: bool,
    flen: i32,
}

fn col_sig(b: &Branch) -> ColSig {
    ColSig {
        name: b.name.clone(),
        variant: std::mem::discriminant(&b.values),
        jagged: b.jagged(),
        stl_vector: b.stl_vector(),
        flen: b.flen(),
    }
}

/// The running aggregate a streamed column must keep so its leaf metadata is
/// correct once every batch has been seen.
enum ColAgg {
    /// A plain data column (scalar / fixed array / jagged data / `std::vector`):
    /// its leaf needs no value-derived aggregate.
    Data,
    /// A synthetic `n<name>` count column: track the maximum multiplicity, which
    /// becomes the count leaf's `fMaximum` (ROOT sizes the read buffer from it).
    Count(i64),
    /// A `TLeafC` string column: track the longest string length + 1, which is
    /// the leaf's `fLen` (the buffer ROOT allocates for the string).
    Str(i32),
}

/// Accumulated state for one effective output column across batches.
struct StreamCol {
    /// A representative branch carrying the column's type/flags (and, for fixed
    /// arrays, its width) plus minimal values, so [`build_tree_object`] can emit
    /// the branch/leaf metadata. Its aggregate-bearing values (count `fMaximum`,
    /// string `fLen`) are kept at the running maximum via [`StreamCol::agg`].
    rep: Branch,
    /// One [`BasketRec`] per batch written so far.
    baskets: Vec<BasketRec>,
    agg: ColAgg,
}

/// A streaming, bounded-memory `TTree` writer. Append entries in batches with
/// [`write_batch`](TTreeWriter::write_batch); each call emits one basket per
/// branch straight to the sink, so only the current batch's data is held in
/// memory (the way ROOT's `TTree::Fill` flushes baskets as they fill).
/// [`finish`](TTreeWriter::finish) writes the small `TTree` metadata, the
/// streamer info, and the key list, then patches the file header.
///
/// Every batch must share the first batch's schema: branch names, element
/// types, the jagged / `std::vector` flags, and fixed-array widths. Split
/// `std::vector<Struct>` branches are not supported here — use
/// [`write_tree_file`] for those.
///
/// ```no_run
/// use oxiroot_io_core::Compression;
/// use oxiroot_tree::{Branch, TTreeWriter};
///
/// let mut w = TTreeWriter::create("big.root", "T", Compression::None)?;
/// for batch in 0..1_000 {
///     let x: Vec<f64> = (0..10_000).map(|i| (batch * 10_000 + i) as f64).collect();
///     w.write_batch(&[Branch::f64("x", x)])?; // one basket, flushed now
/// }
/// w.finish()?;
/// # Ok::<(), oxiroot_io_core::Error>(())
/// ```
pub struct TTreeWriter<W: Write + Seek> {
    sink: W,
    pos: u64,
    file_name: String,
    tree_name: String,
    compression: u32,
    // File-header regions to back-patch at finish (absolute offsets).
    p_end: u64,
    p_nbytes_name: u64,
    p_seek_info: u64,
    p_nbytes_info: u64,
    p_dir_nbytes_keys: u64,
    p_dir_seek_keys: u64,
    f_nbytes_name: u32,
    /// Effective columns (count branches expanded inline); set by the first batch.
    columns: Vec<StreamCol>,
    /// The first batch's schema; `None` until the first batch is written.
    schema: Option<Vec<ColSig>>,
    total_entries: i64,
}

impl TTreeWriter<std::fs::File> {
    /// Create a streaming tree file at `path`. The tree is named `tree_name`;
    /// the file is the small (32-bit) container, so the total must stay under
    /// 2 GiB ([`finish`](TTreeWriter::finish) errors otherwise).
    pub fn create(
        path: impl AsRef<Path>,
        tree_name: &str,
        compression: Compression,
    ) -> Result<Self> {
        let path = path.as_ref();
        let file_name = path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("file.root")
            .to_string();
        let file = std::fs::File::create(path)?;
        TTreeWriter::new(file, &file_name, tree_name, compression)
    }
}

impl<W: Write + Seek> TTreeWriter<W> {
    /// Begin writing into an arbitrary seekable sink. The file header and root
    /// directory record are written immediately (with pointers patched at the
    /// end); `file_name` is the name stored in the directory record.
    pub fn new(
        sink: W,
        file_name: &str,
        tree_name: &str,
        compression: Compression,
    ) -> Result<Self> {
        let compression = compression.setting();
        let mut w = WBuffer::new();
        let pp = write_file_prefix(&mut w, file_name, compression);
        // The prefix is written at file offset 0, so a reserved region's buffer
        // offset is its absolute file offset.
        let p_end = w.patch_offset(pp.p_end) as u64;
        let p_nbytes_name = w.patch_offset(pp.p_nbytes_name) as u64;
        let p_seek_info = w.patch_offset(pp.p_seek_info) as u64;
        let p_nbytes_info = w.patch_offset(pp.p_nbytes_info) as u64;
        let p_dir_nbytes_keys = w.patch_offset(pp.p_dir_nbytes_keys) as u64;
        let p_dir_seek_keys = w.patch_offset(pp.p_dir_seek_keys) as u64;

        let prefix = w.into_vec();
        let pos = prefix.len() as u64;
        let mut sink = sink;
        sink.write_all(&prefix)?;

        Ok(TTreeWriter {
            sink,
            pos,
            file_name: file_name.to_string(),
            tree_name: tree_name.to_string(),
            compression,
            p_end,
            p_nbytes_name,
            p_seek_info,
            p_nbytes_info,
            p_dir_nbytes_keys,
            p_dir_seek_keys,
            f_nbytes_name: pp.f_nbytes_name as u32,
            columns: Vec::new(),
            schema: None,
            total_entries: 0,
        })
    }

    /// Total entries appended so far.
    #[must_use]
    pub fn num_entries(&self) -> i64 {
        self.total_entries
    }

    fn put(&mut self, bytes: &[u8]) -> io::Result<()> {
        self.sink.write_all(bytes)?;
        self.pos += bytes.len() as u64;
        Ok(())
    }

    fn patch_u32(&mut self, offset: u64, value: u32) -> io::Result<()> {
        self.sink.seek(SeekFrom::Start(offset))?;
        self.sink.write_all(&value.to_be_bytes())
    }

    /// Append one batch of entries (one basket per branch). The first batch fixes
    /// the schema; later batches must match it. An empty batch is a no-op.
    pub fn write_batch(&mut self, branches: &[Branch]) -> Result<()> {
        for b in branches {
            if b.split().is_some() {
                return Err(Error::Format(format!(
                    "branch {:?}: TTreeWriter does not support split std::vector<Struct> branches; \
                     use write_tree_file for those",
                    b.name
                )));
            }
            if !b.jagged() && !b.stl_vector() && b.is_jagged() {
                return Err(Error::Format(format!(
                    "branch {:?}: rows differ in length; use Branch::jagged_* or Branch::vector_*",
                    b.name
                )));
            }
        }

        // All branches in a batch must carry the same number of entries.
        let batch_entries = branches.first().map_or(0, Branch::n_entries);
        if let Some(b) = branches.iter().find(|b| b.n_entries() != batch_entries) {
            return Err(Error::Format(format!(
                "branch {:?} has {} entries but the batch's first branch has {batch_entries}",
                b.name,
                b.n_entries()
            )));
        }
        if batch_entries == 0 {
            return Ok(());
        }

        let sig: Vec<ColSig> = branches.iter().map(col_sig).collect();
        match &self.schema {
            Some(prev) if *prev != sig => {
                return Err(Error::Format(
                    "this batch's branch schema differs from the first batch's".into(),
                ))
            }
            Some(_) => {}
            None => {
                self.init_columns(branches);
                self.schema = Some(sig);
            }
        }

        // Walk the effective columns in lockstep: a jagged branch contributes its
        // synthetic count column first, then its data column.
        let mut col = 0;
        let tree_name = self.tree_name.clone();
        for b in branches {
            if b.jagged() {
                let count = b
                    .count_branch()
                    .expect("a jagged branch has a count branch");
                self.emit(col, &count, &tree_name)?;
                col += 1;
            }
            self.emit(col, b, &tree_name)?;
            col += 1;
        }
        self.total_entries += i64::from(batch_entries);
        Ok(())
    }

    /// Emit one basket for column `col`, append its record, and grow that
    /// column's leaf aggregate (count `fMaximum` / string `fLen`).
    fn emit(&mut self, col: usize, branch: &Branch, tree_name: &str) -> Result<()> {
        let (bytes, rec) = basket_bytes(branch, tree_name, self.compression, self.pos);
        self.put(&bytes)?;
        let c = &mut self.columns[col];
        c.baskets.push(rec);
        match &mut c.agg {
            ColAgg::Count(m) => {
                let batch_max = branch.leaf_max();
                if batch_max > *m {
                    *m = batch_max;
                    c.rep.values = BranchValues::I32(vec![*m as i32]);
                }
            }
            ColAgg::Str(len) => {
                let batch_len = branch.str_len();
                if batch_len > *len {
                    *len = batch_len;
                    c.rep.values = str_rep(*len);
                }
            }
            ColAgg::Data => {}
        }
        Ok(())
    }

    /// Build the effective-column list from the first batch (jagged branches
    /// expanded into a synthetic count column followed by the data column).
    fn init_columns(&mut self, branches: &[Branch]) {
        let mut cols = Vec::new();
        for b in branches {
            if b.jagged() {
                let count = b
                    .count_branch()
                    .expect("a jagged branch has a count branch");
                let m = count.leaf_max();
                cols.push(StreamCol {
                    rep: Branch {
                        name: count.name.clone(),
                        values: BranchValues::I32(vec![m as i32]),
                        kind: BranchKind::Plain,
                    },
                    baskets: Vec::new(),
                    agg: ColAgg::Count(m),
                });
            }
            let (agg, values) = if matches!(b.kind(), Kind::Str) {
                let len = b.str_len();
                (ColAgg::Str(len), str_rep(len))
            } else {
                // One representative row/element fixes the type and (for a fixed
                // array) the width; jagged/vector report flen = 1 regardless.
                (ColAgg::Data, chunk_values(&b.values, 0, 1))
            };
            cols.push(StreamCol {
                rep: Branch {
                    name: b.name.clone(),
                    values,
                    kind: b.chunk_kind(),
                },
                baskets: Vec::new(),
                agg,
            });
        }
        self.columns = cols;
    }

    /// Finish the file: write the `TTree` object, streamer info, and key list,
    /// then patch the header pointers. Returns the sink. Errors if no batch was
    /// written or the file exceeds the 2 GiB small-format limit.
    pub fn finish(mut self) -> Result<W> {
        if self.schema.is_none() {
            return Err(Error::Format(
                "TTreeWriter finished with no batches written".into(),
            ));
        }
        let tot_bytes: i64 = self
            .columns
            .iter()
            .flat_map(|c| &c.baskets)
            .map(|r| i64::from(r.nbytes))
            .sum();
        let eff: Vec<&Branch> = self.columns.iter().map(|c| &c.rep).collect();
        let groups: Vec<Vec<BasketRec>> = self.columns.iter().map(|c| c.baskets.clone()).collect();

        // --- TTree object key + object. ---
        let tree_obj = build_tree_object(
            &self.tree_name,
            &eff,
            &groups,
            self.total_entries,
            tot_bytes,
        );
        let tree_payload = on_disk(&tree_obj, self.compression);
        let tree_seek = self.pos;
        let mut kb = WBuffer::new();
        write_key_header(
            &mut kb,
            "TTree",
            &self.tree_name,
            "",
            tree_obj.len() as u32,
            tree_payload.len() as u32,
            tree_seek,
            100,
        );
        let kb = kb.into_vec();
        self.put(&kb)?;
        self.put(&tree_payload)?;

        // --- Streamer-info record (referenced by fSeekInfo). ---
        let streamer_info = crate::streamer_gen::tree_streamer_info();
        let si_payload = on_disk(&streamer_info, self.compression);
        let seek_info = self.pos;
        let mut sib = WBuffer::new();
        write_key_header(
            &mut sib,
            "TList",
            "StreamerInfo",
            "Doubly linked list",
            streamer_info.len() as u32,
            si_payload.len() as u32,
            seek_info,
            100,
        );
        let sib = sib.into_vec();
        self.put(&sib)?;
        self.put(&si_payload)?;
        let nbytes_info =
            key_len("TList", "StreamerInfo", "Doubly linked list") as u32 + si_payload.len() as u32;

        // --- Directory key list (one entry: the TTree). ---
        let keylist_seek = self.pos;
        let tree_klen = key_len("TTree", &self.tree_name, "");
        let keylist_obj_len = 4 + tree_klen as u32;
        let mut klb = WBuffer::new();
        write_key_header(
            &mut klb,
            "TFile",
            &self.file_name,
            "",
            keylist_obj_len,
            keylist_obj_len,
            keylist_seek,
            100,
        );
        klb.be_i32(1); // nkeys
        write_key_header(
            &mut klb,
            "TTree",
            &self.tree_name,
            "",
            tree_obj.len() as u32,
            tree_payload.len() as u32,
            tree_seek,
            100,
        );
        let klb = klb.into_vec();
        self.put(&klb)?;
        let keylist_nbytes = key_len("TFile", &self.file_name, "") as u32 + keylist_obj_len;

        let f_end = self.pos;
        guard_small_format(f_end as usize)?;

        self.patch_u32(self.p_end, f_end as u32)?;
        self.patch_u32(self.p_nbytes_name, self.f_nbytes_name)?;
        self.patch_u32(self.p_seek_info, seek_info as u32)?;
        self.patch_u32(self.p_nbytes_info, nbytes_info)?;
        self.patch_u32(self.p_dir_nbytes_keys, keylist_nbytes)?;
        self.patch_u32(self.p_dir_seek_keys, keylist_seek as u32)?;
        self.sink.flush()?;
        Ok(self.sink)
    }
}

/// A representative `TLeafC` string value whose length yields `fLen` = `len`
/// (longest string + 1), used for a streamed string column's leaf metadata.
fn str_rep(len: i32) -> BranchValues {
    let n = (len - 1).max(0) as usize;
    BranchValues::Str(vec!["\0".repeat(n)])
}

/// The reserved 4-byte regions of a freshly written file prefix (header + root
/// `TDirectory` record), to be back-patched once the file's size and its
/// streamer-info / key-list locations are known.
struct PrefixPatches {
    p_end: Patch,
    p_nbytes_name: Patch,
    p_seek_info: Patch,
    p_nbytes_info: Patch,
    p_dir_nbytes_keys: Patch,
    p_dir_seek_keys: Patch,
    /// `fNbytesName` — the byte count of the first key + the dir name/title.
    f_nbytes_name: usize,
}

/// Write the 100-byte file header and the root `TDirectory` record into `w`
/// (expected empty), returning the regions to patch at the end. Shared by the
/// one-shot [`tree_bytes`] and the streaming [`TTreeWriter`] so both emit a
/// byte-identical prefix.
fn write_file_prefix(w: &mut WBuffer, file_name: &str, compression: u32) -> PrefixPatches {
    // --- File header (100 bytes; pointers patched at the end). ---
    w.bytes(b"root");
    w.be_u32(FILE_VERSION);
    w.be_u32(100); // fBEGIN
    let p_end = w.reserve(4);
    w.be_u32(0); // fSeekFree
    w.be_u32(0); // fNbytesFree
    w.be_u32(0); // nfree
    let p_nbytes_name = w.reserve(4);
    w.u8(4); // fUnits
    w.be_u32(compression); // fCompress
    let p_seek_info = w.reserve(4);
    let p_nbytes_info = w.reserve(4);
    w.be_u16(1);
    w.bytes(&[0u8; 16]);
    while w.len() < 100 {
        w.u8(0);
    }

    // --- Root directory name key + TDirectory record. ---
    let first_klen = key_len("TFile", file_name, "");
    let name_title_len = (1 + file_name.len()) + 1;
    let f_nbytes_name = first_klen as usize + name_title_len;
    let first_obj_len = (name_title_len + 30 + 18) as u32;
    write_key_header(
        w,
        "TFile",
        file_name,
        "",
        first_obj_len,
        first_obj_len,
        100,
        0,
    );
    w.string(file_name);
    w.string("");
    w.be_i16(5);
    w.be_u32(DATIME);
    w.be_u32(DATIME);
    let p_dir_nbytes_keys = w.reserve(4);
    w.be_i32(f_nbytes_name as i32);
    w.be_u32(100); // fSeekDir
    w.be_u32(0); // fSeekParent
    let p_dir_seek_keys = w.reserve(4);
    w.be_u16(1);
    w.bytes(&[0u8; 16]);

    PrefixPatches {
        p_end,
        p_nbytes_name,
        p_seek_info,
        p_nbytes_info,
        p_dir_nbytes_keys,
        p_dir_seek_keys,
        f_nbytes_name,
    }
}

/// Shared body of [`tree_file_bytes`] / [`write_tree_file_baskets`]:
/// `entries_per_basket` of `0` means one basket per branch.
fn tree_bytes(
    file_name: &str,
    tree_name: &str,
    branches: &[Branch],
    compression: Compression,
    entries_per_basket: usize,
) -> Result<Vec<u8>> {
    for b in branches {
        if !b.jagged() && !b.stl_vector() && b.is_jagged() {
            return Err(Error::Format(format!(
                "branch {:?}: rows differ in length; use Branch::jagged_* or Branch::vector_* for \
                 variable-length arrays (Branch::vec_* requires every row to have the same length)",
                b.name
            )));
        }
    }
    let compression = compression.setting();

    // Expand each jagged branch into [count branch, jagged branch], matching
    // ROOT/uproot. `counts` owns the synthetic count branches so the effective
    // list `eff` can borrow them alongside the caller's branches.
    let counts: Vec<Branch> = branches.iter().filter_map(Branch::count_branch).collect();
    let mut eff: Vec<&Branch> = Vec::with_capacity(branches.len() + counts.len());
    let mut ci = 0;
    for b in branches {
        if b.jagged() {
            eff.push(&counts[ci]);
            ci += 1;
        }
        eff.push(b);
    }
    let n_entries = eff.first().map(|b| b.n_entries()).unwrap_or(0);

    let mut w = WBuffer::new();
    let PrefixPatches {
        p_end,
        p_nbytes_name,
        p_seek_info,
        p_nbytes_info,
        p_dir_nbytes_keys,
        p_dir_seek_keys,
        f_nbytes_name,
    } = write_file_prefix(&mut w, file_name, compression);

    // --- Baskets per branch (TBasket TKeys). A leaf branch has one; a split
    // branch has a count basket plus one per member sub-branch. ---
    let basket_groups: Vec<Vec<BasketRec>> = eff
        .iter()
        .map(|&b| write_branch_baskets(&mut w, b, tree_name, compression, entries_per_basket))
        .collect();
    let tot_bytes: i64 = basket_groups
        .iter()
        .flatten()
        .map(|r| r.nbytes as i64)
        .sum();

    // --- TTree object key + object. ---
    let tree_obj = build_tree_object(tree_name, &eff, &basket_groups, n_entries as i64, tot_bytes);
    let tree_payload = on_disk(&tree_obj, compression);
    let tree_seek = w.len();
    write_key_header(
        &mut w,
        "TTree",
        tree_name,
        "",
        tree_obj.len() as u32,
        tree_payload.len() as u32,
        tree_seek as u64,
        100,
    );
    w.bytes(&tree_payload);

    // --- Streamer-info record (referenced by fSeekInfo only). The canonical
    // TTree-hierarchy TStreamerInfo describes every class a tree uses (including
    // the TBranchElement/TLeafElement std::vector streamers); a split branch
    // additionally needs its struct's generated TStreamerInfo appended. ---
    let mut streamer_info = crate::streamer_gen::tree_streamer_info();
    for b in branches {
        if let Some(spec) = b.split() {
            let info = write_class_streamer_info(&spec.class_name, &spec.members);
            streamer_info = append_streamer_info(&streamer_info, &info);
        }
    }
    let si_payload = on_disk(&streamer_info, compression);
    let seek_info = w.len() as u32;
    write_key_header(
        &mut w,
        "TList",
        "StreamerInfo",
        "Doubly linked list",
        streamer_info.len() as u32,
        si_payload.len() as u32,
        seek_info as u64,
        100,
    );
    w.bytes(&si_payload);
    let nbytes_info =
        key_len("TList", "StreamerInfo", "Doubly linked list") as u32 + si_payload.len() as u32;

    // --- Directory key list (one entry: the TTree). ---
    let keylist_seek = w.len();
    let tree_klen = key_len("TTree", tree_name, "");
    let keylist_obj_len = 4 + tree_klen as u32;
    write_key_header(
        &mut w,
        "TFile",
        file_name,
        "",
        keylist_obj_len,
        keylist_obj_len,
        keylist_seek as u64,
        100,
    );
    w.be_i32(1); // nkeys
    write_key_header(
        &mut w,
        "TTree",
        tree_name,
        "",
        tree_obj.len() as u32,
        tree_payload.len() as u32,
        tree_seek as u64,
        100,
    );
    let keylist_nbytes = key_len("TFile", file_name, "") as u32 + keylist_obj_len;
    // The TFile header / directory record use 32-bit seek pointers; reject a tree
    // that would overflow them (the per-basket i32 fields are bounded by this too,
    // since KSTART_BIG_FILE < i32::MAX). Without this the file corrupts silently.
    let f_end = w.len();
    guard_small_format(f_end)?;

    w.patch_be_u32(p_end, f_end as u32);
    w.patch_be_u32(p_nbytes_name, f_nbytes_name as u32);
    w.patch_be_u32(p_seek_info, seek_info);
    w.patch_be_u32(p_nbytes_info, nbytes_info);
    w.patch_be_u32(p_dir_nbytes_keys, keylist_nbytes);
    w.patch_be_u32(p_dir_seek_keys, keylist_seek as u32);

    Ok(w.into_vec())
}

/// On-disk bytes for an object payload: compressed when it helps, else raw.
fn on_disk(object: &[u8], compression: u32) -> Vec<u8> {
    if compression == 0 {
        return object.to_vec();
    }
    match oxiroot_compress::compress(object, compression) {
        Ok(c) if c.len() < object.len() => c,
        _ => object.to_vec(),
    }
}

/// Write a `TBasket` (a big-format `TKey` whose `fKeyLen` includes the 19-byte
/// extension) into `w` at its current end, returning its location.
fn write_basket(w: &mut WBuffer, branch: &Branch, tree_name: &str, compression: u32) -> BasketRec {
    let (bytes, rec) = basket_bytes(branch, tree_name, compression, w.len() as u64);
    w.bytes(&bytes);
    rec
}

/// The on-disk bytes of one `TBasket`, written as if it begins at absolute file
/// offset `seek` (baked into the key's `fSeekKey`), plus its [`BasketRec`]. This
/// is the streaming primitive that [`TTreeWriter`] emits straight to a sink;
/// [`write_basket`] is the in-buffer wrapper that derives `seek` from `w.len()`.
fn basket_bytes(
    branch: &Branch,
    tree_name: &str,
    compression: u32,
    seek: u64,
) -> (Vec<u8>, BasketRec) {
    let (data, offsets) = branch.basket_content();
    let n_entries = branch.n_entries();
    let leaf = branch.leaf();
    // `fNevBufSize` is the per-entry buffer size: `flen * elem_size` for a
    // fixed/scalar branch; ROOT writes a default (1000) for variable baskets.
    let nev_buf_size = match branch.kind() {
        Kind::Str | Kind::Jagged | Kind::StlVector => 1000,
        _ => branch.flen() * leaf.size,
    };

    let klen = key_len_fmt("TBasket", &branch.name, tree_name, true) as u32 + 19;
    let border = data.len() as u32;

    // The uncompressed buffer is the entry data, then (for a variable branch)
    // the `fEntryOffset` array: `int32 count(=n_entries+1)` followed by
    // basket-relative offsets (the data-relative offsets plus `fKeyLen`).
    let mut buffer = data;
    if let Some(offs) = &offsets {
        buffer.extend_from_slice(&(offs.len() as i32).to_be_bytes());
        for &o in offs {
            buffer.extend_from_slice(&((o + klen) as i32).to_be_bytes());
        }
    }
    let obj_len = buffer.len() as u32;
    let payload = on_disk(&buffer, compression);
    let nbytes = klen + payload.len() as u32;
    let f_last = klen + border; // entry data ends at the border

    let mut w = WBuffer::with_capacity(nbytes as usize);
    // Big-format TKey header.
    w.be_i32(nbytes as i32);
    w.be_u16(1004); // big-format key version
    w.be_u32(obj_len);
    w.be_u32(DATIME);
    w.be_u16(klen as u16);
    w.be_u16(0); // cycle
    w.be_u64(seek);
    w.be_u64(100); // fSeekPdir
    w.string("TBasket");
    w.string(&branch.name);
    w.string(tree_name);
    // TBasket extension (the tail of fKeyLen).
    w.be_u16(3); // basket version
    w.be_i32(32000); // fBufferSize
    w.be_i32(nev_buf_size); // fNevBufSize
    w.be_i32(n_entries as i32); // fNevBuf
    w.be_i32(f_last as i32); // fLast
    w.u8(0); // flag
    w.bytes(&payload);

    (
        w.into_vec(),
        BasketRec {
            seek,
            nbytes,
            n_entries,
        },
    )
}

/// A sub-range `[start, start+len)` of a non-split branch's entries, as a fresh
/// `Branch` — used to split a branch into multiple baskets.
fn chunk_branch(branch: &Branch, start: usize, len: usize) -> Branch {
    Branch {
        name: branch.name.clone(),
        values: chunk_values(&branch.values, start, len),
        kind: branch.chunk_kind(),
    }
}

/// Slice a [`BranchValues`] to `[start, start+len)` (clamped), preserving variant.
fn chunk_values(bv: &BranchValues, start: usize, len: usize) -> BranchValues {
    use BranchValues::*;
    macro_rules! sl {
        ($variant:ident, $v:expr) => {{
            let end = (start + len).min($v.len());
            let s = start.min(end);
            $variant($v[s..end].to_vec())
        }};
    }
    match bv {
        Bool(v) => sl!(Bool, v),
        I8(v) => sl!(I8, v),
        U8(v) => sl!(U8, v),
        I16(v) => sl!(I16, v),
        U16(v) => sl!(U16, v),
        I32(v) => sl!(I32, v),
        U32(v) => sl!(U32, v),
        I64(v) => sl!(I64, v),
        U64(v) => sl!(U64, v),
        F32(v) => sl!(F32, v),
        F64(v) => sl!(F64, v),
        VecBool(v) => sl!(VecBool, v),
        VecI8(v) => sl!(VecI8, v),
        VecU8(v) => sl!(VecU8, v),
        VecI16(v) => sl!(VecI16, v),
        VecU16(v) => sl!(VecU16, v),
        VecI32(v) => sl!(VecI32, v),
        VecU32(v) => sl!(VecU32, v),
        VecI64(v) => sl!(VecI64, v),
        VecU64(v) => sl!(VecU64, v),
        VecF32(v) => sl!(VecF32, v),
        VecF64(v) => sl!(VecF64, v),
        Str(v) => sl!(Str, v),
        VecStr(v) => sl!(VecStr, v),
        Nested { .. } => unreachable!("{NESTED_NOT_WRITABLE}"),
    }
}

/// Write the basket(s) backing one branch. A leaf branch has exactly one. A
/// split `std::vector<MyStruct>` branch has a *count* basket (the parent's
/// per-entry element counts, as a variable `i32`) followed by one jagged basket
/// per member sub-branch; `basket_groups[i][0]` is the count basket and `[1..]`
/// the members, matching the order [`write_split_parent`] reads them back.
fn write_branch_baskets(
    w: &mut WBuffer,
    branch: &Branch,
    tree_name: &str,
    compression: u32,
    entries_per_basket: usize,
) -> Vec<BasketRec> {
    let Some(spec) = branch.split() else {
        // A single-leaf branch is split into baskets of `entries_per_basket`
        // entries (0 = one basket). An empty branch still gets one empty basket.
        let n = branch.n_entries() as usize;
        let epb = if entries_per_basket == 0 {
            n.max(1)
        } else {
            entries_per_basket
        };
        let mut recs = Vec::new();
        let mut start = 0;
        while start < n {
            let len = epb.min(n - start);
            recs.push(write_basket(
                w,
                &chunk_branch(branch, start, len),
                tree_name,
                compression,
            ));
            start += len;
        }
        if recs.is_empty() {
            recs.push(write_basket(w, branch, tree_name, compression));
        }
        return recs;
    };
    // Count basket: per-entry element counts as single-element jagged `i32`
    // rows, so it carries the same `fEntryOffset` ROOT writes for the parent.
    let counts = vec_row_lengths(&spec.members[0].values);
    let count_branch = Branch {
        name: branch.name.clone(),
        values: BranchValues::VecI32(counts.into_iter().map(|n| vec![n]).collect()),
        kind: BranchKind::Jagged,
    };
    let mut recs = vec![write_basket(w, &count_branch, tree_name, compression)];
    for m in &spec.members {
        let sub = Branch {
            name: format!("{}.{}", branch.name, m.name),
            values: m.values.clone(),
            kind: BranchKind::Jagged,
        };
        recs.push(write_basket(w, &sub, tree_name, compression));
    }
    recs
}

/// Write a byte-counted att base (`TAttLine`/`Fill`/`Marker`).
fn write_attline(w: &mut WBuffer) {
    let t = w.begin_object(2);
    w.be_i16(602);
    w.be_i16(1);
    w.be_i16(1);
    w.end_object(t);
}
fn write_attfill(w: &mut WBuffer) {
    let t = w.begin_object(2);
    w.be_i16(0);
    w.be_i16(1001);
    w.end_object(t);
}
fn write_attmarker(w: &mut WBuffer) {
    let t = w.begin_object(2);
    w.be_i16(1);
    w.be_i16(1);
    w.be_f32(1.0);
    w.end_object(t);
}

/// Write `ROOT::TIOFeatures` (a byte-counted object with a single `fIOBits`).
fn write_iofeatures(w: &mut WBuffer) {
    let t = w.begin_object(1);
    w.u8(0); // fIOBits
    w.end_object(t);
}

/// Begin a `ReadObjectAny` object: a byte-count placeholder then a fresh class
/// tag (`kNewClassTag` + name). Every object is written with a fresh tag (no
/// back-references), which ROOT/uproot read correctly. Returns the byte-count
/// patch.
fn begin_object_any(w: &mut WBuffer, class: &str) -> Patch {
    let bc = w.reserve(4); // byte-count placeholder
    w.be_u32(0xFFFF_FFFF); // kNewClassTag
    w.bytes(class.as_bytes());
    w.u8(0); // NUL terminator
    bc
}

/// Finish a `ReadObjectAny` object, back-patching its byte count (which covers
/// everything after the 4-byte count word).
fn end_object_any(w: &mut WBuffer, bc: Patch) {
    let start = w.patch_offset(bc) + 4;
    let nbytes = (w.len() - start) as u32 | K_BYTE_COUNT_MASK;
    w.patch_be_u32(bc, nbytes);
}

/// Build the `TObjArray` of branches, then the tree-level `TObjArray` of leaves.
fn build_tree_object(
    tree_name: &str,
    branches: &[&Branch],
    baskets: &[Vec<BasketRec>],
    n_entries: i64,
    tot_bytes: i64,
) -> Vec<u8> {
    // ROOT resolves object references relative to `-keylen` of the TTree key; we
    // must use the same keylen so a jagged leaf's `fLeafCount` reference lands on
    // the count leaf. (This is the keylen `write_key_header` writes for the key.)
    let keylen = key_len("TTree", tree_name, "") as u32;
    let mut refs: LeafRefs = HashMap::new();

    let mut w = WBuffer::new();
    let tree = w.begin_object(20); // TTree v20
    write_tnamed(&mut w, OBJ_BITS, tree_name, "");
    write_attline(&mut w);
    write_attfill(&mut w);
    write_attmarker(&mut w);

    w.be_i64(n_entries); // fEntries
    w.be_i64(tot_bytes); // fTotBytes
    w.be_i64(tot_bytes); // fZipBytes
    w.be_i64(0); // fSavedBytes
    w.be_i64(0); // fFlushedBytes
    w.be_f64(1.0); // fWeight
    w.be_i32(0); // fTimerInterval
    w.be_i32(25); // fScanField
    w.be_i32(0); // fUpdate
    w.be_i32(1000); // fDefaultEntryOffsetLen
    w.be_i32(0); // fNClusterRange
    w.be_i64(1_000_000_000_000); // fMaxEntries
    w.be_i64(1_000_000_000_000); // fMaxEntryLoop
    w.be_i64(0); // fMaxVirtualSize
    w.be_i64(-300_000_000); // fAutoSave
    w.be_i64(-30_000_000); // fAutoFlush
    w.be_i64(1_000_000); // fEstimate
    w.u8(0); // fClusterRangeEnd (empty array marker)
    w.u8(0); // fClusterSize (empty array marker)
    write_iofeatures(&mut w);

    write_branch_array(&mut w, branches, baskets, n_entries, keylen, &mut refs);
    write_tree_leaf_array(&mut w, branches, &refs);

    w.be_u32(0); // fAliases (null TList*)
    w.be_i32(0); // fIndexValues (TArrayD, empty)
    w.be_i32(0); // fIndex (TArrayI, empty)
    w.be_u32(0); // fTreeIndex (null)
    w.be_u32(0); // fFriends (null)
    w.be_u32(0); // fUserInfo (null)
    w.be_u32(0); // fBranchRef (null)

    w.end_object(tree);
    w.into_vec()
}

/// The `TObjArray` header (`{version} TObject name fSize fLowerBound`).
fn obj_array_header(w: &mut WBuffer, size: usize) -> CountToken {
    let tok = w.begin_object(3); // TObjArray v3
    write_tobject(w, 0);
    w.string("");
    w.be_i32(size as i32);
    w.be_i32(0); // fLowerBound
    tok
}

/// Write `fBranches`: a `TObjArray<TBranch>`.
fn write_branch_array(
    w: &mut WBuffer,
    branches: &[&Branch],
    baskets: &[Vec<BasketRec>],
    n_entries: i64,
    keylen: u32,
    refs: &mut LeafRefs,
) {
    let tok = obj_array_header(w, branches.len());
    for (&b, group) in branches.iter().zip(baskets) {
        if b.split().is_some() {
            // The parent's object-map position: its sub-branches reference it
            // (`fBranchCount`) so ROOT can find the collection they belong to.
            let parent_ref = w.len() as u32 + keylen + K_MAP_OFFSET;
            let bc = begin_object_any(w, "TBranchElement");
            write_split_parent(w, b, group, n_entries, keylen, parent_ref, refs);
            end_object_any(w, bc);
        } else if b.stl_vector() {
            let bc = begin_object_any(w, "TBranchElement");
            write_branch_element(w, b, group, n_entries, keylen, refs);
            end_object_any(w, bc);
        } else {
            let bc = begin_object_any(w, "TBranch");
            write_branch(w, b, group, n_entries, keylen, refs);
            end_object_any(w, bc);
        }
    }
    w.end_object(tok);
}

/// Write one `TBranchElement` (v10): the `TBranch` base, then the element
/// members (`fClassName`, `fCheckSum`, …) describing the `std::vector<T>`.
fn write_branch_element(
    w: &mut WBuffer,
    branch: &Branch,
    group: &[BasketRec],
    n_entries: i64,
    keylen: u32,
    refs: &mut LeafRefs,
) {
    let tok = w.begin_object(10); // TBranchElement v10
    write_branch(w, branch, group, n_entries, keylen, refs); // the TBranch base
    let (class_name, checksum) = branch.stl_info();
    w.string(class_name); // fClassName, e.g. "vector<float>"
    w.string(""); // fParentName
    w.string(""); // fClonesName
    w.be_u32(checksum); // fCheckSum
    w.be_u16(6); // fClassVersion (std::vector)
    w.be_i32(-1); // fID
    w.be_i32(0); // fType
    w.be_i32(-1); // fStreamerType
    w.be_i32(0); // fMaximum
    w.be_u32(0); // fBranchCount (null)
    w.be_u32(0); // fBranchCount2 (null)
    w.end_object(tok);
}

/// Write `fBasketBytes`/`fBasketEntry`/`fBasketSeek` for a branch's `group` of
/// baskets (each `int[fMaxBaskets]` / `i64[fMaxBaskets]`, preceded by a marker
/// byte). `fBasketEntry` is cumulative — the entry start of each basket, plus a
/// trailing total — with the unused tail zeroed.
fn write_basket_arrays(w: &mut WBuffer, group: &[BasketRec], max_baskets: i32) {
    let cap = max_baskets as usize;
    // cumulative[i] = entries before basket i; cumulative[group.len()] = total.
    let mut cumulative = Vec::with_capacity(group.len() + 1);
    let mut acc = 0i64;
    cumulative.push(0);
    for b in group {
        acc += i64::from(b.n_entries);
        cumulative.push(acc);
    }

    w.u8(1);
    for i in 0..cap {
        w.be_i32(group.get(i).map_or(0, |b| b.nbytes as i32));
    }
    w.u8(1);
    for i in 0..cap {
        w.be_i64(cumulative.get(i).copied().unwrap_or(0));
    }
    w.u8(1);
    for i in 0..cap {
        w.be_i64(group.get(i).map_or(0, |b| b.seek as i64));
    }
}

/// Write a `TLeafElement` (v1): the `TLeaf` base (`fLen`/`fLenType`/…/`fLeafCount`)
/// then the element extras `fID`/`fType`. `f_leaf_count` is written verbatim — a
/// null (`0`) or an object back-reference to the counter leaf.
fn write_leaf_element(
    w: &mut WBuffer,
    name: &str,
    title: &str,
    len_type: i32,
    f_id: i32,
    f_type: i32,
    f_leaf_count: u32,
) {
    let outer = w.begin_object(1); // TLeafElement v1
    let base = w.begin_object(2); // TLeaf v2
    write_tnamed(w, OBJ_BITS, name, title);
    w.be_i32(1); // fLen
    w.be_i32(len_type); // fLenType
    w.be_i32(0); // fOffset
    w.u8(0); // fIsRange
    w.u8(0); // fIsUnsigned
    w.be_u32(f_leaf_count); // fLeafCount
    w.end_object(base);
    w.be_i32(f_id); // fID
    w.be_i32(f_type); // fType
    w.end_object(outer);
}

/// Write the parent `TBranchElement` (`fType=4`) of a split
/// `std::vector<MyStruct>`: the `TBranch` base (count basket + the `name_`
/// counter leaf), the member sub-branches in `fBranches`, then the element
/// members (`fClassName="vector<MyStruct>"`, `fType=4`, `fMaximum=max count`).
///
/// The counter leaf (`name_`) is written *inline* the first time it is needed —
/// inside the first sub-branch's leaf `fLeafCount` — and back-referenced here and
/// by the other sub-branches, so all four references resolve to one object (ROOT
/// relies on this when wiring `leaf->GetBranch()`/`GetLeafCount()`).
fn write_split_parent(
    w: &mut WBuffer,
    branch: &Branch,
    group: &[BasketRec],
    n_entries: i64,
    keylen: u32,
    parent_ref: u32,
    refs: &mut LeafRefs,
) {
    let spec = branch.split().expect("split spec");
    let counter = format!("{}_", branch.name);
    let count_basket = &group[0];
    let checksum = class_checksum(&spec.class_name, &spec.members);
    let max_count = vec_row_lengths(&spec.members[0].values)
        .into_iter()
        .max()
        .unwrap_or(0);
    let max_baskets = 10i32;

    let te = w.begin_object(10); // TBranchElement v10
    let tb = w.begin_object(13); // TBranch v13
    write_tnamed(w, OBJ_BITS, &branch.name, &counter);
    write_attfill(w);
    w.be_i32(0); // fCompress
    w.be_i32(32000); // fBasketSize
    w.be_i32(1000); // fEntryOffsetLen
    w.be_i32(1); // fWriteBasket
    w.be_i64(n_entries); // fEntryNumber
    write_iofeatures(w);
    w.be_i32(0); // fOffset
    w.be_i32(max_baskets); // fMaxBaskets
    w.be_i32(99); // fSplitLevel
    w.be_i64(n_entries); // fEntries
    w.be_i64(0); // fFirstEntry
    w.be_i64(count_basket.nbytes as i64); // fTotBytes
    w.be_i64(count_basket.nbytes as i64); // fZipBytes

    // fBranches: the member sub-branches. The first writes `counter` inline.
    let sub_tok = obj_array_header(w, spec.members.len());
    for (i, m) in spec.members.iter().enumerate() {
        let bc = begin_object_any(w, "TBranchElement");
        write_split_sub(
            w,
            &branch.name,
            &counter,
            &spec.class_name,
            checksum,
            m,
            i as i32,
            &group[i + 1],
            n_entries,
            keylen,
            parent_ref,
            refs,
            i == 0,
        );
        end_object_any(w, bc);
    }
    w.end_object(sub_tok);

    // fLeaves: one entry, an object back-reference to the inline `counter` leaf.
    let leaf_tok = obj_array_header(w, 1);
    w.be_u32(refs.get(&counter).copied().unwrap_or(0));
    w.end_object(leaf_tok);

    let baskets = obj_array_header(w, 0); // fBaskets (empty)
    w.end_object(baskets);
    write_basket_arrays(w, std::slice::from_ref(count_basket), max_baskets);
    w.string(""); // fFileName
    w.end_object(tb);

    // TBranchElement members for the collection itself.
    w.string(&format!("vector<{}>", spec.class_name)); // fClassName
    w.string(""); // fParentName
    w.string(&spec.class_name); // fClonesName
    w.be_u32(0); // fCheckSum (ROOT does not validate the STL parent's checksum)
    w.be_u16(6); // fClassVersion (std::vector)
    w.be_i32(-1); // fID
    w.be_i32(4); // fType (split STL collection)
    w.be_i32(-1); // fStreamerType
    w.be_i32(max_count); // fMaximum (largest per-entry element count)
    w.be_u32(0); // fBranchCount (null)
    w.be_u32(0); // fBranchCount2 (null)
    w.end_object(te);
}

/// Write one member sub-branch (`fType=41`) of a split collection: a jagged
/// array of the member type, counted by the parent's `counter` leaf. When
/// `write_counter_inline` is set (the first member), the `counter` leaf is
/// emitted in full as this leaf's `fLeafCount` and its position recorded in
/// `refs`; otherwise `fLeafCount` is a back-reference to that recorded object.
#[allow(clippy::too_many_arguments)]
fn write_split_sub(
    w: &mut WBuffer,
    parent: &str,
    counter: &str,
    class_name: &str,
    checksum: u32,
    member: &SplitMember,
    index: i32,
    basket: &BasketRec,
    n_entries: i64,
    keylen: u32,
    parent_ref: u32,
    refs: &mut LeafRefs,
    write_counter_inline: bool,
) {
    let (type_code, _typename, size) = member_type_info(&member.values);
    let name = format!("{parent}.{}", member.name);
    let title = format!("{}[{counter}]", member.name);
    let max_baskets = 10i32;

    let te = w.begin_object(10); // TBranchElement v10
    let tb = w.begin_object(13); // TBranch v13
    write_tnamed(w, OBJ_BITS, &name, &title);
    write_attfill(w);
    w.be_i32(0); // fCompress
    w.be_i32(32000); // fBasketSize
    w.be_i32(1000); // fEntryOffsetLen
    w.be_i32(1); // fWriteBasket
    w.be_i64(n_entries); // fEntryNumber
    write_iofeatures(w);
    w.be_i32(0); // fOffset
    w.be_i32(max_baskets); // fMaxBaskets
    w.be_i32(0); // fSplitLevel
    w.be_i64(n_entries); // fEntries
    w.be_i64(0); // fFirstEntry
    w.be_i64(basket.nbytes as i64); // fTotBytes
    w.be_i64(basket.nbytes as i64); // fZipBytes

    let sub = obj_array_header(w, 0); // fBranches (empty)
    w.end_object(sub);

    // fLeaves: this member's TLeafElement. Its fLeafCount references `counter`.
    let leaf_tok = obj_array_header(w, 1);
    let leaf_pos = w.len() as u32;
    let lbc = begin_object_any(w, "TLeafElement");
    let outer = w.begin_object(1); // TLeafElement v1
    let base = w.begin_object(2); // TLeaf v2
    write_tnamed(w, OBJ_BITS, &name, &title);
    w.be_i32(1); // fLen
    w.be_i32(size); // fLenType (element width in bytes)
    w.be_i32(0); // fOffset
    w.u8(0); // fIsRange
    w.u8(0); // fIsUnsigned
    if write_counter_inline {
        // First occurrence of the counter leaf: write it in full, record it.
        let cpos = w.len() as u32;
        let cbc = begin_object_any(w, "TLeafElement");
        write_leaf_element(w, counter, counter, 0, -1, -1, 0);
        end_object_any(w, cbc);
        refs.entry(counter.to_string())
            .or_insert(cpos + keylen + K_MAP_OFFSET);
    } else {
        w.be_u32(refs.get(counter).copied().unwrap_or(0)); // fLeafCount back-ref
    }
    w.end_object(base);
    w.be_i32(index); // fID
    w.be_i32(type_code); // fType (basic-type code)
    w.end_object(outer);
    end_object_any(w, lbc);
    refs.entry(name.clone())
        .or_insert(leaf_pos + keylen + K_MAP_OFFSET);
    w.end_object(leaf_tok);

    let baskets = obj_array_header(w, 0); // fBaskets (empty)
    w.end_object(baskets);
    write_basket_arrays(w, std::slice::from_ref(basket), max_baskets);
    w.string(""); // fFileName
    w.end_object(tb);

    // TBranchElement members for the member element.
    w.string(class_name); // fClassName (the struct, e.g. "Hit")
    w.string(class_name); // fParentName
    w.string(""); // fClonesName
    w.be_u32(checksum); // fCheckSum (the struct's class checksum)
    w.be_u16(1); // fClassVersion
    w.be_i32(index); // fID (member index within the struct)
    w.be_i32(41); // fType (split STL member)
    w.be_i32(type_code); // fStreamerType
    w.be_i32(0); // fMaximum
    w.be_u32(parent_ref); // fBranchCount (object ref to the parent collection)
    w.be_u32(0); // fBranchCount2 (null)
    w.end_object(te);
}

/// Write one `TBranch` (v13).
fn write_branch(
    w: &mut WBuffer,
    branch: &Branch,
    group: &[BasketRec],
    n_entries: i64,
    keylen: u32,
    refs: &mut LeafRefs,
) {
    let tot_bytes: i64 = group.iter().map(|b| i64::from(b.nbytes)).sum();
    let leaf = branch.leaf();
    // Branch title encodes the layout: `name/CODE`, `name[N]/CODE` (fixed),
    // `name[count]/CODE` (jagged), or `name/C` (string).
    let title = match branch.kind() {
        Kind::Scalar => format!("{}/{}", branch.name, leaf.code),
        Kind::FixedArray(n) => format!("{}[{}]/{}", branch.name, n, leaf.code),
        Kind::Jagged => format!("{}[{}]/{}", branch.name, branch.count_name(), leaf.code),
        // A std::vector branch's title is just its name (the type lives in
        // the TBranchElement's fClassName).
        Kind::StlVector => branch.name.clone(),
        Kind::Str => format!("{}/C", branch.name),
    };
    // Variable (string/jagged/vector) branches carry an `fEntryOffset` array,
    // flagged by a non-zero `fEntryOffsetLen`; fixed/scalar branches set it to 0.
    let entry_offset_len = match branch.kind() {
        Kind::Str | Kind::Jagged | Kind::StlVector => 1000,
        _ => 0,
    };
    // ROOT writes fSplitLevel = 99 for a (top-level, unsplit) std::vector
    // TBranchElement; that is the value its reader/cache expects.
    let split_level = if matches!(branch.kind(), Kind::StlVector) {
        99
    } else {
        0
    };
    // `fMaxBaskets` is the allocated length of the basket arrays; it must be at
    // least `fWriteBasket` (= group.len()). ROOT's default is 10, which we keep
    // for the common case so small files stay byte-identical, but a streamed
    // tree can hold more than 10 baskets per branch, so grow it to fit.
    let max_baskets = (group.len() as i32).max(10);

    let tok = w.begin_object(13); // TBranch v13
    write_tnamed(w, OBJ_BITS, &branch.name, &title);
    write_attfill(w);
    w.be_i32(0); // fCompress
    w.be_i32(32000); // fBasketSize
    w.be_i32(entry_offset_len); // fEntryOffsetLen
    w.be_i32(group.len() as i32); // fWriteBasket
    w.be_i64(n_entries); // fEntryNumber
    write_iofeatures(w);
    w.be_i32(0); // fOffset
    w.be_i32(max_baskets); // fMaxBaskets
    w.be_i32(split_level); // fSplitLevel
    w.be_i64(n_entries); // fEntries
    w.be_i64(0); // fFirstEntry
    w.be_i64(tot_bytes); // fTotBytes
    w.be_i64(tot_bytes); // fZipBytes

    // fBranches (empty), fLeaves (one leaf), fBaskets (empty TObjArrays).
    let e = obj_array_header(w, 0);
    w.end_object(e);
    write_leaf_array(w, &[branch], keylen, refs);
    let e = obj_array_header(w, 0);
    w.end_object(e);

    write_basket_arrays(w, group, max_baskets);
    w.string(""); // fFileName
    w.end_object(tok);
}

/// Write a `TObjArray<TLeaf>` for `branches` (one leaf each), recording each
/// leaf's object-reference position (first occurrence) so a later jagged leaf's
/// `fLeafCount` can point back to its count leaf.
fn write_leaf_array(w: &mut WBuffer, branches: &[&Branch], keylen: u32, refs: &mut LeafRefs) {
    let tok = obj_array_header(w, branches.len());
    for &b in branches {
        let bc_pos = w.len() as u32; // the byte-count word position (object-relative)
        let bc = begin_object_any(w, b.leaf().class);
        write_leaf(w, b, refs);
        end_object_any(w, bc);
        refs.entry(b.name.clone())
            .or_insert(bc_pos + keylen + K_MAP_OFFSET);
    }
    w.end_object(tok);
}

/// The tree-level `fLeaves` references each branch's already-written leaf via an
/// object back-reference, rather than re-emitting it. ROOT relies on these being
/// the *same* leaf objects (so `leaf->GetBranch()` is set when it reconstructs
/// the tree); duplicating them leaves the tree-level copies with a null branch
/// and crashes ROOT's `TTreeCache` on the first read.
fn write_tree_leaf_array(w: &mut WBuffer, branches: &[&Branch], refs: &LeafRefs) {
    let names: Vec<String> = branches
        .iter()
        .flat_map(|&b| branch_leaf_names(b))
        .collect();
    let tok = obj_array_header(w, names.len());
    for name in &names {
        let objref = refs.get(name).copied().unwrap_or(0);
        w.be_u32(objref); // object reference to the branch-level leaf
    }
    w.end_object(tok);
}

/// The leaf names a branch contributes to the tree-level `fLeaves`, in order. A
/// leaf branch contributes one (its own name); a split branch contributes the
/// parent counter leaf (`name_`) followed by each member leaf (`name.member`).
fn branch_leaf_names(b: &Branch) -> Vec<String> {
    match b.split() {
        Some(spec) => std::iter::once(format!("{}_", b.name))
            .chain(
                spec.members
                    .iter()
                    .map(|m| format!("{}.{}", b.name, m.name)),
            )
            .collect(),
        None => vec![b.name.clone()],
    }
}

/// Write one `TLeaf*` (v1): the `TLeaf` base then the subclass min/max. A
/// `std::vector` branch instead writes a `TLeafElement` (the `TLeaf` base then
/// `fID`/`fType`).
fn write_leaf(w: &mut WBuffer, branch: &Branch, refs: &LeafRefs) {
    let leaf = branch.leaf();
    if branch.stl_vector() {
        let outer = w.begin_object(1); // TLeafElement v1
        let base = w.begin_object(2); // TLeaf v2
        write_tnamed(w, OBJ_BITS, &branch.name, &branch.name);
        w.be_i32(1); // fLen
        w.be_i32(0); // fLenType
        w.be_i32(0); // fOffset
        w.u8(0); // fIsRange
        w.u8(0); // fIsUnsigned
        w.be_u32(0); // fLeafCount (null)
        w.end_object(base);
        w.be_i32(-1); // fID
        w.be_i32(-1); // fType
        w.end_object(outer);
        return;
    }
    // The leaf title carries `[N]` (fixed) or `[count]` (jagged), else the name.
    let title = match branch.kind() {
        Kind::FixedArray(n) => format!("{}[{}]", branch.name, n),
        Kind::Jagged => format!("{}[{}]", branch.name, branch.count_name()),
        _ => branch.name.clone(),
    };
    // A jagged leaf's `fLeafCount` is an object reference to its count leaf
    // (already written and recorded); everything else has a null `fLeafCount`.
    let f_leaf_count = match branch.kind() {
        Kind::Jagged => refs.get(&branch.count_name()).copied().unwrap_or(0),
        _ => 0,
    };
    // A TLeafC carries fLen = longest-string + 1 and fLenType = 1 (one char);
    // every other leaf uses its element count/width.
    let is_str = leaf.code == 'C';
    let f_len = if is_str {
        branch.str_len()
    } else {
        branch.flen()
    };
    let f_len_type = if is_str { 1 } else { leaf.len_type };
    let outer = w.begin_object(1); // TLeafX v1
    let base = w.begin_object(2); // TLeaf v2
    write_tnamed(w, OBJ_BITS, &branch.name, &title);
    w.be_i32(f_len); // fLen
    w.be_i32(f_len_type); // fLenType
    w.be_i32(0); // fOffset
    w.u8(0); // fIsRange
    w.u8(leaf.unsigned as u8); // fIsUnsigned
    w.be_u32(f_leaf_count); // fLeafCount (object ref to the count leaf, or null)
    w.end_object(base);
    // fMinimum (0), fMaximum (the leaf's max value, so ROOT can size a buffer
    // when this leaf is a leaf count). TLeafC stores them as 4-byte ints (string
    // lengths); every other leaf uses its element width.
    let minmax_size = if leaf.code == 'C' { 4 } else { leaf.size };
    write_leaf_minmax(w, minmax_size, branch.leaf_max());
    w.end_object(outer);
}

/// Write a leaf's `fMinimum` (0) and `fMaximum` (`max`) in the element width.
fn write_leaf_minmax(w: &mut WBuffer, size: i32, max: i64) {
    match size {
        1 => {
            w.u8(0);
            w.u8(max as u8);
        }
        2 => {
            w.be_i16(0);
            w.be_i16(max as i16);
        }
        8 => {
            w.be_i64(0);
            w.be_i64(max);
        }
        _ => {
            w.be_i32(0);
            w.be_i32(max as i32);
        }
    }
}
