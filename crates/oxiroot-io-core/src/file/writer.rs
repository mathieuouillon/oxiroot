//! [`FileWriter`]: composing a ROOT file from several objects, optionally in
//! subdirectories, or appending them to an existing file.

use std::collections::{HashMap, HashSet};
use std::io::Cursor;
use std::path::Path;

use crate::error::{decompress_payload, Error, Result};
use crate::object_io::{record_of, ObjectRecord, StreamerSet, WriteInto, WriteRoot};
use crate::streamer_gen::StreamerInfoList;
use crate::Compression;

use super::container::{ContainerWriter, DirId, KSTART_BIG_FILE};
use super::key::TKey;
use super::reader::split_cycle;
use super::reader::FileReader;

/// One key in a directory being built: a streamed object, or a multi-record
/// object laid out when the file is written.
enum Entry {
    Object(ObjectRecord),
    Records(Box<dyn WriteInto>),
}

impl Entry {
    fn class(&self) -> String {
        match self {
            Entry::Object(r) => r.class_name.clone(),
            Entry::Records(o) => o.root_class(),
        }
    }

    fn name(&self) -> &str {
        match self {
            Entry::Object(r) => &r.name,
            Entry::Records(o) => o.root_name(),
        }
    }

    fn place(&self, file: &mut ContainerWriter<Cursor<Vec<u8>>>, dir: DirId) -> Result<()> {
        match self {
            Entry::Object(r) => file
                .place_key(dir, &r.class_name, &r.name, &r.title, &r.object)
                .map(drop),
            Entry::Records(o) => o.write_into(file, dir),
        }
    }
}

/// The keys of one directory being built, and what they need described.
#[derive(Default)]
struct Entries {
    entries: Vec<Entry>,
    streamers: StreamerSet,
}

impl Entries {
    fn add(&mut self, object: &dyn WriteRoot) {
        self.streamers.add(object);
        self.entries.push(Entry::Object(record_of(object)));
    }

    fn put(&mut self, object: impl WriteInto + 'static) {
        self.streamers.add_records(&object);
        self.entries.push(Entry::Records(Box::new(object)));
    }

    /// Reject keys that cannot be addressed: an empty name (it could never be
    /// looked up) or two keys sharing a name (the second would silently shadow
    /// the first on read). `subdirs` names the subdirectories created in the same
    /// directory, which share its namespace.
    fn check_names(&self, subdirs: &[&str], location: &str) -> Result<()> {
        let mut seen = std::collections::HashSet::new();
        for entry in &self.entries {
            let name = entry.name();
            if name.is_empty() {
                return Err(Error::InvalidInput(format!(
                    "cannot write an unnamed {} in {location}; give it a key name with `.named(\"...\")`",
                    entry.class()
                )));
            }
            if !seen.insert(name) {
                return Err(Error::DuplicateName {
                    name: name.to_string(),
                    location: location.to_string(),
                });
            }
        }
        for &name in subdirs {
            if name.is_empty() {
                return Err(Error::InvalidInput(format!(
                    "cannot create a subdirectory with an empty name in {location}"
                )));
            }
            if !seen.insert(name) {
                return Err(Error::DuplicateName {
                    name: name.to_string(),
                    location: location.to_string(),
                });
            }
        }
        Ok(())
    }

    fn place(&self, file: &mut ContainerWriter<Cursor<Vec<u8>>>, dir: DirId) -> Result<()> {
        for entry in &self.entries {
            entry.place(file, dir)?;
        }
        Ok(())
    }
}

/// Composes a ROOT file from several objects — optionally organised into
/// subdirectories, or appended to an existing file — and writes it with
/// [`write`](FileWriter::write). [`FileReader`](crate::FileReader) reads files.
///
/// For the common case of a single object, prefer the
/// [`WriteRoot::write_root`] shorthand. Reach for `FileWriter` when a file holds
/// several objects, uses subdirectories, or is being appended to. Any mix of
/// writable types can go in one file: [`add`](FileWriter::add) takes objects
/// stored under a single key (histograms, matrices, parameters, …), and
/// [`put`](FileWriter::put) takes ones stored as several records (a `TTree`, an
/// RNTuple):
///
/// ```no_run
/// use oxiroot_io_core::{Compression, FileWriter, TObjString, TParameter};
/// let lumi = TParameter::f64("lumi", 137.5);
/// let label = TObjString::new("2024 run").named("label");
/// let cut = TParameter::f32("pt_min", 25.0);
/// FileWriter::create("out.root")
///     .add(&lumi)
///     .add(&label)
///     .dir("cuts", |d| d.add(&cut)) // a TDirectory holding `pt_min`
///     .write(Compression::Zstd(5))?;
/// # Ok::<(), oxiroot_io_core::Error>(())
/// ```
///
/// Append to an existing file with [`open`](FileWriter::open):
///
/// ```no_run
/// # use oxiroot_io_core::{Compression, FileWriter, TParameter};
/// # let extra = TParameter::i32("extra", 3);
/// FileWriter::open("out.root")?.add(&extra).write(Compression::None)?;
/// # Ok::<(), oxiroot_io_core::Error>(())
/// ```
#[doc(alias = "RootFile", alias = "TFile")]
#[must_use = "a FileWriter does nothing until `.write(...)` is called"]
pub struct FileWriter {
    path: std::path::PathBuf,
    /// `Some` in append mode (the existing file bytes); `None` for a fresh file.
    existing: Option<Vec<u8>>,
    top: Entries,
    dirs: Vec<DirNode>,
    /// Names to take out of the top directory, `"h"` or `"h;2"`, before the
    /// added objects are written (update mode only).
    deleted: Vec<String>,
    /// Keep only the highest cycle of each name in the top directory.
    purge: bool,
    /// Rewrite the file from what survives, instead of appending to it.
    compact: bool,
}

impl FileWriter {
    /// Start a fresh ROOT file at `path` (any existing file is overwritten on
    /// [`write`](FileWriter::write)).
    pub fn create(path: impl AsRef<Path>) -> FileWriter {
        FileWriter {
            path: path.as_ref().to_path_buf(),
            existing: None,
            top: Entries::default(),
            dirs: Vec::new(),
            deleted: Vec::new(),
            purge: false,
            compact: false,
        }
    }

    /// Open an existing ROOT file at `path` to append more objects to its top
    /// directory: the current contents are kept (appended in place, so existing
    /// objects never move) and the added objects written after them. A new object
    /// whose name matches an existing one lands at a higher cycle, as ROOT does.
    /// Files that contain subdirectories or an RNTuple are preserved — only
    /// *adding* new subdirectories in this mode is unsupported. See
    /// [`ContainerWriter::append`].
    pub fn open(path: impl AsRef<Path>) -> Result<FileWriter> {
        let path = path.as_ref().to_path_buf();
        let existing = std::fs::read(&path)?;
        Ok(FileWriter {
            path,
            existing: Some(existing),
            top: Entries::default(),
            dirs: Vec::new(),
            deleted: Vec::new(),
            purge: false,
            compact: false,
        })
    }

    /// Add an object to the file's top directory. It is serialized now, so it
    /// only needs to be borrowed.
    // `add` is the natural builder verb here; it is not the arithmetic `Add::add`.
    #[allow(clippy::should_implement_trait)]
    pub fn add(mut self, object: &dyn WriteRoot) -> FileWriter {
        self.top.add(object);
        self
    }

    /// Put a multi-record object (a `TTree`, an RNTuple) in the file's top
    /// directory. It is laid out when the file is written, so the builder takes
    /// it over.
    pub fn put(mut self, object: impl WriteInto + 'static) -> FileWriter {
        self.top.put(object);
        self
    }

    /// Take `name` out of the file's top directory: every cycle of it, or the
    /// one an explicit `"name;2"` asks for. Update mode only (see
    /// [`open`](FileWriter::open)); writing fails if the file holds no such key,
    /// rather than quietly doing nothing.
    ///
    /// The record stays where it is and the file does not shrink — what changes
    /// is that the directory no longer lists it, so nothing reads it. This is
    /// ROOT's `TFile::Delete`; to reclaim the space, write the objects you keep
    /// to a fresh file with [`create`](FileWriter::create).
    ///
    /// ```no_run
    /// # use oxiroot_io_core::{Compression, FileWriter};
    /// # fn main() -> oxiroot_io_core::Result<()> {
    /// FileWriter::open("out.root")?
    ///     .delete("scratch")   // every cycle of `scratch`
    ///     .delete("h;1")       // and the first cycle of `h`
    ///     .write(Compression::None)?;
    /// # Ok(()) }
    /// ```
    pub fn delete(mut self, name: impl Into<String>) -> FileWriter {
        self.deleted.push(name.into());
        self
    }

    /// Keep only the current (highest) cycle of each name in the file's top
    /// directory, dropping the older ones — ROOT's `TFile::Purge`. Update mode
    /// only, and, like [`delete`](FileWriter::delete), the file does not shrink.
    pub fn purge(mut self) -> FileWriter {
        self.purge = true;
        self
    }

    /// Write the file afresh from what survives, rather than appending to it, so
    /// that deleted and superseded objects stop taking up space. Update mode
    /// only, and the objects keep their names, titles, cycles order and
    /// subdirectories; the file describes the classes it still holds.
    ///
    /// A `TTree` or an RNTuple is more than its key — its baskets and pages sit
    /// elsewhere in the file — so compacting a file that holds one is refused
    /// rather than half-done. Merge those with `oxiroot::hadd` instead.
    ///
    /// ```no_run
    /// # use oxiroot_io_core::{Compression, FileWriter};
    /// # fn main() -> oxiroot_io_core::Result<()> {
    /// FileWriter::open("out.root")?
    ///     .purge()      // drop the superseded cycles …
    ///     .compact()    // … and take back the space they held
    ///     .write(Compression::Zstd(5))?;
    /// # Ok(()) }
    /// ```
    pub fn compact(mut self) -> FileWriter {
        self.compact = true;
        self
    }

    /// Add a `TDirectory` named `name` holding the objects added inside `build`
    /// (e.g. one directory per analysis region). Only meaningful when creating a
    /// file; see [`open`](FileWriter::open).
    pub fn dir(
        mut self,
        name: impl Into<String>,
        build: impl FnOnce(SubdirWriter) -> SubdirWriter,
    ) -> FileWriter {
        self.dirs.push(SubdirWriter::build(name, build));
        self
    }

    /// Build the file bytes and write them to the path. A fresh builder writes a
    /// new file; one from [`open`](FileWriter::open) rewrites the file with its
    /// existing contents plus the additions. A file that grows past ~2 GiB is
    /// written in ROOT's 64-bit ("big") container form automatically.
    pub fn write(self, compression: Compression) -> Result<()> {
        self.write_threshold(compression, KSTART_BIG_FILE)
    }

    /// Like [`write`](FileWriter::write), but switch to the 64-bit container form
    /// once the file would exceed `threshold` bytes rather than ROOT's ~2 GiB
    /// ([`KSTART_BIG_FILE`]). A threshold of 0 always writes the 64-bit form,
    /// which is useful for testing readers against it.
    pub fn write_threshold(self, compression: Compression, threshold: u64) -> Result<()> {
        let bytes = self.build(compression, threshold)?;
        std::fs::write(&self.path, bytes)?;
        Ok(())
    }

    /// Which of `keys` survive this builder's deletions and purge, in their
    /// original order. Errors on a name the file does not hold, so a typo is not
    /// silently a no-op.
    fn surviving<'k>(&self, keys: &'k [TKey]) -> Result<Vec<&'k TKey>> {
        let mut kept: Vec<&TKey> = keys.iter().filter(|k| !k.is_deleted()).collect();
        for spec in &self.deleted {
            let (name, cycle) = split_cycle(spec);
            let before = kept.len();
            kept.retain(|key| key.name != name || cycle.is_some_and(|c| key.cycle != c));
            if kept.len() == before {
                return Err(Error::NotFound {
                    what: "key to delete",
                    name: spec.clone(),
                });
            }
        }
        if self.purge {
            let current = kept.iter().fold(HashMap::new(), |mut top, key| {
                let cycle: &mut u16 = top.entry(key.name.as_str()).or_default();
                *cycle = (*cycle).max(key.cycle);
                top
            });
            kept.retain(|key| current.get(key.name.as_str()) == Some(&key.cycle));
        }
        Ok(kept)
    }

    /// Take the deleted names, and the superseded cycles when purging, out of
    /// the top directory's key list.
    fn apply_deletions(&self, c: &mut ContainerWriter<Cursor<Vec<u8>>>) -> Result<()> {
        if self.deleted.is_empty() && !self.purge {
            return Ok(());
        }
        let keys = c.keys(DirId::TOP)?.to_vec();
        let kept: HashSet<(String, u16)> = self
            .surviving(&keys)?
            .iter()
            .map(|key| (key.name.clone(), key.cycle))
            .collect();
        c.drop_keys(DirId::TOP, |key| {
            kept.contains(&(key.name.clone(), key.cycle))
        })?;
        Ok(())
    }

    /// The bytes [`write`](FileWriter::write) would write, without writing them.
    pub fn to_bytes(&self, compression: Compression) -> Result<Vec<u8>> {
        self.build(compression, KSTART_BIG_FILE)
    }

    fn build(&self, compression: Compression, threshold: u64) -> Result<Vec<u8>> {
        // Reject unnamed / clashing keys before writing — loudly, instead of
        // ROOT's silent shadow-on-read.
        let subdir_names: Vec<&str> = self.dirs.iter().map(|d| d.name.as_str()).collect();
        self.top.check_names(&subdir_names, "the top directory")?;
        let mut streamers = self.top.streamers.clone();
        for dir in &self.dirs {
            dir.check_names(&dir.name)?;
            dir.collect_streamers(&mut streamers);
        }
        let file_name = self
            .path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("file.root");
        let layout = |c: &mut ContainerWriter<Cursor<Vec<u8>>>| {
            // Deletions come first, so an object added in the same pass takes
            // the next cycle after what is left.
            self.apply_deletions(c)?;
            self.top.place(c, DirId::TOP)?;
            // When appending to a file that has streamer info, only the generated
            // classes it lacks are added (readers know the histogram family).
            c.place_streamer_info(&[], streamers.classes())?;
            for dir in &self.dirs {
                dir.place(c, DirId::TOP)?;
            }
            Ok(())
        };
        if !self.deleted.is_empty() || self.purge {
            if self.existing.is_none() {
                return Err(Error::InvalidInput(
                    "delete and purge need a file to work on: open it with \
                     FileWriter::open, which writes in update mode"
                        .to_string(),
                ));
            }
            for name in &self.deleted {
                if name.trim().is_empty() {
                    return Err(Error::InvalidInput(
                        "cannot delete a key with an empty name".to_string(),
                    ));
                }
            }
        }
        match (&self.existing, self.compact) {
            (Some(existing), true) => {
                self.rebuild(existing, file_name, compression, threshold, &streamers)
            }
            (Some(existing), false) => {
                if !self.dirs.is_empty() {
                    return Err(Error::Unsupported(
                        "adding new subdirectories while appending is not supported \
                         (append adds objects to the top directory; existing \
                         subdirectories are preserved)"
                            .to_string(),
                    ));
                }
                ContainerWriter::build_append(existing, file_name, compression, threshold, layout)
            }
            (None, true) => Err(Error::InvalidInput(
                "compact needs a file to rewrite: open it with FileWriter::open".to_string(),
            )),
            (None, false) => ContainerWriter::build(file_name, compression, threshold, layout),
        }
    }

    /// Write the file afresh, holding what survives the deletions and the purge
    /// plus whatever this builder adds, so the space the rest held is given up.
    fn rebuild(
        &self,
        existing: &[u8],
        file_name: &str,
        compression: Compression,
        threshold: u64,
        streamers: &StreamerSet,
    ) -> Result<Vec<u8>> {
        let file = FileReader::from_bytes(existing.to_vec())?;
        let top = CopiedDir::read(&file, &self.surviving(file.keys())?, "")?;
        // The file describes what it still holds: the classes of the copied
        // objects, with what those depend on, plus the added objects' own.
        let described = match file.streamer_info_list()? {
            Some((list, key_len)) => {
                let classes = top.class_names();
                let names: Vec<&str> = classes.iter().map(String::as_str).collect();
                StreamerInfoList::parse_keyed(&list, key_len)
                    .map(|list| list.classes_for(&names))
                    .unwrap_or_default()
            }
            None => Vec::new(),
        };
        let mut classes = described;
        classes.extend(streamers.classes().iter().cloned());

        ContainerWriter::build(file_name, compression, threshold, |c| {
            top.place(c, DirId::TOP)?;
            self.top.place(c, DirId::TOP)?;
            c.place_streamer_info(&[], &classes)?;
            for dir in &self.dirs {
                dir.place(c, DirId::TOP)?;
            }
            Ok(())
        })
    }
}

/// A directory being copied from one file into another: the objects it holds,
/// read out whole, and the directories inside it.
struct CopiedDir {
    name: String,
    objects: Vec<ObjectRecord>,
    dirs: Vec<CopiedDir>,
}

impl CopiedDir {
    /// Read `keys` — the surviving keys of the directory at `path` — and every
    /// directory below it, out of `file`.
    fn read(file: &FileReader, keys: &[&TKey], path: &str) -> Result<CopiedDir> {
        let mut objects = Vec::new();
        let mut dirs = Vec::new();
        for key in keys {
            if matches!(key.class_name.as_str(), "TDirectory" | "TDirectoryFile") {
                let below = if path.is_empty() {
                    key.name.clone()
                } else {
                    format!("{path}/{}", key.name)
                };
                let sub = file.subdir(&below)?;
                let live: Vec<&TKey> = sub.keys.iter().filter(|k| !k.is_deleted()).collect();
                dirs.push(CopiedDir::read(file, &live, &below)?);
                continue;
            }
            // A TTree or an RNTuple is more than its key: its baskets and pages
            // sit elsewhere in the file, and copying the key alone would leave
            // them behind.
            if is_multi_record(&key.class_name) {
                return Err(Error::Unsupported(format!(
                    "cannot compact a file that holds {} {:?}: its records live outside its \
                     key. Merge the file with oxiroot::hadd instead",
                    key.class_name, key.name
                )));
            }
            let payload = file.key_payload(key)?;
            let object = decompress_payload(
                &payload,
                key.obj_len as usize,
                format_args!("key {:?}", key.name),
            )?;
            objects.push(ObjectRecord {
                class_name: key.class_name.clone(),
                name: key.name.clone(),
                title: key.title.clone(),
                object,
            });
        }
        Ok(CopiedDir {
            name: path.rsplit('/').next().unwrap_or(path).to_string(),
            objects,
            dirs,
        })
    }

    /// The class of every object copied here and below.
    fn class_names(&self) -> Vec<String> {
        let mut names: Vec<String> = self.objects.iter().map(|o| o.class_name.clone()).collect();
        for dir in &self.dirs {
            names.extend(dir.class_names());
        }
        names.sort();
        names.dedup();
        names
    }

    /// Write these objects into directory `dir`, then the directories below it.
    fn place(&self, c: &mut ContainerWriter<Cursor<Vec<u8>>>, dir: DirId) -> Result<()> {
        for object in &self.objects {
            c.place_key(
                dir,
                &object.class_name,
                &object.name,
                &object.title,
                &object.object,
            )?;
        }
        for below in &self.dirs {
            let id = c.mkdir(dir, &below.name)?;
            below.place(c, id)?;
            c.close_dir(id)?;
        }
        Ok(())
    }
}

/// Whether a class writes records outside its own key (a tree's baskets, an
/// RNTuple's pages), which a key-by-key copy would leave behind.
fn is_multi_record(class: &str) -> bool {
    matches!(class, "TTree" | "TNtuple" | "TNtupleD" | "TChain") || class.contains("RNTuple")
}

/// One subdirectory in a file being composed: what it holds, and the
/// subdirectories inside it.
struct DirNode {
    name: String,
    entries: Entries,
    dirs: Vec<DirNode>,
}

impl DirNode {
    /// Check this directory's names and those of every directory below it,
    /// naming each by the path a reader would use.
    fn check_names(&self, path: &str) -> Result<()> {
        let subdirs: Vec<&str> = self.dirs.iter().map(|d| d.name.as_str()).collect();
        self.entries
            .check_names(&subdirs, &format!("subdirectory {path:?}"))?;
        for dir in &self.dirs {
            dir.check_names(&format!("{path}/{}", dir.name))?;
        }
        Ok(())
    }

    /// Every streamer set in this directory and below it.
    fn collect_streamers(&self, into: &mut StreamerSet) {
        into.extend(&self.entries.streamers);
        for dir in &self.dirs {
            dir.collect_streamers(into);
        }
    }

    /// Write this directory inside `parent`: its own objects, then the
    /// directories inside it, which must exist before it closes.
    fn place(&self, c: &mut ContainerWriter<Cursor<Vec<u8>>>, parent: DirId) -> Result<()> {
        let id = c.mkdir(parent, &self.name)?;
        self.entries.place(c, id)?;
        for dir in &self.dirs {
            dir.place(c, id)?;
        }
        c.close_dir(id)?;
        Ok(())
    }
}

/// A subdirectory (a `TDirectory`) being composed inside a [`FileWriter`]; see
/// [`FileWriter::dir`]. The methods take and return `self`, so return the
/// `SubdirWriter` from the `dir` closure.
#[doc(
    alias = "SubdirBuilder",
    alias = "Dir",
    alias = "TDirectory",
    alias = "mkdir"
)]
#[must_use = "SubdirWriter methods consume self; return it from the closure"]
pub struct SubdirWriter {
    entries: Entries,
    dirs: Vec<DirNode>,
}

impl SubdirWriter {
    /// Add an object to this subdirectory.
    #[allow(clippy::should_implement_trait)]
    pub fn add(mut self, object: &dyn WriteRoot) -> SubdirWriter {
        self.entries.add(object);
        self
    }

    /// Put a multi-record object (a `TTree`, an RNTuple) in this subdirectory.
    pub fn put(mut self, object: impl WriteInto + 'static) -> SubdirWriter {
        self.entries.put(object);
        self
    }

    /// Add a `TDirectory` named `name` inside this one, holding what `build`
    /// adds — the same call as [`FileWriter::dir`], so directories nest as deep
    /// as you write them.
    pub fn dir(
        mut self,
        name: impl Into<String>,
        build: impl FnOnce(SubdirWriter) -> SubdirWriter,
    ) -> SubdirWriter {
        self.dirs.push(SubdirWriter::build(name, build));
        self
    }

    /// Run `build` over a fresh subdirectory and take what it composed.
    fn build(name: impl Into<String>, build: impl FnOnce(SubdirWriter) -> SubdirWriter) -> DirNode {
        let dir = build(SubdirWriter {
            entries: Entries::default(),
            dirs: Vec::new(),
        });
        DirNode {
            name: name.into(),
            entries: dir.entries,
            dirs: dir.dirs,
        }
    }
}
