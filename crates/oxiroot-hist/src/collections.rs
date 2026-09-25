//! Collection objects holding other objects: [`HistStack`] (a stack of
//! histograms) and [`GraphStack`] (several graphs drawn together). Both store
//! their members in a `TList` and serialize byte-for-byte as ROOT does, so ROOT
//! and uproot read what oxiroot writes and vice versa.
//!
//! The members are written through the generic object protocol — each one with
//! a fresh class tag (`kNewClassTag`), never a back-reference — so writing is
//! position-independent. Reading uses [`TagReader`], which resolves both the
//! class tags oxiroot writes and the back-references ROOT writes.

use std::ops::Range;

use oxiroot_io_core::streamer_gen::{base, basic, objptr, Cls};
use oxiroot_io_core::{
    read_object_base, write_named, write_object_any, write_object_base, Error, FileReader, RBuffer,
    Result, StreamerSet, TagReader, WBuffer, K_BYTE_COUNT_MASK,
};

use crate::base::object_bytes_any_keyed;
use crate::graph::{decode_tgraph, Graph};
use crate::hist1d::{decode_th1, Hist1D};
use crate::threaded::Mergeable;
use crate::write::{hist_streamer_classes, WriteRoot};

const K_NEW_CLASS_TAG: u32 = 0xFFFF_FFFF;
const K_CLASS_MASK: u32 = 0x8000_0000;
/// HistStack/GraphStack leave `fMaximum`/`fMinimum` at this sentinel until drawn.
const UNSET_LIMIT: f64 = -1111.0;

// --- streamer info -----------------------------------------------------------
//
// uproot models a HistStack or GraphStack only from its streamer, so files that
// store one embed these entries (versions and checksums as ROOT writes them).

/// The `TStreamerInfo` of `HistStack`.
fn thstack_class() -> Cls<'static> {
    Cls {
        name: "THStack".into(),
        version: 2,
        checksum: 1_918_797_077,
        elements: vec![
            base("TNamed", 1),
            objptr("fHists", "TList*"),
            objptr("fHistogram", "TH1*"),
            basic("fMaximum", 8, 8, "double"),
            basic("fMinimum", 8, 8, "double"),
        ],
    }
}

/// The `TStreamerInfo` of `GraphStack`.
fn tmultigraph_class() -> Cls<'static> {
    Cls {
        name: "TMultiGraph".into(),
        version: 2,
        checksum: 3_767_090_389,
        elements: vec![
            base("TNamed", 1),
            objptr("fGraphs", "TList*"),
            objptr("fFunctions", "TList*"),
            objptr("fHistogram", "TH1F*"),
            basic("fMaximum", 8, 8, "double"),
            basic("fMinimum", 8, 8, "double"),
        ],
    }
}

// --- shared object-protocol helpers -----------------------------------------

/// Write a `TList*` member named `list_name` holding `members` (each a
/// `(class, body)` pair), wrapped as a `TList` object via [`write_object_any`].
fn write_object_list(w: &mut WBuffer, list_name: &str, members: &[(String, Vec<u8>)]) {
    let mut body = WBuffer::new();
    let list = body.begin_object(5); // TList version 5
    write_object_base(&mut body, 0);
    body.string(list_name); // fName
    body.be_i32(members.len() as i32); // nobjects
    for (class, member) in members {
        write_object_any(&mut body, class, member);
        body.string(""); // the per-object option string
    }
    body.end_object(list);
    write_object_any(w, "TList", &body.into_vec());
}

/// Read a `Named` base (version header, `TObject`, `fName`, `fTitle`).
fn read_named(r: &mut RBuffer) -> Result<(String, String)> {
    r.read_version()?;
    read_object_base(r)?;
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
    read_object_base(r)?;
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

// --- HistStack --------------------------------------------------------------

/// A `HistStack` — a named stack of histograms (drawn overlaid or summed). Build
/// one with [`HistStack::new`], name it with [`named`](HistStack::named), and
/// [`add`](HistStack::add) the histograms; write it through
/// [`FileWriter`](crate::FileWriter) or [`write_root`](crate::WriteRoot::write_root).
#[derive(Debug, Clone, Default, PartialEq)]
#[doc(alias = "THStack")]
pub struct HistStack {
    name: String,
    title: String,
    hists: Vec<Hist1D>,
}

impl HistStack {
    /// An empty stack (give it a key name with [`named`](Self::named)).
    pub fn new() -> HistStack {
        HistStack::default()
    }

    /// Set the key name this stack is stored under.
    #[must_use]
    pub fn named(mut self, name: impl Into<String>) -> HistStack {
        self.name = name.into();
        self
    }

    /// Set the stack's title.
    #[must_use]
    pub fn titled(mut self, title: impl Into<String>) -> HistStack {
        self.title = title.into();
        self
    }

    /// Add a histogram to the stack.
    // `add` is the natural builder verb here; it is not the arithmetic `Add::add`.
    #[allow(clippy::should_implement_trait)]
    #[must_use]
    pub fn add(mut self, hist: Hist1D) -> HistStack {
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
    pub fn hists(&self) -> &[Hist1D] {
        &self.hists
    }
}

impl Mergeable for HistStack {
    /// Merge the stacks' histograms by name, as ROOT's `HistStack::Merge` and
    /// `hadd` do: a histogram both stacks hold is summed, one only `other`
    /// holds is appended.
    ///
    /// Returns [`oxiroot_io_core::Error::BinningMismatch`] if two histograms of
    /// the same name have different binnings; the histograms merged before it
    /// keep their sums.
    fn merge(&mut self, other: &HistStack) -> Result<()> {
        for from in &other.hists {
            match self.hists.iter_mut().find(|h| h.name == from.name) {
                Some(h) => h.add(from, 1.0)?,
                None => self.hists.push(from.clone()),
            }
        }
        Ok(())
    }
}

impl WriteRoot for HistStack {
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
        let obj = w.begin_object(2); // HistStack version 2
        write_named(&mut w, 0, &self.name, &self.title);
        let members: Vec<(String, Vec<u8>)> = self
            .hists
            .iter()
            .map(|h| (h.class_name(), h.to_root_bytes()))
            .collect();
        write_object_list(&mut w, "", &members); // fHists
        w.be_u32(0); // fHistogram (null Hist1D*)
        w.be_f64(UNSET_LIMIT); // fMaximum
        w.be_f64(UNSET_LIMIT); // fMinimum
        w.end_object(obj);
        w.into_vec()
    }
    fn streamer_classes(&self) -> Vec<Cls<'static>> {
        // Its bases and list, the stacked histograms' classes, then its own.
        let mut set = StreamerSet::default();
        set.add_classes(hist_streamer_classes(&["TNamed", "TList"]));
        for h in &self.hists {
            set.add(h);
        }
        set.add_classes([thstack_class()]);
        set.classes().to_vec()
    }
}

fn decode_thstack(class: &str, object: &[u8], keylen: usize) -> Result<HistStack> {
    if class != "THStack" {
        return Err(Error::WrongClass {
            name: String::new(),
            found: class.to_string(),
            expected: "THStack".to_string(),
        });
    }
    let mut r = RBuffer::new(object);
    r.read_version()?; // HistStack version
    let (name, title) = read_named(&mut r)?;
    let mut tags = TagReader::new(keylen);
    let ranges = list_member_ranges(&mut r, &mut tags)?;
    let mut hists = Vec::with_capacity(ranges.len());
    for (member_class, range) in ranges {
        if member_class.starts_with("TH1") {
            hists.push(decode_th1((member_class, object[range].to_vec()))?);
        }
    }
    Ok(HistStack { name, title, hists })
}

pub(crate) fn read_thstack(file: &FileReader, name: &str) -> Result<HistStack> {
    let (class, object, keylen) = object_bytes_any_keyed(file, name)?;
    decode_thstack(&class, &object, keylen)
}

pub(crate) fn read_thstack_in(file: &FileReader, subdir: &str, name: &str) -> Result<HistStack> {
    let (class, object, keylen) = file.object_in_keyed(subdir, name)?;
    decode_thstack(&class, &object, keylen)
}

// --- GraphStack -------------------------------------------------------------

/// A `GraphStack` — several [`Graph`]s drawn in one frame. Build with
/// [`GraphStack::new`], name it with [`named`](GraphStack::named), and
/// [`add`](GraphStack::add) the graphs.
#[derive(Debug, Clone, Default, PartialEq)]
#[doc(alias = "TMultiGraph")]
pub struct GraphStack {
    name: String,
    title: String,
    graphs: Vec<Graph>,
}

impl GraphStack {
    /// An empty multigraph (give it a key name with [`named`](Self::named)).
    pub fn new() -> GraphStack {
        GraphStack::default()
    }

    /// Set the key name this multigraph is stored under.
    #[must_use]
    pub fn named(mut self, name: impl Into<String>) -> GraphStack {
        self.name = name.into();
        self
    }

    /// Set the multigraph's title.
    #[must_use]
    pub fn titled(mut self, title: impl Into<String>) -> GraphStack {
        self.title = title.into();
        self
    }

    /// Add a graph to the multigraph.
    // `add` is the natural builder verb here; it is not the arithmetic `Add::add`.
    #[allow(clippy::should_implement_trait)]
    #[must_use]
    pub fn add(mut self, graph: Graph) -> GraphStack {
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
    pub fn graphs(&self) -> &[Graph] {
        &self.graphs
    }
}

impl WriteRoot for GraphStack {
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
        let obj = w.begin_object(2); // GraphStack version 2
        write_named(&mut w, 0, &self.name, &self.title);
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
    fn streamer_classes(&self) -> Vec<Cls<'static>> {
        // Its bases and list, the graphs' classes, then its own.
        let mut set = StreamerSet::default();
        set.add_classes(hist_streamer_classes(&["TNamed", "TList"]));
        for g in &self.graphs {
            set.add(g);
        }
        set.add_classes([tmultigraph_class()]);
        set.classes().to_vec()
    }
}

fn decode_tmultigraph(class: &str, object: &[u8], keylen: usize) -> Result<GraphStack> {
    if class != "TMultiGraph" {
        return Err(Error::WrongClass {
            name: String::new(),
            found: class.to_string(),
            expected: "TMultiGraph".to_string(),
        });
    }
    let mut r = RBuffer::new(object);
    r.read_version()?; // GraphStack version
    let (name, title) = read_named(&mut r)?;
    let mut tags = TagReader::new(keylen);
    let ranges = list_member_ranges(&mut r, &mut tags)?;
    let mut graphs = Vec::with_capacity(ranges.len());
    for (member_class, range) in ranges {
        if member_class.starts_with("TGraph") {
            graphs.push(decode_tgraph(&name, &member_class, &object[range])?);
        }
    }
    Ok(GraphStack {
        name,
        title,
        graphs,
    })
}

pub(crate) fn read_tmultigraph(file: &FileReader, name: &str) -> Result<GraphStack> {
    let (class, object, keylen) = object_bytes_any_keyed(file, name)?;
    decode_tmultigraph(&class, &object, keylen)
}

pub(crate) fn read_tmultigraph_in(
    file: &FileReader,
    subdir: &str,
    name: &str,
) -> Result<GraphStack> {
    let (class, object, keylen) = file.object_in_keyed(subdir, name)?;
    decode_tmultigraph(&class, &object, keylen)
}
