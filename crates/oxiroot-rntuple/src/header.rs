//! The RNTuple header envelope: the schema (fields + columns).

use oxiroot_io_core::buffer::RBuffer;
use oxiroot_io_core::error::Result;

use crate::column::ColumnType;
use crate::envelope::{read_feature_flags, read_frame, read_string};

/// Field flag: fixed-size array (a trailing `u64` array size follows).
const FIELD_FLAG_ARRAY: u16 = 0x01;
/// Field flag: projected field (a trailing `u32` source field id follows).
const FIELD_FLAG_PROJECTED: u16 = 0x02;
/// Field flag: carries a ROOT type checksum (a trailing `u32` follows).
const FIELD_FLAG_CHECKSUM: u16 = 0x04;

/// Column flag: deferred column (a trailing `i64` first-element-index follows).
const COLUMN_FLAG_DEFERRED: u16 = 0x01;
/// Column flag: ranged column (trailing `f64` min and max follow).
const COLUMN_FLAG_RANGE: u16 = 0x02;

/// Structural role of a field within the schema.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum StructRole {
    /// A leaf field (scalar, or a string).
    Leaf,
    /// A collection parent (e.g. `std::vector<T>`).
    Collection,
    /// A record parent (a struct of sub-fields).
    Record,
    /// A variant parent.
    Variant,
    /// Streamed via a ROOT streamer.
    Streamer,
    /// An unrecognized role code.
    Unknown(u16),
}

impl StructRole {
    fn from_code(code: u16) -> StructRole {
        match code {
            0 => StructRole::Leaf,
            1 => StructRole::Collection,
            2 => StructRole::Record,
            3 => StructRole::Variant,
            4 => StructRole::Streamer,
            other => StructRole::Unknown(other),
        }
    }
}

/// A field descriptor. Its field id is its index in [`Header::fields`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldDescriptor {
    /// Field version.
    pub field_version: u32,
    /// Type version.
    pub type_version: u32,
    /// Parent field id (equal to own id for top-level fields).
    pub parent_field_id: u32,
    /// Structural role.
    pub struct_role: StructRole,
    /// Raw flag bits.
    pub flags: u16,
    /// Field name.
    pub name: String,
    /// C++ type name.
    pub type_name: String,
    /// Type alias.
    pub type_alias: String,
    /// Description.
    pub description: String,
    /// Fixed array size, for repetitive fields.
    pub array_size: Option<u64>,
}

/// A column descriptor. Its column index is its position in [`Header::columns`].
#[derive(Debug, Clone, PartialEq)]
pub struct ColumnDescriptor {
    /// Physical column type.
    pub column_type: ColumnType,
    /// Bits per stored element.
    pub bits_on_storage: u16,
    /// The field this column belongs to.
    pub field_id: u32,
    /// Raw flag bits.
    pub flags: u16,
    /// Representation index (for multi-representation columns).
    pub representation_index: u16,
    /// First element index, for deferred columns.
    pub first_element_index: Option<i64>,
    /// Value range (min, max), for ranged columns.
    pub value_range: Option<(f64, f64)>,
}

/// The parsed header: the RNTuple's name/description and full schema.
#[derive(Debug, Clone, PartialEq)]
pub struct Header {
    /// RNTuple name.
    pub name: String,
    /// RNTuple description.
    pub description: String,
    /// Writer/library identifier.
    pub writer: String,
    /// Field descriptors (indexed by field id).
    pub fields: Vec<FieldDescriptor>,
    /// Column descriptors (indexed by column index).
    pub columns: Vec<ColumnDescriptor>,
}

impl Header {
    /// Parse the header from the envelope payload (after the 8-byte type/length
    /// word, before the checksum).
    pub fn parse(payload: &[u8]) -> Result<Header> {
        let mut r = RBuffer::new(payload);

        read_feature_flags(&mut r)?;
        let name = read_string(&mut r)?;
        let description = read_string(&mut r)?;
        let writer = read_string(&mut r)?;

        let fields = read_field_list(&mut r)?;
        let columns = read_column_list(&mut r)?;
        // (Alias columns and extra type info follow; not needed yet.)

        Ok(Header {
            name,
            description,
            writer,
            fields,
            columns,
        })
    }
}

fn read_field_list(r: &mut RBuffer) -> Result<Vec<FieldDescriptor>> {
    let list = read_frame(r)?;
    let mut fields = Vec::with_capacity((list.n_items as usize).min(r.remaining()));
    for _ in 0..list.n_items {
        let frame = read_frame(r)?;

        let field_version = r.le_u32()?;
        let type_version = r.le_u32()?;
        let parent_field_id = r.le_u32()?;
        let struct_role = StructRole::from_code(r.le_u16()?);
        let flags = r.le_u16()?;
        let name = read_string(r)?;
        let type_name = read_string(r)?;
        let type_alias = read_string(r)?;
        let description = read_string(r)?;

        let array_size = if flags & FIELD_FLAG_ARRAY != 0 {
            Some(r.le_u64()?)
        } else {
            None
        };
        if flags & FIELD_FLAG_PROJECTED != 0 {
            let _source_field_id = r.le_u32()?;
        }
        if flags & FIELD_FLAG_CHECKSUM != 0 {
            let _type_checksum = r.le_u32()?;
        }

        r.seek(frame.end)?;
        fields.push(FieldDescriptor {
            field_version,
            type_version,
            parent_field_id,
            struct_role,
            flags,
            name,
            type_name,
            type_alias,
            description,
            array_size,
        });
    }
    r.seek(list.end)?;
    Ok(fields)
}

fn read_column_list(r: &mut RBuffer) -> Result<Vec<ColumnDescriptor>> {
    let list = read_frame(r)?;
    let mut columns = Vec::with_capacity((list.n_items as usize).min(r.remaining()));
    for _ in 0..list.n_items {
        let frame = read_frame(r)?;

        let column_type = ColumnType::from_code(r.le_u16()?)?;
        let bits_on_storage = r.le_u16()?;
        let field_id = r.le_u32()?;
        let flags = r.le_u16()?;
        let representation_index = r.le_u16()?;

        let first_element_index = if flags & COLUMN_FLAG_DEFERRED != 0 {
            Some(r.le_i64()?)
        } else {
            None
        };
        let value_range = if flags & COLUMN_FLAG_RANGE != 0 {
            Some((r.le_f64()?, r.le_f64()?))
        } else {
            None
        };

        r.seek(frame.end)?;
        columns.push(ColumnDescriptor {
            column_type,
            bits_on_storage,
            field_id,
            flags,
            representation_index,
            first_element_index,
            value_range,
        });
    }
    r.seek(list.end)?;
    Ok(columns)
}
