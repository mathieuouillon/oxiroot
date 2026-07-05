//! The generic, `TStreamerInfo`-driven object reader.
//!
//! Given a class name, its streamed bytes, and the streamer registry, this walks
//! the class's member layout (from the file's own `TStreamerInfo`) and produces a
//! dynamic [`Value`] tree — no compiled-in knowledge of the class required. It is
//! the engine behind rootls / rootprint-style inspection of arbitrary ROOT files.
//!
//! It reuses ROOT's on-disk primitives verbatim: [`RBuffer`] (`read_version`,
//! `string`, the big-endian readers), [`TagReader`] (`ReadObjectAny` — class tags
//! and back-references), and [`read_tobject`]/[`read_tnamed`]. The member dispatch
//! mirrors the adaptive walker that powers the `TTree` reader, generalised to emit
//! `Value`s and to descend into nested objects, pointers, and STL containers.
//!
//! When a member cannot be decoded (an unhandled `fType`, memberwise STL, a class
//! with no streamer info), it becomes a [`Value::Unsupported`] and — because
//! ROOT wraps objects and containers in byte counts — decoding resynchronises and
//! continues past it rather than failing.

use crate::buffer::RBuffer;
use crate::error::Result;
use crate::object::TagReader;
use crate::streamer::{read_tnamed, read_tobject, skip_versioned};
use crate::streamer_info::{StreamerElement, StreamerRegistry};
use crate::value::Value;

/// Guard against a cyclic or pathologically deep streamer graph.
const MAX_DEPTH: usize = 64;

/// Read an object of `class` from its (decompressed) streamed `bytes` into a
/// [`Value`] tree, using `reg` for the member layout. `keylen` is the source
/// key's `fKeyLen` (needed to resolve object back-references).
///
/// Never panics and never returns an `Err` for an undecodable member: an object
/// the reader cannot parse comes back as [`Value::Unsupported`].
pub fn read_object(reg: &StreamerRegistry, class: &str, bytes: &[u8], keylen: usize) -> Value {
    let mut r = RBuffer::new(bytes);
    let mut tags = TagReader::new(keylen);
    match read_named_object(reg, class, &mut r, &mut tags, 0) {
        Ok(v) => v,
        Err(e) => Value::Unsupported {
            class: class.to_string(),
            reason: e.to_string(),
        },
    }
}

/// Read a versioned object of `class` positioned at its own `{byte-count,
/// version}` header (the shape at the top of a key and after a `ReadObjectAny`
/// header). Dispatches the core ROOT collections specially; everything else is
/// driven by the streamer info.
fn read_named_object(
    reg: &StreamerRegistry,
    class: &str,
    r: &mut RBuffer,
    tags: &mut TagReader,
    depth: usize,
) -> Result<Value> {
    if depth > MAX_DEPTH {
        return Ok(unsupported(class, "maximum nesting depth exceeded"));
    }
    match class {
        // ROOT's core collections are not described by streamer info; read their
        // fixed on-disk shape (version, TObject, fName, count, elements).
        "TList" | "THashList" | "TObjArray" | "TClonesArray" | "TMap" | "TOrdCollection"
        | "TSortedList" => read_collection(reg, class, r, tags, depth),
        // TArray{C,S,I,L,F,D}: `{Int_t n}{n elements}`, no version header.
        _ if tarray_elem(class).is_some() => read_tarray(class, r),
        _ => match reg.get(class) {
            Some(info) => {
                let vh = r.read_version()?;
                let mut members = Vec::new();
                // Decode as far as we can; a mid-object failure yields a partial
                // object, and the byte count resynchronises the buffer.
                if let Err(e) = walk(reg, &info.elements, r, tags, &mut members, depth) {
                    members.push(("<error>".to_string(), unsupported(class, &e.to_string())));
                }
                if let Some(end) = vh.end {
                    r.seek(end)?;
                }
                Ok(Value::Object {
                    class: class.to_string(),
                    members,
                })
            }
            None => {
                // No layout: skip via the byte count if present, else give up.
                let vh = r.read_version()?;
                if let Some(end) = vh.end {
                    r.seek(end)?;
                    Ok(unsupported(
                        class,
                        "class has no TStreamerInfo in this file",
                    ))
                } else {
                    Ok(unsupported(
                        class,
                        "class has no TStreamerInfo and no byte count",
                    ))
                }
            }
        },
    }
}

/// Walk a class's streamer `elements` in order, decoding each member into
/// `members`. Base classes are flattened in place. On a member that cannot be
/// sized, it appends an `Unsupported` marker and stops — the caller then seeks to
/// the enclosing object's byte-count end, so the rest of the tree still decodes.
fn walk(
    reg: &StreamerRegistry,
    elements: &[StreamerElement],
    r: &mut RBuffer,
    tags: &mut TagReader,
    members: &mut Vec<(String, Value)>,
    depth: usize,
) -> Result<()> {
    // Bound the base-class recursion (walk -> read_base -> walk), which a cyclic
    // `TStreamerBase` graph in a hostile/corrupt file could otherwise drive to a
    // stack overflow. `read_named_object` guards the other recursion paths.
    if depth > MAX_DEPTH {
        members.push((
            "<depth>".to_string(),
            unsupported("", "maximum base-class nesting depth exceeded"),
        ));
        return Ok(());
    }
    for el in elements {
        if el.element_class == "TStreamerBase" {
            read_base(reg, &el.name, r, tags, members, depth)?;
            continue;
        }
        match read_member(reg, el, r, tags, members, depth)? {
            Some(v) => members.push((el.name.clone(), v)),
            None => {
                members.push((
                    el.name.clone(),
                    unsupported(
                        &el.type_name,
                        &format!("unhandled streamer type code {}", el.el_type),
                    ),
                ));
                break; // cannot size this member; the byte-count wrapper recovers
            }
        }
    }
    Ok(())
}

/// Read a base-class slot named `class`, flattening its members into `out`.
fn read_base(
    reg: &StreamerRegistry,
    class: &str,
    r: &mut RBuffer,
    tags: &mut TagReader,
    out: &mut Vec<(String, Value)>,
    depth: usize,
) -> Result<()> {
    match class {
        "TObject" => {
            read_tobject(r)?; // consumed; its fUniqueID/fBits are uninteresting
        }
        "TNamed" => {
            let named = read_tnamed(r)?;
            out.push(("fName".to_string(), Value::Str(named.name)));
            out.push(("fTitle".to_string(), Value::Str(named.title)));
        }
        // A `TArrayX` base (e.g. `TH1D : TArrayD` holds the bin contents): it
        // streams as `{Int_t fN}{fN elements}` with no version header.
        _ if tarray_elem(class).is_some() => {
            let base = tarray_elem(class).expect("checked");
            let n = (r.be_i32()?.max(0) as usize).min(r.remaining());
            let mut items = Vec::with_capacity(n);
            for _ in 0..n {
                items.push(read_basic(r, base)?.unwrap_or(Value::Null));
            }
            out.push(("fN".to_string(), Value::I32(n as i32)));
            out.push(("fArray".to_string(), Value::Array(items)));
        }
        _ => match reg.get(class) {
            Some(info) => {
                let vh = r.read_version()?;
                walk(reg, &info.elements, r, tags, out, depth + 1)?;
                if let Some(end) = vh.end {
                    r.seek(end)?;
                }
            }
            None => {
                skip_versioned(r)?;
                out.push((
                    class.to_string(),
                    unsupported(class, "base class has no TStreamerInfo"),
                ));
            }
        },
    }
    Ok(())
}

/// Decode a single non-base member. Returns `None` when the member cannot be
/// sized (the walk then stops and resynchronises on the byte count).
fn read_member(
    reg: &StreamerRegistry,
    el: &StreamerElement,
    r: &mut RBuffer,
    tags: &mut TagReader,
    read_so_far: &[(String, Value)],
    depth: usize,
) -> Result<Option<Value>> {
    let t = el.el_type;
    let v = match t {
        65 => Value::Str(r.string()?), // kTString
        66 => {
            read_tobject(r)?;
            Value::Object {
                class: "TObject".to_string(),
                members: Vec::new(),
            }
        }
        67 => {
            let n = read_tnamed(r)?;
            Value::Object {
                class: "TNamed".to_string(),
                members: vec![
                    ("fName".to_string(), Value::Str(n.name)),
                    ("fTitle".to_string(), Value::Str(n.title)),
                ],
            }
        }
        // Inline object by value (kObject / kAny).
        61 | 62 => read_named_object(reg, &el.type_name, r, tags, depth + 1)?,
        // Object pointer (kObjectp / kObjectP / kAnyp / kAnyP): ReadObjectAny.
        63 | 64 | 68 | 69 => read_ref_object(reg, r, tags, depth + 1)?,
        // STL container (kStreamer / kSTL / kSTLstring / legacy variants).
        300..=365 | 500 | 501 => read_stl(reg, el, r, tags, depth)?,
        // Fixed C array `T[fArrayLength]` (kOffsetL + basic).
        21..=39 => match read_fixed_array(el, t - 20, r)? {
            Some(v) => v,
            None => return Ok(None),
        },
        // Variable array `T* //[fCount]` (kOffsetP + basic).
        41..=59 => match read_ptr_array(el, t - 40, r, read_so_far)? {
            Some(v) => v,
            None => return Ok(None),
        },
        // Basic scalar.
        _ => match read_basic(r, t)? {
            Some(v) => v,
            None => return Ok(None),
        },
    };
    Ok(Some(v))
}

/// Read an element via `ReadObjectAny` (a pointer target or collection element).
fn read_ref_object(
    reg: &StreamerRegistry,
    r: &mut RBuffer,
    tags: &mut TagReader,
    depth: usize,
) -> Result<Value> {
    let header = tags.read_header(r)?;
    match (header.class_name, header.end) {
        (Some(class), end) => {
            // Decode the element; a failure is contained to this element and the
            // byte count (`end`) lets the collection continue.
            let v = match read_named_object(reg, &class, r, tags, depth) {
                Ok(v) => v,
                Err(e) => unsupported(&class, &e.to_string()),
            };
            if let Some(e) = end {
                r.seek(e)?;
            }
            Ok(v)
        }
        // Null, parent, or an unfollowable object back-reference.
        (None, Some(end)) => {
            r.seek(end)?;
            Ok(Value::Null)
        }
        (None, None) => Ok(Value::Null),
    }
}

/// Read a `TList`/`TObjArray`/`TMap`/… collection body into an object with a
/// `fName` and an `items` array (map entries become `{key, value}` objects).
fn read_collection(
    reg: &StreamerRegistry,
    class: &str,
    r: &mut RBuffer,
    tags: &mut TagReader,
    depth: usize,
) -> Result<Value> {
    let vh = r.read_version()?;
    read_tobject(r)?;
    let name = r.string()?; // fName
    let n = (r.be_i32()?.max(0) as usize).min(r.remaining());
    let has_lower_bound = matches!(class, "TObjArray" | "TClonesArray");
    if has_lower_bound {
        r.be_i32()?; // fLowerBound
    }
    let is_list = matches!(
        class,
        "TList" | "THashList" | "TSortedList" | "TOrdCollection"
    );
    let is_map = class == "TMap";

    let mut items = Vec::with_capacity(n);
    for _ in 0..n {
        let element = read_ref_object(reg, r, tags, depth + 1)?;
        if is_map {
            let value = read_ref_object(reg, r, tags, depth + 1)?;
            items.push(Value::Object {
                class: "pair".to_string(),
                members: vec![("key".to_string(), element), ("value".to_string(), value)],
            });
        } else {
            items.push(element);
        }
        if is_list {
            r.string()?; // the per-object option string (TList only)
        }
    }
    if let Some(end) = vh.end {
        r.seek(end)?;
    }
    Ok(Value::Object {
        class: class.to_string(),
        members: vec![
            ("fName".to_string(), Value::Str(name)),
            ("items".to_string(), Value::Array(items)),
        ],
    })
}

/// Read a `TArray{C,S,I,L,F,D}` member: `{Int_t n}{n elements}`, no version header.
fn read_tarray(class: &str, r: &mut RBuffer) -> Result<Value> {
    let base = tarray_elem(class).expect("caller checked");
    let n = (r.be_i32()?.max(0) as usize).min(r.remaining());
    let mut items = Vec::with_capacity(n);
    for _ in 0..n {
        items.push(read_basic(r, base)?.unwrap_or(Value::Null));
    }
    Ok(Value::Array(items))
}

/// Map a `TArrayX` class to the basic streamer type of its elements.
fn tarray_elem(class: &str) -> Option<i32> {
    Some(match class {
        "TArrayC" => 1,
        "TArrayS" => 2,
        "TArrayI" => 3,
        "TArrayL" | "TArrayL64" => 16,
        "TArrayF" => 5,
        "TArrayD" => 8,
        _ => return None,
    })
}

/// Read a fixed-length C array of `array_length` basic elements.
fn read_fixed_array(el: &StreamerElement, base: i32, r: &mut RBuffer) -> Result<Option<Value>> {
    let n = (el.array_length.max(0) as usize).min(r.remaining());
    let mut items = Vec::with_capacity(n);
    for _ in 0..n {
        match read_basic(r, base)? {
            Some(v) => items.push(v),
            None => return Ok(None),
        }
    }
    Ok(Some(Value::Array(items)))
}

/// Read a `T* //[fCount]` variable array: a presence byte, then `fCount` basic
/// elements (`fCount` comes from the counter member named by `count_name`).
fn read_ptr_array(
    el: &StreamerElement,
    base: i32,
    r: &mut RBuffer,
    read_so_far: &[(String, Value)],
) -> Result<Option<Value>> {
    let count = el
        .count_name
        .as_deref()
        .and_then(|c| read_so_far.iter().find(|(n, _)| n == c))
        .and_then(|(_, v)| v.as_i64())
        .unwrap_or(0)
        .max(0) as usize;
    r.u8()?; // is-array / presence marker
    let count = count.min(r.remaining());
    let mut items = Vec::with_capacity(count);
    for _ in 0..count {
        match read_basic(r, base)? {
            Some(v) => items.push(v),
            None => return Ok(None),
        }
    }
    Ok(Some(Value::Array(items)))
}

/// Read one basic-typed scalar. Returns `None` for a code that cannot be sized
/// (a bare `char*`, or an unknown code); `Double32`/`Float16` are consumed at
/// their on-disk width (a 4-byte float / a 2-byte packed half).
fn read_basic(r: &mut RBuffer, t: i32) -> Result<Option<Value>> {
    Ok(Some(match t {
        1 => Value::I8(r.i8()?),                 // kChar
        11 => Value::U8(r.u8()?),                // kUChar
        18 => Value::Bool(r.u8()? != 0),         // kBool
        2 => Value::I16(r.be_i16()?),            // kShort
        12 => Value::U16(r.be_u16()?),           // kUShort
        3 | 6 => Value::I32(r.be_i32()?),        // kInt / kCounter
        13 | 15 => Value::U32(r.be_u32()?),      // kUInt / kBits
        5 => Value::F32(r.be_f32()?),            // kFloat
        9 => Value::F64(f64::from(r.be_f32()?)), // kDouble32 (default: 4-byte float)
        8 => Value::F64(r.be_f64()?),            // kDouble
        4 | 16 => Value::I64(r.be_i64()?),       // kLong / kLong64
        14 | 17 => Value::U64(r.be_u64()?),      // kULong / kULong64
        19 => {
            r.be_u16()?; // kFloat16: packed half — consumed, not decoded
            unsupported("Float16_t", "packed half-float not decoded")
        }
        _ => return Ok(None),
    }))
}

/// Read an STL-container member (`std::vector`/`set`/`map`/…). Handles the common
/// contiguous shapes (`vector<basic>`, `vector<string>`, `vector<vector<basic>>`,
/// `set<basic>`); anything else (memberwise, `map`, containers of objects) is
/// consumed via its byte count and returned as `Unsupported`.
fn read_stl(
    reg: &StreamerRegistry,
    el: &StreamerElement,
    r: &mut RBuffer,
    _tags: &mut TagReader,
    _depth: usize,
) -> Result<Value> {
    let vh = r.read_version()?; // {byte-count, version} wrapper — our safety net
    let decoded = decode_stl(&el.type_name, r);
    // Always resynchronise on the container's byte count, whatever we decoded.
    let value = match (decoded, vh.end) {
        (Some(v), Some(end)) => {
            r.seek(end)?;
            v
        }
        (Some(v), None) => v,
        (None, Some(end)) => {
            r.seek(end)?;
            unsupported(&el.type_name, "unsupported STL container shape")
        }
        (None, None) => unsupported(&el.type_name, "unsupported STL container, no byte count"),
    };
    let _ = reg;
    Ok(value)
}

/// Decode the body of a supported STL container (cursor just past its version
/// header). `None` if the container shape is not one we handle.
fn decode_stl(type_name: &str, r: &mut RBuffer) -> Option<Value> {
    let inner = stl_elem(type_name)?;
    if let Some(base) = basic_type_of(inner) {
        // vector<basic> / set<basic>: {Int_t n}{n elements}.
        let n = (r.be_i32().ok()?.max(0) as usize).min(r.remaining());
        let mut items = Vec::with_capacity(n);
        for _ in 0..n {
            items.push(read_basic(r, base).ok()??);
        }
        Some(Value::Array(items))
    } else if inner == "string" || inner == "std::string" {
        // vector<string>: {Int_t n}{n ROOT strings}.
        let n = (r.be_i32().ok()?.max(0) as usize).min(r.remaining());
        let mut items = Vec::with_capacity(n);
        for _ in 0..n {
            items.push(Value::Str(r.string().ok()?));
        }
        Some(Value::Array(items))
    } else if let Some(base) = stl_elem(inner).and_then(basic_type_of) {
        // vector<vector<basic>>: outer {Int_t n}; each inner {Int_t m}{m elements}.
        let n = (r.be_i32().ok()?.max(0) as usize).min(r.remaining());
        let mut outer = Vec::with_capacity(n);
        for _ in 0..n {
            let m = (r.be_i32().ok()?.max(0) as usize).min(r.remaining());
            let mut inner_vec = Vec::with_capacity(m);
            for _ in 0..m {
                inner_vec.push(read_basic(r, base).ok()??);
            }
            outer.push(Value::Array(inner_vec));
        }
        Some(Value::Array(outer))
    } else {
        None
    }
}

/// Peel one `vector<…>`/`set<…>`/`list<…>` (bare or `std::`) layer, returning the
/// trimmed element type.
fn stl_elem(type_name: &str) -> Option<&str> {
    let t = type_name.trim();
    let t = t.strip_prefix("std::").unwrap_or(t);
    for container in ["vector", "set", "multiset", "list", "deque"] {
        if let Some(rest) = t.strip_prefix(container) {
            let rest = rest.trim_start();
            if let Some(inner) = rest.strip_prefix('<') {
                return Some(inner.trim_end().trim_end_matches('>').trim());
            }
        }
    }
    None
}

/// Map a C++ scalar type name to its basic streamer type code.
fn basic_type_of(type_name: &str) -> Option<i32> {
    Some(
        match type_name
            .trim()
            .strip_prefix("std::")
            .unwrap_or(type_name.trim())
        {
            "bool" | "Bool_t" => 18,
            "char" | "Char_t" | "int8_t" => 1,
            "unsigned char" | "UChar_t" | "uint8_t" | "Byte_t" => 11,
            "short" | "Short_t" | "int16_t" => 2,
            "unsigned short" | "UShort_t" | "uint16_t" => 12,
            "int" | "Int_t" | "int32_t" => 3,
            "unsigned int" | "unsigned" | "UInt_t" | "uint32_t" => 13,
            "float" | "Float_t" => 5,
            "double" | "Double_t" => 8,
            "long" | "Long_t" | "Long64_t" | "long long" | "int64_t" => 16,
            "unsigned long" | "ULong_t" | "ULong64_t" | "unsigned long long" | "uint64_t" => 17,
            _ => return None,
        },
    )
}

fn unsupported(class: &str, reason: &str) -> Value {
    Value::Unsupported {
        class: class.to_string(),
        reason: reason.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::streamer_info::StreamerInfo;

    fn base_of(base: &str) -> StreamerElement {
        StreamerElement {
            element_class: "TStreamerBase".to_string(),
            name: base.to_string(),
            title: String::new(),
            el_type: 0,
            size: 0,
            array_length: 0,
            type_name: base.to_string(),
            base_version: Some(1),
            count_name: None,
        }
    }

    /// Regression: a cyclic `TStreamerBase` graph (A bases B, B bases A) — as a
    /// corrupt/hostile file could carry — must terminate via the depth guard, not
    /// overflow the stack. Before the guard this aborted the process (SIGABRT).
    #[test]
    fn cyclic_base_graph_does_not_overflow() {
        let reg = StreamerRegistry::from_infos(vec![
            StreamerInfo {
                class_name: "A".to_string(),
                class_version: 1,
                checksum: 0,
                elements: vec![base_of("B")],
            },
            StreamerInfo {
                class_name: "B".to_string(),
                class_version: 1,
                checksum: 0,
                elements: vec![base_of("A")],
            },
        ]);
        // Many `{byte-count | mask, version}` headers to feed the descent.
        let mut bytes = Vec::new();
        for _ in 0..10_000 {
            bytes.extend_from_slice(&(0x4000_0002u32).to_be_bytes());
            bytes.extend_from_slice(&1u16.to_be_bytes());
        }
        let v = read_object(&reg, "A", &bytes, 0);
        assert!(matches!(
            v,
            Value::Object { .. } | Value::Unsupported { .. }
        ));
    }
}
