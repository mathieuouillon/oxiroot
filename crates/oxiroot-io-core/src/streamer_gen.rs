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

use std::borrow::Cow;

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
/// `fType` for a pointer to a non-`TObject` class (`TStreamerObjectAnyPointer`).
pub const K_ANY_PTR: i32 = 69;
/// `fType` for an STL container member (`TStreamerSTL`, e.g. `vector<double>`).
pub const K_STL: i32 = 500;

/// One member (or base class) to serialize into a `TStreamerInfo`. Opaque —
/// build with [`base`], [`basic`], [`strf`], [`object`], [`any`], [`objptr`], or
/// [`basicptr`].
#[derive(Clone, Debug)]
pub struct El<'a> {
    name: Cow<'a, str>,
    /// ROOT `fType` code.
    ty: i32,
    /// In-memory size (`fSize`).
    size: i32,
    /// C++ type name (`fTypeName`); `"BASE"` for a base class.
    type_name: Cow<'a, str>,
    kind: Ek<'a>,
}

/// Which `TStreamerElement` subclass an [`El`] is, plus its subclass-specific
/// tail (a base class's referenced version; a `//[fCount]` pointer's counter).
#[derive(Clone, Debug)]
enum Ek<'a> {
    Base(i32),
    Basic,
    Str,
    Object,
    Any,
    ObjectPtr,
    /// A pointer to a non-`TObject` class (`TStreamerObjectAnyPointer`).
    AnyPtr,
    /// An STL container (`TStreamerSTL`): its `fSTLtype` (vector = 1, map = 4)
    /// and `fCtype` (the contained-type code, e.g. `double` = 8).
    Stl(i32, i32),
    /// A `//[count]` pointer: the counter's name, and an optional
    /// `(count_class, count_version)` when the counter lives in a different class
    /// than the one declaring this element (e.g. a base class) — `None` uses the
    /// declaring class.
    BasicPtr(Cow<'a, str>, Option<(Cow<'a, str>, i32)>),
    /// An element copied from another file: its `TStreamerElement` subclass
    /// name and body, written again verbatim.
    Stored(Cow<'a, str>, Cow<'a, [u8]>),
}

/// One class's `TStreamerInfo`: name, on-disk version, ROOT checksum, members
/// (in declared order, bases first). The strings are usually `'static`; a class
/// defined at run time (a user struct) borrows or owns them instead
/// ([`into_owned`](Cls::into_owned) detaches it from what it borrowed).
#[derive(Clone, Debug)]
pub struct Cls<'a> {
    /// Class name (`fName`), e.g. `"TParameter<double>"`.
    pub name: Cow<'a, str>,
    /// On-disk class version.
    pub version: i32,
    /// ROOT's `fCheckSum` for the class (a hash of its member layout).
    pub checksum: u32,
    /// Members and base classes, in the order ROOT streams them.
    pub elements: Vec<El<'a>>,
}

impl Cls<'_> {
    /// This class with every string owned, so it outlives what it borrowed.
    #[must_use]
    pub fn into_owned(self) -> Cls<'static> {
        Cls {
            name: Cow::Owned(self.name.into_owned()),
            version: self.version,
            checksum: self.checksum,
            elements: self.elements.into_iter().map(El::into_owned).collect(),
        }
    }
}

impl El<'_> {
    fn into_owned(self) -> El<'static> {
        let own = |s: Cow<'_, str>| Cow::Owned(s.into_owned());
        El {
            name: own(self.name),
            ty: self.ty,
            size: self.size,
            type_name: own(self.type_name),
            kind: match self.kind {
                Ek::Base(v) => Ek::Base(v),
                Ek::Basic => Ek::Basic,
                Ek::Str => Ek::Str,
                Ek::Object => Ek::Object,
                Ek::Any => Ek::Any,
                Ek::ObjectPtr => Ek::ObjectPtr,
                Ek::AnyPtr => Ek::AnyPtr,
                Ek::Stl(stl, ctype) => Ek::Stl(stl, ctype),
                Ek::BasicPtr(count, owner) => {
                    Ek::BasicPtr(own(count), owner.map(|(class, v)| (own(class), v)))
                }
                Ek::Stored(class, body) => Ek::Stored(own(class), Cow::Owned(body.into_owned())),
            },
        }
    }
}

/// A string an element or class name can be built from: a `&str` (borrowed) or a
/// `String` (owned).
pub trait Name<'a>: Into<Cow<'a, str>> {}
impl<'a, T: Into<Cow<'a, str>>> Name<'a> for T {}

/// A base-class slot referencing `name` at `base_version`.
pub fn base<'a>(name: impl Name<'a>, base_version: i32) -> El<'a> {
    let name = name.into();
    let ty = match name.as_ref() {
        "TObject" => K_TOBJECT,
        "TNamed" => K_TNAMED,
        _ => 0, // kBase
    };
    El {
        name,
        ty,
        size: 0,
        type_name: Cow::Borrowed("BASE"),
        kind: Ek::Base(base_version),
    }
}
/// A basic-type member (`fType`/`fSize`/`fTypeName` as ROOT records them).
pub fn basic<'a>(name: impl Name<'a>, ty: i32, size: i32, type_name: impl Name<'a>) -> El<'a> {
    El {
        name: name.into(),
        ty,
        size,
        type_name: type_name.into(),
        kind: Ek::Basic,
    }
}
/// A `TString` member.
pub fn strf<'a>(name: impl Name<'a>) -> El<'a> {
    El {
        name: name.into(),
        ty: K_TSTRING,
        size: 24,
        type_name: Cow::Borrowed("TString"),
        kind: Ek::Str,
    }
}
/// An inline `TObject`-derived object member (e.g. `TObjArray fBranches`).
pub fn object<'a>(name: impl Name<'a>, type_name: impl Name<'a>) -> El<'a> {
    El {
        name: name.into(),
        ty: K_OBJECT,
        size: 64,
        type_name: type_name.into(),
        kind: Ek::Object,
    }
}
/// An inline non-`TObject` member (e.g. `ROOT::TIOFeatures fIOFeatures`).
pub fn any<'a>(name: impl Name<'a>, size: i32, type_name: impl Name<'a>) -> El<'a> {
    El {
        name: name.into(),
        ty: K_ANY,
        size,
        type_name: type_name.into(),
        kind: Ek::Any,
    }
}
/// An object-pointer member (e.g. `TList* fFriends`).
pub fn objptr<'a>(name: impl Name<'a>, type_name: impl Name<'a>) -> El<'a> {
    El {
        name: name.into(),
        ty: K_OBJECT_PTR,
        size: 8,
        type_name: type_name.into(),
        kind: Ek::ObjectPtr,
    }
}
/// A pointer to a non-`TObject` class (e.g. `TF1Parameters* fParams`).
pub fn objanyptr<'a>(name: impl Name<'a>, type_name: impl Name<'a>) -> El<'a> {
    El {
        name: name.into(),
        ty: K_ANY_PTR,
        size: 8,
        type_name: type_name.into(),
        kind: Ek::AnyPtr,
    }
}
/// An STL container member (`TStreamerSTL`), e.g. a `vector<double>`. `stl_type`
/// is the container kind (`vector` = 1, `map` = 4) and `ctype` the contained
/// element-type code (`double` = 8, `TObject*` = 63, an object = 61).
pub fn stl<'a>(name: impl Name<'a>, type_name: impl Name<'a>, stl_type: i32, ctype: i32) -> El<'a> {
    El {
        name: name.into(),
        ty: K_STL,
        size: 24,
        type_name: type_name.into(),
        kind: Ek::Stl(stl_type, ctype),
    }
}
/// A `//[fCount]`-counted basic-type pointer member; `count` names the counter,
/// which is assumed to live in the same class that declares this element.
pub fn basicptr<'a>(
    name: impl Name<'a>,
    ty: i32,
    size: i32,
    type_name: impl Name<'a>,
    count: impl Name<'a>,
) -> El<'a> {
    El {
        name: name.into(),
        ty,
        size,
        type_name: type_name.into(),
        kind: Ek::BasicPtr(count.into(), None),
    }
}

/// Like [`basicptr`], but the counter lives in `count_class` (version
/// `count_version`) rather than the declaring class — as for a matrix's
/// `fElements`, counted by `fNelems` in its `TMatrixTBase` base.
pub fn basicptr_in<'a>(
    name: impl Name<'a>,
    ty: i32,
    size: i32,
    type_name: impl Name<'a>,
    count: impl Name<'a>,
    count_class: impl Name<'a>,
    count_version: i32,
) -> El<'a> {
    El {
        name: name.into(),
        ty,
        size,
        type_name: type_name.into(),
        kind: Ek::BasicPtr(count.into(), Some((count_class.into(), count_version))),
    }
}

/// An element another file stored: its `TStreamerElement` subclass name
/// (`element_class`) and body, which is written again verbatim. `name` is the
/// member's name, for diagnostics.
pub(crate) fn stored(element_class: String, name: String, body: Vec<u8>) -> El<'static> {
    El {
        name: Cow::Owned(name),
        ty: 0,
        size: 0,
        type_name: Cow::Borrowed(""),
        kind: Ek::Stored(Cow::Owned(element_class), Cow::Owned(body)),
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
fn write_element_base(w: &mut WBuffer, el: &El<'_>) {
    let se = w.begin_object(4); // TStreamerElement v4
    write_tnamed(w, 0, &el.name, "");
    w.be_i32(el.ty); // fType
    w.be_i32(el.size); // fSize
    w.be_i32(0); // fArrayLength
    w.be_i32(0); // fArrayDim
    for _ in 0..5 {
        w.be_i32(0); // fMaxIndex[5]
    }
    w.string(&el.type_name); // fTypeName
    w.end_object(se);
}

/// Write one element as its `TStreamerElement` subclass (`TStreamerBase`,
/// `TStreamerBasicType`, …), wrapping the common base with the subclass tail.
/// `owner`/`owner_version` name the class that declares the element (used for a
/// `//[fCount]` pointer's `fCountClass`/`fCountVersion`).
fn write_element(w: &mut WBuffer, el: &El<'_>, owner: &str, owner_version: i32) {
    if let Ek::Stored(class, body) = &el.kind {
        let bc = begin_object_any(w, class);
        w.bytes(body);
        end_object_any(w, bc);
        return;
    }
    let (class, version) = match &el.kind {
        Ek::Base(_) => ("TStreamerBase", 3),
        Ek::Basic => ("TStreamerBasicType", 2),
        Ek::Str => ("TStreamerString", 2),
        Ek::Object => ("TStreamerObject", 2),
        Ek::Any => ("TStreamerObjectAny", 2),
        Ek::ObjectPtr => ("TStreamerObjectPointer", 2),
        Ek::AnyPtr => ("TStreamerObjectAnyPointer", 1),
        Ek::Stl(..) => ("TStreamerSTL", 3),
        Ek::BasicPtr(..) => ("TStreamerBasicPointer", 2),
        Ek::Stored(..) => unreachable!("written verbatim above"),
    };
    let bc = begin_object_any(w, class);
    let sub = w.begin_object(version);
    write_element_base(w, el);
    match &el.kind {
        Ek::Base(base_version) => w.be_i32(*base_version), // fBaseVersion
        Ek::Stl(stl_type, ctype) => {
            w.be_i32(*stl_type); // fSTLtype
            w.be_i32(*ctype); // fCtype
        }
        Ek::BasicPtr(count_name, count_owner) => {
            let (count_class, count_version) = count_owner
                .as_ref()
                .map_or((owner, owner_version), |(class, v)| (class.as_ref(), *v));
            w.be_i32(count_version); // fCountVersion
            w.string(count_name); // fCountName
            w.string(count_class); // fCountClass
        }
        _ => {}
    }
    w.end_object(sub);
    end_object_any(w, bc);
}

/// Write one `TStreamerInfo` (with `kNewClassTag` framing) followed by its empty
/// `TList` option string — i.e. one entry of the list body.
fn write_info(w: &mut WBuffer, cls: &Cls<'_>) {
    let info_bc = begin_object_any(w, "TStreamerInfo");
    let si = w.begin_object(10); // TStreamerInfo v10
    write_tnamed(w, SI_BITS, &cls.name, "");
    w.be_u32(cls.checksum);
    w.be_i32(cls.version);

    let oa_bc = begin_object_any(w, "TObjArray");
    let oa = w.begin_object(3); // TObjArray v3
    write_tobject(w, 0);
    w.string(""); // fName
    w.be_i32(cls.elements.len() as i32);
    w.be_i32(0); // fLowerBound
    for el in &cls.elements {
        write_element(w, el, &cls.name, cls.version);
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
pub fn streamer_info_list(classes: &[Cls<'_>]) -> Vec<u8> {
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
pub fn append_streamer_infos(base_list: &[u8], extra: &[Cls<'_>]) -> Result<Vec<u8>> {
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
