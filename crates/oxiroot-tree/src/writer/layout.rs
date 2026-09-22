//! How a [`Branch`] lays out on disk: its leaf type, entry count, count
//! branch and the bytes of its baskets.

use super::branch::{vec_row_lengths, Branch, BranchKind};
use super::NESTED_NOT_WRITABLE;
use crate::value::BranchValues;

/// Whether a branch is a scalar, a fixed-size array, a variable-length (jagged)
/// array, a `std::vector<T>` (`TBranchElement`), or a string.
pub(super) enum Kind {
    Scalar,
    FixedArray(usize),
    Jagged,
    StlVector,
    Str,
}

/// The on-disk description of one leaf type.
pub(super) struct LeafInfo {
    /// `TLeafI`/`TLeafD`/`TLeafC`/… class name.
    pub(super) class: &'static str,
    /// Leaflist type code (`I`/`D`/`C`/…) used in the branch title.
    pub(super) code: char,
    /// Element byte width (the data stride; 1 for a `TLeafC` char).
    pub(super) size: i32,
    /// `fLenType` (the element width for numerics, 0 for `TLeafC`).
    pub(super) len_type: i32,
    /// `fIsUnsigned`.
    pub(super) unsigned: bool,
}

impl Branch {
    /// Number of entries (rows for arrays/strings).
    pub(super) fn n_entries(&self) -> u32 {
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
    pub(super) fn leaf(&self) -> LeafInfo {
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

    /// The largest value of an integer scalar branch — the `fMaximum` of a count
    /// leaf, which ROOT uses to size the read buffer of the array it counts —
    /// or, for a string branch, the longest length + 1. 0 for anything else.
    pub(super) fn leaf_max(&self) -> i64 {
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
    pub(super) fn str_len(&self) -> i32 {
        match &self.values {
            BranchValues::Str(v) => v.iter().map(|s| s.len()).max().unwrap_or(0) as i32 + 1,
            _ => 1,
        }
    }

    /// Elements per entry: `N` for a fixed array (from row 0), else 1 (scalar,
    /// string, and the jagged leaf — whose per-entry length is dynamic).
    pub(super) fn flen(&self) -> i32 {
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
    pub(super) fn is_jagged(&self) -> bool {
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

    pub(super) fn kind(&self) -> Kind {
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
    pub(super) fn count_name(&self) -> String {
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
    pub(super) fn count_branch(&self) -> Option<Branch> {
        self.jagged().then(|| Branch {
            name: self.count_name(),
            values: BranchValues::I32(self.row_lengths()),
            kind: BranchKind::Count,
        })
    }

    /// The `std::vector<T>` class name and `fCheckSum` ROOT writes for the
    /// element type (used when this is a `TBranchElement`). Checksums are the
    /// fixed values ROOT computes for each `vector<T>` specialization.
    pub(super) fn stl_info(&self) -> (&'static str, u32) {
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
    pub(super) fn basket_content(&self) -> (Vec<u8>, Option<Vec<u32>>) {
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
