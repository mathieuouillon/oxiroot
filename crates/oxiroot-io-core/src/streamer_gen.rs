//! Generating `TStreamerInfo` records — the write-side counterpart to
//! [`streamer_info`](crate::streamer_info)'s parser.
//!
//! A written file embeds a `TList<TStreamerInfo>` describing every class it
//! stores, so it is self-describing (uproot relies on it; the streamer-driven
//! readers walk it). This module emits that list from a declarative table of
//! [`Cls`] entries — the same class versions, checksums, and member layouts ROOT
//! writes — rather than shipping baked binary blobs. Each object is written with
//! `kNewClassTag` (a full class name, never a back-reference), so the output is
//! position-independent: [`append_streamer_infos`] can splice extra classes onto
//! an existing list without disturbing it.
//!
//! The serialization mirrors the parser in reverse: a `TList<TStreamerInfo>`,
//! each `TStreamerInfo` v10 wrapping a `TObjArray` v3 of `TStreamerElement`
//! subclasses.

use crate::buffer::{Patch, RBuffer, WBuffer, K_BYTE_COUNT_MASK};
use crate::error::Result;
use crate::streamer::{read_tobject, write_tnamed, write_tobject};

/// ROOT `fType` codes for an object/string member and the base-class slots.
const K_TOBJECT: i32 = 66;
const K_TNAMED: i32 = 67;
/// `fType` for an inline object member (e.g. a `TObjArray fBranches`).
pub const K_OBJECT: i32 = 61;
/// `fType` for an inline non-`TObject` member (e.g. `TIOFeatures`).
pub const K_ANY: i32 = 62;
/// `fType` for an object-pointer member.
pub const K_OBJECT_PTR: i32 = 64;
/// `fType` for a `TString` member.
pub const K_TSTRING: i32 = 65;

/// One member (or base class) to serialize into a `TStreamerInfo`. Opaque —
/// build with [`base`], [`basic`], [`strf`], [`object`], [`any`], [`objptr`], or
/// [`basicptr`].
pub struct El {
    name: &'static str,
    /// ROOT `fType` code.
    ty: i32,
    /// In-memory size (`fSize`).
    size: i32,
    /// C++ type name (`fTypeName`); `"BASE"` for a base class.
    type_name: &'static str,
    kind: Ek,
}

/// Which `TStreamerElement` subclass an [`El`] is, plus its subclass-specific
/// tail (a base class's referenced version; a `//[fCount]` pointer's counter).
enum Ek {
    Base(i32),
    Basic,
    Str,
    Object,
    Any,
    ObjectPtr,
    BasicPtr(&'static str),
}

/// One class's `TStreamerInfo`: name, on-disk version, ROOT checksum, members
/// (in declared order, bases first).
pub struct Cls {
    /// Class name (`fName`), e.g. `"TParameter<double>"`.
    pub name: &'static str,
    /// On-disk class version.
    pub version: i32,
    /// ROOT's `fCheckSum` for the class (a hash of its member layout).
    pub checksum: u32,
    /// Members and base classes, in the order ROOT streams them.
    pub elements: Vec<El>,
}

/// A base-class slot referencing `name` at `base_version`.
pub fn base(name: &'static str, base_version: i32) -> El {
    let ty = match name {
        "TObject" => K_TOBJECT,
        "TNamed" => K_TNAMED,
        _ => 0, // kBase
    };
    El {
        name,
        ty,
        size: 0,
        type_name: "BASE",
        kind: Ek::Base(base_version),
    }
}
/// A basic-type member (`fType`/`fSize`/`fTypeName` as ROOT records them).
pub fn basic(name: &'static str, ty: i32, size: i32, type_name: &'static str) -> El {
    El {
        name,
        ty,
        size,
        type_name,
        kind: Ek::Basic,
    }
}
/// A `TString` member.
pub fn strf(name: &'static str) -> El {
    El {
        name,
        ty: K_TSTRING,
        size: 24,
        type_name: "TString",
        kind: Ek::Str,
    }
}
/// An inline `TObject`-derived object member (e.g. `TObjArray fBranches`).
pub fn object(name: &'static str, type_name: &'static str) -> El {
    El {
        name,
        ty: K_OBJECT,
        size: 64,
        type_name,
        kind: Ek::Object,
    }
}
/// An inline non-`TObject` member (e.g. `ROOT::TIOFeatures fIOFeatures`).
pub fn any(name: &'static str, size: i32, type_name: &'static str) -> El {
    El {
        name,
        ty: K_ANY,
        size,
        type_name,
        kind: Ek::Any,
    }
}
/// An object-pointer member (e.g. `TList* fFriends`).
pub fn objptr(name: &'static str, type_name: &'static str) -> El {
    El {
        name,
        ty: K_OBJECT_PTR,
        size: 8,
        type_name,
        kind: Ek::ObjectPtr,
    }
}
/// A `//[fCount]`-counted basic-type pointer member; `count` names the counter.
pub fn basicptr(
    name: &'static str,
    ty: i32,
    size: i32,
    type_name: &'static str,
    count: &'static str,
) -> El {
    El {
        name,
        ty,
        size,
        type_name,
        kind: Ek::BasicPtr(count),
    }
}

/// `fBits` ROOT writes for the embedded `TStreamerInfo`'s `TNamed`.
const SI_BITS: u32 = 0x0001_0000;

/// The byte-count + `kNewClassTag` + class-name header ROOT writes before each
/// streamed object (`TStreamerInfo`, `TObjArray`, every `TStreamerElement`).
fn begin_object_any(w: &mut WBuffer, class: &str) -> Patch {
    let bc = w.reserve(4);
    w.be_u32(0xFFFF_FFFF); // kNewClassTag
    w.bytes(class.as_bytes());
    w.u8(0); // NUL terminator
    bc
}
fn end_object_any(w: &mut WBuffer, bc: Patch) {
    let inner = (w.len() - w.patch_offset(bc) - 4) as u32;
    w.patch_be_u32(bc, inner | K_BYTE_COUNT_MASK);
}

/// Write the `TStreamerElement` v4 base common to every element subclass.
fn write_element_base(w: &mut WBuffer, el: &El) {
    let se = w.begin_object(4); // TStreamerElement v4
    write_tnamed(w, 0, el.name, "");
    w.be_i32(el.ty); // fType
    w.be_i32(el.size); // fSize
    w.be_i32(0); // fArrayLength
    w.be_i32(0); // fArrayDim
    for _ in 0..5 {
        w.be_i32(0); // fMaxIndex[5]
    }
    w.string(el.type_name); // fTypeName
    w.end_object(se);
}

/// Write one element as its `TStreamerElement` subclass (`TStreamerBase`,
/// `TStreamerBasicType`, …), wrapping the common base with the subclass tail.
/// `owner`/`owner_version` name the class that declares the element (used for a
/// `//[fCount]` pointer's `fCountClass`/`fCountVersion`).
fn write_element(w: &mut WBuffer, el: &El, owner: &str, owner_version: i32) {
    let (class, version) = match el.kind {
        Ek::Base(_) => ("TStreamerBase", 3),
        Ek::Basic => ("TStreamerBasicType", 2),
        Ek::Str => ("TStreamerString", 2),
        Ek::Object => ("TStreamerObject", 2),
        Ek::Any => ("TStreamerObjectAny", 2),
        Ek::ObjectPtr => ("TStreamerObjectPointer", 2),
        Ek::BasicPtr(_) => ("TStreamerBasicPointer", 2),
    };
    let bc = begin_object_any(w, class);
    let sub = w.begin_object(version);
    write_element_base(w, el);
    match el.kind {
        Ek::Base(base_version) => w.be_i32(base_version), // fBaseVersion
        Ek::BasicPtr(count_name) => {
            w.be_i32(owner_version); // fCountVersion
            w.string(count_name); // fCountName
            w.string(owner); // fCountClass
        }
        _ => {}
    }
    w.end_object(sub);
    end_object_any(w, bc);
}

/// Write one `TStreamerInfo` (with `kNewClassTag` framing) followed by its empty
/// `TList` option string — i.e. one entry of the list body.
fn write_info(w: &mut WBuffer, cls: &Cls) {
    let info_bc = begin_object_any(w, "TStreamerInfo");
    let si = w.begin_object(10); // TStreamerInfo v10
    write_tnamed(w, SI_BITS, cls.name, "");
    w.be_u32(cls.checksum);
    w.be_i32(cls.version);

    let oa_bc = begin_object_any(w, "TObjArray");
    let oa = w.begin_object(3); // TObjArray v3
    write_tobject(w, 0);
    w.string(""); // fName
    w.be_i32(cls.elements.len() as i32);
    w.be_i32(0); // fLowerBound
    for el in &cls.elements {
        write_element(w, el, cls.name, cls.version);
    }
    w.end_object(oa);
    end_object_any(w, oa_bc);

    w.end_object(si);
    end_object_any(w, info_bc);
    w.string(""); // the TList option string for this entry
}

/// Serialize a `TList<TStreamerInfo>` object body (no key header) describing
/// `classes`, in the given order (bases before the classes that use them, as
/// ROOT writes).
pub fn streamer_info_list(classes: &[Cls]) -> Vec<u8> {
    let mut w = WBuffer::new();

    let list = w.begin_object(5); // TList v5
    write_tobject(&mut w, 0);
    w.string(""); // fName
    w.be_i32(classes.len() as i32); // nobjects

    for cls in classes {
        write_info(&mut w, cls);
    }

    w.end_object(list);
    w.into_vec()
}

/// Splice `extra` classes onto an existing serialized `TList<TStreamerInfo>`
/// (`base_list`, e.g. a baked blob), returning a new list. The original entries
/// are copied verbatim — their `kNewClassTag`/back-reference positions are
/// preserved because they keep the same absolute byte offsets — and the extra
/// entries (which use only `kNewClassTag`) are appended after them. The list's
/// object count and outer byte count are updated.
pub fn append_streamer_infos(base_list: &[u8], extra: &[Cls]) -> Result<Vec<u8>> {
    // Parse the TList header to find the object-count field.
    let mut r = RBuffer::new(base_list);
    r.read_version()?; // [byte count][version]
    read_tobject(&mut r)?; // TObject base
    r.string()?; // fName
    let count_offset = r.pos();
    let count = r.be_i32()?; // nobjects

    // Serialize the extra entries.
    let mut pairs = WBuffer::new();
    for cls in extra {
        write_info(&mut pairs, cls);
    }
    let pairs = pairs.into_vec();

    // header (with bumped count) + original entries + extra entries.
    let mut out = Vec::with_capacity(base_list.len() + pairs.len());
    out.extend_from_slice(&base_list[..count_offset]);
    out.extend_from_slice(&(count + extra.len() as i32).to_be_bytes());
    out.extend_from_slice(&base_list[count_offset + 4..]);
    out.extend_from_slice(&pairs);

    // Re-patch the outer TList byte count.
    let inner = (out.len() - 4) as u32;
    out[..4].copy_from_slice(&(inner | K_BYTE_COUNT_MASK).to_be_bytes());
    Ok(out)
}
