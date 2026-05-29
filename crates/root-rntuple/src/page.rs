//! Reading and decoding RNTuple pages into typed column values.
//!
//! A page's on-disk bytes are (optionally) compressed and (optionally) followed
//! by an XXH3-64 checksum. Once decompressed, elements are decoded according to
//! the column type. This module covers the non-split fixed-width types; split,
//! zigzag/delta and truncated/quantized encodings are added as needed.

use root_io_core::error::{Error, Result};

use crate::column::ColumnType;
use crate::pagelist::PageInfo;

/// Decoded values of a physical column (concatenated across its pages).
#[derive(Debug, Clone, PartialEq)]
pub enum ColumnValues {
    /// `Bit` columns.
    Bits(Vec<bool>),
    /// `Char`/`Byte`/`Int8`/`UInt8` columns.
    Bytes(Vec<u8>),
    /// `Int32` columns.
    I32(Vec<i32>),
    /// `Index64`/`UInt64` columns.
    U64(Vec<u64>),
    /// `Real32` columns.
    F32(Vec<f32>),
    /// `Real64` columns.
    F64(Vec<f64>),
}

/// Uncompressed byte size of `n` elements stored at `bits` bits each.
fn uncompressed_size(bits: u16, n: usize) -> usize {
    (n * bits as usize).div_ceil(8)
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
    root_compress::decompress(compressed, uncompressed_size(bits, n))
        .map_err(|e| Error::Format(format!("decompressing RNTuple page: {e}")))
}

/// Decode all pages of one physical column (in order) into [`ColumnValues`].
pub fn read_column(
    data: &[u8],
    column_type: ColumnType,
    bits: u16,
    pages: &[PageInfo],
) -> Result<ColumnValues> {
    match column_type {
        ColumnType::Bit => {
            let mut out = Vec::new();
            for p in pages {
                let raw = read_page_bytes(data, p, bits)?;
                for i in 0..p.num_elements as usize {
                    out.push((raw[i >> 3] >> (i & 7)) & 1 == 1);
                }
            }
            Ok(ColumnValues::Bits(out))
        }
        ColumnType::Char | ColumnType::Byte | ColumnType::Int8 | ColumnType::UInt8 => {
            let mut out = Vec::new();
            for p in pages {
                let raw = read_page_bytes(data, p, bits)?;
                out.extend_from_slice(&raw[..p.num_elements as usize]);
            }
            Ok(ColumnValues::Bytes(out))
        }
        ColumnType::Int32 => decode_fixed(data, bits, pages, |c| {
            i32::from_le_bytes(c.try_into().unwrap())
        })
        .map(ColumnValues::I32),
        ColumnType::Real32 => decode_fixed(data, bits, pages, |c| {
            f32::from_le_bytes(c.try_into().unwrap())
        })
        .map(ColumnValues::F32),
        ColumnType::Real64 => decode_fixed(data, bits, pages, |c| {
            f64::from_le_bytes(c.try_into().unwrap())
        })
        .map(ColumnValues::F64),
        ColumnType::Index64 | ColumnType::UInt64 => decode_fixed(data, bits, pages, |c| {
            u64::from_le_bytes(c.try_into().unwrap())
        })
        .map(ColumnValues::U64),
        other => Err(Error::Format(format!(
            "decoding column type {other:?} is not implemented yet"
        ))),
    }
}

/// Decode fixed-width little-endian elements (`bits / 8` bytes each) from each
/// page via `convert`.
fn decode_fixed<T>(
    data: &[u8],
    bits: u16,
    pages: &[PageInfo],
    convert: impl Fn(&[u8]) -> T,
) -> Result<Vec<T>> {
    let width = bits as usize / 8;
    let mut out = Vec::new();
    for p in pages {
        let raw = read_page_bytes(data, p, bits)?;
        for chunk in raw.chunks_exact(width).take(p.num_elements as usize) {
            out.push(convert(chunk));
        }
    }
    Ok(out)
}
