//! Decoding branch data: from basket payloads to [`BranchValues`] columns.

use oxiroot_io_core::buffer::{RBuffer, K_BYTE_COUNT_MASK};
use oxiroot_io_core::error::{Error, Result};
use oxiroot_io_core::streamer_info::StreamerRegistry;
use oxiroot_io_core::FileReader;

use super::members::{walk_members, MemberVal, Members};
use super::parse::skip_object;
use super::{Branch, ObjectMember};
use crate::basket::Basket;
use crate::value::{BranchValues, LeafType};

/// Per-entry byte regions of a numeric branch (the shared shape behind the
/// jagged/array/scalar read paths), for the flat [`TreeReader::read_branch_flat`](super::TreeReader::read_branch_flat).
pub(super) fn entry_regions<'a>(branch: &Branch, baskets: &'a [Basket]) -> Vec<&'a [u8]> {
    if let Some((offset, stride)) = branch.leaflist {
        if stride == 0 {
            return Vec::new();
        }
        let width = branch.leaf_len.max(1) as usize * branch.leaf_type.size();
        let mut regions = Vec::new();
        for b in baskets {
            for chunk in b.entry_data().chunks_exact(stride) {
                let end = (offset + width).min(chunk.len());
                regions.push(chunk.get(offset..end).unwrap_or(&[]));
            }
        }
        return regions;
    }
    if baskets.iter().any(|b| b.entry_offsets.is_some()) {
        let mut regions = entry_regions_variable(baskets).unwrap_or_default();
        if branch.elem_header > 0 {
            for r in &mut regions {
                *r = &r[branch.elem_header.min(r.len())..];
            }
        }
        return regions;
    }
    // Fixed array or scalar: one chunk of `leaf_len` elements per entry.
    let stride = branch.leaf_len.max(1) as usize * branch.leaf_type.size();
    chunk_regions(baskets, stride)
}

/// How [`read_baskets`] decompresses: in order on the calling thread, or, only
/// when a caller asks for it, across rayon's global pool. Enabling the `rayon`
/// feature adds the parallel read methods; it never changes the serial ones.
#[derive(Clone, Copy)]
pub(super) enum Decode {
    Serial,
    #[cfg(feature = "rayon")]
    Parallel,
}

/// Read the requested baskets of `branch` (by index) and decompress them,
/// returning them in index order either way.
pub(super) fn read_baskets(
    file: &FileReader,
    branch: &Branch,
    indices: impl Iterator<Item = usize>,
    decode: Decode,
) -> Result<Vec<Basket>> {
    let seek_of = |i: usize| -> Result<u64> {
        branch.basket_seek.get(i).copied().ok_or_else(|| {
            Error::Format(format!("branch {:?}: missing basket {i} seek", branch.name))
        })
    };
    // `fBasketBytes` gives the exact on-disk record size (one exact fetch, no
    // over-read); `None` when the file omits it (the reader probes the header).
    let bytes_of = |i: usize| -> Option<usize> {
        branch
            .basket_bytes
            .get(i)
            .and_then(|&b| (b > 0).then_some(b as usize))
    };

    match decode {
        // Stops at the first error.
        Decode::Serial => indices
            .map(|i| Basket::read(file, seek_of(i)?, bytes_of(i)))
            .collect(),
        #[cfg(feature = "rayon")]
        Decode::Parallel => {
            use rayon::prelude::*;
            let indices: Vec<usize> = indices.collect();
            // par_iter().collect() into a Result preserves order and short-circuits
            // on the first error; the file source and branch are read-only (Sync).
            indices
                .into_par_iter()
                .map(|i| Basket::read(file, seek_of(i)?, bytes_of(i)))
                .collect()
        }
    }
}

/// Decode the given (contiguous, in-order) baskets of `branch` into per-entry
/// [`BranchValues`] — the shared body of [`TreeReader::read_branch`](super::TreeReader::read_branch) and
/// [`TreeReader::read_branch_range`](super::TreeReader::read_branch_range).
pub(super) fn decode_baskets(branch: &Branch, baskets: &[Basket]) -> Result<BranchValues> {
    // A synthesized `TBranchObject` member column: each entry is a whole object,
    // from which this member is extracted.
    if let Some(om) = &branch.object_member {
        return decode_object_members(branch.leaf_type, om, &entry_regions_variable(baskets)?);
    }

    // A leaflist leaf: take this leaf's bytes out of each entry's fixed stride
    // at its offset, then decode like a scalar / fixed array.
    if let Some((offset, stride)) = branch.leaflist {
        if stride == 0 {
            return decode_scalar(branch.leaf_type, &[]);
        }
        let width = branch.leaf_len.max(1) as usize * branch.leaf_type.size();
        let mut regions: Vec<&[u8]> = Vec::new();
        for b in baskets {
            for chunk in b.entry_data().chunks_exact(stride) {
                let end = (offset + width).min(chunk.len());
                regions.push(chunk.get(offset..end).unwrap_or(&[]));
            }
        }
        if branch.leaf_len > 1 {
            return decode_array(branch.leaf_type, &regions);
        }
        let bytes: Vec<u8> = regions.concat();
        return decode_scalar(branch.leaf_type, &bytes);
    }

    // A `std::vector<std::vector<T>>`: each entry's region is the outer vector's
    // 10-byte streamer header (whose last 4 bytes are the outer count) followed
    // by the inner vectors, each a `{count, elements}` block.
    if let Some(elem) = branch.nested_elem {
        return decode_nested_vec(&entry_regions_variable(baskets)?, elem);
    }

    let variable = baskets.iter().any(|b| b.entry_offsets.is_some());
    if branch.leaf_type == LeafType::Str {
        // A `std::vector<std::string>` branch carries a 10-byte streamer header
        // (with the element count); a plain `TLeafC` is one string per entry.
        if branch.elem_header > 0 {
            return decode_vec_strings(&entry_regions_variable(baskets)?);
        }
        return decode_strings(baskets);
    }
    if variable {
        let mut regions = entry_regions_variable(baskets)?;
        // A `std::vector` `TBranchElement` prefixes each entry with a streamer
        // header; strip it so only the element bytes remain.
        if branch.elem_header > 0 {
            for r in &mut regions {
                *r = &r[branch.elem_header.min(r.len())..];
            }
        }
        return decode_array(branch.leaf_type, &regions);
    }
    if branch.leaf_len > 1 {
        let stride = branch.leaf_len as usize * branch.leaf_type.size();
        return decode_array(branch.leaf_type, &chunk_regions(baskets, stride));
    }
    // Scalar: concatenate every basket's entry data, decode once.
    let mut bytes = Vec::new();
    let mut total = 0u64;
    for b in baskets {
        bytes.extend_from_slice(b.entry_data());
        total += b.n_entries as u64;
    }
    if bytes.len() != total as usize * branch.leaf_type.size() {
        return Err(Error::Format(format!(
            "branch {:?}: {} basket bytes for {total} {:?} entries",
            branch.name,
            bytes.len(),
            branch.leaf_type
        )));
    }
    decode_scalar(branch.leaf_type, &bytes)
}

/// Slice a decoded branch's values to the sub-range `[offset, offset + len)`
/// (clamped), preserving the variant.
pub(super) fn slice_values(bv: BranchValues, offset: usize, len: usize) -> BranchValues {
    use BranchValues::*;
    macro_rules! sl {
        ($variant:ident, $v:ident) => {{
            let end = offset.saturating_add(len).min($v.len());
            let start = offset.min(end);
            $variant($v[start..end].to_vec())
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
        Nested { offsets, items } => {
            // Slice the entry range, then slice `items` to the covered inner
            // vectors and rebase the offsets to start at 0.
            let n = offsets.len().saturating_sub(1);
            let end = offset.saturating_add(len).min(n);
            let start = offset.min(end);
            let item_start = offsets[start] as usize;
            let item_end = offsets[end] as usize;
            let base = offsets[start];
            let new_offsets = offsets[start..=end].iter().map(|&o| o - base).collect();
            BranchValues::Nested {
                offsets: new_offsets,
                items: Box::new(slice_values(*items, item_start, item_end - item_start)),
            }
        }
    }
}

/// Per-entry byte regions of a variable-length branch, from each basket's
/// `fEntryOffset` array.
fn entry_regions_variable(baskets: &[Basket]) -> Result<Vec<&[u8]>> {
    let mut regions = Vec::new();
    for b in baskets {
        let offs = b
            .entry_offsets
            .as_ref()
            .ok_or_else(|| Error::Format("variable branch basket missing fEntryOffset".into()))?;
        for i in 0..b.n_entries as usize {
            let (a, c) = (
                *offs.get(i).unwrap_or(&b.border),
                *offs.get(i + 1).unwrap_or(&b.border),
            );
            regions.push(b.data.get(a..c).unwrap_or(&[]));
        }
    }
    Ok(regions)
}

/// Decode a synthesized `TBranchObject` member column: each `region` is one
/// entry's whole object (`[className][version][members]`); walk the object class
/// and collect `om.member`, typed by `leaf_type`. Decoding is registry-free —
/// the object class's elements are carried on `om`, and base classes other than
/// `TObject`/`TNamed` degrade to a byte-count skip.
fn decode_object_members(
    leaf_type: LeafType,
    om: &ObjectMember,
    regions: &[&[u8]],
) -> Result<BranchValues> {
    let reg = StreamerRegistry::default();
    let mut values: Vec<MemberVal> = Vec::with_capacity(regions.len());
    for region in regions {
        let mut r = RBuffer::new(region);
        locate_object(&mut r);
        r.read_version()?; // the object's own version header
        let mut members = Members::new();
        let mut on_object = |_: &str, rb: &mut RBuffer| -> Result<()> { skip_object(rb) };
        walk_members(
            &mut r,
            &reg,
            &om.class_elements,
            &mut members,
            &mut on_object,
            "",
        )?;
        values.push(members.remove(&om.member).unwrap_or(MemberVal::Int(0)));
    }
    Ok(build_member_column(leaf_type, &values))
}

/// Skip an entry's leading class-name string (when present, for a "virtual"
/// `TLeafObject`) and any padding so the cursor sits at the object's version
/// header — recognised by the byte count's mask bit in its leading word.
fn locate_object(r: &mut RBuffer) {
    if r.remaining() < 4 {
        return;
    }
    let start = r.pos();
    let first = r.be_u32().unwrap_or(0);
    let _ = r.seek(start);
    // A small leading byte (a class-name length) rather than a masked byte count
    // means the class name is present; consume it.
    if first & K_BYTE_COUNT_MASK == 0 {
        let _ = r.string();
    }
    // Step over up to a few padding bytes to the object's byte-count word.
    for _ in 0..4 {
        if r.remaining() < 4 {
            return;
        }
        let p = r.pos();
        let word = r.be_u32().unwrap_or(0);
        let _ = r.seek(p);
        if word & K_BYTE_COUNT_MASK != 0 {
            return;
        }
        let _ = r.seek(p + 1);
    }
}

/// Build a `BranchValues` column of `leaf_type` from per-entry member values
/// (integers, floats, or strings extracted from each entry's object).
fn build_member_column(leaf_type: LeafType, vals: &[MemberVal]) -> BranchValues {
    use BranchValues as BV;
    let floats = |v: &[MemberVal]| -> Vec<f64> {
        v.iter()
            .map(|m| match m {
                MemberVal::Float(f) => *f,
                other => other.int() as f64,
            })
            .collect()
    };
    let ints: Vec<i64> = vals.iter().map(MemberVal::int).collect();
    match leaf_type {
        LeafType::Str => BV::Str(vals.iter().map(|m| m.str().to_string()).collect()),
        LeafType::Bool => BV::Bool(ints.iter().map(|&i| i != 0).collect()),
        LeafType::I8 => BV::I8(ints.iter().map(|&i| i as i8).collect()),
        LeafType::U8 => BV::U8(ints.iter().map(|&i| i as u8).collect()),
        LeafType::I16 => BV::I16(ints.iter().map(|&i| i as i16).collect()),
        LeafType::U16 => BV::U16(ints.iter().map(|&i| i as u16).collect()),
        LeafType::I32 => BV::I32(ints.iter().map(|&i| i as i32).collect()),
        LeafType::U32 => BV::U32(ints.iter().map(|&i| i as u32).collect()),
        LeafType::I64 => BV::I64(ints),
        LeafType::U64 => BV::U64(ints.iter().map(|&i| i as u64).collect()),
        LeafType::F32 => BV::F32(floats(vals).iter().map(|&f| f as f32).collect()),
        LeafType::F64 => BV::F64(floats(vals)),
    }
}

/// Per-entry byte regions of a fixed-size array branch: each basket's entry data
/// split into `stride`-byte chunks.
fn chunk_regions(baskets: &[Basket], stride: usize) -> Vec<&[u8]> {
    let mut regions = Vec::new();
    for b in baskets {
        if stride == 0 {
            continue;
        }
        for chunk in b.entry_data().chunks_exact(stride) {
            regions.push(chunk);
        }
    }
    regions
}

/// Decode `bytes` as a contiguous big-endian array of `leaf`-typed scalars.
pub(super) fn decode_scalar(leaf: LeafType, bytes: &[u8]) -> Result<BranchValues> {
    macro_rules! be {
        ($variant:ident, $ty:ty, $w:expr) => {{
            let mut v = Vec::with_capacity(bytes.len() / $w);
            for c in bytes.chunks_exact($w) {
                v.push(<$ty>::from_be_bytes(c.try_into().unwrap()));
            }
            BranchValues::$variant(v)
        }};
    }
    Ok(match leaf {
        LeafType::Bool => BranchValues::Bool(bytes.iter().map(|&b| b != 0).collect()),
        LeafType::I8 => BranchValues::I8(bytes.iter().map(|&b| b as i8).collect()),
        LeafType::U8 => BranchValues::U8(bytes.to_vec()),
        LeafType::I16 => be!(I16, i16, 2),
        LeafType::U16 => be!(U16, u16, 2),
        LeafType::I32 => be!(I32, i32, 4),
        LeafType::U32 => be!(U32, u32, 4),
        LeafType::I64 => be!(I64, i64, 8),
        LeafType::U64 => be!(U64, u64, 8),
        LeafType::F32 => be!(F32, f32, 4),
        LeafType::F64 => be!(F64, f64, 8),
        LeafType::Str => return Err(Error::Format("string branch decoded as scalar".into())),
    })
}

/// Decode each per-entry `region` into a vector of `leaf`-typed values, yielding
/// one inner vector per entry.
fn decode_array(leaf: LeafType, regions: &[&[u8]]) -> Result<BranchValues> {
    macro_rules! be {
        ($variant:ident, $ty:ty, $w:expr) => {{
            let mut out = Vec::with_capacity(regions.len());
            for r in regions {
                let mut g = Vec::with_capacity(r.len() / $w);
                for c in r.chunks_exact($w) {
                    g.push(<$ty>::from_be_bytes(c.try_into().unwrap()));
                }
                out.push(g);
            }
            BranchValues::$variant(out)
        }};
    }
    Ok(match leaf {
        LeafType::Bool => BranchValues::VecBool(
            regions
                .iter()
                .map(|r| r.iter().map(|&b| b != 0).collect())
                .collect(),
        ),
        LeafType::I8 => BranchValues::VecI8(
            regions
                .iter()
                .map(|r| r.iter().map(|&b| b as i8).collect())
                .collect(),
        ),
        LeafType::U8 => BranchValues::VecU8(regions.iter().map(|r| r.to_vec()).collect()),
        LeafType::I16 => be!(VecI16, i16, 2),
        LeafType::U16 => be!(VecU16, u16, 2),
        LeafType::I32 => be!(VecI32, i32, 4),
        LeafType::U32 => be!(VecU32, u32, 4),
        LeafType::I64 => be!(VecI64, i64, 8),
        LeafType::U64 => be!(VecU64, u64, 8),
        LeafType::F32 => be!(VecF32, f32, 4),
        LeafType::F64 => be!(VecF64, f64, 8),
        LeafType::Str => return Err(Error::Format("string branch decoded as array".into())),
    })
}

/// Decode `std::vector<std::vector<T>>` entries into a [`BranchValues::Nested`].
/// Each region is the outer vector's 10-byte streamer header (byte count +
/// version + outer count) followed by that many inner vectors, each a `u32`
/// element count then the contiguous big-endian elements. The flattened inner
/// vectors become `items` (a `Vec*` of element type `elem`), partitioned per
/// entry by the cumulative `offsets`.
fn decode_nested_vec(regions: &[&[u8]], elem: LeafType) -> Result<BranchValues> {
    let size = elem.size();
    let mut offsets = Vec::with_capacity(regions.len() + 1);
    offsets.push(0u64);
    let mut total = 0u64;
    let mut inner: Vec<&[u8]> = Vec::new();
    for region in regions {
        if region.len() >= 10 {
            // The outer count is the last 4 bytes of the 10-byte header.
            let outer = u32::from_be_bytes(region[6..10].try_into().unwrap()) as usize;
            let mut pos = 10;
            for _ in 0..outer {
                let count = u32::from_be_bytes(
                    region
                        .get(pos..pos + 4)
                        .ok_or_else(short_entry)?
                        .try_into()
                        .unwrap(),
                ) as usize;
                pos += 4;
                let nbytes = count * size;
                inner.push(region.get(pos..pos + nbytes).ok_or_else(short_entry)?);
                pos += nbytes;
            }
            total += outer as u64;
        }
        offsets.push(total);
    }
    Ok(BranchValues::Nested {
        offsets,
        items: Box::new(decode_array(elem, &inner)?),
    })
}

/// Error for a `std::vector<std::vector<T>>` entry that ends mid-element.
fn short_entry() -> Error {
    Error::Format("nested vector entry truncated".into())
}

/// Decode `std::vector<std::string>` entries: each region is a 10-byte streamer
/// header (byte count + version + `u32` element count) followed by that many
/// ROOT-encoded strings.
fn decode_vec_strings(regions: &[&[u8]]) -> Result<BranchValues> {
    let mut out = Vec::with_capacity(regions.len());
    for r in regions {
        let mut buf = RBuffer::new(r);
        let mut row = Vec::new();
        if buf.remaining() >= 10 {
            buf.be_u32()?; // byte count
            buf.be_u16()?; // version
            let count = buf.be_u32()? as usize;
            row.reserve(count);
            for _ in 0..count {
                row.push(buf.string()?);
            }
        }
        out.push(row);
    }
    Ok(BranchValues::VecStr(out))
}

/// Decode `TLeafC` baskets: each entry is one ROOT-encoded (length-prefixed)
/// string, read sequentially from the entry data.
fn decode_strings(baskets: &[Basket]) -> Result<BranchValues> {
    let mut out = Vec::new();
    for b in baskets {
        let mut r = RBuffer::new(b.entry_data());
        for _ in 0..b.n_entries {
            out.push(r.string()?);
        }
    }
    Ok(BranchValues::Str(out))
}
