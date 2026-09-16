//! [`RootFile`]: composing a ROOT file from several objects, optionally in
//! subdirectories, or appending them to an existing file.

use std::io::Cursor;
use std::path::Path;

use crate::error::{Error, Result};
use crate::object_io::{record_of, ObjectRecord, StreamerSet, WriteInto, WriteRoot};
use crate::Compression;

use super::container::{ContainerWriter, DirId, KSTART_BIG_FILE};

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
                return Err(Error::Format(format!(
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
                return Err(Error::Format(format!(
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

/// Builder for composing a ROOT file from several objects — optionally organised
/// into subdirectories, or appended to an existing file.
///
/// For the common case of a single object, prefer the
/// [`WriteRoot::write_root`] shorthand. Reach for `RootFile` when a file holds
/// several objects, uses subdirectories, or is being appended to. Any mix of
/// writable types can go in one file: [`add`](RootFile::add) takes objects
/// stored under a single key (histograms, matrices, parameters, …), and
/// [`put`](RootFile::put) takes ones stored as several records (a `TTree`, an
/// RNTuple):
///
/// ```no_run
/// use oxiroot_io_core::{Compression, RootFile, TObjString, TParameter};
/// let lumi = TParameter::f64("lumi", 137.5);
/// let label = TObjString::new("2024 run").named("label");
/// let cut = TParameter::f32("pt_min", 25.0);
/// RootFile::create("out.root")
///     .add(&lumi)
///     .add(&label)
///     .dir("cuts", |d| d.add(&cut)) // a TDirectory holding `pt_min`
///     .write(Compression::Zstd(5))?;
/// # Ok::<(), oxiroot_io_core::Error>(())
/// ```
///
/// Append to an existing file with [`open`](RootFile::open):
///
/// ```no_run
/// # use oxiroot_io_core::{Compression, RootFile, TParameter};
/// # let extra = TParameter::i32("extra", 3);
/// RootFile::open("out.root")?.add(&extra).write(Compression::None)?;
/// # Ok::<(), oxiroot_io_core::Error>(())
/// ```
#[must_use = "a RootFile builder does nothing until `.write(...)` is called"]
pub struct RootFile {
    path: std::path::PathBuf,
    /// `Some` in append mode (the existing file bytes); `None` for a fresh file.
    existing: Option<Vec<u8>>,
    top: Entries,
    dirs: Vec<(String, Entries)>,
}

impl RootFile {
    /// Start a fresh ROOT file at `path` (any existing file is overwritten on
    /// [`write`](RootFile::write)).
    pub fn create(path: impl AsRef<Path>) -> RootFile {
        RootFile {
            path: path.as_ref().to_path_buf(),
            existing: None,
            top: Entries::default(),
            dirs: Vec::new(),
        }
    }

    /// Open an existing ROOT file at `path` to append more objects to its top
    /// directory: the current contents are kept (appended in place, so existing
    /// objects never move) and the added objects written after them. A new object
    /// whose name matches an existing one lands at a higher cycle, as ROOT does.
    /// Files that contain subdirectories or an RNTuple are preserved — only
    /// *adding* new subdirectories in this mode is unsupported. See
    /// [`ContainerWriter::append`].
    pub fn open(path: impl AsRef<Path>) -> Result<RootFile> {
        let path = path.as_ref().to_path_buf();
        let existing = std::fs::read(&path)?;
        Ok(RootFile {
            path,
            existing: Some(existing),
            top: Entries::default(),
            dirs: Vec::new(),
        })
    }

    /// Add an object to the file's top directory. It is serialized now, so it
    /// only needs to be borrowed.
    // `add` is the natural builder verb here; it is not the arithmetic `Add::add`.
    #[allow(clippy::should_implement_trait)]
    pub fn add(mut self, object: &dyn WriteRoot) -> RootFile {
        self.top.add(object);
        self
    }

    /// Put a multi-record object (a `TTree`, an RNTuple) in the file's top
    /// directory. It is laid out when the file is written, so the builder takes
    /// it over.
    pub fn put(mut self, object: impl WriteInto + 'static) -> RootFile {
        self.top.put(object);
        self
    }

    /// Add a `TDirectory` named `name` holding the objects added inside `build`
    /// (e.g. one directory per analysis region). Only meaningful when creating a
    /// file; see [`open`](RootFile::open).
    pub fn dir(mut self, name: impl Into<String>, build: impl FnOnce(Dir) -> Dir) -> RootFile {
        let dir = build(Dir {
            entries: Entries::default(),
        });
        self.dirs.push((name.into(), dir.entries));
        self
    }

    /// Build the file bytes and write them to the path. A fresh builder writes a
    /// new file; one from [`open`](RootFile::open) rewrites the file with its
    /// existing contents plus the additions. A file that grows past ~2 GiB is
    /// written in ROOT's 64-bit ("big") container form automatically.
    pub fn write(self, compression: Compression) -> Result<()> {
        self.write_threshold(compression, KSTART_BIG_FILE)
    }

    /// Like [`write`](RootFile::write), but switch to the 64-bit container form
    /// once the file would exceed `threshold` bytes rather than ROOT's ~2 GiB
    /// ([`KSTART_BIG_FILE`]). A threshold of 0 always writes the 64-bit form,
    /// which is useful for testing readers against it.
    pub fn write_threshold(self, compression: Compression, threshold: u64) -> Result<()> {
        let bytes = self.build(compression, threshold)?;
        std::fs::write(&self.path, bytes)?;
        Ok(())
    }

    /// The bytes [`write`](RootFile::write) would write, without writing them.
    pub fn to_bytes(&self, compression: Compression) -> Result<Vec<u8>> {
        self.build(compression, KSTART_BIG_FILE)
    }

    fn build(&self, compression: Compression, threshold: u64) -> Result<Vec<u8>> {
        // Reject unnamed / clashing keys before writing — loudly, instead of
        // ROOT's silent shadow-on-read.
        let subdir_names: Vec<&str> = self.dirs.iter().map(|(name, _)| name.as_str()).collect();
        self.top.check_names(&subdir_names, "the top directory")?;
        let mut streamers = self.top.streamers.clone();
        for (name, dir) in &self.dirs {
            dir.check_names(&[], &format!("subdirectory {name:?}"))?;
            streamers.extend(&dir.streamers);
        }
        let file_name = self
            .path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("file.root");
        let layout = |c: &mut ContainerWriter<Cursor<Vec<u8>>>| {
            self.top.place(c, DirId::TOP)?;
            // When appending to a file that has streamer info, only the generated
            // classes it lacks are added (readers know the histogram family).
            c.place_streamer_info(streamers.list(), streamers.classes())?;
            for (name, entries) in &self.dirs {
                let id = c.mkdir(DirId::TOP, name)?;
                entries.place(c, id)?;
                c.close_dir(id)?;
            }
            Ok(())
        };
        match &self.existing {
            Some(existing) => {
                if !self.dirs.is_empty() {
                    return Err(Error::Format(
                        "adding new subdirectories while appending is not supported \
                         (append adds objects to the top directory; existing \
                         subdirectories are preserved)"
                            .to_string(),
                    ));
                }
                ContainerWriter::build_append(existing, file_name, compression, threshold, layout)
            }
            None => ContainerWriter::build(file_name, compression, threshold, layout),
        }
    }
}

/// A subdirectory being built inside a [`RootFile`]; see [`RootFile::dir`].
#[must_use]
pub struct Dir {
    entries: Entries,
}

impl Dir {
    /// Add an object to this subdirectory.
    #[allow(clippy::should_implement_trait)]
    pub fn add(mut self, object: &dyn WriteRoot) -> Dir {
        self.entries.add(object);
        self
    }

    /// Put a multi-record object (a `TTree`, an RNTuple) in this subdirectory.
    pub fn put(mut self, object: impl WriteInto + 'static) -> Dir {
        self.entries.put(object);
        self
    }
}
