//! Lowering fields to the on-disk model: one field record per (sub)field and
//! one column with its encoded page per column.

use oxiroot_io_core::{Error, Result};

use super::check::{check_fields, entry_count};
use super::classes::class_checksum;
use super::fields::{Column, Field};
use super::{
    FIELD_FLAG_ARRAY, FIELD_FLAG_CHECKSUM, ROLE_COLLECTION, ROLE_LEAF, ROLE_RECORD, ROLE_VARIANT,
};
use crate::column::ColumnType;

pub(super) struct FieldPlan {
    pub(super) name: String,
    pub(super) type_name: String,
    pub(super) parent_id: u32,
    pub(super) role: u16,
    /// Field flag bits (`FIELD_FLAG_*`): array/checksum.
    pub(super) flags: u16,
    /// `std::array`/`std::bitset` element count, when [`FIELD_FLAG_ARRAY`] is set.
    pub(super) array_size: Option<u64>,
    /// ROOT class checksum, when [`FIELD_FLAG_CHECKSUM`] is set (user classes).
    pub(super) type_checksum: Option<u32>,
}

pub(super) struct ColumnPlan {
    pub(super) column_type: ColumnType,
    pub(super) bits: u16,
    pub(super) field_id: u32,
    pub(super) page: Vec<u8>,
    pub(super) n: u32,
    pub(super) value_range: Option<(f64, f64)>,
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
pub(super) fn flatten<T: Clone>(v: &[Vec<T>]) -> (Vec<u64>, Vec<T>) {
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
pub(super) fn lower(fields: &[Field]) -> Result<(Vec<FieldPlan>, Vec<ColumnPlan>, u32)> {
    check_fields(fields)?;
    // The entry count is a 32-bit field on disk; reject an over-large batch
    // rather than silently wrapping it into a corrupt-but-accepted file.
    let n_rows = fields
        .iter()
        .find_map(|f| entry_count(&f.data))
        .unwrap_or(0);
    let n_entries = u32::try_from(n_rows).map_err(|_| {
        Error::InvalidInput(format!(
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
