//! Collection objects holding other objects: [`THStack`] (a stack of
//! histograms) and [`TMultiGraph`] (several graphs drawn together). Both store
//! their members in a `TList` and serialize byte-for-byte as ROOT does, so ROOT
//! and uproot read what oxiroot writes and vice versa.
//!
//! The members are written through the generic object protocol — each one with
//! a fresh class tag (`kNewClassTag`), never a back-reference — so writing is
//! position-independent. Reading uses [`TagReader`], which resolves both the
//! class tags oxiroot writes and the back-references ROOT writes.

use std::ops::Range;

use oxiroot_io_core::buffer::{RBuffer, WBuffer, K_BYTE_COUNT_MASK};
use oxiroot_io_core::error::{Error, Result};
use oxiroot_io_core::object::TagReader;
use oxiroot_io_core::streamer::{read_tobject, write_tnamed, write_tobject};
use oxiroot_io_core::RFile;

use crate::base::object_bytes_any_keyed;
use crate::graph::{decode_tgraph, TGraph};
use crate::th1::{decode_th1, TH1};
use crate::write::WriteRoot;

const K_NEW_CLASS_TAG: u32 = 0xFFFF_FFFF;
const K_CLASS_MASK: u32 = 0x8000_0000;
/// THStack/TMultiGraph leave `fMaximum`/`fMinimum` at this sentinel until drawn.
const UNSET_LIMIT: f64 = -1111.0;

// --- shared object-protocol helpers -----------------------------------------

/// Write one embedded object: `[byte count][kNewClassTag][class\0][body]`, where
/// `body` is the object's own streamed bytes (as produced by [`WriteRoot`]).
pub(crate) fn write_object(w: &mut WBuffer, class: &str, body: &[u8]) {
    let bc = w.reserve(4);
    w.be_u32(K_NEW_CLASS_TAG);
    w.bytes(class.as_bytes());
    w.u8(0);
    w.bytes(body);
    let inner = (w.len() - w.patch_offset(bc) - 4) as u32;
    w.patch_be_u32(bc, inner | K_BYTE_COUNT_MASK);
}

/// Write a `TList*` member named `list_name` holding `members` (each a
/// `(class, body)` pair), wrapped as a `TList` object via [`write_object`].
fn write_object_list(w: &mut WBuffer, list_name: &str, members: &[(String, Vec<u8>)]) {
    let mut body = WBuffer::new();
    let list = body.begin_object(5); // TList version 5
    write_tobject(&mut body, 0);
    body.string(list_name); // fName
    body.be_i32(members.len() as i32); // nobjects
    for (class, member) in members {
        write_object(&mut body, class, member);
        body.string(""); // the per-object option string
    }
    body.end_object(list);
    write_object(w, "TList", &body.into_vec());
}

/// Read a `TNamed` base (version header, `TObject`, `fName`, `fTitle`).
fn read_tnamed(r: &mut RBuffer) -> Result<(String, String)> {
    r.read_version()?;
    read_tobject(r)?;
    let name = r.string()?;
    let title = r.string()?;
    Ok((name, title))
}

/// Open a `TList*` member and return, for each entry, its class name and the
/// byte range of its body (the bytes one can hand to a per-class decoder). A
/// null member pointer (an absent list) yields an empty vector. Reuses
/// [`TagReader`] so ROOT's class back-references resolve.
fn list_member_ranges(
    r: &mut RBuffer,
    tags: &mut TagReader,
) -> Result<Vec<(String, Range<usize>)>> {
    let start = r.pos();
    let word = r.be_u32()?;
    if word == 0 {
        return Ok(Vec::new()); // null TList* — no members
    }
    r.seek(start)?;

    // The member is a streamed `TList` object: byte count, then a class tag
    // (new-class marker or a high-bit back-reference) introduces it.
    let list_end = if word & K_BYTE_COUNT_MASK != 0 {
        let after = r.pos() + 4;
        r.seek(after)?;
        let tag = r.be_u32()?;
        r.seek(start)?;
        if tag == K_NEW_CLASS_TAG || tag & K_CLASS_MASK != 0 {
            let header = tags.read_header(r)?;
            r.read_version()?; // the list's own (inner) version header
            header.end
        } else {
            r.read_version()?.end
        }
    } else {
        r.read_version()?.end
    };
    read_tobject(r)?;
    r.string()?; // the list's fName
    let n = r.be_i32()?.max(0);

    let mut out = Vec::with_capacity(n as usize);
    for _ in 0..n {
        let header = tags.read_header(r)?;
        match (header.class_name, header.end) {
            (Some(class), Some(end)) => {
                out.push((class, r.pos()..end));
                r.seek(end)?;
            }
            (_, Some(end)) => r.seek(end)?, // a null/parent slot
            _ => {}
        }
        r.string()?; // the per-object option string
    }
    if let Some(end) = list_end {
        r.seek(end)?;
    }
    Ok(out)
}

// --- THStack ----------------------------------------------------------------

/// A `THStack` — a named stack of histograms (drawn overlaid or summed). Build
/// one with [`THStack::new`], name it with [`named`](THStack::named), and
/// [`add`](THStack::add) the histograms; write it through
/// [`RootFile`](crate::RootFile) or [`write_root`](crate::WriteRoot::write_root).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct THStack {
    name: String,
    title: String,
    hists: Vec<TH1>,
}

impl THStack {
    /// An empty stack (give it a key name with [`named`](Self::named)).
    pub fn new() -> THStack {
        THStack::default()
    }

    /// Set the key name this stack is stored under.
    #[must_use]
    pub fn named(mut self, name: impl Into<String>) -> THStack {
        self.name = name.into();
        self
    }

    /// Set the stack's title.
    #[must_use]
    pub fn titled(mut self, title: impl Into<String>) -> THStack {
        self.title = title.into();
        self
    }

    /// Add a histogram to the stack.
    // `add` is the natural builder verb here; it is not the arithmetic `Add::add`.
    #[allow(clippy::should_implement_trait)]
    #[must_use]
    pub fn add(mut self, hist: TH1) -> THStack {
        self.hists.push(hist);
        self
    }

    /// The key name.
    pub fn name(&self) -> &str {
        &self.name
    }
    /// The title.
    pub fn title(&self) -> &str {
        &self.title
    }
    /// The stacked histograms, in the order they were added.
    pub fn hists(&self) -> &[TH1] {
        &self.hists
    }
}

impl WriteRoot for THStack {
    fn root_class(&self) -> String {
        "THStack".to_string()
    }
    fn root_name(&self) -> &str {
        &self.name
    }
    fn root_title(&self) -> &str {
        &self.title
    }
    fn to_root_bytes(&self) -> Vec<u8> {
        let mut w = WBuffer::new();
        let obj = w.begin_object(2); // THStack version 2
        write_tnamed(&mut w, 0, &self.name, &self.title);
        let members: Vec<(String, Vec<u8>)> = self
            .hists
            .iter()
            .map(|h| (h.class_name(), h.to_root_bytes()))
            .collect();
        write_object_list(&mut w, "", &members); // fHists
        w.be_u32(0); // fHistogram (null TH1*)
        w.be_f64(UNSET_LIMIT); // fMaximum
        w.be_f64(UNSET_LIMIT); // fMinimum
        w.end_object(obj);
        w.into_vec()
    }
}

fn decode_thstack(class: &str, object: &[u8], keylen: usize) -> Result<THStack> {
    if class != "THStack" {
        return Err(Error::Format(format!("key is a {class}, not a THStack")));
    }
    let mut r = RBuffer::new(object);
    r.read_version()?; // THStack version
    let (name, title) = read_tnamed(&mut r)?;
    let mut tags = TagReader::new(keylen);
    let ranges = list_member_ranges(&mut r, &mut tags)?;
    let mut hists = Vec::with_capacity(ranges.len());
    for (member_class, range) in ranges {
        if member_class.starts_with("TH1") {
            hists.push(decode_th1((member_class, object[range].to_vec()))?);
        }
    }
    Ok(THStack { name, title, hists })
}

pub(crate) fn read_thstack(file: &RFile, name: &str) -> Result<THStack> {
    let (class, object, keylen) = object_bytes_any_keyed(file, name)?;
    decode_thstack(&class, &object, keylen)
}

pub(crate) fn read_thstack_in(file: &RFile, subdir: &str, name: &str) -> Result<THStack> {
    let (class, object, keylen) = file.object_in_keyed(subdir, name)?;
    decode_thstack(&class, &object, keylen)
}

// --- TMultiGraph ------------------------------------------------------------

/// A `TMultiGraph` — several [`TGraph`]s drawn in one frame. Build with
/// [`TMultiGraph::new`], name it with [`named`](TMultiGraph::named), and
/// [`add`](TMultiGraph::add) the graphs.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TMultiGraph {
    name: String,
    title: String,
    graphs: Vec<TGraph>,
}

impl TMultiGraph {
    /// An empty multigraph (give it a key name with [`named`](Self::named)).
    pub fn new() -> TMultiGraph {
        TMultiGraph::default()
    }

    /// Set the key name this multigraph is stored under.
    #[must_use]
    pub fn named(mut self, name: impl Into<String>) -> TMultiGraph {
        self.name = name.into();
        self
    }

    /// Set the multigraph's title.
    #[must_use]
    pub fn titled(mut self, title: impl Into<String>) -> TMultiGraph {
        self.title = title.into();
        self
    }

    /// Add a graph to the multigraph.
    // `add` is the natural builder verb here; it is not the arithmetic `Add::add`.
    #[allow(clippy::should_implement_trait)]
    #[must_use]
    pub fn add(mut self, graph: TGraph) -> TMultiGraph {
        self.graphs.push(graph);
        self
    }

    /// The key name.
    pub fn name(&self) -> &str {
        &self.name
    }
    /// The title.
    pub fn title(&self) -> &str {
        &self.title
    }
    /// The member graphs, in the order they were added.
    pub fn graphs(&self) -> &[TGraph] {
        &self.graphs
    }
}

impl WriteRoot for TMultiGraph {
    fn root_class(&self) -> String {
        "TMultiGraph".to_string()
    }
    fn root_name(&self) -> &str {
        &self.name
    }
    fn root_title(&self) -> &str {
        &self.title
    }
    fn to_root_bytes(&self) -> Vec<u8> {
        let mut w = WBuffer::new();
        let obj = w.begin_object(2); // TMultiGraph version 2
        write_tnamed(&mut w, 0, &self.name, &self.title);
        let members: Vec<(String, Vec<u8>)> = self
            .graphs
            .iter()
            .map(|g| (g.class_name().to_string(), g.to_root_bytes()))
            .collect();
        write_object_list(&mut w, "", &members); // fGraphs
        w.be_u32(0); // fFunctions (null TList*)
        w.be_u32(0); // fHistogram (null TH1F*)
        w.be_f64(UNSET_LIMIT); // fMaximum
        w.be_f64(UNSET_LIMIT); // fMinimum
        w.end_object(obj);
        w.into_vec()
    }
}

fn decode_tmultigraph(class: &str, object: &[u8], keylen: usize) -> Result<TMultiGraph> {
    if class != "TMultiGraph" {
        return Err(Error::Format(format!(
            "key is a {class}, not a TMultiGraph"
        )));
    }
    let mut r = RBuffer::new(object);
    r.read_version()?; // TMultiGraph version
    let (name, title) = read_tnamed(&mut r)?;
    let mut tags = TagReader::new(keylen);
    let ranges = list_member_ranges(&mut r, &mut tags)?;
    let mut graphs = Vec::with_capacity(ranges.len());
    for (member_class, range) in ranges {
        if member_class.starts_with("TGraph") {
            graphs.push(decode_tgraph(&name, &member_class, &object[range])?);
        }
    }
    Ok(TMultiGraph {
        name,
        title,
        graphs,
    })
}

pub(crate) fn read_tmultigraph(file: &RFile, name: &str) -> Result<TMultiGraph> {
    let (class, object, keylen) = object_bytes_any_keyed(file, name)?;
    decode_tmultigraph(&class, &object, keylen)
}

pub(crate) fn read_tmultigraph_in(file: &RFile, subdir: &str, name: &str) -> Result<TMultiGraph> {
    let (class, object, keylen) = file.object_in_keyed(subdir, name)?;
    decode_tmultigraph(&class, &object, keylen)
}
