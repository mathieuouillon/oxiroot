//! Branch value types.

/// The element type of a branch's leaf, derived from the leaf class and its
/// `fIsUnsigned` flag.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum LeafType {
    /// `TLeafO` — `bool`.
    Bool,
    /// `TLeafB` — `int8_t`.
    I8,
    /// `TLeafB` (unsigned) — `uint8_t`.
    U8,
    /// `TLeafS` — `int16_t`.
    I16,
    /// `TLeafS` (unsigned) — `uint16_t`.
    U16,
    /// `TLeafI` — `int32_t`.
    I32,
    /// `TLeafI` (unsigned) — `uint32_t`.
    U32,
    /// `TLeafL` — `int64_t`.
    I64,
    /// `TLeafL` (unsigned) — `uint64_t`.
    U64,
    /// `TLeafF` — `float`.
    F32,
    /// `TLeafD` — `double`.
    F64,
    /// `TLeafC` — `char*` / `std::string` (length-prefixed, variable length).
    Str,
}

impl LeafType {
    /// Resolve a leaf class name + signedness to an element type.
    pub(crate) fn from_leaf(class: &str, unsigned: bool) -> Option<LeafType> {
        Some(match (class, unsigned) {
            ("TLeafO", _) => LeafType::Bool,
            ("TLeafB", false) => LeafType::I8,
            ("TLeafB", true) => LeafType::U8,
            ("TLeafS", false) => LeafType::I16,
            ("TLeafS", true) => LeafType::U16,
            ("TLeafI", false) => LeafType::I32,
            ("TLeafI", true) => LeafType::U32,
            ("TLeafL", false) => LeafType::I64,
            ("TLeafL", true) => LeafType::U64,
            ("TLeafF", _) => LeafType::F32,
            ("TLeafD", _) => LeafType::F64,
            ("TLeafC", _) => LeafType::Str,
            _ => return None,
        })
    }

    /// On-disk byte width of one numeric element (1 for the string placeholder).
    pub(crate) fn size(self) -> usize {
        match self {
            LeafType::Bool | LeafType::I8 | LeafType::U8 | LeafType::Str => 1,
            LeafType::I16 | LeafType::U16 => 2,
            LeafType::I32 | LeafType::U32 | LeafType::F32 => 4,
            LeafType::I64 | LeafType::U64 | LeafType::F64 => 8,
        }
    }
}

/// A branch's values across all entries.
///
/// Scalar branches yield a flat vector; fixed-size array (`x[N]`) and
/// variable-length (`x[n]`) branches yield a nested vector (one inner vector per
/// entry); `TLeafC` branches yield strings.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum BranchValues {
    /// `bool` branch.
    Bool(Vec<bool>),
    /// `int8_t` branch.
    I8(Vec<i8>),
    /// `uint8_t` branch.
    U8(Vec<u8>),
    /// `int16_t` branch.
    I16(Vec<i16>),
    /// `uint16_t` branch.
    U16(Vec<u16>),
    /// `int32_t` branch.
    I32(Vec<i32>),
    /// `uint32_t` branch.
    U32(Vec<u32>),
    /// `int64_t` branch.
    I64(Vec<i64>),
    /// `uint64_t` branch.
    U64(Vec<u64>),
    /// `float` branch.
    F32(Vec<f32>),
    /// `double` branch.
    F64(Vec<f64>),
    /// Per-entry `bool` array.
    VecBool(Vec<Vec<bool>>),
    /// Per-entry `int8_t` array.
    VecI8(Vec<Vec<i8>>),
    /// Per-entry `uint8_t` array.
    VecU8(Vec<Vec<u8>>),
    /// Per-entry `int16_t` array.
    VecI16(Vec<Vec<i16>>),
    /// Per-entry `uint16_t` array.
    VecU16(Vec<Vec<u16>>),
    /// Per-entry `int32_t` array.
    VecI32(Vec<Vec<i32>>),
    /// Per-entry `uint32_t` array.
    VecU32(Vec<Vec<u32>>),
    /// Per-entry `int64_t` array.
    VecI64(Vec<Vec<i64>>),
    /// Per-entry `uint64_t` array.
    VecU64(Vec<Vec<u64>>),
    /// Per-entry `float` array.
    VecF32(Vec<Vec<f32>>),
    /// Per-entry `double` array.
    VecF64(Vec<Vec<f64>>),
    /// Per-entry string (`TLeafC`).
    Str(Vec<String>),
}
