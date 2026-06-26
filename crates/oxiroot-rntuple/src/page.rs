//! Reading and decoding RNTuple pages into typed column values.
//!
//! A page's on-disk bytes are (optionally) compressed and (optionally) followed
//! by an XXH3-64 checksum. Once decompressed, multi-byte split columns are
//! byte-transposed back ("unsplit"), then signed-integer columns are
//! zigzag-decoded and index columns are delta-decoded (cumulative sum).

use oxiroot_io_core::error::{Error, Result};

use crate::column::ColumnType;
use crate::pagelist::PageInfo;

/// Decoded values of a physical column (concatenated across its pages).
#[derive(Debug, Clone, PartialEq)]
pub enum ColumnValues {
    /// `Bit` columns.
    Bits(Vec<bool>),
    /// `Char`/`Byte` columns (raw bytes, e.g. string characters).
    Bytes(Vec<u8>),
    /// 8-bit signed integer columns (`Int8`).
    I8(Vec<i8>),
    /// 8-bit unsigned integer columns (`UInt8`).
    U8(Vec<u8>),
    /// 16-bit signed integer columns (`Int16`, `SplitInt16`).
    I16(Vec<i16>),
    /// 16-bit unsigned integer columns (`UInt16`, `SplitUInt16`).
    U16(Vec<u16>),
    /// 32-bit signed integer columns (`Int32`, `SplitInt32`).
    I32(Vec<i32>),
    /// 64-bit signed integer columns (`Int64`, `SplitInt64`).
    I64(Vec<i64>),
    /// Unsigned 32-bit leaf columns (`UInt32`, `SplitUInt32`).
    U32(Vec<u32>),
    /// Unsigned 64-bit columns: `UInt64`, and decoded `Index*` offsets.
    U64(Vec<u64>),
    /// 32-bit float columns (`Real32`, `SplitReal32`).
    F32(Vec<f32>),
    /// 64-bit float columns (`Real64`, `SplitReal64`).
    F64(Vec<f64>),
}

/// Uncompressed byte size of `n` elements stored at `bits` bits each. Computed
/// in 64-bit to avoid overflow on 32-bit targets for adversarially large counts.
fn uncompressed_size(bits: u16, n: usize) -> usize {
    (n as u64 * bits as u64).div_ceil(8).min(usize::MAX as u64) as usize
}

/// Read and decompress one page, verifying its XXH3-64 checksum if present.
fn read_page_bytes(data: &[u8], page: &PageInfo, bits: u16) -> Result<Vec<u8>> {
    let off = page.locator.offset as usize;
    let size = page.locator.size as usize;
    let end = off
        .checked_add(size)
        .filter(|&e| e <= data.len())
        .ok_or_else(|| Error::Format("RNTuple page runs past end of file".into()))?;
    let compressed = &data[off..end];

    if page.has_checksum {
        let cs_end = end + 8;
        if cs_end > data.len() {
            return Err(Error::Format(
                "RNTuple page checksum past end of file".into(),
            ));
        }
        let stored = u64::from_le_bytes(data[end..cs_end].try_into().unwrap());
        let computed = xxhash_rust::xxh3::xxh3_64(compressed);
        if computed != stored {
            return Err(Error::Format(format!(
                "RNTuple page checksum mismatch: computed {computed:#018x}, stored {stored:#018x}"
            )));
        }
    }

    let n = page.num_elements as usize;
    oxiroot_compress::decompress(compressed, uncompressed_size(bits, n))
        .map_err(|e| Error::Format(format!("decompressing RNTuple page: {e}")))
}

/// Invert RNTuple "split" (byte-transposed) storage: byte `j` of element `i`
/// lives at `raw[j * n + i]`.
fn unsplit(raw: &[u8], n: usize, width: usize) -> Vec<u8> {
    let mut out = vec![0u8; n * width];
    for j in 0..width {
        let plane = &raw[j * n..(j + 1) * n];
        for (i, &b) in plane.iter().enumerate() {
            out[i * width + j] = b;
        }
    }
    out
}

/// Zigzag-decode an unsigned value to signed (`(u >> 1) ^ -(u & 1)`).
fn zigzag32(u: u32) -> i32 {
    ((u >> 1) as i32) ^ -((u & 1) as i32)
}

fn zigzag64(u: u64) -> i64 {
    ((u >> 1) as i64) ^ -((u & 1) as i64)
}

fn zigzag16(u: u16) -> i16 {
    ((u >> 1) as i16) ^ -((u & 1) as i16)
}

/// Delta-decode (cumulative sum) for index/offset columns.
fn delta_decode(deltas: Vec<u64>) -> Vec<u64> {
    let mut acc = 0u64;
    deltas
        .into_iter()
        .map(|d| {
            acc = acc.wrapping_add(d);
            acc
        })
        .collect()
}

/// Decode all pages of one physical column (in order) into [`ColumnValues`].
pub fn read_column(
    data: &[u8],
    column_type: ColumnType,
    bits: u16,
    pages: &[PageInfo],
) -> Result<ColumnValues> {
    use ColumnType::*;
    // Reject a header whose declared bit width contradicts the column type
    // before it is used to size pages — otherwise a hostile `bits_on_storage`
    // (e.g. 0 or 64 for an Int32 column) panics in the decode below.
    if let Some(expected) = column_type.storage_bits() {
        if bits != expected {
            return Err(Error::Format(format!(
                "column {column_type:?}: bits_on_storage {bits}, expected {expected}"
            )));
        }
    }
    match column_type {
        Bit => {
            let mut out = Vec::new();
            for p in pages {
                let raw = read_page_bytes(data, p, bits)?;
                for i in 0..p.num_elements as usize {
                    out.push((raw[i >> 3] >> (i & 7)) & 1 == 1);
                }
            }
            Ok(ColumnValues::Bits(out))
        }
        Char | Byte => {
            let mut out = Vec::new();
            for p in pages {
                let raw = read_page_bytes(data, p, bits)?;
                out.extend_from_slice(&raw[..p.num_elements as usize]);
            }
            Ok(ColumnValues::Bytes(out))
        }

        // 8-bit integers have no split form (transposing single bytes is a no-op).
        Int8 => Ok(ColumnValues::I8(fixed(data, bits, pages, false, le_i8)?)),
        UInt8 => Ok(ColumnValues::U8(fixed(data, bits, pages, false, le_u8)?)),
        Int16 => Ok(ColumnValues::I16(fixed(data, bits, pages, false, le_i16)?)),
        SplitInt16 => {
            let raw = fixed(data, bits, pages, true, le_u16)?;
            Ok(ColumnValues::I16(raw.into_iter().map(zigzag16).collect()))
        }
        UInt16 => Ok(ColumnValues::U16(fixed(data, bits, pages, false, le_u16)?)),
        SplitUInt16 => Ok(ColumnValues::U16(fixed(data, bits, pages, true, le_u16)?)),

        Int32 => Ok(ColumnValues::I32(fixed(data, bits, pages, false, le_i32)?)),
        SplitInt32 => {
            let raw = fixed(data, bits, pages, true, le_u32)?;
            Ok(ColumnValues::I32(raw.into_iter().map(zigzag32).collect()))
        }
        Int64 => Ok(ColumnValues::I64(fixed(data, bits, pages, false, le_i64)?)),
        SplitInt64 => {
            let raw = fixed(data, bits, pages, true, le_u64)?;
            Ok(ColumnValues::I64(raw.into_iter().map(zigzag64).collect()))
        }

        UInt64 => Ok(ColumnValues::U64(fixed(data, bits, pages, false, le_u64)?)),
        // Leaf uint32 columns keep their 32-bit identity; only the Index*
        // offset columns below widen to u64 (they index element data as usize).
        UInt32 => Ok(ColumnValues::U32(fixed(data, bits, pages, false, le_u32)?)),
        SplitUInt32 => Ok(ColumnValues::U32(fixed(data, bits, pages, true, le_u32)?)),
        Index64 => Ok(ColumnValues::U64(fixed(data, bits, pages, false, le_u64)?)),
        SplitIndex64 => {
            let raw = fixed(data, bits, pages, true, le_u64)?;
            Ok(ColumnValues::U64(delta_decode(raw)))
        }
        Index32 => {
            let raw = fixed(data, bits, pages, false, le_u32)?;
            Ok(ColumnValues::U64(raw.into_iter().map(u64::from).collect()))
        }
        SplitIndex32 => {
            let raw = fixed(data, bits, pages, true, le_u32)?;
            Ok(ColumnValues::U64(delta_decode(
                raw.into_iter().map(u64::from).collect(),
            )))
        }

        Real32 => Ok(ColumnValues::F32(fixed(data, bits, pages, false, le_f32)?)),
        SplitReal32 => Ok(ColumnValues::F32(fixed(data, bits, pages, true, le_f32)?)),
        Real64 => Ok(ColumnValues::F64(fixed(data, bits, pages, false, le_f64)?)),
        SplitReal64 => Ok(ColumnValues::F64(fixed(data, bits, pages, true, le_f64)?)),

        other => Err(Error::Format(format!(
            "decoding column type {other:?} is not implemented yet"
        ))),
    }
}

/// Decode fixed-width little-endian elements from each page, unsplitting first
/// when `split` is set.
fn fixed<T>(
    data: &[u8],
    bits: u16,
    pages: &[PageInfo],
    split: bool,
    convert: impl Fn(&[u8]) -> T,
) -> Result<Vec<T>> {
    let width = bits as usize / 8;
    let mut out = Vec::new();
    for p in pages {
        let raw = read_page_bytes(data, p, bits)?;
        let n = p.num_elements as usize;
        let bytes = if split { unsplit(&raw, n, width) } else { raw };
        for chunk in bytes.chunks_exact(width).take(n) {
            out.push(convert(chunk));
        }
    }
    Ok(out)
}

fn le_i8(c: &[u8]) -> i8 {
    c[0] as i8
}
fn le_u8(c: &[u8]) -> u8 {
    c[0]
}
fn le_i16(c: &[u8]) -> i16 {
    i16::from_le_bytes(c.try_into().unwrap())
}
fn le_u16(c: &[u8]) -> u16 {
    u16::from_le_bytes(c.try_into().unwrap())
}
fn le_i32(c: &[u8]) -> i32 {
    i32::from_le_bytes(c.try_into().unwrap())
}
fn le_u32(c: &[u8]) -> u32 {
    u32::from_le_bytes(c.try_into().unwrap())
}
fn le_i64(c: &[u8]) -> i64 {
    i64::from_le_bytes(c.try_into().unwrap())
}
fn le_u64(c: &[u8]) -> u64 {
    u64::from_le_bytes(c.try_into().unwrap())
}
fn le_f32(c: &[u8]) -> f32 {
    f32::from_le_bytes(c.try_into().unwrap())
}
fn le_f64(c: &[u8]) -> f64 {
    f64::from_le_bytes(c.try_into().unwrap())
}
