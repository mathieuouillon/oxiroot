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

/// A jagged (or array) branch viewed as cumulative `offsets` over one flat
/// [`BranchValues`], instead of the per-entry-allocated `Vec<Vec<_>>` form.
///
/// `offsets` has `num_entries + 1` values with `offsets[0] == 0`; entry `i`'s
/// elements are `values[offsets[i] .. offsets[i+1]]`. Built by
/// [`TTree::read_branch_flat`](crate::TTree::read_branch_flat). `values` is
/// always a *scalar* variant (e.g. `F64`), never a `Vec*`.
#[derive(Debug, Clone, PartialEq)]
pub struct Jagged {
    /// Cumulative element boundaries, one per entry plus a leading `0`.
    pub offsets: Vec<u64>,
    /// The flattened element values (a scalar [`BranchValues`] variant).
    pub values: BranchValues,
}

impl Jagged {
    /// The number of entries.
    #[must_use]
    pub fn len(&self) -> usize {
        self.offsets.len().saturating_sub(1)
    }

    /// Whether there are no entries.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl BranchValues {
    /// The number of entries (top-level rows).
    #[must_use]
    pub fn len(&self) -> usize {
        use BranchValues::*;
        match self {
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
            VecBool(v) => v.len(),
            VecI8(v) => v.len(),
            VecU8(v) => v.len(),
            VecI16(v) => v.len(),
            VecU16(v) => v.len(),
            VecI32(v) => v.len(),
            VecU32(v) => v.len(),
            VecI64(v) => v.len(),
            VecU64(v) => v.len(),
            VecF32(v) => v.len(),
            VecF64(v) => v.len(),
            Str(v) => v.len(),
        }
    }

    /// Whether there are no entries.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// This branch's element type.
    #[must_use]
    pub fn leaf_type(&self) -> LeafType {
        use BranchValues::*;
        match self {
            Bool(_) | VecBool(_) => LeafType::Bool,
            I8(_) | VecI8(_) => LeafType::I8,
            U8(_) | VecU8(_) => LeafType::U8,
            I16(_) | VecI16(_) => LeafType::I16,
            U16(_) | VecU16(_) => LeafType::U16,
            I32(_) | VecI32(_) => LeafType::I32,
            U32(_) | VecU32(_) => LeafType::U32,
            I64(_) | VecI64(_) => LeafType::I64,
            U64(_) | VecU64(_) => LeafType::U64,
            F32(_) | VecF32(_) => LeafType::F32,
            F64(_) | VecF64(_) => LeafType::F64,
            Str(_) => LeafType::Str,
        }
    }
}

/// Generate `as_<ty>() -> Option<&[..]>` accessors for the common scalar types.
macro_rules! scalar_accessors {
    ($($method:ident => $variant:ident($ty:ty)),* $(,)?) => {
        impl BranchValues {
            $(
                #[doc = concat!("The values if this is a scalar `", stringify!($variant), "` branch.")]
                #[must_use]
                pub fn $method(&self) -> Option<&[$ty]> {
                    if let BranchValues::$variant(v) = self { Some(v) } else { None }
                }
            )*
        }
    };
}
scalar_accessors! {
    as_bool => Bool(bool), as_i8 => I8(i8), as_u8 => U8(u8),
    as_i16 => I16(i16), as_u16 => U16(u16), as_i32 => I32(i32), as_u32 => U32(u32),
    as_i64 => I64(i64), as_u64 => U64(u64), as_f32 => F32(f32), as_f64 => F64(f64),
    as_str => Str(String),
}
