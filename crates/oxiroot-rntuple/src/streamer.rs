//! Decoding an RNTuple *streamer* field (the `kStreamer` structural role).
//!
//! Unlike a split record, a streamer field stores each entry's object as one
//! opaque blob — the bytes ROOT's `TBufferFile` produces from the class
//! streamer — in a single `Byte` column, with a per-entry offset (`Index`)
//! column. To read it back we interpret each blob with the class
//! `TStreamerInfo` (carried in the file's streamer info), member by member.
//!
//! Only flat classes of basic members and `std::string` are supported: no base
//! classes, nested objects, arrays, or STL collections beyond `std::string`.

use oxiroot_io_core::buffer::RBuffer;
use oxiroot_io_core::error::{Error, Result};
use oxiroot_io_core::streamer_info::{StreamerElement, StreamerInfo};

use crate::field::FieldValues;

/// Decode a streamer field into a struct-of-arrays [`FieldValues::Record`]: one
/// named column per member, in declaration order. `offsets` are the per-entry
/// cumulative blob end positions into `bytes`.
pub(crate) fn decode(info: &StreamerInfo, offsets: &[u64], bytes: &[u8]) -> Result<FieldValues> {
    let mut accs = info
        .elements
        .iter()
        .map(Acc::for_element)
        .collect::<Result<Vec<_>>>()?;

    let mut start = 0usize;
    for &end in offsets {
        let end = end as usize;
        let blob = bytes
            .get(start..end)
            .ok_or_else(|| Error::Format("streamer blob offset out of range".into()))?;
        let mut r = RBuffer::new(blob);
        read_object_header(&mut r)?;
        for (acc, el) in accs.iter_mut().zip(&info.elements) {
            acc.read_one(&mut r, el)?;
        }
        start = end;
    }

    let record = info
        .elements
        .iter()
        .zip(accs)
        .map(|(el, acc)| (el.name.clone(), acc.into_values()))
        .collect();
    Ok(FieldValues::Record(record))
}

/// Read a streamed object's `{byte count, version}` header. A version of `0`
/// signals a class identified by checksum (the common case for the plain
/// structs ROOT stores in streamer fields), so a 4-byte checksum follows.
fn read_object_header(r: &mut RBuffer) -> Result<()> {
    let _byte_count = r.be_u32()?;
    let version = r.be_u16()?;
    if version == 0 {
        let _checksum = r.be_u32()?;
    }
    Ok(())
}

/// One member's values across all entries, typed by the streamer element.
enum Acc {
    Bool(Vec<bool>),
    I8(Vec<i8>),
    U8(Vec<u8>),
    I16(Vec<i16>),
    U16(Vec<u16>),
    I32(Vec<i32>),
    U32(Vec<u32>),
    I64(Vec<i64>),
    U64(Vec<u64>),
    F32(Vec<f32>),
    F64(Vec<f64>),
    Str(Vec<String>),
}

impl Acc {
    /// Choose the accumulator for a streamer element's ROOT type code (`fType`).
    fn for_element(el: &StreamerElement) -> Result<Acc> {
        Ok(match el.el_type {
            18 => Acc::Bool(Vec::new()),            // Bool
            1 => Acc::I8(Vec::new()),               // Char
            11 => Acc::U8(Vec::new()),              // UChar
            2 => Acc::I16(Vec::new()),              // Short
            12 => Acc::U16(Vec::new()),             // UShort
            3 | 6 => Acc::I32(Vec::new()),          // Int / Counter
            13 | 15 => Acc::U32(Vec::new()),        // UInt / Bits
            4 | 16 => Acc::I64(Vec::new()),         // Long / Long64
            14 | 17 => Acc::U64(Vec::new()),        // ULong / ULong64
            5 => Acc::F32(Vec::new()),              // Float
            8 => Acc::F64(Vec::new()),              // Double
            65 | 365 | 500 => Acc::Str(Vec::new()), // TString / string / STLstring
            other => {
                return Err(Error::Format(format!(
                    "streamer member {:?} has unsupported type code {other} ({})",
                    el.name, el.type_name
                )))
            }
        })
    }

    /// Read this member's value for one entry from the blob cursor.
    fn read_one(&mut self, r: &mut RBuffer, el: &StreamerElement) -> Result<()> {
        match self {
            Acc::Bool(v) => v.push(r.u8()? != 0),
            Acc::I8(v) => v.push(r.i8()?),
            Acc::U8(v) => v.push(r.u8()?),
            Acc::I16(v) => v.push(r.be_i16()?),
            Acc::U16(v) => v.push(r.be_u16()?),
            Acc::I32(v) => v.push(r.be_i32()?),
            Acc::U32(v) => v.push(r.be_u32()?),
            Acc::I64(v) => v.push(r.be_i64()?),
            Acc::U64(v) => v.push(r.be_u64()?),
            Acc::F32(v) => v.push(r.be_f32()?),
            Acc::F64(v) => v.push(r.be_f64()?),
            Acc::Str(v) => v.push(read_string_member(r, el)?),
        }
        Ok(())
    }

    fn into_values(self) -> FieldValues {
        match self {
            Acc::Bool(v) => FieldValues::Bool(v),
            Acc::I8(v) => FieldValues::I8(v),
            Acc::U8(v) => FieldValues::U8(v),
            Acc::I16(v) => FieldValues::I16(v),
            Acc::U16(v) => FieldValues::U16(v),
            Acc::I32(v) => FieldValues::I32(v),
            Acc::U32(v) => FieldValues::U32(v),
            Acc::I64(v) => FieldValues::I64(v),
            Acc::U64(v) => FieldValues::U64(v),
            Acc::F32(v) => FieldValues::F32(v),
            Acc::F64(v) => FieldValues::F64(v),
            Acc::Str(v) => FieldValues::Str(v),
        }
    }
}

/// Read a string member. A `std::string` (`STLstring`) is wrapped in its own
/// `{byte count, version}` header; a bare `TString` is not.
fn read_string_member(r: &mut RBuffer, el: &StreamerElement) -> Result<String> {
    if el.el_type == 65 {
        // TString: a length-prefixed string, no object header.
        r.string()
    } else {
        let _byte_count = r.be_u32()?;
        let _version = r.be_u16()?;
        r.string()
    }
}
