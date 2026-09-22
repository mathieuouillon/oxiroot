//! Laying out a ROOT `TFile` container.
//!
//! [`ContainerWriter`] is the one place that knows the file's structure: the
//! 100-byte header, the directory records, each directory's key list, the
//! streamer-info record, and the switch between the 32-bit ("small") and 64-bit
//! ("big") forms. Format writers (histograms, `TTree`, RNTuple) only decide what
//! goes where: [`place_key`](ContainerWriter::place_key) stores an object under
//! a `TKey`, and [`place_blob`](ContainerWriter::place_blob) stores raw bytes that
//! something else points at by absolute offset (RNTuple pages, `TTree` baskets).
//!
//! The writer streams to any `Write + Seek` sink and back-patches the header and
//! directory records at the end. [`ContainerWriter::build`] runs a layout in
//! memory and picks the form from the result's size.

use std::borrow::Cow;
use std::io::{Cursor, Seek, SeekFrom, Write};

use crate::buffer::{RBuffer, WBuffer};
use crate::error::{Error, Result};
use crate::streamer_gen::{append_streamer_infos, streamer_info_list, Cls};
use crate::streamer_info::{parse_streamer_info, StreamerRegistry};
use crate::Compression;

use super::header::{TUuid, BIG_FILE_VERSION, MAGIC};
use super::key::{TDatime, TKey};
use super::reader::FileReader;

/// `fVersion` of a new small-form file (ROOT 6.24's format). The big form adds
/// [`BIG_FILE_VERSION`].
pub const FILE_VERSION: u32 = 62400;

/// The creation and modification time stamped on every key and directory
/// record. Readers don't check it, and a constant keeps output reproducible.
pub const DATIME: u32 = 0x7d7a_79ca;

/// ROOT switches a file to the 64-bit form once any file pointer would exceed
/// this many bytes (`kStartBigFile`). Past it, keys, directory records and the
/// file header all widen their seek fields to 64 bits.
pub const KSTART_BIG_FILE: u64 = 2_000_000_000;

/// `fBEGIN` of a new file: the top directory's key follows the 100-byte header.
const BEGIN: u64 = 100;

/// Class, name and title of the streamer-info key, as ROOT writes them.
const STREAMER_INFO_CLASS: &str = "TList";
const STREAMER_INFO_NAME: &str = "StreamerInfo";
const STREAMER_INFO_TITLE: &str = "Doubly linked list";

/// `KeyLen` of the streamer-info key in a new file. A streamed list refers back
/// to classes it already named by their offset inside the key, header included,
/// so a list captured from a ROOT file (like the baked histogram list) only reads
/// back under a key of the length it was captured with: 64 bytes, ROOT's
/// small-form length. The big form keeps that length with a shorter title.
const STREAMER_INFO_KEY_LEN: u16 = 64;

/// Class written on the top directory's own keys, and on subdirectory keys.
const TOP_DIR_CLASS: &str = "TFile";
const SUBDIR_CLASS: &str = "TDirectory";

/// Key version of the small form; the big form adds 1000, which is what readers
/// key on.
const KEY_VERSION_SMALL: u16 = 4;

/// The bytes to store for an object or page: compressed with `setting` (ROOT's
/// `algorithm*100 + level`, as returned by [`Compression::setting`]) when that
/// makes it smaller, otherwise the bytes unchanged. A reader tells the two apart
/// by comparing the stored length to the uncompressed one.
pub fn compress_if_smaller(bytes: &[u8], setting: u32) -> Cow<'_, [u8]> {
    if setting == 0 {
        return Cow::Borrowed(bytes);
    }
    match oxiroot_compress::compress(bytes, setting) {
        Ok(compressed) if compressed.len() < bytes.len() => Cow::Owned(compressed),
        _ => Cow::Borrowed(bytes),
    }
}

/// A directory inside a [`ContainerWriter`]: the top directory is
/// [`DirId::TOP`], and [`mkdir`](ContainerWriter::mkdir) returns the others.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DirId(usize);

impl DirId {
    /// The file's top directory.
    pub const TOP: DirId = DirId(0);
}

/// How [`ContainerWriter::finish`] brings the header and the top directory
/// record up to date.
enum HeaderUpdate {
    /// Patch the pointer fields in place. The header and record already have the
    /// container's form: a new file, or an append that stays small.
    Patch,
    /// Rewrite both in the big form, keeping the existing file's identity: an
    /// append whose output is big.
    Rewrite {
        version: u32,
        compress: u32,
        uuid: TUuid,
    },
}

struct DirState {
    class: &'static str,
    name: String,
    title: String,
    /// Offset of the directory's own key (`fSeekDir`); `fBEGIN` for the top one.
    seek: u64,
    /// Offset of the directory's `TDirectory` record.
    record: u64,
    /// The directory's keys, listed in its key list when it is closed.
    keys: Vec<TKey>,
    open: bool,
}

/// Writes a ROOT `TFile` to a seekable sink.
///
/// A new file starts with [`new`](ContainerWriter::new), which writes the header
/// and the top directory. Objects go in with
/// [`place_key`](ContainerWriter::place_key), subdirectories with
/// [`mkdir`](ContainerWriter::mkdir) and [`close_dir`](ContainerWriter::close_dir),
/// and [`finish`](ContainerWriter::finish) writes the top directory's key list
/// and fills in the header. Everything is written in the order these calls are
/// made.
///
/// The form (small or big) is fixed when the writer is created, because the
/// header and directory records are written at their final width straight away.
/// To choose it from the finished size instead, lay the file out in memory with
/// [`build`](ContainerWriter::build).
///
/// ```
/// use oxiroot_io_core::{Compression, ContainerWriter, DirId, FileReader};
///
/// let bytes = ContainerWriter::build("demo.root", Compression::None, u64::MAX, |c| {
///     c.place_key(DirId::TOP, "TObjString", "note", "", b"payload")?;
///     let sub = c.mkdir(DirId::TOP, "sub")?;
///     c.place_key(sub, "TObjString", "inner", "", b"more")?;
///     c.close_dir(sub)
/// })?;
/// let f = FileReader::from_bytes(bytes)?;
/// assert_eq!(f.keys().len(), 2);
/// assert_eq!(f.subdir("sub")?.keys.len(), 1);
/// # Ok::<(), oxiroot_io_core::Error>(())
/// ```
pub struct ContainerWriter<W: Write + Seek> {
    sink: W,
    /// Absolute offset of the next byte to write (the file's current end).
    pos: u64,
    big: bool,
    /// Compression setting applied to [`place_key`](Self::place_key) payloads.
    compression: u32,
    /// `fBEGIN`: offset of the top directory's key.
    begin: u64,
    /// `fNbytesName`: the top directory key plus its name and title.
    nbytes_name: u32,
    header_update: HeaderUpdate,
    dirs: Vec<DirState>,
    seek_info: u64,
    nbytes_info: u32,
    /// The streamer-info record of the file being continued, when there is one.
    existing_info: Option<ExistingInfo>,
}

/// A continued file's streamer-info record.
enum ExistingInfo {
    /// Its list, the `KeyLen` the list was written under, and the classes it
    /// describes.
    Readable {
        list: Vec<u8>,
        key_len: u16,
        classes: Vec<(String, i32)>,
    },
    /// A record this crate cannot parse; it is kept as it is.
    Opaque,
}

impl<W: Write + Seek> ContainerWriter<W> {
    /// Start a new file in `sink`, writing the header and the top directory.
    /// `file_name` is the name recorded for the top directory. `big` selects the
    /// 64-bit form, needed once the file may exceed [`KSTART_BIG_FILE`] bytes.
    pub fn new(mut sink: W, file_name: &str, compression: Compression, big: bool) -> Result<Self> {
        check_key_strings(TOP_DIR_CLASS, file_name, "")?;
        let compression = compression.setting();
        let mut w = WBuffer::new();

        w.bytes(MAGIC);
        w.be_u32(if big {
            FILE_VERSION + BIG_FILE_VERSION
        } else {
            FILE_VERSION
        });
        w.be_u32(BEGIN as u32);
        seek_value(&mut w, 0, big); // fEND
        seek_value(&mut w, 0, big); // fSeekFree
        w.be_u32(0); // fNbytesFree
        w.be_u32(0); // nfree
        w.be_u32(0); // fNbytesName
        w.u8(if big { 8 } else { 4 }); // fUnits
        w.be_u32(compression); // fCompress
        seek_value(&mut w, 0, big); // fSeekInfo
        w.be_u32(0); // fNbytesInfo
        w.be_u16(1); // fUUID version
        w.bytes(&[0u8; 16]); // fUUID
        while (w.len() as u64) < BEGIN {
            w.u8(0);
        }

        // The top directory: its key, then its name and title, then its record.
        // The record is reserved at the big width even in a small file, as ROOT
        // does, so an append can later widen it in place.
        let key_len = key_len_fmt(TOP_DIR_CLASS, file_name, "", big);
        let name_title_len = (1 + file_name.len()) + 1;
        let nbytes_name = key_len as u32 + name_title_len as u32;
        let obj_len = name_title_len as u32 + dir_record_len(true);
        write_key_header_fmt(
            &mut w,
            TOP_DIR_CLASS,
            file_name,
            "",
            obj_len,
            obj_len,
            BEGIN,
            0,
            1,
            big,
        );
        w.string(file_name);
        w.string("");
        let record = w.len() as u64;
        write_dir_record(&mut w, BEGIN, 0, nbytes_name, big);
        if !big {
            w.bytes(&[0u8; 12]); // the width the big form adds (three seeks, 4 → 8)
        }

        sink.write_all(w.as_slice())?;
        Ok(ContainerWriter {
            sink,
            pos: w.len() as u64,
            big,
            compression,
            begin: BEGIN,
            nbytes_name,
            header_update: HeaderUpdate::Patch,
            dirs: vec![DirState {
                class: TOP_DIR_CLASS,
                name: file_name.to_string(),
                title: String::new(),
                seek: BEGIN,
                record,
                keys: Vec::new(),
                open: true,
            }],
            seek_info: 0,
            nbytes_info: 0,
            existing_info: None,
        })
    }

    /// Continue the file `file` so that new records are added to its top
    /// directory. `sink` must hold the file's bytes up to its `fEND`; new records
    /// are written after them and nothing before `fEND` moves, so objects,
    /// subdirectories and RNTuples (which hold absolute offsets) stay valid.
    /// Only the header and the top directory record are updated in place.
    ///
    /// The top directory's live keys are listed again, with their cycles, and the
    /// file's streamer info is kept (see
    /// [`place_streamer_info`](Self::place_streamer_info)). `file_name` names the
    /// new key list.
    ///
    /// With `big`, the output is written in the 64-bit form, and the header and
    /// top directory record are rewritten at that width. That requires the
    /// record to have been reserved at its 64-bit size, as ROOT and oxiroot
    /// do; a file that reserved only the small size is an error. A file that is
    /// already big must be continued with `big`.
    pub fn append(
        mut sink: W,
        file: &FileReader,
        file_name: &str,
        compression: Compression,
        big: bool,
    ) -> Result<Self> {
        let header = file.header();
        if file.size() < header.end {
            return Err(Error::Format(format!(
                "file is truncated: fEND={} but only {} bytes present",
                header.end,
                file.size()
            )));
        }
        if header.is_big() && !big {
            return Err(Error::InvalidInput(
                "a 64-bit file must be continued in the 64-bit form".to_string(),
            ));
        }
        let header_update = if big {
            // Everything after the top directory's name and title belongs to its
            // record; the big form needs its full width there.
            let name_key = TKey::read(&mut RBuffer::new(
                &file.read_at(header.begin, header.nbytes_name as usize)?,
            ))?;
            let name_title_len = header
                .nbytes_name
                .saturating_sub(u32::from(name_key.key_len));
            let reserved = name_key.obj_len.saturating_sub(name_title_len);
            if reserved < dir_record_len(true) {
                return Err(Error::Unsupported(format!(
                    "cannot append into the 64-bit form: this file's root directory record \
                     reserves {reserved} bytes, but the big form needs {}. Rewrite the file \
                     with FileWriter::create (which reserves the 64-bit width) first.",
                    dir_record_len(true)
                )));
            }
            HeaderUpdate::Rewrite {
                version: if header.is_big() {
                    header.version
                } else {
                    header.version + BIG_FILE_VERSION
                },
                compress: header.compress,
                uuid: header.uuid,
            }
        } else {
            HeaderUpdate::Patch
        };

        sink.seek(SeekFrom::Start(header.end))?;
        Ok(ContainerWriter {
            sink,
            pos: header.end,
            big,
            compression: compression.setting(),
            begin: header.begin,
            nbytes_name: header.nbytes_name,
            header_update,
            dirs: vec![DirState {
                class: TOP_DIR_CLASS,
                name: file_name.to_string(),
                title: String::new(),
                seek: header.begin,
                record: header.begin + u64::from(header.nbytes_name),
                keys: file
                    .keys()
                    .iter()
                    .filter(|k| !k.is_deleted())
                    .cloned()
                    .collect(),
                open: true,
            }],
            seek_info: header.seek_info,
            nbytes_info: header.nbytes_info,
            existing_info: (header.seek_info != 0)
                .then(|| read_existing_info(file).unwrap_or(ExistingInfo::Opaque)),
        })
    }

    /// Whether the file is written in the 64-bit form.
    #[must_use]
    pub fn is_big(&self) -> bool {
        self.big
    }

    /// The absolute offset the next record will be written at.
    #[must_use]
    pub fn position(&self) -> u64 {
        self.pos
    }

    /// The offset of directory `dir`'s own key (`fSeekDir`), which records placed
    /// for that directory point back to.
    pub fn dir_offset(&self, dir: DirId) -> Result<u64> {
        self.dirs
            .get(dir.0)
            .map(|d| d.seek)
            .ok_or_else(|| Error::InvalidInput(format!("{dir:?} does not belong to this file")))
    }

    /// The compression setting (`algorithm*100 + level`) this file applies to
    /// [`place_key`](Self::place_key) payloads, for callers that compress their
    /// own blobs the same way.
    #[must_use]
    pub fn compression_setting(&self) -> u32 {
        self.compression
    }

    /// Write `bytes` at the end of the file and return their offset. The bytes
    /// belong to no directory: something else has to point at them (an RNTuple
    /// anchor, a `TTree`'s basket list).
    pub fn place_blob(&mut self, bytes: &[u8]) -> Result<u64> {
        let seek = self.pos;
        self.put(bytes)?;
        Ok(seek)
    }

    /// Store `object` (a streamed object, including its own byte count and
    /// version) under a new key in directory `dir`, compressed with the file's
    /// setting when that makes it smaller. Returns the key's offset.
    ///
    /// A key whose name is already in `dir` gets the next cycle, so the new
    /// object is the one readers see.
    pub fn place_key(
        &mut self,
        dir: DirId,
        class: &str,
        name: &str,
        title: &str,
        object: &[u8],
    ) -> Result<u64> {
        let payload = compress_if_smaller(object, self.compression);
        self.write_key(dir, class, name, title, object.len(), &payload)
    }

    /// Like [`place_key`](Self::place_key), but always store `object`
    /// uncompressed.
    pub fn place_key_uncompressed(
        &mut self,
        dir: DirId,
        class: &str,
        name: &str,
        title: &str,
        object: &[u8],
    ) -> Result<u64> {
        self.write_key(dir, class, name, title, object.len(), object)
    }

    /// Store the file's streamer info, referenced from the header rather than
    /// listed in a directory: `list` is a streamed `TList<TStreamerInfo>` (or
    /// empty), and `extra` are further classes added after its entries, skipping
    /// any it already describes at the same version. Nothing is written when both
    /// are empty. A later call replaces the reference.
    ///
    /// When continuing a file that already has streamer info, `list` is not
    /// used: the file's own entries are kept, and only the `extra` classes they
    /// lack are added after them. Nothing is written when none are missing, or
    /// when the existing record cannot be read.
    pub fn place_streamer_info(&mut self, list: &[u8], extra: &[Cls<'_>]) -> Result<()> {
        match &self.existing_info {
            Some(ExistingInfo::Readable {
                list: existing,
                key_len,
                classes,
            }) => {
                let missing: Vec<Cls<'_>> = extra
                    .iter()
                    .filter(|c| !describes(classes, c))
                    .cloned()
                    .collect();
                if missing.is_empty() {
                    return Ok(());
                }
                let Some(title) = streamer_info_title(*key_len, self.big) else {
                    // No title gives the old key length, so the old entries could
                    // not be kept valid; leave the record alone.
                    return Ok(());
                };
                let merged = append_streamer_infos(existing, &missing)?;
                self.write_streamer_info(&merged, &title)
            }
            Some(ExistingInfo::Opaque) => Ok(()),
            None => {
                let listed = if list.is_empty() {
                    Vec::new()
                } else {
                    described_classes(list, STREAMER_INFO_KEY_LEN)
                };
                let extra: Vec<Cls<'_>> = extra
                    .iter()
                    .filter(|c| !describes(&listed, c))
                    .cloned()
                    .collect();
                let list = match (list.is_empty(), extra.is_empty()) {
                    (true, true) => return Ok(()),
                    (_, true) => Cow::Borrowed(list),
                    (true, false) => Cow::Owned(streamer_info_list(&extra)),
                    (false, false) => Cow::Owned(append_streamer_infos(list, &extra)?),
                };
                let title = streamer_info_title(STREAMER_INFO_KEY_LEN, self.big)
                    .expect("a 64-byte key fits both forms");
                self.write_streamer_info(&list, &title)
            }
        }
    }

    fn write_streamer_info(&mut self, list: &[u8], title: &str) -> Result<()> {
        let payload = compress_if_smaller(list, self.compression);
        let seek = self.pos;
        let mut w = WBuffer::new();
        write_key_header_fmt(
            &mut w,
            STREAMER_INFO_CLASS,
            STREAMER_INFO_NAME,
            title,
            checked_len(list.len(), STREAMER_INFO_NAME)?,
            checked_len(payload.len(), STREAMER_INFO_NAME)?,
            seek,
            self.begin,
            1,
            self.big,
        );
        self.put(w.as_slice())?;
        self.put(&payload)?;
        self.seek_info = seek;
        self.nbytes_info = checked_len(w.len() + payload.len(), STREAMER_INFO_NAME)?;
        Ok(())
    }

    /// Create a subdirectory `name` inside `parent` and return it. Its keys are
    /// listed when it is closed with [`close_dir`](Self::close_dir), or by
    /// [`finish`](Self::finish).
    pub fn mkdir(&mut self, parent: DirId, name: &str) -> Result<DirId> {
        self.check_open(parent)?;
        check_key_strings(SUBDIR_CLASS, name, name)?;
        let big = self.big;
        let parent_seek = self.dirs[parent.0].seek;
        let seek = self.pos;
        let record_len = dir_record_len(big);
        let key_len = key_len_fmt(SUBDIR_CLASS, name, name, big);
        let cycle = self.next_cycle(parent, name);

        let mut w = WBuffer::new();
        write_key_header_fmt(
            &mut w,
            SUBDIR_CLASS,
            name,
            name,
            record_len,
            record_len,
            seek,
            parent_seek,
            cycle,
            big,
        );
        let record = seek + w.len() as u64;
        write_dir_record(&mut w, seek, parent_seek, u32::from(key_len), big);
        self.put(w.as_slice())?;

        self.dirs[parent.0].keys.push(new_key(
            SUBDIR_CLASS,
            name,
            name,
            record_len,
            record_len,
            seek,
            parent_seek,
            cycle,
            big,
        ));
        self.dirs.push(DirState {
            class: SUBDIR_CLASS,
            name: name.to_string(),
            title: name.to_string(),
            seek,
            record,
            keys: Vec::new(),
            open: true,
        });
        Ok(DirId(self.dirs.len() - 1))
    }

    /// Write subdirectory `dir`'s key list and point its record at it. The
    /// directory takes no more keys afterwards. The top directory is closed by
    /// [`finish`](Self::finish).
    pub fn close_dir(&mut self, dir: DirId) -> Result<()> {
        if dir == DirId::TOP {
            return Err(Error::InvalidInput(
                "the top directory is closed by finish()".to_string(),
            ));
        }
        self.check_open(dir)?;
        self.write_key_list(dir).map(drop)
    }

    /// Finish the file: close any open subdirectories, write the top directory's
    /// key list, and fill in the header. Returns the sink.
    ///
    /// A small file that grew past [`KSTART_BIG_FILE`] bytes cannot address its
    /// own records: that is [`Error::FileTooLarge`].
    pub fn finish(self) -> Result<W> {
        self.finish_checked(true)
    }

    fn finish_checked(mut self, check_size: bool) -> Result<W> {
        // Close children before their parents; a child is always created after
        // its parent.
        for id in (1..self.dirs.len()).rev() {
            if self.dirs[id].open {
                self.close_dir(DirId(id))?;
            }
        }
        let (keys_seek, keys_nbytes) = self.write_key_list(DirId::TOP)?;

        let end = self.pos;
        if check_size && !self.big && end > KSTART_BIG_FILE {
            return Err(Error::FileTooLarge { size: end });
        }

        match std::mem::replace(&mut self.header_update, HeaderUpdate::Patch) {
            HeaderUpdate::Patch => self.patch_header(end)?,
            HeaderUpdate::Rewrite {
                version,
                compress,
                uuid,
            } => self.rewrite_header(end, version, compress, uuid, keys_seek, keys_nbytes)?,
        }
        self.sink.flush()?;
        Ok(self.sink)
    }

    fn put(&mut self, bytes: &[u8]) -> Result<()> {
        self.sink.write_all(bytes)?;
        self.pos += bytes.len() as u64;
        Ok(())
    }

    /// Overwrite `bytes` at `offset`, then return to the end of the file.
    fn patch(&mut self, offset: u64, bytes: &[u8]) -> Result<()> {
        self.sink.seek(SeekFrom::Start(offset))?;
        self.sink.write_all(bytes)?;
        self.sink.seek(SeekFrom::Start(self.pos))?;
        Ok(())
    }

    /// Overwrite a seek field: 8 bytes in the big form, 4 in the small form.
    fn patch_seek(&mut self, offset: u64, value: u64) -> Result<()> {
        if self.big {
            self.patch(offset, &value.to_be_bytes())
        } else {
            self.patch(offset, &(value as u32).to_be_bytes())
        }
    }

    fn check_open(&self, dir: DirId) -> Result<()> {
        match self.dirs.get(dir.0) {
            Some(d) if d.open => Ok(()),
            Some(d) => Err(Error::InvalidInput(format!(
                "directory {:?} is already closed",
                d.name
            ))),
            None => Err(Error::InvalidInput(format!(
                "{dir:?} does not belong to this file"
            ))),
        }
    }

    /// The cycle a new key called `name` gets in `dir`: one past the highest
    /// cycle already there, so the new key is the one readers pick.
    fn next_cycle(&self, dir: DirId, name: &str) -> u16 {
        self.dirs[dir.0]
            .keys
            .iter()
            .filter(|k| k.name == name)
            .map(|k| k.cycle)
            .max()
            .map_or(1, |c| c.saturating_add(1))
    }

    fn write_key(
        &mut self,
        dir: DirId,
        class: &str,
        name: &str,
        title: &str,
        obj_len: usize,
        payload: &[u8],
    ) -> Result<u64> {
        self.check_open(dir)?;
        check_key_strings(class, name, title)?;
        let obj_len = checked_len(obj_len, name)?;
        let payload_len = checked_len(payload.len(), name)?;
        let big = self.big;
        let seek = self.pos;
        let seek_pdir = self.dirs[dir.0].seek;
        let cycle = self.next_cycle(dir, name);

        let mut w = WBuffer::new();
        write_key_header_fmt(
            &mut w,
            class,
            name,
            title,
            obj_len,
            payload_len,
            seek,
            seek_pdir,
            cycle,
            big,
        );
        self.put(w.as_slice())?;
        self.put(payload)?;
        self.dirs[dir.0].keys.push(new_key(
            class,
            name,
            title,
            obj_len,
            payload_len,
            seek,
            seek_pdir,
            cycle,
            big,
        ));
        Ok(seek)
    }

    /// Write `dir`'s key list — a key whose payload is the key count followed by
    /// a copy of each key's header — then point the directory record at it.
    /// Returns the list's offset and size.
    fn write_key_list(&mut self, dir: DirId) -> Result<(u64, u32)> {
        let big = self.big;
        let seek = self.pos;
        let d = &mut self.dirs[dir.0];
        d.open = false;
        let headers: usize = d.keys.iter().map(|k| usize::from(k.key_len)).sum();
        let obj_len = checked_len(4 + headers, &d.name)?;

        let mut w = WBuffer::new();
        write_key_header_fmt(
            &mut w, d.class, &d.name, &d.title, obj_len, obj_len, seek, d.seek, 1, big,
        );
        w.be_i32(d.keys.len() as i32);
        for k in &d.keys {
            // Each entry keeps the form of the key it copies: a small key read from
            // an existing file stays small even in a big file's list, since its
            // `KeyLen` tells a reader where the payload starts.
            write_key_header_fmt(
                &mut w,
                &k.class_name,
                &k.name,
                &k.title,
                k.obj_len,
                (k.nbytes as u32).saturating_sub(u32::from(k.key_len)),
                k.seek_key,
                d.seek,
                k.cycle,
                k.version > 1000,
            );
        }
        let nbytes = u32::from(key_len_fmt(d.class, &d.name, &d.title, big)) + obj_len;
        let record = d.record;
        self.put(w.as_slice())?;

        // fNbytesKeys, then fSeekKeys after fNbytesName, fSeekDir and fSeekParent.
        self.patch(record + 10, &nbytes.to_be_bytes())?;
        self.patch_seek(record + if big { 34 } else { 26 }, seek)?;
        Ok((seek, nbytes))
    }

    /// Fill in the header's pointers, in whichever form it was written.
    fn patch_header(&mut self, end: u64) -> Result<()> {
        // Offsets of fEND, fSeekFree, fNbytesFree, nfree, fNbytesName, fSeekInfo
        // and fNbytesInfo.
        let [p_end, p_seek_free, p_nbytes_free, p_nfree, p_nbytes_name, p_seek_info, p_nbytes_info] =
            if self.big {
                [12, 20, 28, 32, 36, 45, 53]
            } else {
                [12, 16, 20, 24, 28, 37, 41]
            };
        self.patch_seek(p_end, end)?;
        // The free-segment list is not maintained, so declare it empty.
        self.patch_seek(p_seek_free, 0)?;
        self.patch(p_nbytes_free, &0u32.to_be_bytes())?;
        self.patch(p_nfree, &0u32.to_be_bytes())?;
        self.patch(p_nbytes_name, &self.nbytes_name.to_be_bytes())?;
        self.patch_seek(p_seek_info, self.seek_info)?;
        self.patch(p_nbytes_info, &self.nbytes_info.to_be_bytes())
    }

    /// Rewrite the header and the top directory record in the big form, pointing
    /// the record at the key list written at `keys_seek`.
    fn rewrite_header(
        &mut self,
        end: u64,
        version: u32,
        compress: u32,
        uuid: TUuid,
        keys_seek: u64,
        keys_nbytes: u32,
    ) -> Result<()> {
        let mut h = WBuffer::new();
        h.bytes(MAGIC);
        h.be_u32(version);
        h.be_u32(self.begin as u32);
        h.be_u64(end);
        h.be_u64(0); // fSeekFree
        h.be_u32(0); // fNbytesFree
        h.be_u32(0); // nfree
        h.be_u32(self.nbytes_name);
        h.u8(8); // fUnits
        h.be_u32(compress);
        h.be_u64(self.seek_info);
        h.be_u32(self.nbytes_info);
        h.be_u16(uuid.version);
        h.bytes(&uuid.bytes);
        while (h.len() as u64) < self.begin {
            h.u8(0);
        }
        self.patch(0, h.as_slice())?;

        let top = &self.dirs[DirId::TOP.0];
        let (record, seek_dir) = (top.record, top.seek);
        let mut d = WBuffer::new();
        write_dir_record(&mut d, seek_dir, 0, self.nbytes_name, true);
        let mut d = d.into_vec();
        d[10..14].copy_from_slice(&keys_nbytes.to_be_bytes());
        d[34..42].copy_from_slice(&keys_seek.to_be_bytes());
        self.patch(record, &d)
    }
}

impl ContainerWriter<Cursor<Vec<u8>>> {
    /// Lay out a whole new file in memory and return its bytes. `layout` places
    /// the file's content; it runs once for the small form and, if that result is
    /// larger than `big_threshold` bytes (or [`KSTART_BIG_FILE`], whichever is
    /// smaller), once more for the big form. Pass [`KSTART_BIG_FILE`] to switch
    /// where ROOT does, or 0 to always write the big form.
    pub fn build(
        file_name: &str,
        compression: Compression,
        big_threshold: u64,
        mut layout: impl FnMut(&mut Self) -> Result<()>,
    ) -> Result<Vec<u8>> {
        let limit = big_threshold.min(KSTART_BIG_FILE);
        let small = Self::run(
            Self::new(Cursor::default(), file_name, compression, false)?,
            &mut layout,
        )?;
        if small.len() as u64 <= limit {
            return Ok(small);
        }
        Self::run(
            Self::new(Cursor::default(), file_name, compression, true)?,
            &mut layout,
        )
    }

    /// Like [`build`](Self::build), but continue the file whose bytes are
    /// `existing` (see [`append`](ContainerWriter::append)). A file that is
    /// already big stays big.
    pub fn build_append(
        existing: &[u8],
        file_name: &str,
        compression: Compression,
        big_threshold: u64,
        mut layout: impl FnMut(&mut Self) -> Result<()>,
    ) -> Result<Vec<u8>> {
        let file = FileReader::from_bytes(existing.to_vec())?;
        let end = usize::try_from(file.header().end).unwrap_or(usize::MAX);
        let prefix = existing.get(..end).ok_or_else(|| {
            Error::Format(format!(
                "file is truncated: fEND={end} but only {} bytes present",
                existing.len()
            ))
        })?;
        let limit = big_threshold.min(KSTART_BIG_FILE);
        let start = |big| {
            let mut sink = Cursor::new(prefix.to_vec());
            sink.set_position(prefix.len() as u64);
            Self::append(sink, &file, file_name, compression, big)
        };
        if !file.header().is_big() {
            let small = Self::run(start(false)?, &mut layout)?;
            if small.len() as u64 <= limit {
                return Ok(small);
            }
        }
        Self::run(start(true)?, &mut layout)
    }

    fn run(mut writer: Self, layout: &mut impl FnMut(&mut Self) -> Result<()>) -> Result<Vec<u8>> {
        layout(&mut writer)?;
        // The size decides the form here, so a small result is never an error.
        Ok(writer.finish_checked(false)?.into_inner())
    }
}

/// The classes a serialized list describes, with their versions, or none if it
/// does not parse.
fn described_classes(list: &[u8], key_len: u16) -> Vec<(String, i32)> {
    parse_streamer_info(list, usize::from(key_len))
        .map(|registry| versions_of(&registry))
        .unwrap_or_default()
}

/// Each class a registry describes, with its version.
fn versions_of(registry: &StreamerRegistry) -> Vec<(String, i32)> {
    registry
        .infos()
        .iter()
        .map(|info| (info.class_name.clone(), info.class_version))
        .collect()
}

/// Whether `listed` already describes `class` at its version. A file may hold
/// several versions of a class (objects written by older releases keep theirs),
/// so a class listed at another version is still added.
fn describes(listed: &[(String, i32)], class: &Cls<'_>) -> bool {
    listed
        .iter()
        .any(|(name, version)| *name == class.name && *version == class.version)
}

/// Read a continued file's streamer-info record.
fn read_existing_info(file: &FileReader) -> Result<ExistingInfo> {
    let header = file.header();
    let record = file.read_at(header.seek_info, header.nbytes_info as usize)?;
    let key_len = TKey::read(&mut RBuffer::new(&record))?.key_len;
    let list = file
        .streamer_info_object()?
        .ok_or_else(|| Error::Format("no streamer-info record".to_string()))?;
    let classes = versions_of(&parse_streamer_info(&list, usize::from(key_len))?);
    Ok(ExistingInfo::Readable {
        list,
        key_len,
        classes,
    })
}

/// A title that gives the streamer-info key a `KeyLen` of `key_len` in the
/// small or big form: ROOT's own title when that fits, otherwise the same text
/// cut or space-padded to length. `None` when no title can.
fn streamer_info_title(key_len: u16, big: bool) -> Option<String> {
    let without_title = TKey::header_len(STREAMER_INFO_CLASS, STREAMER_INFO_NAME, "", big);
    // The empty title above already counts its one-byte length prefix.
    let len = usize::from(key_len).checked_sub(without_title)?;
    if len >= 255 {
        return None;
    }
    let mut title: String = STREAMER_INFO_TITLE.chars().take(len).collect();
    while title.len() < len {
        title.push(' ');
    }
    Some(title)
}

/// A key header records its own length in 16 bits, which bounds the combined
/// length of its class, name and title.
fn check_key_strings(class: &str, name: &str, title: &str) -> Result<()> {
    let len = TKey::header_len(class, name, title, true);
    if len > usize::from(u16::MAX) {
        return Err(Error::InvalidInput(format!(
            "key {:?}: class, name and title total {len} bytes, more than a ROOT key header can hold",
            name.chars().take(40).collect::<String>()
        )));
    }
    Ok(())
}

/// A key's lengths must fit its 32-bit fields.
fn checked_len(len: usize, name: &str) -> Result<u32> {
    u32::try_from(len).map_err(|_| {
        Error::InvalidInput(format!(
            "record {name:?} is {len} bytes, more than a ROOT key can hold"
        ))
    })
}

#[allow(clippy::too_many_arguments)]
fn new_key(
    class: &str,
    name: &str,
    title: &str,
    obj_len: u32,
    payload_len: u32,
    seek: u64,
    seek_pdir: u64,
    cycle: u16,
    big: bool,
) -> TKey {
    let key_len = key_len_fmt(class, name, title, big);
    TKey {
        nbytes: (u32::from(key_len) + payload_len) as i32,
        version: if big {
            KEY_VERSION_SMALL + 1000
        } else {
            KEY_VERSION_SMALL
        },
        obj_len,
        datime: TDatime(DATIME),
        key_len,
        cycle,
        seek_key: seek,
        seek_pdir,
        class_name: class.to_string(),
        name: name.to_string(),
        title: title.to_string(),
    }
}

/// Length of a key header, which [`check_key_strings`] has bounded to 16 bits.
fn key_len_fmt(class: &str, name: &str, title: &str, big: bool) -> u16 {
    TKey::header_len(class, name, title, big) as u16
}

/// Write a `TKey` header (no payload) in the small or big form. `obj_len` is the
/// uncompressed object size; `payload_len` the stored size.
#[allow(clippy::too_many_arguments)]
fn write_key_header_fmt(
    w: &mut WBuffer,
    class: &str,
    name: &str,
    title: &str,
    obj_len: u32,
    payload_len: u32,
    seek_key: u64,
    seek_pdir: u64,
    cycle: u16,
    big: bool,
) {
    let key_len = key_len_fmt(class, name, title, big);
    w.be_i32((u32::from(key_len) + payload_len) as i32); // Nbytes
    w.be_u16(if big {
        KEY_VERSION_SMALL + 1000
    } else {
        KEY_VERSION_SMALL
    });
    w.be_u32(obj_len);
    w.be_u32(DATIME);
    w.be_u16(key_len);
    w.be_u16(cycle);
    seek_value(w, seek_key, big);
    seek_value(w, seek_pdir, big);
    w.string(class);
    w.string(name);
    w.string(title);
}

/// Write a seek value: 8 bytes in the big form, 4 in the small form.
fn seek_value(w: &mut WBuffer, v: u64, big: bool) {
    if big {
        w.be_u64(v);
    } else {
        w.be_u32(v as u32);
    }
}

/// Size of a `TDirectory` record, UUID included: 48 bytes in the small form and
/// 60 in the big form.
fn dir_record_len(big: bool) -> u32 {
    if big {
        60
    } else {
        48
    }
}

/// Write a `TDirectory` record with empty `fNbytesKeys` and `fSeekKeys`, which
/// the directory's key list fills in.
fn write_dir_record(w: &mut WBuffer, seek_dir: u64, seek_parent: u64, nbytes_name: u32, big: bool) {
    w.be_i16(if big { 1005 } else { 5 }); // version (> 1000: 64-bit seeks)
    w.be_u32(DATIME); // fDatimeC
    w.be_u32(DATIME); // fDatimeM
    w.be_u32(0); // fNbytesKeys
    w.be_i32(nbytes_name as i32); // fNbytesName
    seek_value(w, seek_dir, big); // fSeekDir
    seek_value(w, seek_parent, big); // fSeekParent
    seek_value(w, 0, big); // fSeekKeys
    w.be_u16(1); // UUID version
    w.bytes(&[0u8; 16]); // UUID
}
