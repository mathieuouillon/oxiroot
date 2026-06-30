//! A bare collection of objects stored under one key: [`ObjList`], which writes
//! and reads as a ROOT `TList` or `TObjArray`. Build it from any writable objects
//! and, on read, pull the members back out by type with
//! [`items`](ObjList::items).
//!
//! Members are serialized through ROOT's object protocol (each with a fresh class
//! tag), so ROOT and uproot read what oxiroot writes; reading uses [`TagReader`]
//! so the class back-references ROOT emits for repeated member types resolve.

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
