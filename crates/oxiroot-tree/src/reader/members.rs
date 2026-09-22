//! Reading an object member by member, following the file's `TStreamerInfo`.

use oxiroot_io_core::{
    read_tnamed, read_tobject, Error, RBuffer, Result, StreamerElement, StreamerRegistry,
};

/// A scalar or array member captured while walking a class's streamer elements.
/// Integers (any width) are widened to `i64`; arrays keep their element values.
pub(super) enum MemberVal {
    Int(i64),
    /// A floating member (e.g. `fWeight`). Captured for completeness so the walk
    /// stays generic; no member the tree reader consumes is a float yet.
    Float(#[allow(dead_code)] f64),
    IntArray(Vec<i64>),
    Str(String),
}

impl MemberVal {
    pub(super) fn int(&self) -> i64 {
        match self {
            MemberVal::Int(v) => *v,
            _ => 0,
        }
    }
    pub(super) fn ints(&self) -> &[i64] {
        match self {
            MemberVal::IntArray(v) => v,
            _ => &[],
        }
    }
    pub(super) fn str(&self) -> &str {
        match self {
            MemberVal::Str(s) => s,
            _ => "",
        }
    }
}

/// Members captured from one object, keyed by streamer-element name.
pub(super) type Members = std::collections::HashMap<String, MemberVal>;

pub(super) fn member_int(m: &Members, name: &str) -> i64 {
    m.get(name).map_or(0, MemberVal::int)
}
pub(super) fn member_str(m: &Members, name: &str) -> String {
    m.get(name).map_or(String::new(), |v| v.str().to_string())
}

/// The on-disk width (bytes) and float-ness of a basic streamer type code
/// (`fType` < 20). `None` for codes whose on-disk encoding we don't handle
/// (e.g. `Double32`/`Float16`), so the walker errors rather than misparsing.
fn basic_kind(t: i32) -> Option<(usize, bool)> {
    Some(match t {
        1 | 11 | 18 => (1, false),      // Char / UChar / Bool
        2 | 12 => (2, false),           // Short / UShort
        3 | 13 | 6 | 15 => (4, false),  // Int / UInt / Counter / Bits
        4 | 14 | 16 | 17 => (8, false), // Long / ULong / Long64 / ULong64
        5 => (4, true),                 // Float
        8 => (8, true),                 // Double
        _ => return None,
    })
}

/// Read one basic value of the given width, widening integers to `i64`.
fn read_basic(r: &mut RBuffer, width: usize, is_float: bool) -> Result<MemberVal> {
    Ok(match (width, is_float) {
        (1, false) => MemberVal::Int(i64::from(r.u8()? as i8)),
        (2, false) => MemberVal::Int(i64::from(r.be_i16()?)),
        (4, false) => MemberVal::Int(i64::from(r.be_i32()?)),
        (8, false) => MemberVal::Int(r.be_i64()?),
        (4, true) => MemberVal::Float(f64::from(r.be_f32()?)),
        (_, true) => MemberVal::Float(r.be_f64()?),
        _ => MemberVal::Int(0),
    })
}

fn unsupported_element(el: &StreamerElement) -> Error {
    Error::Format(format!(
        "streamer element {:?} has unsupported type code {} ({})",
        el.name, el.el_type, el.type_name
    ))
}

/// Walk a class's streamer `elements`, reading each member from `r`. Scalar and
/// array members are captured into `out` by name (counted pointer arrays use the
/// already-read counter named by `fCountName`); base classes are read in place
/// (recursing through their own streamer info); object members are handed to
/// `on_object`. Reading stops after the element named `stop_after` (the caller
/// then seeks to the object end), so a long trailing tail of unread members is
/// skipped via the enclosing byte count. This is the adaptive replacement for
/// fixed-offset parsing: layout comes from the file's `TStreamerInfo`, not pins.
pub(super) fn walk_members(
    r: &mut RBuffer,
    reg: &StreamerRegistry,
    elements: &[StreamerElement],
    out: &mut Members,
    on_object: &mut dyn FnMut(&str, &mut RBuffer) -> Result<()>,
    stop_after: &str,
) -> Result<()> {
    for el in elements {
        let t = el.el_type;
        if el.element_class == "TStreamerBase" {
            read_base(r, reg, &el.name, out, on_object, stop_after)?;
        } else if t == 65 {
            // kTString
            out.insert(el.name.clone(), MemberVal::Str(r.string()?));
        } else if (61..=71).contains(&t) || (300..=365).contains(&t) || t == 500 || t == 501 {
            // Object / object-pointer / STL / streamer member: the caller reads
            // the ones it needs (e.g. fBranches/fLeaves) and skips the rest.
            on_object(&el.name, r)?;
        } else if (40..61).contains(&t) {
            // kOffsetP + basic: a `T* //[fCount]` variable-length array.
            let (width, is_float) = basic_kind(t - 40).ok_or_else(|| unsupported_element(el))?;
            let count = el
                .count_name
                .as_deref()
                .map_or(0, |c| member_int(out, c))
                .max(0) as usize;
            r.u8()?; // is-array marker
            let mut vals = Vec::with_capacity(count.min(r.remaining()));
            for _ in 0..count {
                vals.push(read_basic(r, width, is_float)?.int());
            }
            out.insert(el.name.clone(), MemberVal::IntArray(vals));
        } else if (20..40).contains(&t) {
            // kOffsetL + basic: a fixed `T[fArrayLength]` member (read, not kept).
            let (width, is_float) = basic_kind(t - 20).ok_or_else(|| unsupported_element(el))?;
            for _ in 0..el.array_length.max(0) {
                read_basic(r, width, is_float)?;
            }
        } else {
            let (width, is_float) = basic_kind(t).ok_or_else(|| unsupported_element(el))?;
            out.insert(el.name.clone(), read_basic(r, width, is_float)?);
        }
        if el.name == stop_after {
            break;
        }
    }
    Ok(())
}

/// Read a base-class slot named `class`. `TObject`/`TNamed` are read with their
/// dedicated readers (the latter captures `fName`/`fTitle`); any other base is
/// walked through its own streamer info when present, else skipped via its
/// version byte count.
fn read_base(
    r: &mut RBuffer,
    reg: &StreamerRegistry,
    class: &str,
    out: &mut Members,
    on_object: &mut dyn FnMut(&str, &mut RBuffer) -> Result<()>,
    stop_after: &str,
) -> Result<()> {
    match class {
        "TObject" => {
            read_tobject(r)?;
        }
        "TNamed" => {
            let named = read_tnamed(r)?;
            out.insert("fName".to_string(), MemberVal::Str(named.name));
            out.insert("fTitle".to_string(), MemberVal::Str(named.title));
        }
        _ => {
            // Read the version first: the base is walked with the description
            // of its own class version.
            let vh = r.read_version()?;
            match reg.get_at(class, i32::from(vh.version)) {
                Some(info) => {
                    walk_members(r, reg, &info.elements, out, on_object, stop_after)?;
                    if let Some(end) = vh.end {
                        r.seek(end)?;
                    }
                }
                None => {
                    let end = vh.end.ok_or_else(|| {
                        Error::Format(format!("cannot skip a {class} that carries no byte count"))
                    })?;
                    r.seek(end)?;
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{member_int, walk_members, MemberVal, Members};
    use oxiroot_io_core::RBuffer;
    use oxiroot_io_core::Result;
    use oxiroot_io_core::{StreamerElement, StreamerRegistry};

    fn elem(class: &str, name: &str, el_type: i32, count: Option<&str>) -> StreamerElement {
        StreamerElement {
            element_class: class.to_string(),
            name: name.to_string(),
            title: String::new(),
            el_type,
            size: 0,
            array_length: 0,
            type_name: String::new(),
            base_version: None,
            count_name: count.map(str::to_string),
        }
    }

    /// The walker reads members by the streamer element list, so it adapts to a
    /// member order this reader was never compiled against, picks up an extra
    /// member ROOT might add in a future version, and sizes a `//[fCount]` array
    /// from the named counter — none of which a fixed-offset reader could do.
    #[test]
    fn walker_reads_by_element_list_not_fixed_offsets() {
        // Layout: a:int, b:Long64, c:int (a hypothetical *new* member), n:counter,
        // arr:Long64*[n]. A pinned reader keyed to "a,b" would misread c and arr.
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&7i32.to_be_bytes()); // a
        bytes.extend_from_slice(&100i64.to_be_bytes()); // b
        bytes.extend_from_slice(&42i32.to_be_bytes()); // c (the evolved member)
        bytes.extend_from_slice(&2i32.to_be_bytes()); // n = 2
        bytes.push(1); // is-array marker
        bytes.extend_from_slice(&555i64.to_be_bytes()); // arr[0]
        bytes.extend_from_slice(&666i64.to_be_bytes()); // arr[1]

        let elements = vec![
            elem("TStreamerBasicType", "a", 3, None),            // kInt
            elem("TStreamerBasicType", "b", 16, None),           // kLong64
            elem("TStreamerBasicType", "c", 3, None),            // kInt
            elem("TStreamerBasicType", "n", 6, None),            // kCounter
            elem("TStreamerBasicPointer", "arr", 56, Some("n")), // kOffsetP + kLong64
        ];

        let reg = StreamerRegistry::default();
        let mut out = Members::new();
        let mut r = RBuffer::new(&bytes);
        let mut on_object = |_: &str, _: &mut RBuffer| -> Result<()> { Ok(()) };
        walk_members(&mut r, &reg, &elements, &mut out, &mut on_object, "").unwrap();

        assert_eq!(member_int(&out, "a"), 7);
        assert_eq!(member_int(&out, "b"), 100);
        assert_eq!(member_int(&out, "c"), 42); // read purely by its element name
        assert_eq!(member_int(&out, "n"), 2);
        match out.get("arr") {
            Some(MemberVal::IntArray(v)) => assert_eq!(v, &[555, 666]),
            other => panic!("arr not a counted array: {:?}", other.map(|_| ())),
        }
        assert_eq!(r.remaining(), 0, "the whole record was consumed");
    }

    /// `stop_after` ends the walk early (the caller then seeks past the rest via
    /// the object byte count), the way the tree reader stops after `fBranches`.
    #[test]
    fn walker_stops_after_named_member() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&1i32.to_be_bytes()); // a
        bytes.extend_from_slice(&2i32.to_be_bytes()); // b
        bytes.extend_from_slice(&3i32.to_be_bytes()); // c (must remain unread)
        let elements = vec![
            elem("TStreamerBasicType", "a", 3, None),
            elem("TStreamerBasicType", "b", 3, None),
            elem("TStreamerBasicType", "c", 3, None),
        ];
        let reg = StreamerRegistry::default();
        let mut out = Members::new();
        let mut r = RBuffer::new(&bytes);
        let mut on_object = |_: &str, _: &mut RBuffer| -> Result<()> { Ok(()) };
        walk_members(&mut r, &reg, &elements, &mut out, &mut on_object, "b").unwrap();
        assert_eq!(member_int(&out, "a"), 1);
        assert_eq!(member_int(&out, "b"), 2);
        assert!(!out.contains_key("c"), "stopped before reading c");
        assert_eq!(
            r.remaining(),
            4,
            "c's 4 bytes are left for the caller to skip"
        );
    }
}
