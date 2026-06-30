//! Writing an RNTuple into a ROOT file.
//!
//! [`write_rntuple_file`] writes a whole RNTuple in one shot, supporting scalar
//! (`bool`/`i32`/`i64`/`f32`/`f64`), `std::string`, and `std::vector<T>` fields
//! in a single cluster, with non-split column encodings and optional page
//! compression. [`RNTupleWriter`] writes those same field types one cluster per
//! batch, so a large dataset need not be held in memory at once. The header/page/
//! page-list/footer envelopes are written as raw blobs at the offsets the anchor
//! (and the page locators) point to; only the anchor is a `TKey`. Validated by
//! reading the result back and by official ROOT / uproot.

use std::io::{self, Seek, SeekFrom, Write};
use std::path::Path;

use oxiroot_io_core::buffer::{Patch, WBuffer};
use oxiroot_io_core::error::{Error, Result};
use oxiroot_io_core::{key_len_fmt, write_key_header_fmt, Compression, KSTART_BIG_FILE};

use crate::column::ColumnType;

const K_BYTE_COUNT_MASK: u32 = 0x4000_0000;
const DATIME: u32 = 0x7d7a_79ca;
const FILE_VERSION: u32 = 62400;

/// The on-disk bytes for one page: ROOT-compressed when `compression != 0` and
/// the result is actually smaller, otherwise the raw column bytes. A reader
/// tells the two apart by comparing the on-disk size to the uncompressed size
/// (derived from the element count), exactly as ROOT does.
fn on_disk_page(page: &[u8], compression: u32) -> Vec<u8> {
    if compression == 0 {
        return page.to_vec();
    }
    match oxiroot_compress::compress(page, compression) {
        Ok(compressed) if compressed.len() < page.len() => compressed,
        _ => page.to_vec(),
    }
}

const ROLE_LEAF: u16 = 0;
const ROLE_COLLECTION: u16 = 1;
const ROLE_RECORD: u16 = 2;
const ROLE_VARIANT: u16 = 3;

/// Field flag: the field is a fixed-size array (`std::array`/`std::bitset`); an
/// element count (`u64`) trails the field record.
const FIELD_FLAG_ARRAY: u16 = 0x01;
/// Field flag: a type checksum (`u32`) trails the field record (user classes).
const FIELD_FLAG_CHECKSUM: u16 = 0x04;
/// The on-disk type version a checksummed user-class field carries (`-1`).
const CHECKSUM_TYPE_VERSION: u32 = 0xFFFF_FFFF;

/// Column flag: the descriptor carries an `(f64, f64)` value range (e.g. for a
/// quantized real column).
const COLUMN_FLAG_RANGE: u16 = 0x02;

/// Column flag: a deferred column (a schema-extension field added late) — the
/// descriptor carries an `i64` first-element index trailer.
const COLUMN_FLAG_DEFERRED: u16 = 0x01;

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
    fn len(&self) -> usize {
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
    pub fn bitset(name: impl Into<String>, data: Vec<Vec<bool>>) -> Field {
        let len = data.first().map_or(0, Vec::len);
        let bits: Vec<bool> = data.into_iter().flatten().collect();
        Field::new(name, Column::Bitset { len, bits })
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
fn array_column<T: Clone>(data: Vec<Vec<T>>, make: impl Fn(Vec<T>) -> Column) -> Column {
    let len = data.first().map_or(0, Vec::len);
    let items: Vec<T> = data.into_iter().flatten().collect();
    Column::Array {
        len,
        items: Box::new(make(items)),
    }
}

/// Generate typed `Field::array_*` shortcuts taking per-entry chunks.
macro_rules! array_ctors {
    ($($method:ident => $variant:ident($elem:ty)),* $(,)?) => {
        impl Field {
            $(
                #[doc = concat!("A `std::array<", stringify!($elem), ", N>` field from per-entry chunks (all length `N`).")]
                pub fn $method(name: impl Into<String>, data: Vec<Vec<$elem>>) -> Field {
                    Field::new(name, array_column(data, Column::$variant))
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

// --- internal lowered model ------------------------------------------------

struct FieldPlan {
    name: String,
    type_name: String,
    parent_id: u32,
    role: u16,
    /// Field flag bits (`FIELD_FLAG_*`): array/checksum.
    flags: u16,
    /// `std::array`/`std::bitset` element count, when [`FIELD_FLAG_ARRAY`] is set.
    array_size: Option<u64>,
    /// ROOT class checksum, when [`FIELD_FLAG_CHECKSUM`] is set (user classes).
    type_checksum: Option<u32>,
}

struct ColumnPlan {
    column_type: ColumnType,
    bits: u16,
    field_id: u32,
    page: Vec<u8>,
    n: u32,
    value_range: Option<(f64, f64)>,
}

fn le_bytes<T, const N: usize>(values: &[T], to: impl Fn(&T) -> [u8; N]) -> Vec<u8> {
    let mut out = Vec::with_capacity(values.len() * N);
    for v in values {
        out.extend_from_slice(&to(v));
    }
    out
}

fn pack_bits(v: &[bool]) -> Vec<u8> {
    let mut out = vec![0u8; v.len().div_ceil(8)];
    for (i, &b) in v.iter().enumerate() {
        if b {
            out[i >> 3] |= 1 << (i & 7);
        }
    }
    out
}

/// Cumulative end offsets (Index64) for collections, plus the flattened data.
fn flatten<T: Clone>(v: &[Vec<T>]) -> (Vec<u64>, Vec<T>) {
    let mut offsets = Vec::with_capacity(v.len());
    let mut data = Vec::new();
    for inner in v {
        data.extend_from_slice(inner);
        offsets.push(data.len() as u64);
    }
    (offsets, data)
}

/// Bit-pack `bits`-wide unsigned values LSB-first into little-endian bytes (the
/// inverse of the reader's unpacking; used by the truncated/quantized reals).
fn pack_uints(values: &[u64], bits: u16) -> Vec<u8> {
    let nbits = bits as usize;
    let mut out = vec![0u8; (values.len() * nbits).div_ceil(8)];
    let mut pos = 0usize;
    for &v in values {
        for b in 0..nbits {
            if (v >> b) & 1 != 0 {
                let g = pos + b;
                out[g >> 3] |= 1 << (g & 7);
            }
        }
        pos += nbits;
    }
    out
}

/// Encode an IEEE-754 single into a half (binary16), round-to-nearest-even.
fn f32_to_half(value: f32) -> u16 {
    let x = value.to_bits();
    let sign = ((x >> 16) & 0x8000) as u16;
    let mut mant = (x & 0x007f_ffff) as i32;
    let exp = ((x >> 23) & 0xff) as i32;

    if exp == 0xff {
        // Inf, or NaN (keep it a non-signalling NaN).
        return sign | 0x7c00 | if mant != 0 { 0x0200 } else { 0 };
    }
    let he = exp - 127 + 15; // rebias to the half's exponent
    if he >= 0x1f {
        return sign | 0x7c00; // overflow -> Inf
    }
    if he <= 0 {
        if he < -10 {
            return sign; // underflow -> +/-0
        }
        // Subnormal half: shift the (restored) mantissa down, rounding to even.
        mant |= 0x0080_0000;
        let shift = 14 - he;
        let mut h = (mant >> shift) as u16;
        let rem = mant & ((1 << shift) - 1);
        let halfway = 1 << (shift - 1);
        if rem > halfway || (rem == halfway && (h & 1) == 1) {
            h += 1;
        }
        return sign | h;
    }
    // Normal half; rounding may carry into the exponent, which is correct.
    let mut h = ((he as u16) << 10) | (mant >> 13) as u16;
    let rem = mant & 0x1fff;
    if rem > 0x1000 || (rem == 0x1000 && (h & 1) == 1) {
        h += 1;
    }
    sign | h
}

/// One column's lowered bytes, before its field id is known.
struct RawCol {
    column_type: ColumnType,
    bits: u16,
    page: Vec<u8>,
    n: u32,
    value_range: Option<(f64, f64)>,
}

/// A lowered field subtree — a field, its own columns, and its children —
/// before field ids are assigned by the depth-first walk in [`flatten_tree`].
struct Node {
    name: String,
    type_name: String,
    role: u16,
    cols: Vec<RawCol>,
    children: Vec<Node>,
    /// Field flag bits (`FIELD_FLAG_*`).
    flags: u16,
    /// Fixed array element count (`std::array`/`std::bitset`).
    array_size: Option<u64>,
    /// ROOT class checksum (user-class records).
    type_checksum: Option<u32>,
}

fn raw(column_type: ColumnType, bits: u16, page: Vec<u8>, n: usize) -> RawCol {
    RawCol {
        column_type,
        bits,
        page,
        n: n as u32,
        value_range: None,
    }
}

/// A scalar leaf: one field, one column, no children.
fn leaf_node(name: &str, type_name: &str, col: RawCol) -> Node {
    Node {
        name: name.to_string(),
        type_name: type_name.to_string(),
        role: ROLE_LEAF,
        cols: vec![col],
        children: vec![],
        flags: 0,
        array_size: None,
        type_checksum: None,
    }
}

/// A `std::string` leaf: an Index64 offset column plus a Char column.
fn string_node(name: &str, v: &[String]) -> Node {
    let mut bytes = Vec::new();
    let mut offsets = Vec::with_capacity(v.len());
    for s in v {
        bytes.extend_from_slice(s.as_bytes());
        offsets.push(bytes.len() as u64);
    }
    let n_chars = bytes.len();
    Node {
        name: name.to_string(),
        type_name: "std::string".to_string(),
        role: ROLE_LEAF,
        cols: vec![
            raw(
                ColumnType::Index64,
                64,
                le_bytes(&offsets, |x| x.to_le_bytes()),
                v.len(),
            ),
            raw(ColumnType::Char, 8, bytes, n_chars),
        ],
        children: vec![],
        flags: 0,
        array_size: None,
        type_checksum: None,
    }
}

/// A collection field: an Index64 offset column over `offsets` plus the single
/// element `child`. Its type name is `std::vector<child>`.
fn collection_node(name: &str, offsets: &[u64], n_outer: usize, child: Node) -> Node {
    collection_node_wrapped(name, "std::vector", offsets, n_outer, child)
}

/// Like [`collection_node`] but with a caller-chosen container `wrapper`
/// (`"std::vector"`, `"std::optional"`, `"std::unique_ptr"`, …). All share the
/// same on-disk shape — an `Index64` offset column over a single child — so they
/// differ only in the field's `type_name`.
fn collection_node_wrapped(
    name: &str,
    wrapper: &str,
    offsets: &[u64],
    n_outer: usize,
    child: Node,
) -> Node {
    Node {
        name: name.to_string(),
        type_name: format!("{wrapper}<{}>", child.type_name),
        role: ROLE_COLLECTION,
        cols: vec![raw(
            ColumnType::Index64,
            64,
            le_bytes(offsets, |x| x.to_le_bytes()),
            n_outer,
        )],
        children: vec![child],
        flags: 0,
        array_size: None,
        type_checksum: None,
    }
}

/// Cumulative present-count offsets for an optional/unique_ptr presence mask:
/// `offsets[i]` is the number of present entries in `0..=i` (the Index64 column).
fn present_offsets(present: &[bool]) -> Vec<u64> {
    let mut acc = 0u64;
    present
        .iter()
        .map(|&p| {
            if p {
                acc += 1;
            }
            acc
        })
        .collect()
}

/// ROOT's anonymous-record type names: a 2-field record serializes as a
/// `std::pair`, more as a `std::tuple`.
fn record_type_name(children: &[Node]) -> String {
    let inner: Vec<&str> = children.iter().map(|c| c.type_name.as_str()).collect();
    match inner.as_slice() {
        [a, b] => format!("std::pair<{a},{b}>"),
        _ => format!("std::tuple<{}>", inner.join(",")),
    }
}

/// Lower one field's [`Column`] into a [`Node`] subtree.
fn lower_column(name: &str, data: &Column) -> Node {
    match data {
        Column::Bool(v) => leaf_node(name, "bool", raw(ColumnType::Bit, 1, pack_bits(v), v.len())),
        Column::I8(v) => leaf_node(
            name,
            "std::int8_t",
            raw(
                ColumnType::Int8,
                8,
                le_bytes(v, |x| x.to_le_bytes()),
                v.len(),
            ),
        ),
        Column::U8(v) => leaf_node(
            name,
            "std::uint8_t",
            raw(
                ColumnType::UInt8,
                8,
                le_bytes(v, |x| x.to_le_bytes()),
                v.len(),
            ),
        ),
        Column::I16(v) => leaf_node(
            name,
            "std::int16_t",
            raw(
                ColumnType::Int16,
                16,
                le_bytes(v, |x| x.to_le_bytes()),
                v.len(),
            ),
        ),
        Column::U16(v) => leaf_node(
            name,
            "std::uint16_t",
            raw(
                ColumnType::UInt16,
                16,
                le_bytes(v, |x| x.to_le_bytes()),
                v.len(),
            ),
        ),
        Column::I32(v) => leaf_node(
            name,
            "std::int32_t",
            raw(
                ColumnType::Int32,
                32,
                le_bytes(v, |x| x.to_le_bytes()),
                v.len(),
            ),
        ),
        Column::I64(v) => leaf_node(
            name,
            "std::int64_t",
            raw(
                ColumnType::Int64,
                64,
                le_bytes(v, |x| x.to_le_bytes()),
                v.len(),
            ),
        ),
        Column::U32(v) => leaf_node(
            name,
            "std::uint32_t",
            raw(
                ColumnType::UInt32,
                32,
                le_bytes(v, |x| x.to_le_bytes()),
                v.len(),
            ),
        ),
        Column::U64(v) => leaf_node(
            name,
            "std::uint64_t",
            raw(
                ColumnType::UInt64,
                64,
                le_bytes(v, |x| x.to_le_bytes()),
                v.len(),
            ),
        ),
        Column::F32(v) => leaf_node(
            name,
            "float",
            raw(
                ColumnType::Real32,
                32,
                le_bytes(v, |x| x.to_le_bytes()),
                v.len(),
            ),
        ),
        Column::F64(v) => leaf_node(
            name,
            "double",
            raw(
                ColumnType::Real64,
                64,
                le_bytes(v, |x| x.to_le_bytes()),
                v.len(),
            ),
        ),
        Column::Str(v) => string_node(name, v),
        Column::VecBool(v) => {
            let (offsets, data) = flatten(v);
            let child = leaf_node(
                "_0",
                "bool",
                raw(ColumnType::Bit, 1, pack_bits(&data), data.len()),
            );
            collection_node(name, &offsets, v.len(), child)
        }
        Column::VecI8(v) => {
            let (offsets, data) = flatten(v);
            let child = leaf_node(
                "_0",
                "std::int8_t",
                raw(
                    ColumnType::Int8,
                    8,
                    le_bytes(&data, |x| x.to_le_bytes()),
                    data.len(),
                ),
            );
            collection_node(name, &offsets, v.len(), child)
        }
        Column::VecU8(v) => {
            let (offsets, data) = flatten(v);
            let child = leaf_node(
                "_0",
                "std::uint8_t",
                raw(
                    ColumnType::UInt8,
                    8,
                    le_bytes(&data, |x| x.to_le_bytes()),
                    data.len(),
                ),
            );
            collection_node(name, &offsets, v.len(), child)
        }
        Column::VecI16(v) => {
            let (offsets, data) = flatten(v);
            let child = leaf_node(
                "_0",
                "std::int16_t",
                raw(
                    ColumnType::Int16,
                    16,
                    le_bytes(&data, |x| x.to_le_bytes()),
                    data.len(),
                ),
            );
            collection_node(name, &offsets, v.len(), child)
        }
        Column::VecU16(v) => {
            let (offsets, data) = flatten(v);
            let child = leaf_node(
                "_0",
                "std::uint16_t",
                raw(
                    ColumnType::UInt16,
                    16,
                    le_bytes(&data, |x| x.to_le_bytes()),
                    data.len(),
                ),
            );
            collection_node(name, &offsets, v.len(), child)
        }
        Column::VecI32(v) => {
            let (offsets, data) = flatten(v);
            let child = leaf_node(
                "_0",
                "std::int32_t",
                raw(
                    ColumnType::Int32,
                    32,
                    le_bytes(&data, |x| x.to_le_bytes()),
                    data.len(),
                ),
            );
            collection_node(name, &offsets, v.len(), child)
        }
        Column::VecI64(v) => {
            let (offsets, data) = flatten(v);
            let child = leaf_node(
                "_0",
                "std::int64_t",
                raw(
                    ColumnType::Int64,
                    64,
                    le_bytes(&data, |x| x.to_le_bytes()),
                    data.len(),
                ),
            );
            collection_node(name, &offsets, v.len(), child)
        }
        Column::VecF32(v) => {
            let (offsets, data) = flatten(v);
            let child = leaf_node(
                "_0",
                "float",
                raw(
                    ColumnType::Real32,
                    32,
                    le_bytes(&data, |x| x.to_le_bytes()),
                    data.len(),
                ),
            );
            collection_node(name, &offsets, v.len(), child)
        }
        Column::VecF64(v) => {
            let (offsets, data) = flatten(v);
            let child = leaf_node(
                "_0",
                "double",
                raw(
                    ColumnType::Real64,
                    64,
                    le_bytes(&data, |x| x.to_le_bytes()),
                    data.len(),
                ),
            );
            collection_node(name, &offsets, v.len(), child)
        }
        Column::VecStr(v) => {
            let (offsets, data) = flatten(v);
            collection_node(name, &offsets, v.len(), string_node("_0", &data))
        }
        Column::HalfF32(v) => {
            let page: Vec<u8> = v
                .iter()
                .flat_map(|&x| f32_to_half(x).to_le_bytes())
                .collect();
            leaf_node(name, "float", raw(ColumnType::Real16, 16, page, v.len()))
        }
        Column::TruncF32 { values, bits } => {
            let shift = 32 - u32::from(*bits);
            let packed: Vec<u64> = values
                .iter()
                .map(|&x| u64::from(x.to_bits() >> shift))
                .collect();
            let page = pack_uints(&packed, *bits);
            leaf_node(
                name,
                "float",
                raw(ColumnType::Real32Trunc, *bits, page, values.len()),
            )
        }
        Column::QuantF32 {
            values,
            range: (min, max),
            bits,
        } => {
            let denom = ((1u64 << bits) - 1) as f64;
            let span = max - min;
            let packed: Vec<u64> = values
                .iter()
                .map(|&x| {
                    let t = if span != 0.0 {
                        ((f64::from(x) - min) / span).clamp(0.0, 1.0)
                    } else {
                        0.0
                    };
                    (t * denom).round() as u64
                })
                .collect();
            let page = pack_uints(&packed, *bits);
            let mut col = raw(ColumnType::Real32Quant, *bits, page, values.len());
            col.value_range = Some((*min, *max));
            leaf_node(name, "float", col)
        }
        Column::Nested { offsets, items } => {
            let child = lower_column("_0", items);
            collection_node(name, offsets, offsets.len(), child)
        }
        Column::Optional {
            unique,
            present,
            values,
        } => {
            // optional/unique_ptr share the collection shape; only the wrapper
            // type name differs. The index column counts present entries.
            let child = lower_column("_0", values);
            let offsets = present_offsets(present);
            let wrapper = if *unique {
                "std::unique_ptr"
            } else {
                "std::optional"
            };
            collection_node_wrapped(name, wrapper, &offsets, present.len(), child)
        }
        Column::Atomic(inner) => {
            // std::atomic<T> is a Leaf field that delegates to a single child
            // leaf `_0` carrying the bare value (no column of its own).
            let child = lower_column("_0", inner);
            Node {
                name: name.to_string(),
                type_name: format!("std::atomic<{}>", child.type_name),
                role: ROLE_LEAF,
                cols: vec![],
                children: vec![child],
                flags: 0,
                array_size: None,
                type_checksum: None,
            }
        }
        Column::Record(subs) => {
            let children: Vec<Node> = subs.iter().map(|(n, c)| lower_column(n, c)).collect();
            let type_name = record_type_name(&children);
            Node {
                name: name.to_string(),
                type_name,
                role: ROLE_RECORD,
                cols: vec![],
                children,
                flags: 0,
                array_size: None,
                type_checksum: None,
            }
        }
        Column::Variant { alternatives, tags } => {
            // Derive each entry's index (a running counter within its active
            // alternative) and encode the Switch column: 8-byte index + 4-byte
            // tag per entry.
            let mut counters = vec![0u64; alternatives.len()];
            let mut switch = Vec::with_capacity(tags.len() * 12);
            for &tag in tags {
                let (index, out_tag) = if tag == 0 || (tag as usize) > alternatives.len() {
                    (0u64, 0u32)
                } else {
                    let k = (tag - 1) as usize;
                    let i = counters[k];
                    counters[k] += 1;
                    (i, tag)
                };
                switch.extend_from_slice(&index.to_le_bytes());
                switch.extend_from_slice(&out_tag.to_le_bytes());
            }
            // Alternatives are named `_0`, `_1`, … on disk.
            let children: Vec<Node> = alternatives
                .iter()
                .enumerate()
                .map(|(k, c)| lower_column(&format!("_{k}"), c))
                .collect();
            let inner: Vec<&str> = children.iter().map(|c| c.type_name.as_str()).collect();
            Node {
                name: name.to_string(),
                type_name: format!("std::variant<{}>", inner.join(",")),
                role: ROLE_VARIANT,
                cols: vec![raw(ColumnType::Switch, 96, switch, tags.len())],
                children,
                flags: 0,
                array_size: None,
                type_checksum: None,
            }
        }
        Column::Array { len, items } => {
            // A fixed array: no own column, one element child `_0` carrying the
            // flattened values; the field flags an `array_size` element count.
            let child = lower_column("_0", items);
            Node {
                name: name.to_string(),
                type_name: format!("std::array<{},{}>", child.type_name, len),
                role: ROLE_LEAF,
                cols: vec![],
                children: vec![child],
                flags: FIELD_FLAG_ARRAY,
                array_size: Some(*len as u64),
                type_checksum: None,
            }
        }
        Column::Bitset { len, bits } => Node {
            name: name.to_string(),
            type_name: format!("std::bitset<{len}>"),
            role: ROLE_LEAF,
            cols: vec![raw(ColumnType::Bit, 1, pack_bits(bits), bits.len())],
            children: vec![],
            flags: FIELD_FLAG_ARRAY,
            array_size: Some(*len as u64),
            type_checksum: None,
        },
        Column::Object { type_name, members } => {
            let children: Vec<Node> = members.iter().map(|(n, c)| lower_column(n, c)).collect();
            let checksum = class_checksum(type_name, members);
            Node {
                name: name.to_string(),
                type_name: type_name.clone(),
                role: ROLE_RECORD,
                cols: vec![],
                children,
                flags: FIELD_FLAG_CHECKSUM,
                array_size: None,
                type_checksum: Some(checksum),
            }
        }
        Column::Assoc {
            type_name,
            offsets,
            items,
        } => {
            // Like a collection, but the field carries the associative type name
            // (std::set / std::map) instead of std::vector.
            let mut node = collection_node(name, offsets, offsets.len(), lower_column("_0", items));
            node.type_name = type_name.clone();
            node
        }
    }
}

/// ROOT's class checksum (`TClass::GetCheckSum`) for a flat record: fold each
/// character of the class name, then of every member's name and C++ type, into
/// `id = id * 3 + ch`. Matches `TClass::GetCheckSum()` for plain-member classes,
/// which the RNTuple field record stores so ROOT can validate the on-disk schema.
fn class_checksum(class_name: &str, members: &[(String, Column)]) -> u32 {
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

/// The C++ type spelling ROOT uses for a member when computing a class checksum
/// (the fundamental-type keyword, e.g. `int`/`double`), for each writable
/// scalar [`Column`]. Panics on a column kind not valid as a flat class member.
fn checksum_type_name(col: &Column) -> &'static str {
    match col {
        Column::Bool(_) => "bool",
        Column::I8(_) => "char",
        Column::U8(_) => "unsigned char",
        Column::I16(_) => "short",
        Column::U16(_) => "unsigned short",
        Column::I32(_) => "int",
        Column::U32(_) => "unsigned int",
        Column::I64(_) => "long long",
        Column::U64(_) => "unsigned long long",
        Column::F32(_) | Column::HalfF32(_) | Column::TruncF32 { .. } | Column::QuantF32 { .. } => {
            "float"
        }
        Column::F64(_) => "double",
        _ => "void", // non-scalar members are not part of the supported checksum
    }
}

/// Assign field ids by a depth-first pre-order walk (parents before children,
/// matching ROOT's field/column ordering) and attach each node's columns.
fn flatten_tree(roots: Vec<Node>) -> (Vec<FieldPlan>, Vec<ColumnPlan>) {
    let mut fields = Vec::new();
    let mut cols = Vec::new();
    for node in roots {
        push_node(node, None, &mut fields, &mut cols);
    }
    (fields, cols)
}

fn push_node(
    node: Node,
    parent: Option<u32>,
    fields: &mut Vec<FieldPlan>,
    cols: &mut Vec<ColumnPlan>,
) {
    let id = fields.len() as u32;
    fields.push(FieldPlan {
        name: node.name,
        type_name: node.type_name,
        parent_id: parent.unwrap_or(id), // a top-level field is its own parent
        role: node.role,
        flags: node.flags,
        array_size: node.array_size,
        type_checksum: node.type_checksum,
    });
    for c in node.cols {
        cols.push(ColumnPlan {
            column_type: c.column_type,
            bits: c.bits,
            field_id: id,
            page: c.page,
            n: c.n,
            value_range: c.value_range,
        });
    }
    for child in node.children {
        push_node(child, Some(id), fields, cols);
    }
}

/// Lower user fields into field and column plans (depth-first, parents before
/// children), returning the top-level entry count.
fn lower(fields: &[Field]) -> Result<(Vec<FieldPlan>, Vec<ColumnPlan>, u32)> {
    // The entry count is a 32-bit field on disk; reject an over-large batch
    // rather than silently wrapping it into a corrupt-but-accepted file.
    let n_rows = fields.first().map_or(0, |f| f.data.len());
    let n_entries = u32::try_from(n_rows).map_err(|_| {
        Error::Format(format!(
            "RNTuple batch has {n_rows} entries, over the {} limit for one write",
            u32::MAX
        ))
    })?;
    let roots: Vec<Node> = fields
        .iter()
        .map(|f| lower_column(&f.name, &f.data))
        .collect();
    let (field_plans, columns) = flatten_tree(roots);
    Ok((field_plans, columns, n_entries))
}

// --- envelope / frame / string primitives ---------------------------------

fn rstr(s: &str) -> Vec<u8> {
    let mut out = (s.len() as u32).to_le_bytes().to_vec();
    out.extend_from_slice(s.as_bytes());
    out
}

fn envelope(type_id: u16, payload: &[u8]) -> Vec<u8> {
    let length = (8 + payload.len() + 8) as u64;
    let word = (type_id as u64) | (length << 16);
    let mut out = word.to_le_bytes().to_vec();
    out.extend_from_slice(payload);
    let checksum = xxhash_rust::xxh3::xxh3_64(&out);
    out.extend_from_slice(&checksum.to_le_bytes());
    out
}

fn record_frame(payload: &[u8]) -> Vec<u8> {
    let size = (8 + payload.len()) as i64;
    let mut out = size.to_le_bytes().to_vec();
    out.extend_from_slice(payload);
    out
}

fn list_frame(items: &[Vec<u8>]) -> Vec<u8> {
    let body_len: usize = items.iter().map(|i| i.len()).sum();
    let size = (8 + 4 + body_len) as i64;
    let mut out = (-size).to_le_bytes().to_vec();
    out.extend_from_slice(&(items.len() as u32).to_le_bytes());
    for item in items {
        out.extend_from_slice(item);
    }
    out
}

// --- envelope builders ------------------------------------------------------

/// One field descriptor record (shared by the header and the footer's
/// schema-extension record).
fn field_record(f: &FieldPlan) -> Vec<u8> {
    let mut r = Vec::new();
    r.extend_from_slice(&0u32.to_le_bytes()); // field version
                                              // A checksummed (user-class) field records type version -1.
    let type_version = if f.type_checksum.is_some() {
        CHECKSUM_TYPE_VERSION
    } else {
        0
    };
    r.extend_from_slice(&type_version.to_le_bytes());
    r.extend_from_slice(&f.parent_id.to_le_bytes());
    r.extend_from_slice(&f.role.to_le_bytes()); // struct role
    r.extend_from_slice(&f.flags.to_le_bytes()); // flags
    r.extend_from_slice(&rstr(&f.name));
    r.extend_from_slice(&rstr(&f.type_name));
    r.extend_from_slice(&rstr("")); // type alias
    r.extend_from_slice(&rstr("")); // description
                                    // Flag-gated trailers (reader order: array, projection source, checksum).
    if let Some(size) = f.array_size {
        r.extend_from_slice(&size.to_le_bytes());
    }
    if let Some(checksum) = f.type_checksum {
        r.extend_from_slice(&checksum.to_le_bytes());
    }
    record_frame(&r)
}

/// One column descriptor record. `first_element_index` is `Some` for a deferred
/// (schema-extension) column — its data starts at that global element index — and
/// sets the [`COLUMN_FLAG_DEFERRED`] flag plus the index trailer (before the value
/// range, matching the reader's order).
fn column_record(c: &ColumnPlan, first_element_index: Option<i64>) -> Vec<u8> {
    let mut r = Vec::new();
    r.extend_from_slice(&(c.column_type as u16).to_le_bytes());
    r.extend_from_slice(&c.bits.to_le_bytes());
    r.extend_from_slice(&c.field_id.to_le_bytes());
    let mut flags = 0u16;
    if first_element_index.is_some() {
        flags |= COLUMN_FLAG_DEFERRED;
    }
    if c.value_range.is_some() {
        flags |= COLUMN_FLAG_RANGE;
    }
    r.extend_from_slice(&flags.to_le_bytes());
    r.extend_from_slice(&0u16.to_le_bytes()); // representation index
    if let Some(first) = first_element_index {
        r.extend_from_slice(&first.to_le_bytes());
    }
    if let Some((min, max)) = c.value_range {
        r.extend_from_slice(&min.to_le_bytes());
        r.extend_from_slice(&max.to_le_bytes());
    }
    record_frame(&r)
}

fn build_header(name: &str, fields: &[FieldPlan], cols: &[ColumnPlan]) -> Vec<u8> {
    let mut p = Vec::new();
    p.extend_from_slice(&0i64.to_le_bytes()); // feature flags
    p.extend_from_slice(&rstr(name));
    p.extend_from_slice(&rstr("")); // description
    p.extend_from_slice(&rstr("oxiroot")); // writer

    let field_records: Vec<Vec<u8>> = fields.iter().map(field_record).collect();
    p.extend_from_slice(&list_frame(&field_records));

    let column_records: Vec<Vec<u8>> = cols.iter().map(|c| column_record(c, None)).collect();
    p.extend_from_slice(&list_frame(&column_records));

    p.extend_from_slice(&list_frame(&[])); // alias columns
    p.extend_from_slice(&list_frame(&[])); // extra type info

    envelope(0x01, &p)
}

/// A page-list entry stores a page's element count and on-disk size as signed
/// 32-bit fields — the element count's sign bit flags a trailing per-page
/// checksum — so a single page holds at most `i32::MAX` elements and `i32::MAX`
/// on-disk bytes. Reject anything larger rather than letting the `as i32` cast
/// wrap into a negative value that mislabels the page (a corrupt file). A genuine
/// page that big would need to be split across more clusters by the caller.
fn check_page_limits(n_elements: u32, disk_size: usize) -> Result<()> {
    if n_elements > i32::MAX as u32 {
        return Err(Error::Format(format!(
            "RNTuple page has {n_elements} elements, over the per-page limit of {} \
             (write fewer entries per cluster)",
            i32::MAX
        )));
    }
    if disk_size > i32::MAX as usize {
        return Err(Error::Format(format!(
            "RNTuple page on-disk size {disk_size} exceeds the per-page limit of {} bytes",
            i32::MAX
        )));
    }
    Ok(())
}

fn build_page_list(
    n_entries: u32,
    page_offsets: &[usize],
    disk_sizes: &[usize],
    cols: &[ColumnPlan],
    compression: u32,
    header_checksum: u64,
) -> Result<Vec<u8>> {
    // A normal single cluster: every column starts at element 0.
    let element_offsets = vec![0i64; cols.len()];
    build_page_list_offsets(
        n_entries,
        page_offsets,
        disk_sizes,
        cols,
        &element_offsets,
        compression,
        header_checksum,
    )
}

/// Like [`build_page_list`] but with each column's first-element offset given
/// explicitly: a deferred (schema-extension) column's page starts at that global
/// element index, so the reader knows the earlier entries are defaulted.
#[allow(clippy::too_many_arguments)]
fn build_page_list_offsets(
    n_entries: u32,
    page_offsets: &[usize],
    disk_sizes: &[usize],
    cols: &[ColumnPlan],
    element_offsets: &[i64],
    compression: u32,
    header_checksum: u64,
) -> Result<Vec<u8>> {
    let mut p = Vec::new();
    p.extend_from_slice(&header_checksum.to_le_bytes());

    let mut summary = Vec::new();
    summary.extend_from_slice(&0u64.to_le_bytes()); // first entry
    summary.extend_from_slice(&(n_entries as u64).to_le_bytes()); // num entries (flags=0)
    p.extend_from_slice(&list_frame(&[record_frame(&summary)]));

    let mut column_frames: Vec<Vec<u8>> = Vec::with_capacity(cols.len());
    for (i, c) in cols.iter().enumerate() {
        check_page_limits(c.n, disk_sizes[i])?;
        let mut page = Vec::new();
        page.extend_from_slice(&(c.n as i32).to_le_bytes()); // positive: no checksum
        page.extend_from_slice(&(disk_sizes[i] as i32).to_le_bytes()); // on-disk locator size
        page.extend_from_slice(&(page_offsets[i] as u64).to_le_bytes()); // locator offset
        let mut body = Vec::new();
        body.extend_from_slice(&1u32.to_le_bytes()); // one page
        body.extend_from_slice(&page);
        body.extend_from_slice(&element_offsets[i].to_le_bytes()); // element offset
        body.extend_from_slice(&compression.to_le_bytes()); // compression settings
        let size = (8 + body.len()) as i64;
        let mut frame = (-size).to_le_bytes().to_vec();
        frame.extend_from_slice(&body);
        column_frames.push(frame);
    }
    let inner = list_frame(&column_frames); // over columns
    p.extend_from_slice(&list_frame(&[inner])); // over clusters

    Ok(envelope(0x03, &p))
}

fn build_footer(
    n_entries: u32,
    num_clusters: u32,
    page_list_offset: usize,
    page_list_len: usize,
    header_checksum: u64,
) -> Vec<u8> {
    build_footer_ext(
        n_entries,
        num_clusters,
        page_list_offset,
        page_list_len,
        header_checksum,
        &[],
        &[],
        &[],
    )
}

/// Like [`build_footer`] but with a non-empty schema-extension record: the late
/// `ext_fields` and their deferred `ext_cols` (each paired with its first-element
/// index) go into the header-extension record so readers merge them and back-fill.
#[allow(clippy::too_many_arguments)]
fn build_footer_ext(
    n_entries: u32,
    num_clusters: u32,
    page_list_offset: usize,
    page_list_len: usize,
    header_checksum: u64,
    ext_fields: &[FieldPlan],
    ext_cols: &[ColumnPlan],
    first_entries: &[i64],
) -> Vec<u8> {
    let mut p = Vec::new();
    p.extend_from_slice(&0i64.to_le_bytes()); // feature flags
    p.extend_from_slice(&header_checksum.to_le_bytes());

    // Header-extension record: late field list, late column list, then the two
    // empty trailers (alias columns, extra type info).
    let mut ext = Vec::new();
    let field_records: Vec<Vec<u8>> = ext_fields.iter().map(field_record).collect();
    ext.extend_from_slice(&list_frame(&field_records));
    let column_records: Vec<Vec<u8>> = ext_cols
        .iter()
        .zip(first_entries)
        .map(|(c, &first)| column_record(c, Some(first)))
        .collect();
    ext.extend_from_slice(&list_frame(&column_records));
    ext.extend_from_slice(&list_frame(&[])); // alias columns
    ext.extend_from_slice(&list_frame(&[])); // extra type info
    p.extend_from_slice(&record_frame(&ext));

    // One cluster group spanning every cluster; it links to the single page-list
    // envelope that details all clusters' pages.
    let mut cg = Vec::new();
    cg.extend_from_slice(&0u64.to_le_bytes()); // min entry
    cg.extend_from_slice(&(n_entries as u64).to_le_bytes()); // entry span
    cg.extend_from_slice(&num_clusters.to_le_bytes()); // num clusters
    cg.extend_from_slice(&(page_list_len as u64).to_le_bytes()); // envelope link: uncompressed len
    cg.extend_from_slice(&(page_list_len as i32).to_le_bytes()); // locator size
    cg.extend_from_slice(&(page_list_offset as u64).to_le_bytes()); // locator offset
    p.extend_from_slice(&list_frame(&[record_frame(&cg)]));

    // Linked attribute sets (RNTuple format >= 1.0.1.0); empty here.
    p.extend_from_slice(&list_frame(&[]));

    envelope(0x02, &p)
}

fn build_anchor(
    seek_header: usize,
    len_header: usize,
    seek_footer: usize,
    len_footer: usize,
) -> Vec<u8> {
    let mut fields = Vec::with_capacity(64);
    fields.extend_from_slice(&1u16.to_be_bytes()); // epoch
    fields.extend_from_slice(&0u16.to_be_bytes()); // major
    fields.extend_from_slice(&1u16.to_be_bytes()); // minor
    fields.extend_from_slice(&1u16.to_be_bytes()); // patch
    fields.extend_from_slice(&(seek_header as u64).to_be_bytes());
    fields.extend_from_slice(&(len_header as u64).to_be_bytes());
    fields.extend_from_slice(&(len_header as u64).to_be_bytes());
    fields.extend_from_slice(&(seek_footer as u64).to_be_bytes());
    fields.extend_from_slice(&(len_footer as u64).to_be_bytes());
    fields.extend_from_slice(&(len_footer as u64).to_be_bytes());
    fields.extend_from_slice(&0x4000_0000u64.to_be_bytes()); // max key size

    let checksum = xxhash_rust::xxh3::xxh3_64(&fields);

    let mut obj = Vec::new();
    obj.extend_from_slice(&((66u32) | K_BYTE_COUNT_MASK).to_be_bytes());
    obj.extend_from_slice(&2u16.to_be_bytes()); // class version
    obj.extend_from_slice(&fields);
    obj.extend_from_slice(&checksum.to_be_bytes());
    obj
}

/// Build a complete ROOT file containing one RNTuple named `ntuple_name`,
/// optionally compressing pages (`compression` is e.g. `Compression::None` or
/// `Compression::Zstd(5)`). Automatically switches to ROOT's 64-bit ("big")
/// container form once the file would exceed 2 GiB.
pub fn rntuple_file_bytes(
    file_name: &str,
    ntuple_name: &str,
    fields: &[Field],
    compression: Compression,
) -> Result<Vec<u8>> {
    rntuple_file_bytes_threshold(file_name, ntuple_name, fields, compression, KSTART_BIG_FILE)
}

/// Like [`rntuple_file_bytes`] but with the big-file threshold injectable for
/// tests. Writes the small (32-bit) container first; only if that already
/// exceeds the threshold does it rewrite in the big (64-bit) form, so the
/// expensive page bytes are copied twice only for genuinely >2 GiB files.
fn rntuple_file_bytes_threshold(
    file_name: &str,
    ntuple_name: &str,
    fields: &[Field],
    compression: Compression,
    threshold: u64,
) -> Result<Vec<u8>> {
    // A one-RNTuple file is just the general layout with a single root entry and
    // no subdirectories.
    rntuples_file_bytes_threshold(
        file_name,
        &[(ntuple_name, fields)],
        &[],
        compression,
        threshold,
    )
}

/// Write a zeroed file-header seek field (8 bytes big, 4 small).
fn seek_zero(w: &mut WBuffer, big: bool) {
    if big {
        w.be_u64(0);
    } else {
        w.be_u32(0);
    }
}

/// Write a known seek value as 8 bytes (big) or 4 bytes (small).
fn seek_value(w: &mut WBuffer, v: u64, big: bool) {
    if big {
        w.be_u64(v);
    } else {
        w.be_u32(v as u32);
    }
}

/// One RNTuple's fully-lowered, page-encoded payload, ready to place into a file
/// at any offset: the header envelope (and its checksum), each column's on-disk
/// page bytes, the column plans, and the entry count. Independent of where in the
/// file it lands, so the same prep serves both container-form passes.
struct NtuplePrep {
    name: String,
    header_env: Vec<u8>,
    header_checksum: u64,
    disk_pages: Vec<Vec<u8>>,
    disk_sizes: Vec<usize>,
    cols: Vec<ColumnPlan>,
    n_entries: u32,
    /// Set when this is a schema-extended RNTuple: the late field/column
    /// descriptors (placed in the footer's extension record) and the per-column
    /// element offsets. `cols`/`disk_pages` already include the late columns.
    ext: Option<ExtPrep>,
}

/// The schema-extension half of a [`NtuplePrep`]: the late field descriptors, the
/// boundary between header and late columns, and each column's first-element
/// offset (`0` for header columns, the late field's first entry for late ones).
struct ExtPrep {
    ext_fields: Vec<FieldPlan>,
    base_col_count: usize,
    first_entries: Vec<i64>,
    element_offsets: Vec<i64>,
}

/// Lower one RNTuple's fields and encode its pages (the placement-independent
/// work), shared by every multi-RNTuple file pass.
fn prep_ntuple(name: &str, fields: &[Field], compression: u32) -> Result<NtuplePrep> {
    let (field_plans, cols, n_entries) = lower(fields)?;
    let header_env = build_header(name, &field_plans, &cols);
    let header_checksum =
        u64::from_le_bytes(header_env[header_env.len() - 8..].try_into().unwrap());
    let disk_pages: Vec<Vec<u8>> = cols
        .iter()
        .map(|c| on_disk_page(&c.page, compression))
        .collect();
    let disk_sizes: Vec<usize> = disk_pages.iter().map(|p| p.len()).collect();
    Ok(NtuplePrep {
        name: name.to_string(),
        header_env,
        header_checksum,
        disk_pages,
        disk_sizes,
        cols,
        n_entries,
        ext: None,
    })
}

/// Lower a schema-extended RNTuple: `base_fields` go in the header, and each
/// `(first_entry, late_field)` becomes a deferred field in the footer's extension
/// record whose data covers entries `first_entry..n_entries`. The late field's
/// IDs continue the base's. Late fields must be scalar leaves (one column each)
/// and supply exactly `n_entries - first_entry` values.
fn prep_ntuple_extended(
    name: &str,
    base_fields: &[Field],
    late: &[(u64, Field)],
    compression: u32,
) -> Result<NtuplePrep> {
    let (base_fields_plan, base_cols, n_entries) = lower(base_fields)?;
    let header_env = build_header(name, &base_fields_plan, &base_cols);
    let header_checksum =
        u64::from_le_bytes(header_env[header_env.len() - 8..].try_into().unwrap());

    let base_col_count = base_cols.len();
    let mut cols = base_cols;
    let mut ext_fields = Vec::new();
    let mut first_entries = Vec::new();
    let mut element_offsets = vec![0i64; base_col_count];

    for (first_entry, field) in late {
        let (mut late_fp, mut late_cols, late_n) = lower(std::slice::from_ref(field))?;
        if late_cols.len() != 1 || late_fp.len() != 1 {
            return Err(Error::Format(format!(
                "late RNTuple field {:?} must be a scalar leaf",
                field.name
            )));
        }
        if *first_entry + late_n as u64 != u64::from(n_entries) {
            return Err(Error::Format(format!(
                "late RNTuple field {:?} has {late_n} values starting at entry {first_entry}, \
                 but the RNTuple has {n_entries} entries",
                field.name
            )));
        }
        // Continue the schema's field/column IDs past everything added so far.
        let id_base = (base_fields_plan.len() + ext_fields.len()) as u32;
        for f in &mut late_fp {
            f.parent_id += id_base;
        }
        for c in &mut late_cols {
            c.field_id += id_base;
        }
        ext_fields.append(&mut late_fp);
        first_entries.push(*first_entry as i64);
        element_offsets.push(*first_entry as i64);
        cols.append(&mut late_cols);
    }

    let disk_pages: Vec<Vec<u8>> = cols
        .iter()
        .map(|c| on_disk_page(&c.page, compression))
        .collect();
    let disk_sizes: Vec<usize> = disk_pages.iter().map(|p| p.len()).collect();

    Ok(NtuplePrep {
        name: name.to_string(),
        header_env,
        header_checksum,
        disk_pages,
        disk_sizes,
        cols,
        n_entries,
        ext: Some(ExtPrep {
            ext_fields,
            base_col_count,
            first_entries,
            element_offsets,
        }),
    })
}

/// Build a complete ROOT file with one schema-extended RNTuple: `base_fields` in
/// the header plus late `(first_entry, field)` fields in the footer's extension
/// record (see [`prep_ntuple_extended`]). Small container form only — schema
/// extension is an append-time operation, not a >2 GiB one.
fn extended_rntuple_file_bytes(
    file_name: &str,
    ntuple_name: &str,
    base_fields: &[Field],
    late: &[(u64, Field)],
    compression: Compression,
) -> Result<Vec<u8>> {
    let compression = compression.setting();
    let prep = prep_ntuple_extended(ntuple_name, base_fields, late, compression)?;
    rntuples_one_shot_pass(
        file_name,
        std::slice::from_ref(&prep),
        &[],
        compression,
        false,
    )
}

/// Write one RNTuple's blobs (header, pages, page list, footer) into `w`, then
/// its anchor `TKey` (parented to the directory at `seek_pdir`). Returns the
/// anchor key's `(seek, length)` for that directory's key list.
fn write_one_rntuple(
    w: &mut WBuffer,
    p: &NtuplePrep,
    seek_pdir: u64,
    compression: u32,
    big: bool,
) -> Result<(u64, u32)> {
    let seek_header = w.len();
    w.bytes(&p.header_env);
    let mut page_offsets = Vec::with_capacity(p.cols.len());
    for dp in &p.disk_pages {
        page_offsets.push(w.len());
        w.bytes(dp);
    }
    let page_list_offset = w.len();
    let (page_list_env, footer_env);
    if let Some(ext) = &p.ext {
        // Schema-extended: the late columns' pages start at their first element
        // index (the page list records the offset), and the late field/column
        // descriptors go in the footer's schema-extension record.
        page_list_env = build_page_list_offsets(
            p.n_entries,
            &page_offsets,
            &p.disk_sizes,
            &p.cols,
            &ext.element_offsets,
            compression,
            p.header_checksum,
        )?;
        w.bytes(&page_list_env);
        let seek = w.len();
        footer_env = build_footer_ext(
            p.n_entries,
            1,
            page_list_offset,
            page_list_env.len(),
            p.header_checksum,
            &ext.ext_fields,
            &p.cols[ext.base_col_count..],
            &ext.first_entries,
        );
        debug_assert_eq!(seek, w.len());
    } else {
        page_list_env = build_page_list(
            p.n_entries,
            &page_offsets,
            &p.disk_sizes,
            &p.cols,
            compression,
            p.header_checksum,
        )?;
        w.bytes(&page_list_env);
        footer_env = build_footer(
            p.n_entries,
            1,
            page_list_offset,
            page_list_env.len(),
            p.header_checksum,
        );
    }
    let seek_footer = w.len();
    w.bytes(&footer_env);

    let anchor_obj = build_anchor(
        seek_header,
        p.header_env.len(),
        seek_footer,
        footer_env.len(),
    );
    let anchor_seek = w.len() as u64;
    let anchor_len = anchor_obj.len() as u32;
    write_key_header_fmt(
        w,
        "ROOT::RNTuple",
        &p.name,
        "",
        anchor_len,
        anchor_len,
        anchor_seek,
        seek_pdir,
        1,
        big,
    );
    w.bytes(&anchor_obj);
    Ok((anchor_seek, anchor_len))
}

/// Write a format-aware `TDirectory` record (the body after a directory's name
/// key). Returns the `(nbytesKeys, seekKeys)` patch handles to fill in once that
/// directory's key list has been written.
fn write_dir_record_fmt(
    w: &mut WBuffer,
    seek_dir: u64,
    seek_parent: u64,
    nbytes_name: u32,
    big: bool,
) -> (Patch, Patch) {
    w.be_i16(if big { 1005 } else { 5 });
    w.be_u32(DATIME);
    w.be_u32(DATIME);
    let p_nbytes_keys = w.reserve(4);
    w.be_i32(nbytes_name as i32);
    seek_value(w, seek_dir, big); // fSeekDir
    seek_value(w, seek_parent, big); // fSeekParent
    let p_seek_keys = w.reserve(if big { 8 } else { 4 });
    w.be_u16(1);
    w.bytes(&[0u8; 16]);
    (p_nbytes_keys, p_seek_keys)
}

/// Total on-disk size of a `TDirectory` record (the `obj_len` of its key).
fn dir_record_total(big: bool) -> u32 {
    if big {
        60
    } else {
        48
    }
}

/// Write a directory's key list: a wrapping `TKey` whose payload is the entry
/// count then a `TKey` header per entry. `entries` are `(class, name, title,
/// obj_len, seek)`. Each entry's `title` must match the referenced key's title,
/// since its `KeyLen` (and so a reader's `seek_key + key_len` payload offset)
/// depends on it — a subdirectory entry carries the directory name as its title.
/// Returns `(seek, nbytes)`.
fn write_key_list_fmt(
    w: &mut WBuffer,
    dir_class: &str,
    dir_name: &str,
    dir_title: &str,
    seek_pdir: u64,
    entries: &[(String, String, String, u32, u64)],
    big: bool,
) -> (u64, u32) {
    let seek = w.len() as u64;
    let headers: usize = entries
        .iter()
        .map(|(class, name, title, _, _)| key_len_fmt(class, name, title, big) as usize)
        .sum();
    let obj_len = (4 + headers) as u32;
    write_key_header_fmt(
        w, dir_class, dir_name, dir_title, obj_len, obj_len, seek, seek_pdir, 1, big,
    );
    w.be_i32(entries.len() as i32);
    for (class, name, title, len, sk) in entries {
        write_key_header_fmt(w, class, name, title, *len, *len, *sk, seek_pdir, 1, big);
    }
    let nbytes = key_len_fmt(dir_class, dir_name, dir_title, big) as u32 + obj_len;
    (seek, nbytes)
}

/// Write a complete ROOT file holding several RNTuples in the root directory plus
/// one level of subdirectories, each holding its own RNTuples, in the small
/// (32-bit) or big (64-bit) container form.
fn rntuples_one_shot_pass(
    file_name: &str,
    root: &[NtuplePrep],
    dirs: &[(String, Vec<NtuplePrep>)],
    compression: u32,
    big: bool,
) -> Result<Vec<u8>> {
    let mut w = WBuffer::new();

    // --- File header (100 bytes). ---
    w.bytes(b"root");
    w.be_u32(if big {
        FILE_VERSION + 1_000_000
    } else {
        FILE_VERSION
    });
    w.be_u32(100);
    let p_end = w.reserve(if big { 8 } else { 4 });
    seek_zero(&mut w, big); // fSeekFree
    w.be_u32(0); // fNbytesFree
    w.be_u32(0); // nfree
    let p_nbytes_name = w.reserve(4);
    w.u8(if big { 8 } else { 4 });
    w.be_u32(compression);
    seek_zero(&mut w, big); // fSeekInfo
    w.be_u32(0); // fNbytesInfo
    w.be_u16(1);
    w.bytes(&[0u8; 16]);
    while w.len() < 100 {
        w.u8(0);
    }

    // --- Root directory name key + record (at fBEGIN = 100). ---
    let first_klen = key_len_fmt("TFile", file_name, "", big);
    let name_title_len = (1 + file_name.len()) + 1;
    let f_nbytes_name = first_klen as usize + name_title_len;
    let dir_record_len = if big { 42 } else { 30 };
    let first_obj_len = (name_title_len + dir_record_len + 18) as u32;
    write_key_header_fmt(
        &mut w,
        "TFile",
        file_name,
        "",
        first_obj_len,
        first_obj_len,
        100,
        0,
        1,
        big,
    );
    w.string(file_name);
    w.string("");
    let (p_root_nbk, p_root_sk) = write_dir_record_fmt(&mut w, 100, 0, f_nbytes_name as u32, big);

    // --- Root RNTuples. ---
    let mut root_entries: Vec<(String, String, String, u32, u64)> = Vec::new();
    for p in root {
        let (seek, len) = write_one_rntuple(&mut w, p, 100, compression, big)?;
        root_entries.push((
            "ROOT::RNTuple".to_string(),
            p.name.clone(),
            String::new(),
            len,
            seek,
        ));
    }

    // --- Subdirectories: each = TDirectory key + record, its RNTuples, key list. ---
    let dir_total = dir_record_total(big);
    let mut sub_seeks = Vec::with_capacity(dirs.len());
    for (name, ntuples) in dirs {
        let sub_klen = key_len_fmt("TDirectory", name, name, big);
        let s_sub = w.len() as u64;
        write_key_header_fmt(
            &mut w,
            "TDirectory",
            name,
            name,
            dir_total,
            dir_total,
            s_sub,
            100,
            1,
            big,
        );
        let (p_sub_nbk, p_sub_sk) = write_dir_record_fmt(&mut w, s_sub, 100, sub_klen as u32, big);

        let mut entries: Vec<(String, String, String, u32, u64)> = Vec::new();
        for p in ntuples {
            let (seek, len) = write_one_rntuple(&mut w, p, s_sub, compression, big)?;
            entries.push((
                "ROOT::RNTuple".to_string(),
                p.name.clone(),
                String::new(),
                len,
                seek,
            ));
        }
        let (sub_kl_seek, sub_kl_nbytes) =
            write_key_list_fmt(&mut w, "TDirectory", name, name, s_sub, &entries, big);
        w.patch_be_u32(p_sub_nbk, sub_kl_nbytes);
        if big {
            w.patch_be_u64(p_sub_sk, sub_kl_seek);
        } else {
            w.patch_be_u32(p_sub_sk, sub_kl_seek as u32);
        }
        sub_seeks.push(s_sub);
    }

    // --- Root key list: root RNTuples + a TDirectory entry per subdirectory. ---
    // A subdirectory's `TDirectory` key carries the directory name as its title,
    // so its key-list entry must too (its KeyLen sets a reader's payload offset).
    let mut entries = root_entries;
    for ((name, _), &seek) in dirs.iter().zip(&sub_seeks) {
        entries.push((
            "TDirectory".to_string(),
            name.clone(),
            name.clone(),
            dir_total,
            seek,
        ));
    }
    let (root_kl_seek, root_kl_nbytes) =
        write_key_list_fmt(&mut w, "TFile", file_name, "", 100, &entries, big);
    w.patch_be_u32(p_root_nbk, root_kl_nbytes);
    if big {
        w.patch_be_u64(p_root_sk, root_kl_seek);
    } else {
        w.patch_be_u32(p_root_sk, root_kl_seek as u32);
    }

    let f_end = w.len() as u64;
    if big {
        w.patch_be_u64(p_end, f_end);
    } else {
        w.patch_be_u32(p_end, f_end as u32);
    }
    w.patch_be_u32(p_nbytes_name, f_nbytes_name as u32);

    Ok(w.into_vec())
}

/// One RNTuple to write: its key name and its fields.
type NtupleSpec<'a> = (&'a str, &'a [Field]);
/// A subdirectory of RNTuples: its directory name and the RNTuples it holds.
type DirSpec<'a> = (&'a str, Vec<NtupleSpec<'a>>);

/// Build a ROOT file holding several RNTuples (`root`, in the top directory) plus
/// one level of subdirectories (`dirs`, each `(name, ntuples)`). Writes the small
/// (32-bit) container first and only re-emits the big (64-bit) form if it would
/// exceed 2 GiB.
fn rntuples_file_bytes_threshold(
    file_name: &str,
    root: &[NtupleSpec],
    dirs: &[DirSpec],
    compression: Compression,
    threshold: u64,
) -> Result<Vec<u8>> {
    let compression = compression.setting();
    let root_preps: Vec<NtuplePrep> = root
        .iter()
        .map(|(n, f)| prep_ntuple(n, f, compression))
        .collect::<Result<_>>()?;
    let dir_preps: Vec<(String, Vec<NtuplePrep>)> = dirs
        .iter()
        .map(|(name, ntuples)| {
            let preps = ntuples
                .iter()
                .map(|(n, f)| prep_ntuple(n, f, compression))
                .collect::<Result<Vec<_>>>()?;
            Ok(((*name).to_string(), preps))
        })
        .collect::<Result<_>>()?;

    let small = rntuples_one_shot_pass(file_name, &root_preps, &dir_preps, compression, false)?;
    if small.len() as u64 <= threshold {
        return Ok(small);
    }
    rntuples_one_shot_pass(file_name, &root_preps, &dir_preps, compression, true)
}

/// Write a one-RNTuple ROOT file to `path`, optionally compressing pages
/// (`compression` is e.g. `Compression::None` or `Compression::Zstd(5)`).
pub fn write_rntuple_file(
    path: impl AsRef<Path>,
    ntuple_name: &str,
    fields: &[Field],
    compression: Compression,
) -> Result<()> {
    let path = path.as_ref();
    let file_name = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("file.root");
    let bytes = rntuple_file_bytes(file_name, ntuple_name, fields, compression)?;
    std::fs::write(path, bytes)?;
    Ok(())
}

/// An RNTuple to write: a name and its [`Field`]s. The method-based,
/// write-side counterpart to the free [`write_rntuple_file`] function (and to
/// the read-only [`RNTuple`](crate::RNTuple)) — build one, then call
/// [`write_root`](Ntuple::write_root), mirroring `hist.write_root`:
///
/// ```no_run
/// use oxiroot_rntuple::{Field, Ntuple};
/// use oxiroot_io_core::Compression;
///
/// let fields = vec![
///     Field::f64("mass", vec![91.2, 125.0]),
///     Field::i32("charge", vec![0, -1]),
/// ];
/// Ntuple::new("events", fields).write_root("data.root", Compression::None)?;
/// # Ok::<(), oxiroot_io_core::error::Error>(())
/// ```
pub struct Ntuple {
    name: String,
    fields: Vec<Field>,
}

impl Ntuple {
    /// Create a writable RNTuple from a name and its fields.
    pub fn new(name: impl Into<String>, fields: Vec<Field>) -> Ntuple {
        Ntuple {
            name: name.into(),
            fields,
        }
    }

    /// The RNTuple's name (the in-file key).
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The RNTuple's fields.
    pub fn fields(&self) -> &[Field] {
        &self.fields
    }

    /// Write this RNTuple as a new one-RNTuple ROOT file, optionally compressing
    /// pages. The method form of [`write_rntuple_file`].
    pub fn write_root(&self, path: impl AsRef<Path>, compression: Compression) -> Result<()> {
        write_rntuple_file(path, &self.name, &self.fields, compression)
    }

    /// The complete ROOT-file bytes for this RNTuple (the method form of
    /// [`rntuple_file_bytes`]); `file_name` is the `TFile` name recorded in the
    /// file header.
    pub fn to_root_bytes(&self, file_name: &str, compression: Compression) -> Result<Vec<u8>> {
        rntuple_file_bytes(file_name, &self.name, &self.fields, compression)
    }

    /// Write this RNTuple **with a schema late extension**: this RNTuple's fields
    /// go in the header, and each `(first_entry, field)` in `late` is added as a
    /// deferred field via the footer's schema-extension record — its data covers
    /// entries `first_entry..N`, and the earlier entries default. ROOT reads the
    /// result as a schema-extended RNTuple (as if the field had been added
    /// mid-writing with a model updater). Late fields must be scalar leaves and
    /// supply exactly `N - first_entry` values.
    ///
    /// ```no_run
    /// use oxiroot_rntuple::{Field, Ntuple};
    /// use oxiroot_io_core::Compression;
    /// // 4 entries of `x`; `y` added late, covering only entries 2 and 3.
    /// Ntuple::new("events", vec![Field::i32("x", vec![1, 2, 3, 4])])
    ///     .write_root_extended(
    ///         "ext.root",
    ///         &[(2, Field::f32("y", vec![3.5, 4.5]))],
    ///         Compression::None,
    ///     )?;
    /// # Ok::<(), oxiroot_io_core::error::Error>(())
    /// ```
    pub fn write_root_extended(
        &self,
        path: impl AsRef<Path>,
        late: &[(u64, Field)],
        compression: Compression,
    ) -> Result<()> {
        let file_name = path
            .as_ref()
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("file.root")
            .to_string();
        let bytes =
            extended_rntuple_file_bytes(&file_name, &self.name, &self.fields, late, compression)?;
        std::fs::write(path, bytes)?;
        Ok(())
    }
}

/// A ROOT file holding **several RNTuples** — in the top directory and inside
/// `TDirectory` subdirectories. [`Ntuple::write_root`] writes the one-RNTuple,
/// root-directory case; this builder adds more than one per file and one level
/// of nesting. ROOT and uproot navigate the result natively.
///
/// ```no_run
/// use oxiroot_rntuple::{Field, NtupleFile, Ntuple};
/// use oxiroot_io_core::Compression;
/// NtupleFile::new()
///     .add(Ntuple::new("events", vec![Field::i32("x", vec![1, 2, 3])]))
///     .add(Ntuple::new("runs", vec![Field::i32("run", vec![7])]))
///     .dir("cal", |d| d.add(Ntuple::new("pedestals", vec![Field::f64("p", vec![0.5])])))
///     .write_root("multi.root", Compression::None)?;
/// # Ok::<(), oxiroot_io_core::error::Error>(())
/// ```
#[derive(Default)]
pub struct NtupleFile {
    root: Vec<Ntuple>,
    dirs: Vec<(String, Vec<Ntuple>)>,
}

impl NtupleFile {
    /// Start an empty file.
    pub fn new() -> NtupleFile {
        NtupleFile::default()
    }

    /// Add an RNTuple to the file's top directory.
    // `add` is the natural builder verb here; it is not the arithmetic `Add::add`.
    #[allow(clippy::should_implement_trait)]
    pub fn add(mut self, ntuple: Ntuple) -> NtupleFile {
        self.root.push(ntuple);
        self
    }

    /// Add a `TDirectory` named `name` holding the RNTuples added inside `build`.
    pub fn dir(
        mut self,
        name: impl Into<String>,
        build: impl FnOnce(NtupleDir) -> NtupleDir,
    ) -> NtupleFile {
        let dir = build(NtupleDir {
            ntuples: Vec::new(),
        });
        self.dirs.push((name.into(), dir.ntuples));
        self
    }

    /// Build the file bytes and write them to `path`.
    pub fn write_root(&self, path: impl AsRef<Path>, compression: Compression) -> Result<()> {
        let file_name = path
            .as_ref()
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("file.root")
            .to_string();
        let bytes = self.to_root_bytes(&file_name, compression)?;
        std::fs::write(path, bytes)?;
        Ok(())
    }

    /// The complete ROOT-file bytes; `file_name` is the `TFile` name in the header.
    pub fn to_root_bytes(&self, file_name: &str, compression: Compression) -> Result<Vec<u8>> {
        self.check_names()?;
        let root: Vec<NtupleSpec> = self
            .root
            .iter()
            .map(|n| (n.name.as_str(), n.fields.as_slice()))
            .collect();
        let dirs: Vec<DirSpec> = self
            .dirs
            .iter()
            .map(|(name, nts)| {
                (
                    name.as_str(),
                    nts.iter()
                        .map(|n| (n.name.as_str(), n.fields.as_slice()))
                        .collect(),
                )
            })
            .collect();
        rntuples_file_bytes_threshold(file_name, &root, &dirs, compression, KSTART_BIG_FILE)
    }

    /// Reject empty or clashing names before writing — loudly, rather than ROOT's
    /// silent shadow-on-read. Top-directory RNTuple names and subdirectory names
    /// share one namespace; each subdirectory's RNTuple names must be unique
    /// within it.
    fn check_names(&self) -> Result<()> {
        let mut top: Vec<&str> = Vec::new();
        for n in &self.root {
            top.push(n.name.as_str());
        }
        for (name, _) in &self.dirs {
            top.push(name.as_str());
        }
        check_unique(&top, "the top directory")?;
        for (name, nts) in &self.dirs {
            let names: Vec<&str> = nts.iter().map(|n| n.name.as_str()).collect();
            check_unique(&names, &format!("subdirectory {name:?}"))?;
        }
        Ok(())
    }
}

/// A subdirectory being built inside an [`NtupleFile`]; see [`NtupleFile::dir`].
#[must_use]
pub struct NtupleDir {
    ntuples: Vec<Ntuple>,
}

impl NtupleDir {
    /// Add an RNTuple to this subdirectory.
    // `add` is the natural builder verb here; it is not the arithmetic `Add::add`.
    #[allow(clippy::should_implement_trait)]
    pub fn add(mut self, ntuple: Ntuple) -> NtupleDir {
        self.ntuples.push(ntuple);
        self
    }
}

/// Error on any empty or duplicate `name` in `where_` (one directory level).
fn check_unique(names: &[&str], where_: &str) -> Result<()> {
    let mut seen = std::collections::HashSet::new();
    for &name in names {
        if name.is_empty() {
            return Err(Error::Format(format!("{where_} has an unnamed entry")));
        }
        if !seen.insert(name) {
            return Err(Error::Format(format!(
                "{where_} has two entries named {name:?}"
            )));
        }
    }
    Ok(())
}

// --- streaming, multi-cluster writer --------------------------------------

/// One page's location for the page list (one page per column per cluster).
struct PageRec {
    offset: u64,
    disk_size: usize,
    n_elements: u32,
    element_offset: i64,
}

/// A batch's full lowered schema identity. Every batch must produce an equal
/// one, otherwise its pages would be appended under the first batch's header and
/// silently mis-described. Compares field identity (name, type, parent, role) as
/// well as physical columns — `(type, bits)` alone would let a field rename or
/// reorder slip through.
#[derive(PartialEq, Eq)]
struct SchemaSig {
    /// `(name, type_name, parent_id, role)` per field, in lowered order.
    fields: Vec<(String, String, u32, u16)>,
    /// `(column type, bit width, owning field id)` per physical column.
    columns: Vec<(u16, u16, u32)>,
}

fn schema_sig(field_plans: &[FieldPlan], cols: &[ColumnPlan]) -> SchemaSig {
    SchemaSig {
        fields: field_plans
            .iter()
            .map(|f| (f.name.clone(), f.type_name.clone(), f.parent_id, f.role))
            .collect(),
        columns: cols
            .iter()
            .map(|c| (c.column_type as u16, c.bits, c.field_id))
            .collect(),
    }
}

/// Schema + header bookkeeping, fixed once the first batch defines it.
struct HeaderState {
    seek: u64,
    len: usize,
    checksum: u64,
    /// The lowered schema the first batch committed — must match every batch.
    signature: SchemaSig,
}

/// A streaming RNTuple writer: each [`write_batch`](RNTupleWriter::write_batch)
/// flushes one *cluster* to the sink, so a large dataset can be written one
/// chunk at a time without ever holding it all in memory. Call
/// [`finish`](RNTupleWriter::finish) to write the page list, footer, and anchor.
///
/// Handles the same field types as [`write_rntuple_file`] — scalars,
/// `std::string`, and `std::vector<T>` — writing each batch's collection/string
/// index offsets relative to its own cluster, as the format requires.
pub struct RNTupleWriter<W: Write + Seek> {
    sink: W,
    pos: u64,
    file_name: String,
    ntuple_name: String,
    compression: u32,
    /// Whether the container uses the 64-bit ("big") on-disk form. Fixed at
    /// construction (the header/directory widths are written immediately and
    /// cannot be widened in place afterwards).
    big: bool,
    // TFile pointers to patch once the layout is known.
    p_end: u64,
    p_nbytes_name: u64,
    p_dir_nbytes_keys: u64,
    p_dir_seek_keys: u64,
    f_nbytes_name: u32,
    // Set when the first batch defines the schema and writes the header.
    header: Option<HeaderState>,
    element_base: Vec<u64>,
    // Accumulated per-cluster metadata.
    total_entries: u64,
    summaries: Vec<(u64, u64)>,
    cluster_pages: Vec<Vec<PageRec>>,
}

impl RNTupleWriter<std::fs::File> {
    /// Create a streaming RNTuple file at `path` (32-bit container; supports up
    /// to 2 GiB — [`finish`](RNTupleWriter::finish) errors if that is exceeded).
    pub fn create(
        path: impl AsRef<Path>,
        ntuple_name: &str,
        compression: Compression,
    ) -> Result<Self> {
        Self::create_fmt(path, ntuple_name, compression, false)
    }

    /// Like [`create`](RNTupleWriter::create), but writes the 64-bit ("big")
    /// container form so the file may exceed 2 GiB. Use this when the streamed
    /// dataset is expected to be large; small files are still valid, just stored
    /// in the wider form.
    pub fn create_large(
        path: impl AsRef<Path>,
        ntuple_name: &str,
        compression: Compression,
    ) -> Result<Self> {
        Self::create_fmt(path, ntuple_name, compression, true)
    }

    fn create_fmt(
        path: impl AsRef<Path>,
        ntuple_name: &str,
        compression: Compression,
        big: bool,
    ) -> Result<Self> {
        let path = path.as_ref();
        let file_name = path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("file.root")
            .to_string();
        let file = std::fs::File::create(path)?;
        RNTupleWriter::new_fmt(file, &file_name, ntuple_name, compression, big)
    }
}

impl<W: Write + Seek> RNTupleWriter<W> {
    /// Begin writing into an arbitrary seekable sink (the TFile header and root
    /// directory are written immediately, with pointers to patch at the end).
    /// Small (32-bit) container — see [`new_large`](RNTupleWriter::new_large) for
    /// the >2 GiB form.
    pub fn new(
        sink: W,
        file_name: &str,
        ntuple_name: &str,
        compression: Compression,
    ) -> Result<Self> {
        Self::new_fmt(sink, file_name, ntuple_name, compression, false)
    }

    /// Like [`new`](RNTupleWriter::new), but writes the 64-bit ("big") container
    /// form so the streamed file may exceed 2 GiB.
    pub fn new_large(
        sink: W,
        file_name: &str,
        ntuple_name: &str,
        compression: Compression,
    ) -> Result<Self> {
        Self::new_fmt(sink, file_name, ntuple_name, compression, true)
    }

    fn new_fmt(
        mut sink: W,
        file_name: &str,
        ntuple_name: &str,
        compression: Compression,
        big: bool,
    ) -> Result<Self> {
        let compression = compression.setting();
        let mut w = WBuffer::new();

        // TFile header (100 bytes; fBEGIN is always 100). Record the offsets to
        // patch later — their widths follow `big`.
        w.bytes(b"root");
        w.be_u32(if big {
            FILE_VERSION + 1_000_000
        } else {
            FILE_VERSION
        });
        w.be_u32(100); // fBEGIN
        let p_end = w.len() as u64;
        seek_zero(&mut w, big); // fEND
        seek_zero(&mut w, big); // fSeekFree
        w.be_u32(0); // fNbytesFree
        w.be_u32(0); // nfree
        let p_nbytes_name = w.len() as u64;
        w.be_u32(0); // fNbytesName
        w.u8(if big { 8 } else { 4 }); // fUnits
        w.be_u32(compression); // fCompress
        seek_zero(&mut w, big); // fSeekInfo
        w.be_u32(0); // fNbytesInfo
        w.be_u16(1);
        w.bytes(&[0u8; 16]);
        while w.len() < 100 {
            w.u8(0);
        }

        // Root directory name key + TDirectory record (at fBEGIN = 100).
        let first_klen = key_len_fmt("TFile", file_name, "", big);
        let name_title_len = (1 + file_name.len()) + 1;
        let f_nbytes_name = (first_klen as usize + name_title_len) as u32;
        let dir_record_len = if big { 42 } else { 30 };
        let first_obj_len = (name_title_len + dir_record_len + 18) as u32;
        write_key_header_fmt(
            &mut w,
            "TFile",
            file_name,
            "",
            first_obj_len,
            first_obj_len,
            100,
            0,
            1,
            big,
        );
        w.string(file_name);
        w.string("");
        w.be_i16(if big { 1005 } else { 5 });
        w.be_u32(DATIME);
        w.be_u32(DATIME);
        let p_dir_nbytes_keys = w.len() as u64;
        w.be_u32(0); // fNbytesKeys
        w.be_i32(f_nbytes_name as i32);
        seek_value(&mut w, 100, big); // fSeekDir
        seek_value(&mut w, 0, big); // fSeekParent
        let p_dir_seek_keys = w.len() as u64;
        seek_zero(&mut w, big); // fSeekKeys
        w.be_u16(1);
        w.bytes(&[0u8; 16]);

        let prefix = w.into_vec();
        let pos = prefix.len() as u64;
        sink.write_all(&prefix)?;

        Ok(RNTupleWriter {
            sink,
            pos,
            file_name: file_name.to_string(),
            ntuple_name: ntuple_name.to_string(),
            compression,
            big,
            p_end,
            p_nbytes_name,
            p_dir_nbytes_keys,
            p_dir_seek_keys,
            f_nbytes_name,
            header: None,
            element_base: Vec::new(),
            total_entries: 0,
            summaries: Vec::new(),
            cluster_pages: Vec::new(),
        })
    }

    fn put(&mut self, bytes: &[u8]) -> io::Result<()> {
        self.sink.write_all(bytes)?;
        self.pos += bytes.len() as u64;
        Ok(())
    }

    fn patch(&mut self, offset: u64, value: u32) -> io::Result<()> {
        self.sink.seek(SeekFrom::Start(offset))?;
        self.sink.write_all(&value.to_be_bytes())
    }

    /// Patch a seek pointer in the on-disk container width: 8 bytes when `big`,
    /// 4 otherwise.
    fn patch_seek(&mut self, offset: u64, value: u64) -> io::Result<()> {
        self.sink.seek(SeekFrom::Start(offset))?;
        if self.big {
            self.sink.write_all(&value.to_be_bytes())
        } else {
            self.sink.write_all(&(value as u32).to_be_bytes())
        }
    }

    /// Append one cluster holding the entries in `fields`. All batches must share
    /// the same field schema; the first batch fixes it and writes the header.
    pub fn write_batch(&mut self, fields: &[Field]) -> Result<()> {
        if fields.is_empty() {
            return Ok(());
        }
        let (field_plans, cols, n_entries) = lower(fields)?;
        if n_entries == 0 {
            return Ok(());
        }
        let signature = schema_sig(&field_plans, &cols);

        match self.header.as_ref().map(|h| h.signature == signature) {
            Some(true) => {} // schema matches; header already written
            Some(false) => {
                return Err(Error::SchemaChanged {
                    detail: "this batch's field schema differs from the first batch's".into(),
                })
            }
            None => {
                // First batch fixes the schema and writes the header.
                let header_env = build_header(&self.ntuple_name, &field_plans, &cols);
                let checksum =
                    u64::from_le_bytes(header_env[header_env.len() - 8..].try_into().unwrap());
                let seek = self.pos;
                self.put(&header_env)?;
                self.element_base = vec![0u64; cols.len()];
                self.header = Some(HeaderState {
                    seek,
                    len: header_env.len(),
                    checksum,
                    signature,
                });
            }
        }

        let first_entry = self.total_entries;
        let mut recs = Vec::with_capacity(cols.len());
        for (i, c) in cols.iter().enumerate() {
            let disk = on_disk_page(&c.page, self.compression);
            let offset = self.pos;
            let element_offset = self.element_base[i] as i64;
            self.put(&disk)?;
            recs.push(PageRec {
                offset,
                disk_size: disk.len(),
                n_elements: c.n,
                element_offset,
            });
            self.element_base[i] += c.n as u64;
        }
        self.cluster_pages.push(recs);
        self.summaries.push((first_entry, n_entries as u64));
        self.total_entries += n_entries as u64;
        Ok(())
    }

    /// Finish the file: write the page list (all clusters), footer, anchor key,
    /// and key list, then patch the header pointers.
    pub fn finish(mut self) -> Result<()> {
        let header = self.header.take().ok_or_else(|| {
            Error::Format("RNTuple writer finished with no batches written".into())
        })?;
        let num_clusters = self.summaries.len() as u32;

        let page_list_offset = self.pos;
        let page_list_env = build_page_list_multi(
            &self.summaries,
            &self.cluster_pages,
            self.compression,
            header.checksum,
        )?;
        self.put(&page_list_env)?;

        let seek_footer = self.pos;
        let footer_env = build_footer(
            self.total_entries as u32,
            num_clusters,
            page_list_offset as usize,
            page_list_env.len(),
            header.checksum,
        );
        self.put(&footer_env)?;

        // A small (32-bit) container cannot address past 2 GiB. Fail loudly
        // rather than truncating the anchor / key-list seek pointers into a
        // corrupt file; the caller can re-run with `create_large`/`new_large`.
        if !self.big && self.pos > KSTART_BIG_FILE {
            return Err(Error::Format(format!(
                "streamed RNTuple reached {} bytes, over the 2 GiB limit of the 32-bit \
                 container — construct the writer with create_large / new_large for 64-bit",
                self.pos
            )));
        }

        let anchor_obj = build_anchor(
            header.seek as usize,
            header.len,
            seek_footer as usize,
            footer_env.len(),
        );
        let anchor_seek = self.pos;
        let anchor_len = anchor_obj.len() as u32;
        let mut kb = WBuffer::new();
        write_key_header_fmt(
            &mut kb,
            "ROOT::RNTuple",
            &self.ntuple_name,
            "",
            anchor_len,
            anchor_len,
            anchor_seek,
            100,
            1,
            self.big,
        );
        let kb = kb.into_vec();
        self.put(&kb)?;
        self.put(&anchor_obj)?;

        let keylist_seek = self.pos;
        let keylist_obj_len =
            4 + key_len_fmt("ROOT::RNTuple", &self.ntuple_name, "", self.big) as u32;
        let mut klb = WBuffer::new();
        write_key_header_fmt(
            &mut klb,
            "TFile",
            &self.file_name,
            "",
            keylist_obj_len,
            keylist_obj_len,
            keylist_seek,
            100,
            1,
            self.big,
        );
        klb.be_i32(1);
        write_key_header_fmt(
            &mut klb,
            "ROOT::RNTuple",
            &self.ntuple_name,
            "",
            anchor_len,
            anchor_len,
            anchor_seek,
            100,
            1,
            self.big,
        );
        let klb = klb.into_vec();
        self.put(&klb)?;
        let keylist_nbytes =
            key_len_fmt("TFile", &self.file_name, "", self.big) as u32 + keylist_obj_len;

        self.patch_seek(self.p_end, self.pos)?;
        self.patch(self.p_nbytes_name, self.f_nbytes_name)?;
        self.patch(self.p_dir_nbytes_keys, keylist_nbytes)?;
        self.patch_seek(self.p_dir_seek_keys, keylist_seek)?;
        self.sink.flush()?;
        Ok(())
    }
}

/// Build the page-list envelope for any number of clusters: cluster summaries,
/// then page locations nested clusters → columns → (one) page.
fn build_page_list_multi(
    summaries: &[(u64, u64)],
    cluster_pages: &[Vec<PageRec>],
    compression: u32,
    header_checksum: u64,
) -> Result<Vec<u8>> {
    let mut p = Vec::new();
    p.extend_from_slice(&header_checksum.to_le_bytes());

    let summary_frames: Vec<Vec<u8>> = summaries
        .iter()
        .map(|&(first, n)| {
            let mut s = Vec::new();
            s.extend_from_slice(&first.to_le_bytes());
            s.extend_from_slice(&n.to_le_bytes()); // high byte = flags (0)
            record_frame(&s)
        })
        .collect();
    p.extend_from_slice(&list_frame(&summary_frames));

    let mut cluster_frames: Vec<Vec<u8>> = Vec::with_capacity(cluster_pages.len());
    for cols in cluster_pages {
        let mut col_frames: Vec<Vec<u8>> = Vec::with_capacity(cols.len());
        for pr in cols {
            check_page_limits(pr.n_elements, pr.disk_size)?;
            let mut page = Vec::new();
            page.extend_from_slice(&(pr.n_elements as i32).to_le_bytes()); // no checksum
            page.extend_from_slice(&(pr.disk_size as i32).to_le_bytes()); // on-disk size
            page.extend_from_slice(&pr.offset.to_le_bytes()); // locator offset
            let mut body = Vec::new();
            body.extend_from_slice(&1u32.to_le_bytes()); // one page
            body.extend_from_slice(&page);
            body.extend_from_slice(&pr.element_offset.to_le_bytes());
            body.extend_from_slice(&compression.to_le_bytes());
            let size = (8 + body.len()) as i64;
            let mut frame = (-size).to_le_bytes().to_vec();
            frame.extend_from_slice(&body);
            col_frames.push(frame);
        }
        cluster_frames.push(list_frame(&col_frames));
    }
    p.extend_from_slice(&list_frame(&cluster_frames));

    Ok(envelope(0x03, &p))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{FieldValues, RNTuple};
    use oxiroot_io_core::RFile;

    #[test]
    fn one_shot_writes_and_reads_big_format() {
        let fields = vec![
            Field::i32("x", vec![1, 2, 3, 4]),
            Field::f64("y", vec![1.5, 2.5, 3.5, 4.5]),
        ];

        // Tiny file, but forced into the 64-bit container form via a low
        // threshold — it must still parse and yield identical values.
        let bytes =
            rntuple_file_bytes_threshold("t.root", "ntpl", &fields, Compression::None, 64).unwrap();
        // Also drop it to a temp file so an external reader (uproot) can be run
        // against the one-shot big-format output out of band.
        let _ = std::fs::write("/tmp/rootrs_oneshot_big.root", &bytes);
        let f = RFile::from_bytes(bytes).unwrap();
        assert!(f.header().is_big(), "forced into big-format container");
        let ntpl = RNTuple::open(&f, "ntpl").unwrap();
        assert_eq!(ntpl.num_entries(), 4);
        assert_eq!(
            ntpl.read_field(&f, "x").unwrap(),
            FieldValues::I32(vec![1, 2, 3, 4])
        );
        assert_eq!(
            ntpl.read_field(&f, "y").unwrap(),
            FieldValues::F64(vec![1.5, 2.5, 3.5, 4.5])
        );

        // The same data under the real threshold stays in small (32-bit) form.
        let small = rntuple_file_bytes("t.root", "ntpl", &fields, Compression::None).unwrap();
        let fs = RFile::from_bytes(small).unwrap();
        assert!(!fs.header().is_big());
        let ntpl = RNTuple::open(&fs, "ntpl").unwrap();
        assert_eq!(
            ntpl.read_field(&fs, "x").unwrap(),
            FieldValues::I32(vec![1, 2, 3, 4])
        );
    }

    #[test]
    fn page_limits_reject_oversized_counts_and_sizes() {
        // In range, including exactly at the boundary, is accepted.
        assert!(check_page_limits(1_000_000, 1_000_000).is_ok());
        assert!(check_page_limits(i32::MAX as u32, i32::MAX as usize).is_ok());
        // One element past the limit would flip the count's i32 sign bit, which
        // the format reads as "this page has a trailing checksum" — rejected.
        assert!(check_page_limits(i32::MAX as u32 + 1, 0).is_err());
        // One byte past the on-disk-size limit would flip the locator size sign.
        assert!(check_page_limits(0, i32::MAX as usize + 1).is_err());
    }
}
