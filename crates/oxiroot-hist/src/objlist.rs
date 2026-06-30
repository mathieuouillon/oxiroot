//! Collections of objects stored under one key: [`ObjList`] (a ROOT `TList` or
//! `TObjArray`) and [`TMap`] (a keyed object → object map). Build them from any
//! writable objects and, on read, pull the members back out by type with
//! [`items`](ObjList::items) / [`get`](TMap::get).
//!
//! Members are serialized through ROOT's object protocol (each with a fresh class
//! tag), so ROOT reads what oxiroot writes; reading uses [`TagReader`] so the
//! class back-references ROOT emits for repeated member types resolve. (uproot
//! reads `TList`/`TObjArray` but has no `TMap` model — see [`TMap`].)

use std::ops::Range;

use oxiroot_io_core::buffer::{RBuffer, WBuffer};
use oxiroot_io_core::error::{Error, Result};
use oxiroot_io_core::object::TagReader;
use oxiroot_io_core::streamer::{read_tobject, write_tobject};
use oxiroot_io_core::RFile;

use crate::base::object_bytes_any_keyed;
use crate::collections::write_object;
use crate::graph::{decode_tgraph, TGraph};
use crate::linalg::{
    decode_tmatrixd, decode_tmatrixdsym, decode_tvectord, TMatrixD, TMatrixDSym, TVectorD,
};
use crate::objects::{decode_tobjstring, decode_tparameter, TObjString, TParameter};
use crate::th1::{decode_th1, TH1};
use crate::th2::{decode_th2, TH2};
use crate::th3::{decode_th3, TH3};
use crate::write::WriteRoot;

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
/// [`ObjList::read_root`] and extract members by type with [`items`](ObjList::items).
#[derive(Debug, Clone, PartialEq)]
pub struct ObjList {
    kind: ListKind,
    name: String,
    /// Each member as `(class_name, streamed object body)`.
    members: Vec<(String, Vec<u8>)>,
}

impl ObjList {
    /// An empty `TList`.
    pub fn list() -> ObjList {
        ObjList {
            kind: ListKind::List,
            name: String::new(),
            members: Vec::new(),
        }
    }

    /// An empty `TObjArray`.
    pub fn array() -> ObjList {
        ObjList {
            kind: ListKind::Array,
            name: String::new(),
            members: Vec::new(),
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
    fn contained_classes(&self) -> Vec<String> {
        self.members.iter().map(|(c, _)| c.clone()).collect()
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
                    write_object(&mut w, class, body);
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
                    write_object(&mut w, class, body);
                }
                w.end_object(obj);
            }
        }
        w.into_vec()
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
    })
}

pub(crate) fn read_objlist(file: &RFile, name: &str) -> Result<ObjList> {
    let (class, object, keylen) = object_bytes_any_keyed(file, name)?;
    decode_objlist(&class, &object, keylen)
}

pub(crate) fn read_objlist_in(file: &RFile, subdir: &str, name: &str) -> Result<ObjList> {
    let (class, object, keylen) = file.object_in_keyed(subdir, name)?;
    decode_objlist(&class, &object, keylen)
}

/// A type that can be decoded from an [`ObjList`] member's `(class, body)`.
/// Implemented for the object types oxiroot models; [`ObjList::items`] uses it to
/// pull members of one type out of a mixed collection.
pub trait FromMember: Sized {
    /// Decode from a member's class name and streamed body, or `None` if the
    /// member is not this type.
    fn from_member(class: &str, bytes: &[u8]) -> Option<Result<Self>>;
}

impl FromMember for TH1 {
    fn from_member(class: &str, bytes: &[u8]) -> Option<Result<Self>> {
        class
            .starts_with("TH1")
            .then(|| decode_th1((class.to_string(), bytes.to_vec())))
    }
}
impl FromMember for TH2 {
    fn from_member(class: &str, bytes: &[u8]) -> Option<Result<Self>> {
        (class.starts_with("TH2") && class != "TH2Poly")
            .then(|| decode_th2((class.to_string(), bytes.to_vec())))
    }
}
impl FromMember for TH3 {
    fn from_member(class: &str, bytes: &[u8]) -> Option<Result<Self>> {
        class
            .starts_with("TH3")
            .then(|| decode_th3((class.to_string(), bytes.to_vec())))
    }
}
impl FromMember for TGraph {
    fn from_member(class: &str, bytes: &[u8]) -> Option<Result<Self>> {
        matches!(class, "TGraph" | "TGraphErrors" | "TGraphAsymmErrors")
            .then(|| decode_tgraph(class, class, bytes))
    }
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
impl FromMember for TVectorD {
    fn from_member(class: &str, bytes: &[u8]) -> Option<Result<Self>> {
        (class == "TVectorT<double>").then(|| decode_tvectord("", class, bytes))
    }
}
impl FromMember for TMatrixD {
    fn from_member(class: &str, bytes: &[u8]) -> Option<Result<Self>> {
        (class == "TMatrixT<double>").then(|| decode_tmatrixd("", class, bytes))
    }
}
impl FromMember for TMatrixDSym {
    fn from_member(class: &str, bytes: &[u8]) -> Option<Result<Self>> {
        (class == "TMatrixTSym<double>").then(|| decode_tmatrixdsym("", class, bytes))
    }
}

// --- TMap -------------------------------------------------------------------

/// One side of a [`TMap`] pair: a member's `(class_name, streamed body)`.
type MapEntry = (String, Vec<u8>);

/// A `TMap` — ROOT's keyed map of object → object, stored under one key (the way
/// ROOT keeps string-keyed metadata). Build it with [`TMap::insert`] (string
/// keys) or [`TMap::add`] (any key object); read one back with
/// [`TMap::read_root`] and look values up by string key with [`get`](TMap::get).
///
/// Note: uproot has no `TMap` model, so a `TMap` is unreadable there (ROOT's own
/// `TMap`s share this). ROOT C++ reads what oxiroot writes, and oxiroot reads
/// ROOT's `TMap`s.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TMap {
    name: String,
    pairs: Vec<(MapEntry, MapEntry)>,
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
    fn contained_classes(&self) -> Vec<String> {
        self.pairs
            .iter()
            .flat_map(|((kc, _), (vc, _))| [kc.clone(), vc.clone()])
            .collect()
    }
    fn to_root_bytes(&self) -> Vec<u8> {
        let mut w = WBuffer::new();
        let obj = w.begin_object(3); // TMap version 3
        write_tobject(&mut w, 0);
        w.string(&self.name); // fName
        w.be_i32(self.pairs.len() as i32); // number of pairs
        for ((kc, kb), (vc, vb)) in &self.pairs {
            write_object(&mut w, kc, kb); // key object
            write_object(&mut w, vc, vb); // value object
        }
        w.end_object(obj);
        w.into_vec()
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
    Ok(TMap { name, pairs })
}

pub(crate) fn read_tmap(file: &RFile, name: &str) -> Result<TMap> {
    let (class, object, keylen) = object_bytes_any_keyed(file, name)?;
    decode_tmap(&class, &object, keylen)
}

pub(crate) fn read_tmap_in(file: &RFile, subdir: &str, name: &str) -> Result<TMap> {
    let (class, object, keylen) = file.object_in_keyed(subdir, name)?;
    decode_tmap(&class, &object, keylen)
}
