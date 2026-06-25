//! Branch value types.

/// The primitive element type of a branch's leaf, derived from the leaf class
/// and its `fIsUnsigned` flag.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
            _ => return None,
        })
    }

    /// On-disk byte width of one element.
    pub(crate) fn size(self) -> usize {
        match self {
            LeafType::Bool | LeafType::I8 | LeafType::U8 => 1,
            LeafType::I16 | LeafType::U16 => 2,
            LeafType::I32 | LeafType::U32 | LeafType::F32 => 4,
            LeafType::I64 | LeafType::U64 | LeafType::F64 => 8,
        }
    }
}

/// A branch's values across all entries, one element per entry (for scalar
/// branches). The variant mirrors the leaf's element type.
#[derive(Debug, Clone, PartialEq)]
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
}
