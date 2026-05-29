//! Parsing the `TList<TStreamerInfo>` record at `fSeekInfo`.
//!
//! The streamer info describes every class stored in the file: its members,
//! types and versions. Reading it requires ROOT's generic object protocol
//! (`ReadObjectAny`), which prefixes embedded objects with a byte count and a
//! class tag that is either a new class name (`kNewClassTag`), a back-reference
//! to a previously named class (`kClassMask`), or an object reference. Class
//! references are keyed by the object's buffer offset **plus the key length**
//! (ROOT reads key objects with `origin = -fKeylen`).

use std::collections::HashMap;

use crate::buffer::{RBuffer, K_BYTE_COUNT_MASK};
use crate::error::{Error, Result};
use crate::streamer::{read_tnamed, read_tobject};

const K_NEW_CLASS_TAG: u32 = 0xFFFF_FFFF;
const K_CLASS_MASK: u32 = 0x8000_0000;
const K_MAP_OFFSET: i64 = 2;

/// One member (or base class) entry within a [`StreamerInfo`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StreamerElement {
    /// The streamer-element class (e.g. `TStreamerBase`, `TStreamerBasicType`).
    pub element_class: String,
    /// Member name (`fName`); the base class name for `TStreamerBase`.
    pub name: String,
    /// Member title/comment (`fTitle`).
    pub title: String,
    /// ROOT type code (`fType`).
    pub el_type: i32,
    /// In-memory size in bytes (`fSize`).
    pub size: i32,
    /// Fixed array length, or 0 for scalars (`fArrayLength`).
    pub array_length: i32,
    /// C++ type name (`fTypeName`).
    pub type_name: String,
    /// Base-class version, for `TStreamerBase` elements only (`fBaseVersion`).
    pub base_version: Option<i32>,
}

/// The streamer description of a single class.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StreamerInfo {
    /// The described class name.
    pub class_name: String,
    /// The class version (`fClassVersion`).
    pub class_version: i32,
    /// The class checksum (`fCheckSum`).
    pub checksum: u32,
    /// The class's members and base classes, in stream order.
    pub elements: Vec<StreamerElement>,
}

/// All streamer infos parsed from a file's `fSeekInfo` record.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StreamerRegistry {
    infos: Vec<StreamerInfo>,
}

impl StreamerRegistry {
    /// All streamer infos, in file order.
    pub fn infos(&self) -> &[StreamerInfo] {
        &self.infos
    }

    /// Look up the (first) streamer info for a class.
    pub fn get(&self, class: &str) -> Option<&StreamerInfo> {
        self.infos.iter().find(|i| i.class_name == class)
    }

    /// The names of all described classes.
    pub fn class_names(&self) -> Vec<&str> {
        self.infos.iter().map(|i| i.class_name.as_str()).collect()
    }
}

/// Read a NUL-terminated class name (used by the `kNewClassTag` path).
fn read_cstring(r: &mut RBuffer) -> Result<String> {
    let mut bytes = Vec::new();
    loop {
        let b = r.u8()?;
        if b == 0 {
            break;
        }
        bytes.push(b);
    }
    String::from_utf8(bytes).map_err(|_| Error::InvalidUtf8)
}

/// Resolves the generic object protocol's class tags, tracking back-references.
struct TagReader {
    refs: HashMap<i64, String>,
    seq: i64,
    keylen: i64,
}

/// The outcome of reading an object's `{byte-count, class-tag}` header.
struct ObjHeader {
    class_name: Option<String>,
    /// Absolute buffer offset one past the object, when a byte count was present.
    end: Option<usize>,
}

impl TagReader {
    fn new(keylen: usize) -> Self {
        TagReader {
            refs: HashMap::new(),
            seq: 0,
            keylen: keylen as i64,
        }
    }

    /// Read a `ReadObjectAny`-style header, resolving the class name and the
    /// object's end offset. Leaves the cursor at the object body.
    fn read_header(&mut self, r: &mut RBuffer) -> Result<ObjHeader> {
        let beg = r.pos();
        let bcnt_raw = r.be_u32()?;

        let (versioned, tag, bcnt) =
            if (bcnt_raw & K_BYTE_COUNT_MASK) == 0 || bcnt_raw == K_NEW_CLASS_TAG {
                (false, bcnt_raw, 0u32)
            } else {
                let tag = r.be_u32()?;
                (true, tag, bcnt_raw & !K_BYTE_COUNT_MASK)
            };
        let end = if versioned {
            Some(beg + 4 + bcnt as usize)
        } else {
            None
        };

        if tag & K_CLASS_MASK == 0 {
            // Null (0), parent (1), or an object back-reference (unsupported here).
            if tag == 0 || tag == 1 {
                return Ok(ObjHeader {
                    class_name: None,
                    end,
                });
            }
            Err(Error::Format(format!(
                "object back-reference (tag {tag}) is unsupported in streamer info"
            )))
        } else if tag == K_NEW_CLASS_TAG {
            let classname = read_cstring(r)?;
            // The class is registered at the tag's displacement (+ keylen + offset).
            if versioned {
                let start_disp = (beg + 4) as i64 + self.keylen;
                self.refs
                    .insert(start_disp + K_MAP_OFFSET, classname.clone());
            } else {
                self.seq += 1;
                self.refs.insert(self.seq, classname.clone());
            }
            Ok(ObjHeader {
                class_name: Some(classname),
                end,
            })
        } else {
            let refpos = (tag & !K_CLASS_MASK) as i64;
            let classname =
                self.refs.get(&refpos).cloned().ok_or_else(|| {
                    Error::Format(format!("unknown class-tag reference {refpos}"))
                })?;
            Ok(ObjHeader {
                class_name: Some(classname),
                end,
            })
        }
    }
}

/// Parse the `TList<TStreamerInfo>` object bytes (already decompressed).
/// `keylen` is the streamer-info key's header length (`fKeyLen`).
pub fn parse_streamer_info(object: &[u8], keylen: usize) -> Result<StreamerRegistry> {
    let mut r = RBuffer::new(object);
    let mut tags = TagReader::new(keylen);

    // The top-level object is a TList (known from the key's class).
    let list = r.read_version()?;
    read_tobject(&mut r)?;
    let _name = r.string()?;
    let count = r.be_i32()?.max(0);

    let mut infos = Vec::with_capacity(count as usize);
    for _ in 0..count {
        let header = tags.read_header(&mut r)?;
        if header.class_name.as_deref() == Some("TStreamerInfo") {
            infos.push(parse_one_info(&mut r, &mut tags)?);
        }
        // Align to the object end, then read this element's TList option string.
        if let Some(end) = header.end {
            r.seek(end)?;
        }
        let _option = r.string()?;
    }

    if let Some(end) = list.end {
        r.seek(end)?;
    }
    Ok(StreamerRegistry { infos })
}

/// Parse a `TStreamerInfo` body (after its `ReadObjectAny` header).
fn parse_one_info(r: &mut RBuffer, tags: &mut TagReader) -> Result<StreamerInfo> {
    let _version = r.read_version()?;
    let named = read_tnamed(r)?;
    let checksum = r.be_u32()?;
    let class_version = r.be_i32()?;

    // fElements is a TObjArray, read via the generic protocol.
    let header = tags.read_header(r)?;
    let elements = if header.class_name.as_deref() == Some("TObjArray") {
        parse_element_array(r, tags)?
    } else {
        Vec::new()
    };
    if let Some(end) = header.end {
        r.seek(end)?;
    }

    Ok(StreamerInfo {
        class_name: named.name,
        class_version,
        checksum,
        elements,
    })
}

/// Parse a `TObjArray` of `TStreamerElement`s (after its `ReadObjectAny` header).
fn parse_element_array(r: &mut RBuffer, tags: &mut TagReader) -> Result<Vec<StreamerElement>> {
    let _version = r.read_version()?;
    read_tobject(r)?;
    let _name = r.string()?;
    let size = r.be_i32()?.max(0);
    let _lower_bound = r.be_i32()?;

    let mut elements = Vec::with_capacity(size as usize);
    for _ in 0..size {
        let header = tags.read_header(r)?;
        if let Some(class) = header.class_name.clone() {
            elements.push(parse_one_element(r, &class)?);
        }
        if let Some(end) = header.end {
            r.seek(end)?;
        }
    }
    Ok(elements)
}

/// Parse a single `TStreamerElement` (or subclass) body. The element class's own
/// version wraps the `TStreamerElement` base, which carries the common members.
fn parse_one_element(r: &mut RBuffer, element_class: &str) -> Result<StreamerElement> {
    let _subclass_version = r.read_version()?;
    let element_base = r.read_version()?;

    let named = read_tnamed(r)?;
    let el_type = r.be_i32()?;
    let size = r.be_i32()?;
    let array_length = r.be_i32()?;
    let _array_dim = r.be_i32()?;
    for _ in 0..5 {
        let _max_index = r.be_i32()?; // fMaxIndex[5]
    }
    let type_name = r.string()?;

    // Seek past the rest of the TStreamerElement base (e.g. v>3 range data).
    if let Some(end) = element_base.end {
        r.seek(end)?;
    }

    // Subclass-specific tail we care about: TStreamerBase carries fBaseVersion.
    let base_version = if element_class == "TStreamerBase" {
        Some(r.be_i32()?)
    } else {
        None
    };

    Ok(StreamerElement {
        element_class: element_class.to_string(),
        name: named.name,
        title: named.title,
        el_type,
        size,
        array_length,
        type_name,
        base_version,
    })
}
