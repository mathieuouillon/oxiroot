//! RNTuple column types (the on-disk physical column encodings).

use oxiroot_io_core::error::{Error, Result};

/// A physical column type, per the RNTuple spec's type-code table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u16)]
#[non_exhaustive]
pub enum ColumnType {
    Bit = 0x00,
    Byte = 0x01,
    Char = 0x02,
    Int8 = 0x03,
    UInt8 = 0x04,
    Int16 = 0x05,
    UInt16 = 0x06,
    Int32 = 0x07,
    UInt32 = 0x08,
    Int64 = 0x09,
    UInt64 = 0x0A,
    Real16 = 0x0B,
    Real32 = 0x0C,
    Real64 = 0x0D,
    Index32 = 0x0E,
    Index64 = 0x0F,
    Switch = 0x10,
    SplitInt16 = 0x11,
    SplitUInt16 = 0x12,
    SplitInt32 = 0x13,
    SplitUInt32 = 0x14,
    SplitInt64 = 0x15,
    SplitUInt64 = 0x16,
    SplitReal16 = 0x17,
    SplitReal32 = 0x18,
    SplitReal64 = 0x19,
    SplitIndex32 = 0x1A,
    SplitIndex64 = 0x1B,
    Real32Trunc = 0x1C,
    Real32Quant = 0x1D,
}

impl ColumnType {
    /// Map an on-disk column-type code to a [`ColumnType`].
    pub fn from_code(code: u16) -> Result<ColumnType> {
        use ColumnType::*;
        Ok(match code {
            0x00 => Bit,
            0x01 => Byte,
            0x02 => Char,
            0x03 => Int8,
            0x04 => UInt8,
            0x05 => Int16,
            0x06 => UInt16,
            0x07 => Int32,
            0x08 => UInt32,
            0x09 => Int64,
            0x0A => UInt64,
            0x0B => Real16,
            0x0C => Real32,
            0x0D => Real64,
            0x0E => Index32,
            0x0F => Index64,
            0x10 => Switch,
            0x11 => SplitInt16,
            0x12 => SplitUInt16,
            0x13 => SplitInt32,
            0x14 => SplitUInt32,
            0x15 => SplitInt64,
            0x16 => SplitUInt64,
            0x17 => SplitReal16,
            0x18 => SplitReal32,
            0x19 => SplitReal64,
            0x1A => SplitIndex32,
            0x1B => SplitIndex64,
            0x1C => Real32Trunc,
            0x1D => Real32Quant,
            other => {
                return Err(Error::Format(format!(
                    "unknown RNTuple column type {other:#x}"
                )))
            }
        })
    }

    /// The fixed number of bits each element of this column occupies on storage,
    /// or `None` for variable-width types (`Switch`, truncated/quantized reals).
    /// Used to reject a header whose declared `bits_on_storage` contradicts the
    /// column type, which would otherwise mis-size pages and panic on decode.
    pub fn storage_bits(self) -> Option<u16> {
        use ColumnType::*;
        Some(match self {
            Bit => 1,
            Byte | Char | Int8 | UInt8 => 8,
            Int16 | UInt16 | Real16 | SplitInt16 | SplitUInt16 | SplitReal16 => 16,
            Int32 | UInt32 | Real32 | Index32 | SplitInt32 | SplitUInt32 | SplitReal32
            | SplitIndex32 => 32,
            Int64 | UInt64 | Real64 | Index64 | SplitInt64 | SplitUInt64 | SplitReal64
            | SplitIndex64 => 64,
            Switch | Real32Trunc | Real32Quant => return None,
        })
    }

    /// Whether this is an index/offset column (collection lengths).
    pub fn is_index(self) -> bool {
        matches!(
            self,
            ColumnType::Index32
                | ColumnType::Index64
                | ColumnType::SplitIndex32
                | ColumnType::SplitIndex64
        )
    }
}

impl TryFrom<u16> for ColumnType {
    type Error = Error;
    /// Parse an on-disk column-type code (the std-trait counterpart to
    /// [`from_code`](ColumnType::from_code), so callers can use `.try_into()`).
    fn try_from(code: u16) -> Result<ColumnType> {
        ColumnType::from_code(code)
    }
}
