//! Collections of objects stored under one key: [`ObjList`] (a ROOT `TList` or
//! `TObjArray`) and [`TMap`] (a keyed object → object map). Build them from any
//! writable objects and, on read, pull the members back out by type with
//! [`items`](ObjList::items) / [`get`](TMap::get).
//!
//! Members are serialized through ROOT's object protocol (each with a fresh class
//! tag), so ROOT reads what oxiroot writes; reading uses [`TagReader`] so the
//! class back-references ROOT emits for repeated member types resolve. (uproot
//! reads `TList`/`TObjArray` but has no `TMap` model — see [`TMap`].)

use std::borrow::Cow;
use std::ops::Range;

use crate::buffer::K_BYTE_COUNT_MASK;
use crate::buffer::{RBuffer, WBuffer};
use crate::error::{Error, Result};
use crate::object::TagReader;
use crate::object_io::{object_bytes_any_keyed, ReadRoot, StreamerSet, WriteRoot};
use crate::streamer::{read_tobject, write_object_any, write_tobject};
use crate::streamer_gen::{stored, Cls};
use crate::streamer_info::StoredInfo;
use crate::FileReader;

use super::scalars::{
    decode_tobjstring, decode_tparameter, member_classes, TObjString, TParameter,
};

/// Whether an [`ObjList`] serializes as a `TList` (ordered, with per-element
/// options) or a `TObjArray` (an indexed array).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListKind {
    /// ROOT's `TList`.
    List,
    /// ROOT's `TObjArray`.
    Array,
}

/// A `TList` or `TObjArray` of objects stored under a single key. Build with
/// [`ObjList::list`] / [`ObjList::array`], name it with [`named`](ObjList::named),
/// and [`add`](ObjList::add) any writable objects; read one back with
/// [`ObjList::read_root`](ReadRoot::read_root) and extract members by type with
/// [`items`](ObjList::items).
///
/// A list read from a file can be written again as it is, whatever its members:
/// it keeps the streamer info that file stores for their classes (and the
/// classes those depend on), and a list stored without a name (as ROOT writes
/// them) takes its key's name.
#[derive(Debug, Clone)]
pub struct ObjList {
    kind: ListKind,
    name: String,
    /// Each member as `(class_name, streamed object body)`.
    members: Vec<(String, Vec<u8>)>,
    /// The streamer info the members added with [`add`](ObjList::add) need.
    streamers: StreamerSet,
}

/// Two lists are equal when they hold the same members; where the members'
/// streamer info came from does not matter.
impl PartialEq for ObjList {
    fn eq(&self, other: &Self) -> bool {
        (self.kind, &self.name, &self.members) == (other.kind, &other.name, &other.members)
    }
}

impl ObjList {
    /// An empty `TList`.
    pub fn list() -> ObjList {
        ObjList::empty(ListKind::List)
    }

    /// An empty `TObjArray`.
    pub fn array() -> ObjList {
        ObjList::empty(ListKind::Array)
    }

    fn empty(kind: ListKind) -> ObjList {
        ObjList {
            kind,
            name: String::new(),
            members: Vec::new(),
            streamers: StreamerSet::default(),
        }
    }

    /// Set the key name this collection is stored under.
    #[must_use]
    pub fn named(mut self, name: impl Into<String>) -> ObjList {
        self.name = name.into();
        self
    }

    /// Add an object to the collection.
    // `add` is the natural builder verb here; it is not the arithmetic `Add::add`.
    #[allow(clippy::should_implement_trait)]
    #[must_use]
    pub fn add(mut self, object: &dyn WriteRoot) -> ObjList {
        self.streamers.add(object);
        self.members
            .push((object.root_class(), object.to_root_bytes()));
        self
    }

    /// Whether this is a `TList` or a `TObjArray`.
    pub fn kind(&self) -> ListKind {
        self.kind
    }
    /// The key name.
    pub fn name(&self) -> &str {
        &self.name
    }
    /// The number of members.
    pub fn len(&self) -> usize {
        self.members.len()
    }
    /// Whether the collection is empty.
    pub fn is_empty(&self) -> bool {
        self.members.is_empty()
    }
    /// The class name of every member, in order.
    pub fn class_names(&self) -> impl Iterator<Item = &str> {
        self.members.iter().map(|(c, _)| c.as_str())
    }

    /// Decode every member that is a `T`, in order, skipping the rest. For
    /// example `list.items::<TH1>()?` returns the histograms in the collection.
    pub fn items<T: FromMember>(&self) -> Result<Vec<T>> {
        self.members
            .iter()
            .filter_map(|(class, bytes)| T::from_member(class, bytes))
            .collect()
    }
}

impl WriteRoot for ObjList {
    fn root_class(&self) -> String {
        match self.kind {
            ListKind::List => "TList".to_string(),
            ListKind::Array => "TObjArray".to_string(),
        }
    }
    fn root_name(&self) -> &str {
        &self.name
    }
    fn root_title(&self) -> &str {
        ""
    }
    fn to_root_bytes(&self) -> Vec<u8> {
        let mut w = WBuffer::new();
        match self.kind {
            ListKind::List => {
                let obj = w.begin_object(5); // TList version 5
                write_tobject(&mut w, 0);
                w.string(&self.name); // fName
                w.be_i32(self.members.len() as i32); // nobjects
                for (class, body) in &self.members {
                    write_object_any(&mut w, class, body);
                    w.string(""); // the per-object option string
                }
                w.end_object(obj);
            }
            ListKind::Array => {
                let obj = w.begin_object(3); // TObjArray version 3
                write_tobject(&mut w, 0);
                w.string(&self.name); // fName
                w.be_i32(self.members.len() as i32); // nobjects
                w.be_i32(0); // fLowerBound
                for (class, body) in &self.members {
                    write_object_any(&mut w, class, body);
                }
                w.end_object(obj);
            }
        }
        w.into_vec()
    }
    fn streamer_blob(&self) -> Cow<'static, [u8]> {
        self.streamers.blob()
    }
    fn streamer_classes(&self) -> Vec<Cls<'static>> {
        member_streamer_classes(&self.streamers, self.members.iter().map(|(c, _)| c))
    }
}

/// What a collection's members need described: the classes gathered when they
/// were added, plus those known by name (a collection read from a file keeps
/// only its members' class names).
fn member_streamer_classes<'a>(
    added: &StreamerSet,
    names: impl Iterator<Item = &'a String>,
) -> Vec<Cls<'static>> {
    let mut set = added.clone();
    for class in names {
        set.add_classes(member_classes(class));
    }
    set.classes().to_vec()
}

/// The streamer info `file` stores for the classes of `members` (each a class
/// name and streamed body) and every class they depend on, dependencies first.
/// A collection read from `file` keeps them, so wherever it is written its
/// members are described, whatever their class. A file whose streamer info
/// cannot be read contributes nothing.
fn source_classes<'a>(
    file: &FileReader,
    members: impl Iterator<Item = (&'a str, &'a [u8])>,
) -> Vec<Cls<'static>> {
    let Ok(infos) = file.stored_streamer_infos() else {
        return Vec::new();
    };
    let mut seen = Vec::new();
    let mut out = Vec::new();
    for (class, body) in members {
        if !class.is_empty() {
            collect_stored(&infos, class, object_version(body), &mut seen, &mut out);
        }
    }
    out
}

/// The class version at the head of a streamed object body, if it has one.
fn object_version(body: &[u8]) -> Option<i32> {
    let head: [u8; 6] = body.get(..6)?.try_into().ok()?;
    let count = u32::from_be_bytes([head[0], head[1], head[2], head[3]]);
    let version = if count & K_BYTE_COUNT_MASK != 0 {
        u16::from_be_bytes([head[4], head[5]])
    } else {
        u16::from_be_bytes([head[0], head[1]])
    };
    Some(i32::from(version))
}

/// Add the stored info for `class` (at `version` when the file has that one)
/// to `out`, after the classes it depends on: its bases, and any class named
/// in a member's type (a `TAxis`, a `vector<TLorentzVector>`, …).
fn collect_stored(
    infos: &[StoredInfo],
    class: &str,
    version: Option<i32>,
    seen: &mut Vec<(String, i32)>,
    out: &mut Vec<Cls<'static>>,
) {
    let named = |s: &&StoredInfo| s.info.class_name == class;
    let Some(entry) = infos
        .iter()
        .filter(named)
        .find(|s| version.is_none_or(|v| s.info.class_version == v))
        .or_else(|| infos.iter().find(named))
    else {
        return;
    };
    let key = (entry.info.class_name.clone(), entry.info.class_version);
    if seen.contains(&key) {
        return;
    }
    seen.push(key);
    for element in &entry.info.elements {
        if element.element_class == "TStreamerBase" {
            collect_stored(infos, &element.name, element.base_version, seen, out);
        } else {
            let names = element
                .type_name
                .split(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == ':'))
                .filter(|name| !name.is_empty() && *name != class);
            for name in names {
                collect_stored(infos, name, None, seen, out);
            }
        }
    }
    if let Some(bodies) = &entry.elements {
        out.push(Cls {
            name: entry.info.class_name.clone().into(),
            version: entry.info.class_version,
            checksum: entry.info.checksum,
            elements: bodies
                .iter()
                .zip(&entry.info.elements)
                .map(|((element_class, body), element)| {
                    stored(element_class.clone(), element.name.clone(), body.clone())
                })
                .collect(),
        });
    }
}

/// A member's class name and the byte range of its streamed body within the
/// collection's object buffer.
type MemberRange = (String, Range<usize>);

/// Read a top-level `TList`/`TObjArray` object body into `(name, members)`, each
/// member as its class name and the byte range of its streamed body.
fn read_members(class: &str, object: &[u8], keylen: usize) -> Result<(String, Vec<MemberRange>)> {
    let kind = match class {
        "TList" => ListKind::List,
        "TObjArray" => ListKind::Array,
        other => {
            return Err(Error::Format(format!(
                "key is a {other}, not a TList or TObjArray"
            )))
        }
    };
    let mut r = RBuffer::new(object);
    r.read_version()?; // TList v5 / TObjArray v3
    read_tobject(&mut r)?;
    let name = r.string()?; // fName
    let n = r.be_i32()?.max(0);
    if kind == ListKind::Array {
        r.be_i32()?; // fLowerBound
    }

    let mut tags = TagReader::new(keylen);
    let mut out = Vec::with_capacity(n as usize);
    for _ in 0..n {
        let header = tags.read_header(&mut r)?;
        match (header.class_name, header.end) {
            (Some(member_class), Some(end)) => {
                out.push((member_class, r.pos()..end));
                r.seek(end)?;
            }
            (_, Some(end)) => r.seek(end)?, // a null/parent slot
            _ => {}
        }
        if kind == ListKind::List {
            r.string()?; // the per-object option string (TList only)
        }
    }
    Ok((name, out))
}

fn decode_objlist(class: &str, object: &[u8], keylen: usize) -> Result<ObjList> {
    let kind = if class == "TObjArray" {
        ListKind::Array
    } else {
        ListKind::List
    };
    let (name, ranges) = read_members(class, object, keylen)?;
    let members = ranges
        .into_iter()
        .map(|(c, range)| (c, object[range].to_vec()))
        .collect();
    Ok(ObjList {
        kind,
        name,
        members,
        streamers: StreamerSet::default(),
    })
}

fn read_objlist(file: &FileReader, name: &str) -> Result<ObjList> {
    let (class, object, keylen) = object_bytes_any_keyed(file, name)?;
    Ok(read_back(
        file,
        name,
        decode_objlist(&class, &object, keylen)?,
    ))
}

fn read_objlist_in(file: &FileReader, subdir: &str, name: &str) -> Result<ObjList> {
    let (class, object, keylen) = file.object_in_keyed(subdir, name)?;
    Ok(read_back(
        file,
        name,
        decode_objlist(&class, &object, keylen)?,
    ))
}

/// `list`, read from `file` under the key `key`, ready to be written again: named
/// by its key when the list itself has no name (ROOT writes lists that way), and
/// carrying the streamer info `file` stores for its members.
fn read_back(file: &FileReader, key: &str, mut list: ObjList) -> ObjList {
    if list.name.is_empty() {
        list.name = key.to_string();
    }
    let members = list.members.iter().map(|(c, b)| (c.as_str(), b.as_slice()));
    let classes = source_classes(file, members);
    list.streamers.add_classes(classes);
    list
}

/// A type that can be decoded from an [`ObjList`] or [`TMap`] member's
/// `(class, body)`. The crate that defines an object type implements it (the
/// histogram and matrix crates do for theirs); [`ObjList::items`] uses it to
/// pull members of one type out of a mixed collection.
pub trait FromMember: Sized {
    /// Decode from a member's class name and streamed body, or `None` if the
    /// member is not this type.
    fn from_member(class: &str, bytes: &[u8]) -> Option<Result<Self>>;
}

impl FromMember for TObjString {
    fn from_member(class: &str, bytes: &[u8]) -> Option<Result<Self>> {
        (class == "TObjString").then(|| decode_tobjstring("", class, bytes))
    }
}
impl FromMember for TParameter {
    fn from_member(class: &str, bytes: &[u8]) -> Option<Result<Self>> {
        class
            .starts_with("TParameter<")
            .then(|| decode_tparameter("", class, bytes))
    }
}
// --- TMap -------------------------------------------------------------------

/// One side of a [`TMap`] pair: a member's `(class_name, streamed body)`.
type MapEntry = (String, Vec<u8>);

/// A `TMap` — ROOT's keyed map of object → object, stored under one key (the way
/// ROOT keeps string-keyed metadata). Build it with [`TMap::insert`] (string
/// keys) or [`TMap::add`] (any key object); read one back with
/// [`TMap::read_root`](ReadRoot::read_root) and look values up by string key with
/// [`get`](TMap::get).
///
/// Like an [`ObjList`], a map read from a file keeps the streamer info that file
/// stores for its keys' and values' classes, and takes its key's name if it has
/// none of its own.
///
/// Note: uproot has no `TMap` model, so a `TMap` is unreadable there (ROOT's own
/// `TMap`s share this). ROOT C++ reads what oxiroot writes, and oxiroot reads
/// ROOT's `TMap`s.
#[derive(Debug, Clone, Default)]
pub struct TMap {
    name: String,
    pairs: Vec<(MapEntry, MapEntry)>,
    /// The streamer info the entries added with [`add`](TMap::add) need.
    streamers: StreamerSet,
}

/// Two maps are equal when they hold the same entries.
impl PartialEq for TMap {
    fn eq(&self, other: &Self) -> bool {
        (&self.name, &self.pairs) == (&other.name, &other.pairs)
    }
}

impl TMap {
    /// An empty map.
    pub fn new() -> TMap {
        TMap::default()
    }

    /// Set the key name this map is stored under.
    #[must_use]
    pub fn named(mut self, name: impl Into<String>) -> TMap {
        self.name = name.into();
        self
    }

    /// Insert a `value` under a string `key` (stored as a `TObjString`, the usual
    /// map-key type).
    #[must_use]
    pub fn insert(self, key: &str, value: &dyn WriteRoot) -> TMap {
        let key_obj = TObjString::new(key);
        self.add(&key_obj, value)
    }

    /// Insert a `value` under an arbitrary object `key`.
    #[must_use]
    pub fn add(mut self, key: &dyn WriteRoot, value: &dyn WriteRoot) -> TMap {
        self.streamers.add(key);
        self.streamers.add(value);
        self.pairs.push((
            (key.root_class(), key.to_root_bytes()),
            (value.root_class(), value.to_root_bytes()),
        ));
        self
    }

    /// The key name.
    pub fn name(&self) -> &str {
        &self.name
    }
    /// The number of entries.
    pub fn len(&self) -> usize {
        self.pairs.len()
    }
    /// Whether the map is empty.
    pub fn is_empty(&self) -> bool {
        self.pairs.is_empty()
    }

    /// The string (`TObjString`) keys, in insertion order; keys of other types
    /// are skipped.
    pub fn string_keys(&self) -> Vec<String> {
        self.pairs
            .iter()
            .filter_map(|((kc, kb), _)| {
                (kc == "TObjString")
                    .then(|| {
                        decode_tobjstring("", kc, kb)
                            .ok()
                            .map(|s| s.value().to_string())
                    })
                    .flatten()
            })
            .collect()
    }

    /// The value stored under the string `key`, decoded as `T` — `None` if no
    /// entry has that `TObjString` key or its value is not a `T`.
    pub fn get<T: FromMember>(&self, key: &str) -> Option<Result<T>> {
        self.pairs.iter().find_map(|((kc, kb), (vc, vb))| {
            if kc != "TObjString" {
                return None;
            }
            match decode_tobjstring("", kc, kb) {
                Ok(k) if k.value() == key => T::from_member(vc, vb),
                Ok(_) => None,
                Err(e) => Some(Err(e)),
            }
        })
    }

    /// Every value that is a `T`, in insertion order.
    pub fn values<T: FromMember>(&self) -> Result<Vec<T>> {
        self.pairs
            .iter()
            .filter_map(|(_, (vc, vb))| T::from_member(vc, vb))
            .collect()
    }
}

impl WriteRoot for TMap {
    fn root_class(&self) -> String {
        "TMap".to_string()
    }
    fn root_name(&self) -> &str {
        &self.name
    }
    fn root_title(&self) -> &str {
        ""
    }
    fn to_root_bytes(&self) -> Vec<u8> {
        let mut w = WBuffer::new();
        let obj = w.begin_object(3); // TMap version 3
        write_tobject(&mut w, 0);
        w.string(&self.name); // fName
        w.be_i32(self.pairs.len() as i32); // number of pairs
        for ((kc, kb), (vc, vb)) in &self.pairs {
            write_object_any(&mut w, kc, kb); // key object
            write_object_any(&mut w, vc, vb); // value object
        }
        w.end_object(obj);
        w.into_vec()
    }
    fn streamer_blob(&self) -> Cow<'static, [u8]> {
        self.streamers.blob()
    }
    fn streamer_classes(&self) -> Vec<Cls<'static>> {
        member_streamer_classes(
            &self.streamers,
            self.pairs.iter().flat_map(|((kc, _), (vc, _))| [kc, vc]),
        )
    }
}

/// Read one map entry (key or value): its class name and body byte range, or an
/// empty entry for a null slot. Advances the cursor past the object.
fn read_entry(r: &mut RBuffer, tags: &mut TagReader, object: &[u8]) -> Result<MapEntry> {
    let header = tags.read_header(r)?;
    let entry = match (header.class_name, header.end) {
        (Some(class), Some(end)) => {
            let body = object[r.pos()..end].to_vec();
            r.seek(end)?;
            (class, body)
        }
        (_, Some(end)) => {
            r.seek(end)?;
            (String::new(), Vec::new())
        }
        _ => (String::new(), Vec::new()),
    };
    Ok(entry)
}

fn decode_tmap(class: &str, object: &[u8], keylen: usize) -> Result<TMap> {
    if class != "TMap" {
        return Err(Error::Format(format!("key is a {class}, not a TMap")));
    }
    let mut r = RBuffer::new(object);
    r.read_version()?; // TMap version
    read_tobject(&mut r)?;
    let name = r.string()?; // fName
    let n = r.be_i32()?.max(0);

    let mut tags = TagReader::new(keylen);
    let mut pairs = Vec::with_capacity(n as usize);
    for _ in 0..n {
        let key = read_entry(&mut r, &mut tags, object)?;
        let value = read_entry(&mut r, &mut tags, object)?;
        pairs.push((key, value));
    }
    Ok(TMap {
        name,
        pairs,
        streamers: StreamerSet::default(),
    })
}

fn read_tmap(file: &FileReader, name: &str) -> Result<TMap> {
    let (class, object, keylen) = object_bytes_any_keyed(file, name)?;
    Ok(map_read_back(
        file,
        name,
        decode_tmap(&class, &object, keylen)?,
    ))
}

fn read_tmap_in(file: &FileReader, subdir: &str, name: &str) -> Result<TMap> {
    let (class, object, keylen) = file.object_in_keyed(subdir, name)?;
    Ok(map_read_back(
        file,
        name,
        decode_tmap(&class, &object, keylen)?,
    ))
}

/// `map`, read from `file` under the key `key`, ready to be written again; see
/// [`read_back`].
fn map_read_back(file: &FileReader, key: &str, mut map: TMap) -> TMap {
    if map.name.is_empty() {
        map.name = key.to_string();
    }
    let entries = map
        .pairs
        .iter()
        .flat_map(|(key, value)| [key, value])
        .map(|(c, b)| (c.as_str(), b.as_slice()));
    let classes = source_classes(file, entries);
    map.streamers.add_classes(classes);
    map
}

impl ReadRoot for ObjList {
    fn read_root(file: &FileReader, name: &str) -> Result<Self> {
        read_objlist(file, name)
    }
    fn read_root_in(file: &FileReader, dir: &str, name: &str) -> Result<Self> {
        read_objlist_in(file, dir, name)
    }
}

impl ReadRoot for TMap {
    fn read_root(file: &FileReader, name: &str) -> Result<Self> {
        read_tmap(file, name)
    }
    fn read_root_in(file: &FileReader, dir: &str, name: &str) -> Result<Self> {
        read_tmap_in(file, dir, name)
    }
}
