//! The fields a caller writes: [`Field`] and the [`Column`] data it holds.

use oxiroot_io_core::{Error, Result};

use super::lower::flatten;

/// A column of data for one RNTuple field.
#[non_exhaustive]
pub enum Column {
    /// `bool` (Bit column).
    Bool(Vec<bool>),
    /// 8-bit signed integers.
    I8(Vec<i8>),
    /// 8-bit unsigned integers.
    U8(Vec<u8>),
    /// 16-bit signed integers.
    I16(Vec<i16>),
    /// 16-bit unsigned integers.
    U16(Vec<u16>),
    /// 32-bit signed integers.
    I32(Vec<i32>),
    /// 64-bit signed integers.
    I64(Vec<i64>),
    /// 32-bit unsigned integers.
    U32(Vec<u32>),
    /// 64-bit unsigned integers.
    U64(Vec<u64>),
    /// 32-bit floats.
    F32(Vec<f32>),
    /// 64-bit floats.
    F64(Vec<f64>),
    /// `std::string`.
    Str(Vec<String>),
    /// `std::vector<bool>`.
    VecBool(Vec<Vec<bool>>),
    /// `std::vector<int8_t>`.
    VecI8(Vec<Vec<i8>>),
    /// `std::vector<uint8_t>`.
    VecU8(Vec<Vec<u8>>),
    /// `std::vector<int16_t>`.
    VecI16(Vec<Vec<i16>>),
    /// `std::vector<uint16_t>`.
    VecU16(Vec<Vec<u16>>),
    /// `std::vector<float>`.
    VecF32(Vec<Vec<f32>>),
    /// `std::vector<double>`.
    VecF64(Vec<Vec<f64>>),
    /// `std::vector<int32_t>`.
    VecI32(Vec<Vec<i32>>),
    /// `std::vector<int64_t>`.
    VecI64(Vec<Vec<i64>>),
    /// `std::vector<std::string>`.
    VecStr(Vec<Vec<String>>),
    /// A `float` field stored at half precision (the `Real16` column).
    HalfF32(Vec<f32>),
    /// A `float` field stored with its mantissa truncated to `bits` bits total
    /// (the `Real32Trunc` column, `10 <= bits <= 31`).
    TruncF32 {
        /// The values to store.
        values: Vec<f32>,
        /// Bits kept per value (sign + exponent + high mantissa).
        bits: u16,
    },
    /// A `float` field linearly quantized into `bits`-wide integers over
    /// `[min, max]` (the `Real32Quant` column, `1 <= bits <= 32`). Values are
    /// assumed to lie within the range.
    QuantF32 {
        /// The values to store.
        values: Vec<f32>,
        /// The (inclusive) value range the quantization spans.
        range: (f64, f64),
        /// Bits per quantized value.
        bits: u16,
    },
    /// A record / struct: named sub-fields (a struct-of-arrays), each with one
    /// value per record instance. At top level this is a struct field; wrap it
    /// in [`Nested`](Self::Nested) for a `std::vector<MyStruct>`.
    Record(Vec<(String, Column)>),
    /// A collection whose element is itself a collection or record — e.g.
    /// `std::vector<std::vector<T>>` or `std::vector<MyStruct>`. The cumulative
    /// `offsets` (one per entry) partition the flattened child `items`. The
    /// `vec_vec_*` constructors build the common nested-vector cases for you.
    Nested {
        /// Cumulative element boundaries, one per entry.
        offsets: Vec<u64>,
        /// The flattened child column.
        items: Box<Column>,
    },
    /// A `std::variant`: the `alternatives` (named `_0`, `_1`, … on disk) each
    /// hold their densely-packed active values, and `tags` selects the active
    /// alternative per entry (1-based, `0` = valueless). The per-alternative
    /// indices are derived from the tags (sequential within each alternative),
    /// so each `alternatives[k]` must hold exactly as many values as there are
    /// `tags == k + 1`.
    Variant {
        /// The variant alternatives, in order.
        alternatives: Vec<Column>,
        /// Per entry, the 1-based active alternative (`0` = valueless).
        tags: Vec<u32>,
    },
    /// A fixed-size array (`std::array<T, N>`): exactly `len` elements per entry.
    /// `items` is the flattened element column (`len * entries` values); on disk
    /// the array field carries no column of its own and holds a single element
    /// child `_0`.
    Array {
        /// Elements per entry (`N`).
        len: usize,
        /// The flattened element column.
        items: Box<Column>,
    },
    /// A `std::bitset<N>`: exactly `len` bits per entry, stored in the field's own
    /// Bit column. `bits` is the flattened bit stream (`len * entries`).
    Bitset {
        /// Bits per entry (`N`).
        len: usize,
        /// The flattened bits.
        bits: Vec<bool>,
    },
    /// A user-defined class split into a record of named `members`, tagged with
    /// its C++ `type_name` and the ROOT class checksum (computed from the type
    /// name and members). ROOT reads it back as the class when its dictionary is
    /// loaded.
    Object {
        /// The C++ class name (e.g. `"Hit"`).
        type_name: String,
        /// The named members, in declaration order.
        members: Vec<(String, Column)>,
    },
    /// An associative container stored as a collection (`std::set<T>`,
    /// `std::map<K, V>`, …): an Index offset column over `offsets` plus a single
    /// element child `_0` (`items` — a leaf for a set, a `Record` of key/value
    /// for a map). `type_name` is the full C++ container type written to the
    /// field record.
    Assoc {
        /// The C++ container type name (e.g. `"std::set<std::int32_t>"`).
        type_name: String,
        /// Cumulative element boundaries, one per entry.
        offsets: Vec<u64>,
        /// The flattened element child.
        items: Box<Column>,
    },
    /// A nullable / "late" field — `std::optional<T>` or `std::unique_ptr<T>`.
    /// `present[i]` flags whether entry `i` holds a value; `values` holds the
    /// present values densely, in order. Built by the `optional_*` /
    /// `unique_ptr_*` constructors (which split a `Vec<Option<T>>`).
    Optional {
        /// `true` for `std::unique_ptr<T>`, `false` for `std::optional<T>`.
        unique: bool,
        /// Per-entry presence (length = entry count).
        present: Vec<bool>,
        /// The present values, densely packed in entry order.
        values: Box<Column>,
    },
    /// A `std::atomic<T>` field — stored as the bare `T`. Built by `atomic_*`.
    Atomic(Box<Column>),
}

impl Column {
    /// Number of top-level entries.
    pub(super) fn len(&self) -> usize {
        match self {
            Column::Bool(v) => v.len(),
            Column::I8(v) => v.len(),
            Column::U8(v) => v.len(),
            Column::I16(v) => v.len(),
            Column::U16(v) => v.len(),
            Column::I32(v) => v.len(),
            Column::I64(v) => v.len(),
            Column::U32(v) => v.len(),
            Column::U64(v) => v.len(),
            Column::F32(v) => v.len(),
            Column::F64(v) => v.len(),
            Column::Str(v) => v.len(),
            Column::VecBool(v) => v.len(),
            Column::VecI8(v) => v.len(),
            Column::VecU8(v) => v.len(),
            Column::VecI16(v) => v.len(),
            Column::VecU16(v) => v.len(),
            Column::VecF32(v) => v.len(),
            Column::VecF64(v) => v.len(),
            Column::VecI32(v) => v.len(),
            Column::VecI64(v) => v.len(),
            Column::VecStr(v) => v.len(),
            Column::HalfF32(v) => v.len(),
            Column::TruncF32 { values, .. } => values.len(),
            Column::QuantF32 { values, .. } => values.len(),
            Column::Record(subs) => subs.first().map_or(0, |(_, c)| c.len()),
            Column::Nested { offsets, .. } => offsets.len(),
            Column::Variant { tags, .. } => tags.len(),
            Column::Array { len, items } => items.len().checked_div(*len).unwrap_or(0),
            Column::Bitset { len, bits } => bits.len().checked_div(*len).unwrap_or(0),
            Column::Object { members, .. } => members.first().map_or(0, |(_, c)| c.len()),
            Column::Assoc { offsets, .. } => offsets.len(),
            Column::Optional { present, .. } => present.len(),
            Column::Atomic(inner) => inner.len(),
        }
    }
}

/// A named RNTuple field.
pub struct Field {
    /// Field name.
    pub name: String,
    /// Field data.
    pub data: Column,
}

impl Field {
    /// A field named `name` holding `data`.
    pub fn new(name: impl Into<String>, data: Column) -> Field {
        Field {
            name: name.into(),
            data,
        }
    }
}

/// Generate `Field::<name>(name, Vec<T>)` shortcuts, e.g. `Field::f64("pt", v)`.
macro_rules! field_ctors {
    ($($method:ident => $variant:ident($elem:ty)),* $(,)?) => {
        impl Field {
            $(
                #[doc = concat!("A field holding `", stringify!($variant), "` data.")]
                pub fn $method(name: impl Into<String>, data: Vec<$elem>) -> Field {
                    Field::new(name, Column::$variant(data))
                }
            )*
        }
    };
}

field_ctors! {
    bools => Bool(bool),
    i8 => I8(i8),
    u8 => U8(u8),
    i16 => I16(i16),
    u16 => U16(u16),
    i32 => I32(i32),
    i64 => I64(i64),
    u32 => U32(u32),
    u64 => U64(u64),
    f32 => F32(f32),
    f64 => F64(f64),
    strings => Str(String),
    vec_bool => VecBool(Vec<bool>),
    vec_i8 => VecI8(Vec<i8>),
    vec_u8 => VecU8(Vec<u8>),
    vec_i16 => VecI16(Vec<i16>),
    vec_u16 => VecU16(Vec<u16>),
    vec_i32 => VecI32(Vec<i32>),
    vec_i64 => VecI64(Vec<i64>),
    vec_f32 => VecF32(Vec<f32>),
    vec_f64 => VecF64(Vec<f64>),
    vec_str => VecStr(Vec<String>),
}

/// Wrap a flattened child column in a `std::vector<...>` by grouping it with
/// outer (per-entry) offsets — the building block for the `vec_vec_*` shortcuts.
fn nested_vec<T: Clone>(data: Vec<Vec<Vec<T>>>, wrap: impl Fn(Vec<Vec<T>>) -> Column) -> Column {
    let (offsets, inner) = flatten(&data);
    Column::Nested {
        offsets,
        items: Box::new(wrap(inner)),
    }
}

/// Generate `Field::<name>(name, Vec<Vec<Vec<T>>>)` shortcuts for
/// `std::vector<std::vector<T>>` fields.
macro_rules! vec_vec_ctors {
    ($($method:ident => $variant:ident($elem:ty)),* $(,)?) => {
        impl Field {
            $(
                #[doc = concat!("A `std::vector<std::vector<", stringify!($elem), ">>` field.")]
                pub fn $method(name: impl Into<String>, data: Vec<Vec<Vec<$elem>>>) -> Field {
                    Field::new(name, nested_vec(data, Column::$variant))
                }
            )*
        }
    };
}

vec_vec_ctors! {
    vec_vec_bool => VecBool(bool),
    vec_vec_i32 => VecI32(i32),
    vec_vec_i64 => VecI64(i64),
    vec_vec_f32 => VecF32(f32),
    vec_vec_f64 => VecF64(f64),
    vec_vec_str => VecStr(String),
}

/// Split a `Vec<Option<T>>` into a presence mask and the densely-packed present
/// values wrapped as a `Column`, for the `optional_*` / `unique_ptr_*` builders.
fn optional_column<T>(
    data: Vec<Option<T>>,
    wrap: impl Fn(Vec<T>) -> Column,
    unique: bool,
) -> Column {
    let present: Vec<bool> = data.iter().map(Option::is_some).collect();
    let values: Vec<T> = data.into_iter().flatten().collect();
    Column::Optional {
        unique,
        present,
        values: Box::new(wrap(values)),
    }
}

/// Generate the `optional_<ty>` / `unique_ptr_<ty>` (`Vec<Option<T>>`) and
/// `atomic_<ty>` (`Vec<T>`) field shortcuts for each primitive.
macro_rules! late_ctors {
    ($($opt:ident, $uniq:ident, $atom:ident => $variant:ident($elem:ty)),* $(,)?) => {
        impl Field {
            $(
                #[doc = concat!("A `std::optional<", stringify!($elem), ">` field.")]
                pub fn $opt(name: impl Into<String>, data: Vec<Option<$elem>>) -> Field {
                    Field::new(name, optional_column(data, Column::$variant, false))
                }
                #[doc = concat!("A `std::unique_ptr<", stringify!($elem), ">` field.")]
                pub fn $uniq(name: impl Into<String>, data: Vec<Option<$elem>>) -> Field {
                    Field::new(name, optional_column(data, Column::$variant, true))
                }
                #[doc = concat!("A `std::atomic<", stringify!($elem), ">` field (stored as the bare value).")]
                pub fn $atom(name: impl Into<String>, data: Vec<$elem>) -> Field {
                    Field::new(name, Column::Atomic(Box::new(Column::$variant(data))))
                }
            )*
        }
    };
}

late_ctors! {
    optional_bool, unique_ptr_bool, atomic_bool => Bool(bool),
    optional_i32, unique_ptr_i32, atomic_i32 => I32(i32),
    optional_i64, unique_ptr_i64, atomic_i64 => I64(i64),
    optional_u32, unique_ptr_u32, atomic_u32 => U32(u32),
    optional_u64, unique_ptr_u64, atomic_u64 => U64(u64),
    optional_f32, unique_ptr_f32, atomic_f32 => F32(f32),
    optional_f64, unique_ptr_f64, atomic_f64 => F64(f64),
}

impl Field {
    /// A `float` field stored at half precision (the `Real16` column) — half the
    /// space, ~3 decimal digits.
    pub fn half(name: impl Into<String>, values: Vec<f32>) -> Field {
        Field::new(name, Column::HalfF32(values))
    }

    /// A `float` field with its mantissa truncated to `bits` bits total (the
    /// `Real32Trunc` column, `10 <= bits <= 31`).
    pub fn truncated(name: impl Into<String>, values: Vec<f32>, bits: u16) -> Field {
        Field::new(name, Column::TruncF32 { values, bits })
    }

    /// A `float` field linearly quantized into `bits`-wide integers over
    /// `[min, max]` (the `Real32Quant` column, `1 <= bits <= 32`). All values
    /// must lie within the range.
    pub fn quantized(
        name: impl Into<String>,
        values: Vec<f32>,
        min: f64,
        max: f64,
        bits: u16,
    ) -> Field {
        Field::new(
            name,
            Column::QuantF32 {
                values,
                range: (min, max),
                bits,
            },
        )
    }

    /// A `std::variant` field: `alternatives` are the densely-packed active
    /// values per alternative, `tags` the 1-based active alternative per entry
    /// (`0` = valueless). See [`Column::Variant`].
    pub fn variant(name: impl Into<String>, alternatives: Vec<Column>, tags: Vec<u32>) -> Field {
        Field::new(name, Column::Variant { alternatives, tags })
    }

    /// A fixed-size array field (`std::array<T, N>`): `len` elements per entry,
    /// `items` the flattened element column (`len * entries` values). The element
    /// type spelling is taken from `items`. Use this for element types without a
    /// dedicated `array_*` shortcut.
    pub fn array(name: impl Into<String>, len: usize, items: Column) -> Field {
        Field::new(
            name,
            Column::Array {
                len,
                items: Box::new(items),
            },
        )
    }

    /// A `std::bitset<N>` field: `data` is one fixed-length bit vector per entry
    /// (all inner vectors must share the same length `N`).
    ///
    /// # Errors
    /// [`Error::LengthMismatch`] if an entry's bit vector is not as long as the
    /// first one's.
    pub fn bitset(name: impl Into<String>, data: Vec<Vec<bool>>) -> Result<Field> {
        let name = name.into();
        let len = uniform_len(&name, &data)?;
        let bits: Vec<bool> = data.into_iter().flatten().collect();
        Ok(Field::new(name, Column::Bitset { len, bits }))
    }

    /// A user-class field (`type_name`) split into named `members`. The ROOT
    /// class checksum is computed from the type name and members so ROOT reads it
    /// back as the class (with its dictionary loaded).
    pub fn object(
        name: impl Into<String>,
        type_name: impl Into<String>,
        members: Vec<(String, Column)>,
    ) -> Field {
        Field::new(
            name,
            Column::Object {
                type_name: type_name.into(),
                members,
            },
        )
    }
}

/// Build an [`Column::Array`] from per-entry chunks `data` (every chunk must have
/// the same length `N`), with `make` turning the flattened values into the
/// element column. Used by the typed `Field::array_*` shortcuts.
fn array_column<T: Clone>(
    name: &str,
    data: Vec<Vec<T>>,
    make: impl Fn(Vec<T>) -> Column,
) -> Result<Column> {
    let len = uniform_len(name, &data)?;
    let items: Vec<T> = data.into_iter().flatten().collect();
    Ok(Column::Array {
        len,
        items: Box::new(make(items)),
    })
}

/// The common length of the per-entry chunks of a fixed-size field (the first
/// chunk's), or an error naming the first entry that differs.
fn uniform_len<T>(name: &str, data: &[Vec<T>]) -> Result<usize> {
    let len = data.first().map_or(0, Vec::len);
    match data.iter().position(|chunk| chunk.len() != len) {
        Some(i) => Err(Error::LengthMismatch {
            what: format!("field {name:?} entry {i}"),
            expected: len,
            found: data[i].len(),
        }),
        None => Ok(len),
    }
}

/// Generate typed `Field::array_*` shortcuts taking per-entry chunks.
macro_rules! array_ctors {
    ($($method:ident => $variant:ident($elem:ty)),* $(,)?) => {
        impl Field {
            $(
                #[doc = concat!("A `std::array<", stringify!($elem), ", N>` field from per-entry chunks (all length `N`).")]
                ///
                /// # Errors
                /// [`Error::LengthMismatch`] if a chunk is not as long as the first one.
                pub fn $method(name: impl Into<String>, data: Vec<Vec<$elem>>) -> Result<Field> {
                    let name = name.into();
                    let column = array_column(&name, data, Column::$variant)?;
                    Ok(Field::new(name, column))
                }
            )*
        }
    };
}

array_ctors! {
    array_i32 => I32(i32),
    array_i64 => I64(i64),
    array_u32 => U32(u32),
    array_u64 => U64(u64),
    array_f32 => F32(f32),
    array_f64 => F64(f64),
}

impl Field {
    /// A `std::map<K, V>` field from per-entry key/value pairs. `key_type` and
    /// `val_type` are the C++ element type spellings ROOT uses (e.g.
    /// `"std::int32_t"`, `"double"`); `keys` and `vals` are the flattened key and
    /// value columns, partitioned per entry by `offsets`. On disk a map is a
    /// collection of `std::pair<K, V>` records.
    pub fn map(
        name: impl Into<String>,
        key_type: &str,
        val_type: &str,
        offsets: Vec<u64>,
        keys: Column,
        vals: Column,
    ) -> Field {
        let items = Column::Record(vec![("_0".into(), keys), ("_1".into(), vals)]);
        Field::new(
            name,
            Column::Assoc {
                type_name: format!("std::map<{key_type},{val_type}>"),
                offsets,
                items: Box::new(items),
            },
        )
    }

    /// A `std::map<std::int32_t, double>` field from one `(key, value)` list per
    /// entry. The pairs are stored in the order given (ROOT re-sorts a real
    /// `std::map` by key on read).
    pub fn map_i32_f64(name: impl Into<String>, data: Vec<Vec<(i32, f64)>>) -> Field {
        let mut offsets = Vec::with_capacity(data.len());
        let mut keys = Vec::new();
        let mut vals = Vec::new();
        for entry in &data {
            for &(k, v) in entry {
                keys.push(k);
                vals.push(v);
            }
            offsets.push(keys.len() as u64);
        }
        Field::map(
            name,
            "std::int32_t",
            "double",
            offsets,
            Column::I32(keys),
            Column::F64(vals),
        )
    }
}

/// Generate typed `Field::set_*` shortcuts: a `std::set<T>` from per-entry lists.
macro_rules! set_ctors {
    ($($method:ident => $variant:ident($elem:ty, $cxx:literal)),* $(,)?) => {
        impl Field {
            $(
                #[doc = concat!("A `std::set<", $cxx, ">` field from one element list per entry.")]
                pub fn $method(name: impl Into<String>, data: Vec<Vec<$elem>>) -> Field {
                    let (offsets, flat) = flatten(&data);
                    Field::new(name, Column::Assoc {
                        type_name: concat!("std::set<", $cxx, ">").to_string(),
                        offsets,
                        items: Box::new(Column::$variant(flat)),
                    })
                }
            )*
        }
    };
}

set_ctors! {
    set_i32 => I32(i32, "std::int32_t"),
    set_i64 => I64(i64, "std::int64_t"),
    set_u32 => U32(u32, "std::uint32_t"),
    set_u64 => U64(u64, "std::uint64_t"),
    set_f32 => F32(f32, "float"),
    set_f64 => F64(f64, "double"),
    set_str => Str(String, "std::string"),
}
