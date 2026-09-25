//! The generic, `TStreamerInfo`-driven object reader.
//!
//! Given a class name, its streamed bytes, and the streamer registry, this walks
//! the class's member layout (from the file's own `TStreamerInfo`) and produces a
//! dynamic [`Value`] tree — no compiled-in knowledge of the class required. It is
//! the engine behind rootls / rootprint-style inspection of arbitrary ROOT files.
//!
//! It reuses ROOT's on-disk primitives verbatim: [`RBuffer`] (`read_version`,
//! `string`, the big-endian readers), [`TagReader`] (`ReadObjectAny` — class tags
//! and back-references), and [`read_object_base`]/[`read_named`]. The member dispatch
//! mirrors the adaptive walker that powers the `TTree` reader, generalised to emit
//! `Value`s and to descend into nested objects, pointers, and STL containers.
//!
//! STL members are read from their C++ type name: `vector`, `set`, `list`,
//! `map` and `pair`, holding numbers, strings, `TArray`s, nested containers,
//! objects or pointers, written objectwise or memberwise. The name says what the
//! shape should be and the container's byte count says whether that was right,
//! so a shape read wrong is reported rather than passed off as values.
//!
//! When a member cannot be decoded (an unhandled `fType`, a class with no
//! streamer info), it becomes a [`Value::Unsupported`] and — because ROOT wraps
//! objects and containers in byte counts — decoding resynchronises and continues
//! past it rather than failing.

use crate::buffer::RBuffer;
use crate::error::{Error, Result};
use crate::object::TagReader;
use crate::streamer::{read_named, read_object_base};
use crate::streamer_info::{StreamerElement, StreamerRegistry};
use crate::value::Value;

/// Guard against a cyclic or pathologically deep streamer graph.
const MAX_DEPTH: usize = 64;

/// Read an object of `class` from its (decompressed) streamed `bytes` into a
/// [`Value`] tree, using `reg` for the member layout. `keylen` is the source
/// key's `fKeyLen` (needed to resolve object back-references).
///
/// Never panics and never returns an `Err` for an undecodable member: an object
/// the reader cannot parse comes back as [`Value::Unsupported`]. So does a member
/// it cannot decode (an unhandled `fType`, a class with no streamer info);
/// because ROOT wraps objects and containers in byte counts, decoding
/// resynchronises past it and continues.
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
        // Read the version first: the object is decoded with the description of
        // its own class version.
        _ => match r
            .read_version()
            .map(|vh| (reg.get_at(class, i32::from(vh.version)), vh))?
        {
            (Some(info), vh) => {
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
            (None, vh) => {
                // No layout: skip via the byte count if present, else give up.
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
            read_object_base(r)?; // consumed; its fUniqueID/fBits are uninteresting
        }
        "TNamed" => {
            let named = read_named(r)?;
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
        _ => match r
            .read_version()
            .map(|vh| (reg.get_at(class, i32::from(vh.version)), vh))?
        {
            (Some(info), vh) => {
                walk(reg, &info.elements, r, tags, out, depth + 1)?;
                if let Some(end) = vh.end {
                    r.seek(end)?;
                }
            }
            (None, vh) => {
                let end = vh.end.ok_or_else(|| {
                    Error::Unsupported(
                        "cannot skip a versioned object that carries no byte count".into(),
                    )
                })?;
                r.seek(end)?;
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
            read_object_base(r)?;
            Value::Object {
                class: "TObject".to_string(),
                members: Vec::new(),
            }
        }
        67 => {
            let n = read_named(r)?;
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
        300..=365 | 500 => read_stl(reg, el, r, tags, depth)?,
        // `T* //[fCount]` array of objects (kStreamLoop).
        501 => read_stream_loop(reg, el, r, tags, read_so_far, depth)?,
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
        // A reference to an object written earlier in the same object: name what
        // it points at. ROOT streams a shared object once and points at it from
        // everywhere else it appears (a `PolyHist`'s `fBins` points at the bins
        // its `fCells` grid holds in full), so the object is already in the tree
        // — repeating it here would blow a file up by whatever it is shared by.
        (None, end) if header.back_ref.is_some() => {
            let target = header.back_ref.expect("checked");
            if let Some(e) = end {
                r.seek(e)?;
            }
            Ok(Value::Ref {
                class: target.class_name,
            })
        }
        // Null, parent, or a reference to an object we did not read.
        (None, Some(end)) => {
            r.seek(end)?;
            Ok(Value::Null)
        }
        (None, None) => Ok(Value::Null),
    }
}

/// Read a `TList`/`TObjArray`/`ObjMap`/… collection body into an object with a
/// `fName` and an `items` array (map entries become `{key, value}` objects).
fn read_collection(
    reg: &StreamerRegistry,
    class: &str,
    r: &mut RBuffer,
    tags: &mut TagReader,
    depth: usize,
) -> Result<Value> {
    let vh = r.read_version()?;
    read_object_base(r)?;
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

/// The value of the `//[fCount]` counter member an element points at, read from
/// the members decoded so far (`0` when it is missing or not a number).
fn counter_value(el: &StreamerElement, read_so_far: &[(String, Value)]) -> usize {
    el.count_name
        .as_deref()
        .and_then(|c| read_so_far.iter().find(|(n, _)| n == c))
        .and_then(|(_, v)| v.as_i64())
        .unwrap_or(0)
        .max(0) as usize
}

/// Read a `T* //[fCount]` variable array: a presence byte, then `fCount` basic
/// elements (`fCount` comes from the counter member named by `count_name`).
fn read_ptr_array(
    el: &StreamerElement,
    base: i32,
    r: &mut RBuffer,
    read_so_far: &[(String, Value)],
) -> Result<Option<Value>> {
    let count = counter_value(el, read_so_far);
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

/// ROOT's `kStreamedMemberWise` bit in an STL container's version word: its
/// elements were streamed a member at a time — a column holding every element's
/// first member, then a column holding the second — rather than one whole
/// element after another.
const K_MEMBERWISE: u16 = 0x4000;

/// Read an STL-container member (`std::vector`/`set`/`map`/…), streamed
/// objectwise or memberwise.
///
/// The container's shape comes from the member's C++ type name, so a decode is
/// accepted only when it consumed exactly the bytes the container's byte count
/// claims. A shape read wrong resynchronises on that byte count and becomes
/// [`Value::Unsupported`] rather than plausible-looking nonsense.
fn read_stl(
    reg: &StreamerRegistry,
    el: &StreamerElement,
    r: &mut RBuffer,
    tags: &mut TagReader,
    depth: usize,
) -> Result<Value> {
    let vh = r.read_version()?; // {byte-count, version} wrapper — our safety net
    let memberwise = vh.version & K_MEMBERWISE != 0;
    let decoded = decode_stl(reg, &el.type_name, memberwise, r, tags, depth);
    let value = match (decoded, vh.end) {
        (Some(v), None) => v,
        (Some(v), Some(end)) if r.pos() == end => v,
        (decoded, Some(end)) => {
            r.seek(end)?;
            unsupported(&el.type_name, stl_failure(decoded.is_some(), memberwise))
        }
        (None, None) => unsupported(&el.type_name, "unsupported STL container, no byte count"),
    };
    Ok(value)
}

/// Why an STL container came back undecoded: the shape was one we do not read,
/// or we read it and it did not fill the container.
fn stl_failure(decoded: bool, memberwise: bool) -> &'static str {
    match (decoded, memberwise) {
        (true, _) => "STL container decoded to a length its byte count contradicts",
        (false, true) => "unsupported memberwise STL container shape",
        (false, false) => "unsupported STL container shape",
    }
}

/// Read a `TStreamerLoop` member (`T* //[fCount]`, an array of objects held by
/// pointer): a `{byte-count, version}` header, then one inline object per count.
/// `PolyHist::fCells` — the bins overlapping each cell of the lookup grid, a
/// `TList` per cell — is the one the histogram family writes.
fn read_stream_loop(
    reg: &StreamerRegistry,
    el: &StreamerElement,
    r: &mut RBuffer,
    tags: &mut TagReader,
    read_so_far: &[(String, Value)],
    depth: usize,
) -> Result<Value> {
    let vh = r.read_version()?;
    let class = el.type_name.trim_end_matches('*').trim();
    let n = counter_value(el, read_so_far).min(r.remaining());
    let decoded: Result<Vec<Value>> = (0..n)
        .map(|_| read_named_object(reg, class, r, tags, depth + 1))
        .collect();
    match (decoded, vh.end) {
        (Ok(items), None) => Ok(Value::Array(items)),
        (Ok(items), Some(end)) if r.pos() == end => Ok(Value::Array(items)),
        (_, Some(end)) => {
            r.seek(end)?;
            Ok(unsupported(
                &el.type_name,
                "array of objects decoded to a length its byte count contradicts",
            ))
        }
        (Err(e), None) => Ok(unsupported(&el.type_name, &e.to_string())),
    }
}

/// The STL shape a C++ type name describes.
enum Stl<'a> {
    /// A sequence of one element type (`vector`, `set`, `list`, …).
    Seq(&'a str),
    /// An associative container, by key type and value type (`map`, `multimap`).
    Map(&'a str, &'a str),
}

/// Classify an STL container type name; `None` for anything else.
fn stl_shape(type_name: &str) -> Option<Stl<'_>> {
    let (container, args) = template_parts(type_name)?;
    match (container, args.as_slice()) {
        // A map's trailing arguments — its comparator, its allocator — are types,
        // not data, and are not streamed.
        ("map" | "multimap" | "unordered_map" | "unordered_multimap", [key, value, ..]) => {
            Some(Stl::Map(key, value))
        }
        (
            "vector" | "set" | "multiset" | "unordered_set" | "unordered_multiset" | "list"
            | "forward_list" | "deque",
            [elem, ..],
        ) => Some(Stl::Seq(elem)),
        _ => None,
    }
}

/// Decode an STL container's body (the cursor sits just past its version
/// header). `None` if the shape is not one we handle; the caller then
/// resynchronises on the byte count.
fn decode_stl(
    reg: &StreamerRegistry,
    type_name: &str,
    memberwise: bool,
    r: &mut RBuffer,
    tags: &mut TagReader,
    depth: usize,
) -> Option<Value> {
    if depth > MAX_DEPTH {
        return None;
    }
    match stl_shape(type_name)? {
        Stl::Seq(elem) => {
            let items = if memberwise {
                let mw = read_memberwise_header(r)?;
                read_memberwise(reg, elem, &mw, r, tags, depth)?
            } else {
                let n = stl_count(r)?;
                read_stl_elements(reg, elem, n, r, tags, depth)?
            };
            Some(Value::Array(items))
        }
        Stl::Map(key, value) => {
            let (keys, values) = if memberwise {
                // Memberwise: every key, then every value.
                let mw = read_memberwise_header(r)?;
                let keys = read_stl_elements(reg, key, mw.count, r, tags, depth)?;
                let values = read_stl_elements(reg, value, mw.count, r, tags, depth)?;
                (keys, values)
            } else {
                let n = stl_count(r)?;
                let mut keys = Vec::with_capacity(n);
                let mut values = Vec::with_capacity(n);
                for _ in 0..n {
                    keys.push(read_stl_element(reg, key, r, tags, depth)?);
                    values.push(read_stl_element(reg, value, r, tags, depth)?);
                }
                (keys, values)
            };
            let entries = keys.into_iter().zip(values).map(|(k, v)| pair_value(k, v));
            Some(Value::Array(entries.collect()))
        }
    }
}

/// An STL container's `{Int_t n}` element count, clamped to the bytes left (a
/// corrupt count then shortens the decode, which the byte-count check catches).
fn stl_count(r: &mut RBuffer) -> Option<usize> {
    Some((r.be_i32().ok()?.max(0) as usize).min(r.remaining()))
}

/// A memberwise container's header: the version of the element class, and the
/// element count.
struct MemberwiseHeader {
    /// The element class's version, or `0` when it has none (a `pair`, whose
    /// checksum stands in for one).
    version: i32,
    /// The number of elements the columns that follow each hold.
    count: usize,
}

/// Read a memberwise container's header: the element class's version, then its
/// checksum when it has no version, then the element count.
fn read_memberwise_header(r: &mut RBuffer) -> Option<MemberwiseHeader> {
    let version = i32::from(r.be_i16().ok()?);
    if version <= 0 {
        r.be_u32().ok()?; // the element class's checksum, in place of a version
    }
    Some(MemberwiseHeader {
        version: version.max(0),
        count: stl_count(r)?,
    })
}

/// One member of a memberwise-streamed element type: the whole column of that
/// member, one value per element, is streamed before the next member's.
enum Column<'a> {
    /// A member given by its type name (a `pair`'s `first` and `second`).
    Typed(&'a str),
    /// A member the file's streamer info describes.
    Member(&'a StreamerElement),
}

/// Read `count` elements of `elem_type` streamed member by member: the column of
/// every element's first member, then the column of the second, and so on.
/// `None` for an element type whose columns we cannot name — a class the file
/// does not describe, or one with a base class, which ROOT streams as a
/// memberwise block of its own.
fn read_memberwise<'a>(
    reg: &'a StreamerRegistry,
    elem_type: &'a str,
    header: &MemberwiseHeader,
    r: &mut RBuffer,
    tags: &mut TagReader,
    depth: usize,
) -> Option<Vec<Value>> {
    let (class, columns) = memberwise_columns(reg, elem_type, header.version)?;
    let mut elements = vec![Vec::with_capacity(columns.len()); header.count];
    for (name, column) in &columns {
        for element in elements.iter_mut() {
            let value = match column {
                Column::Typed(ty) => read_stl_element(reg, ty, r, tags, depth + 1)?,
                Column::Member(el) => read_member(reg, el, r, tags, &[], depth + 1).ok()??,
            };
            element.push((name.clone(), value));
        }
    }
    let objects = elements.into_iter().map(|members| Value::Object {
        class: class.to_string(),
        members,
    });
    Some(objects.collect())
}

/// The columns a memberwise-streamed element type is written in: the class name
/// to label an element with, and one column per member.
fn memberwise_columns<'a>(
    reg: &'a StreamerRegistry,
    elem_type: &'a str,
    version: i32,
) -> Option<(&'a str, Vec<(String, Column<'a>)>)> {
    // A `pair` carries no streamer info of its own; its members are its two
    // template arguments.
    if let Some(("pair", args)) = template_parts(elem_type) {
        if let [first, second] = args.as_slice() {
            return Some((
                "pair",
                vec![
                    ("first".to_string(), Column::Typed(first)),
                    ("second".to_string(), Column::Typed(second)),
                ],
            ));
        }
    }
    let info = if version > 0 {
        reg.get_at(elem_type, version)?
    } else {
        reg.get(elem_type)?
    };
    if info.elements.iter().any(is_base) {
        return None;
    }
    let columns = info
        .elements
        .iter()
        .map(|e| (e.name.clone(), Column::Member(e)))
        .collect();
    Some((elem_type, columns))
}

/// Read `n` elements of `type_name`, one whole element after the next.
fn read_stl_elements(
    reg: &StreamerRegistry,
    type_name: &str,
    n: usize,
    r: &mut RBuffer,
    tags: &mut TagReader,
    depth: usize,
) -> Option<Vec<Value>> {
    let mut items = Vec::with_capacity(n);
    for _ in 0..n {
        items.push(read_stl_element(reg, type_name, r, tags, depth)?);
    }
    Some(items)
}

/// Read one element of an STL container, from its C++ type name.
fn read_stl_element(
    reg: &StreamerRegistry,
    type_name: &str,
    r: &mut RBuffer,
    tags: &mut TagReader,
    depth: usize,
) -> Option<Value> {
    if depth > MAX_DEPTH {
        return None;
    }
    let t = strip_std(type_name);
    if let Some(base) = basic_type_of(t) {
        return read_basic(r, base).ok()?;
    }
    if t == "string" || t == "TString" {
        return Some(Value::Str(r.string().ok()?));
    }
    if t.ends_with('*') {
        // Held by pointer: ROOT's `ReadObjectAny` protocol (a class tag or a
        // back-reference to an object already read).
        return read_ref_object(reg, r, tags, depth + 1).ok();
    }
    if tarray_elem(t).is_some() {
        // A `TArrayX` element streams as `{Int_t n}{n elements}`, no header.
        return read_tarray(t, r).ok();
    }
    match template_parts(t) {
        Some(("pair", args)) if args.len() == 2 => {
            let first = read_stl_element(reg, args[0], r, tags, depth + 1)?;
            let second = read_stl_element(reg, args[1], r, tags, depth + 1)?;
            Some(pair_value(first, second))
        }
        // A nested container streams inline: its count and its elements, with no
        // header of its own.
        Some(_) if stl_shape(t).is_some() => decode_stl(reg, t, false, r, tags, depth + 1),
        // A class the file describes, with its own `{byte-count, version}` header.
        _ => read_named_object(reg, t, r, tags, depth + 1).ok(),
    }
}

/// A `pair` — a `map` entry, or a `pair` element — as an object.
fn pair_value(first: Value, second: Value) -> Value {
    Value::Object {
        class: "pair".to_string(),
        members: vec![("first".to_string(), first), ("second".to_string(), second)],
    }
}

/// Whether a streamer element is a base-class slot.
fn is_base(el: &StreamerElement) -> bool {
    el.element_class == "TStreamerBase"
}

/// Strip a leading `std::` and the surrounding whitespace from a type name.
fn strip_std(type_name: &str) -> &str {
    let t = type_name.trim();
    t.strip_prefix("std::").unwrap_or(t)
}

/// Split a template type name into its name and its top-level arguments:
/// `map<TString,int,TFormulaParamOrder>` becomes `("map", ["TString", "int",
/// "TFormulaParamOrder"])`. A nested `<…>` stays inside one argument.
fn template_parts(type_name: &str) -> Option<(&str, Vec<&str>)> {
    let t = strip_std(type_name);
    let open = t.find('<')?;
    let close = t.rfind('>')?;
    if close < open {
        return None;
    }
    let inner = &t[open + 1..close];
    let mut args = Vec::new();
    let (mut depth, mut start) = (0usize, 0usize);
    for (i, c) in inner.char_indices() {
        match c {
            '<' => depth += 1,
            '>' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                args.push(inner[start..i].trim());
                start = i + 1;
            }
            _ => {}
        }
    }
    args.push(inner[start..].trim());
    Some((t[..open].trim(), args))
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

    fn stl_of(name: &str, type_name: &str) -> StreamerElement {
        StreamerElement {
            element_class: "TStreamerSTL".to_string(),
            name: name.to_string(),
            title: String::new(),
            el_type: 500,
            size: 24,
            array_length: 0,
            type_name: type_name.to_string(),
            base_version: None,
            count_name: None,
        }
    }

    fn int_of(name: &str) -> StreamerElement {
        StreamerElement {
            element_class: "TStreamerBasicType".to_string(),
            name: name.to_string(),
            title: String::new(),
            el_type: 3,
            size: 4,
            array_length: 0,
            type_name: "int".to_string(),
            base_version: None,
            count_name: None,
        }
    }

    /// ROOT's `{byte-count, version}` framing around `body`.
    fn framed(version: u16, body: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(&(0x4000_0000u32 | (body.len() as u32 + 2)).to_be_bytes());
        out.extend_from_slice(&version.to_be_bytes());
        out.extend_from_slice(body);
        out
    }

    /// A class holding `container` and, after it, an `int` — so a test can check
    /// both what the container decoded to and that the cursor came out of it in
    /// the right place.
    fn one_container(type_name: &str) -> StreamerRegistry {
        StreamerRegistry::from_infos(vec![StreamerInfo {
            class_name: "X".to_string(),
            class_version: 1,
            checksum: 0,
            elements: vec![stl_of("v", type_name), int_of("after")],
        }])
    }

    #[test]
    fn template_parts_splits_top_level_arguments() {
        assert_eq!(
            template_parts("map<TString,int,TFormulaParamOrder>"),
            Some(("map", vec!["TString", "int", "TFormulaParamOrder"]))
        );
        // A nested container keeps its own arguments, and ROOT's spelling — a
        // space before the closing angle bracket — is not part of the type.
        assert_eq!(
            template_parts("std::vector<pair<double,double> >"),
            Some(("vector", vec!["pair<double,double>"]))
        );
        assert_eq!(
            template_parts("vector<vector<int> >"),
            Some(("vector", vec!["vector<int>"]))
        );
        assert_eq!(template_parts("TH1D"), None);
    }

    /// A memberwise `vector<pair<double,double>>` (`Efficiency`'s per-bin beta
    /// parameters): the header carries the pair's checksum in place of a version,
    /// then every `first`, then every `second`.
    #[test]
    fn memberwise_pair_vector_reads_column_by_column() {
        let reg = one_container("vector<pair<double,double> >");
        let mut body = Vec::new();
        body.extend_from_slice(&0i16.to_be_bytes()); // a pair has no class version
        body.extend_from_slice(&0x00d7_bed2u32.to_be_bytes()); // so its checksum follows
        body.extend_from_slice(&2i32.to_be_bytes());
        for first in [1.0f64, 3.0] {
            body.extend_from_slice(&first.to_be_bytes());
        }
        for second in [2.0f64, 4.0] {
            body.extend_from_slice(&second.to_be_bytes());
        }
        let mut object = framed(0x400a, &body); // 0x4000: streamed memberwise
        object.extend_from_slice(&7i32.to_be_bytes());

        let v = read_object(&reg, "X", &framed(1, &object), 0);
        let pairs = v.get("v").and_then(Value::as_array).expect("decoded");
        let got: Vec<(f64, f64)> = pairs
            .iter()
            .map(|p| {
                (
                    p.get("first").and_then(Value::as_f64).unwrap(),
                    p.get("second").and_then(Value::as_f64).unwrap(),
                )
            })
            .collect();
        assert_eq!(got, vec![(1.0, 2.0), (3.0, 4.0)]);
        assert_eq!(v.get("after").and_then(Value::as_i64), Some(7), "{v}");
    }

    /// An objectwise `map<TString,int>` (`TFormula`'s parameter names): the
    /// entries are written one whole pair after another.
    #[test]
    fn objectwise_map_reads_entries_in_order() {
        let reg = one_container("map<TString,int,TFormulaParamOrder>");
        let mut body = 2i32.to_be_bytes().to_vec();
        for (name, index) in [("p0", 0i32), ("p1", 1)] {
            body.push(name.len() as u8);
            body.extend_from_slice(name.as_bytes());
            body.extend_from_slice(&index.to_be_bytes());
        }
        let mut object = framed(0x000a, &body);
        object.extend_from_slice(&7i32.to_be_bytes());

        let v = read_object(&reg, "X", &framed(1, &object), 0);
        let entries = v.get("v").and_then(Value::as_array).expect("decoded");
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[1].get("first").and_then(Value::as_str), Some("p1"));
        assert_eq!(entries[1].get("second").and_then(Value::as_i64), Some(1));
        assert_eq!(v.get("after").and_then(Value::as_i64), Some(7), "{v}");
    }

    /// A container whose shape does not fit the bytes must not pass off what it
    /// read as values: the byte count says how long the container is, and a
    /// decode that disagrees with it is reported and resynchronised past.
    #[test]
    fn a_container_the_bytes_contradict_is_not_decoded() {
        let reg = one_container("vector<double>");
        // A count of three doubles, with room for two: reading it would run into
        // the member after it.
        let mut body = 3i32.to_be_bytes().to_vec();
        for value in [1.0f64, 2.0] {
            body.extend_from_slice(&value.to_be_bytes());
        }
        let mut object = framed(0x000a, &body);
        object.extend_from_slice(&7i32.to_be_bytes());

        let v = read_object(&reg, "X", &framed(1, &object), 0);
        assert!(matches!(v.get("v"), Some(Value::Unsupported { .. })), "{v}");
        assert_eq!(v.get("after").and_then(Value::as_i64), Some(7), "{v}");
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
