//! ROOT's generic object protocol (`ReadObjectAny`).
//!
//! Embedded objects are prefixed with a byte count and a class tag that is
//! either a new class name (`kNewClassTag`), a back-reference to a
//! previously-named class (`kClassMask`), or a null/parent marker. Class
//! references are keyed by the object's buffer offset **plus the key length**
//! (ROOT reads objects from a key with `origin = -fKeylen`). This machinery is
//! shared by the streamer-info parser, `TObjArray` reading, and `TTree`.

use std::collections::HashMap;

use crate::buffer::{RBuffer, K_BYTE_COUNT_MASK};
use crate::error::{Error, Result};

const K_NEW_CLASS_TAG: u32 = 0xFFFF_FFFF;
const K_CLASS_MASK: u32 = 0x8000_0000;
const K_MAP_OFFSET: i64 = 2;

/// The outcome of reading an object's `{byte-count, class-tag}` header.
pub struct ObjHeader {
    /// The resolved class name, or `None` for a null/parent slot.
    pub class_name: Option<String>,
    /// Absolute buffer offset one past the object, when a byte count was present.
    pub end: Option<usize>,
}

/// Read a NUL-terminated class name (the `kNewClassTag` path).
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
/// Construct one per top-level object with that object's key length, then call
/// [`read_header`](TagReader::read_header) for each embedded object.
pub struct TagReader {
    refs: HashMap<i64, String>,
    seq: i64,
    keylen: i64,
}

impl TagReader {
    /// Begin resolving tags for an object read from a key whose header is
    /// `keylen` bytes (ROOT keys objects relative to `-keylen`).
    pub fn new(keylen: usize) -> Self {
        TagReader {
            refs: HashMap::new(),
            seq: 0,
            keylen: keylen as i64,
        }
    }

    /// Read a `ReadObjectAny`-style header, resolving the class name and the
    /// object's end offset. Leaves the cursor at the object body.
    pub fn read_header(&mut self, r: &mut RBuffer) -> Result<ObjHeader> {
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
            // Null (0), parent (1), or an object back-reference (unsupported).
            if tag == 0 || tag == 1 {
                return Ok(ObjHeader {
                    class_name: None,
                    end,
                });
            }
            Err(Error::Format(format!(
                "object back-reference (tag {tag}) is unsupported"
            )))
        } else if tag == K_NEW_CLASS_TAG {
            let classname = read_cstring(r)?;
            // Register the class at the tag's displacement (+ keylen + offset).
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
