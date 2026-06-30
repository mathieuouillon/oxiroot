//! `TEntryList` — a persisted set of selected `TTree` entry numbers.
//!
//! A `TEntryList` is a standalone key (not part of a `TTree`). It records which
//! entries of a tree passed a selection, so an analysis can replay just those
//! entries. The passing entries are stored in one or more `TEntryListBlock`s,
//! each covering a fixed window of [`K_BLOCK_SIZE`] entries as a bitmap (bit *b*
//! of word *i* ⇒ entry `block*K_BLOCK_SIZE + 16*i + b`) or, when few entries
//! pass, as an explicit list of offsets. This reader decodes the common
//! single-tree case (the `fBlocks` array); a multi-tree list (`fLists`) is not
//! yet expanded.

use oxiroot_io_core::buffer::RBuffer;
use oxiroot_io_core::error::{Error, Result};
use oxiroot_io_core::object::TagReader;
use oxiroot_io_core::streamer::{read_tnamed, read_tobject};
use oxiroot_io_core::RFile;

/// Entries per `TEntryListBlock` window (ROOT's `TEntryListBlock::kBlockSize`).
const K_BLOCK_SIZE: u64 = 64000;

/// A `TEntryList` read from a file: its name and the selected entry numbers, in
/// ascending order.
#[derive(Debug, Clone)]
pub struct TEntryList {
    name: String,
    tree_name: String,
    file_name: String,
    entries: Vec<u64>,
}

impl TEntryList {
    /// Open the `TEntryList` named `name` in `file`.
    pub fn open(file: &RFile, name: &str) -> Result<TEntryList> {
        let key = file
            .key(name)
            .ok_or_else(|| Error::Format(format!("no key named {name:?}")))?;
        if key.class_name != "TEntryList" {
            return Err(Error::Format(format!(
                "key {name:?} is a {}, not a TEntryList",
                key.class_name
            )));
        }
        let payload = key.payload(file.data())?;
        let object = oxiroot_compress::decompress(payload, key.obj_len as usize)
            .map_err(|e| Error::Format(format!("decompressing TEntryList: {e}")))?;
        read_entry_list(&object, key.key_len as usize)
    }

    /// The entry-list name (`fName`).
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The name of the tree these entries index (`fTreeName`).
    pub fn tree_name(&self) -> &str {
        &self.tree_name
    }

    /// The file the indexed tree lives in (`fFileName`), or `""` for the file
    /// holding the list.
    pub fn file_name(&self) -> &str {
        &self.file_name
    }

    /// The selected entry numbers, ascending.
    pub fn entries(&self) -> &[u64] {
        &self.entries
    }

    /// How many entries the list selects.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the list selects no entries.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Whether `entry` is in the list.
    pub fn contains(&self, entry: u64) -> bool {
        self.entries.binary_search(&entry).is_ok()
    }
}

/// Parse a decompressed `TEntryList` object (`keylen` is its key's header length).
fn read_entry_list(object: &[u8], keylen: usize) -> Result<TEntryList> {
    let mut r = RBuffer::new(object);
    let mut tags = TagReader::new(keylen);

    r.read_version()?; // TEntryList version header
    let named = read_tnamed(&mut r)?; // fName, fTitle

    // fLists (TList*): sub-lists for a multi-tree list; null for a single tree.
    // We don't expand sub-lists yet — step over it via the object header.
    let lists = tags.read_header(&mut r)?;
    if let Some(end) = lists.end {
        r.seek(end)?;
    }

    let _n_blocks = r.be_i32()?; // fNBlocks (redundant with the fBlocks size)

    // fBlocks (TObjArray* of TEntryListBlock).
    let mut entries = Vec::new();
    let blocks = tags.read_header(&mut r)?;
    if blocks.class_name.is_some() {
        r.read_version()?; // the TObjArray's own version header
        read_tobject(&mut r)?;
        r.string()?; // fName
        let size = r.be_i32()?.max(0);
        let _lower = r.be_i32()?; // fLowerBound
        for bi in 0..size {
            let block = tags.read_header(&mut r)?;
            if block.class_name.as_deref() == Some("TEntryListBlock") {
                read_block(&mut r, bi as u64, &mut entries)?;
            }
            if let Some(end) = block.end {
                r.seek(end)?;
            }
        }
    }
    if let Some(end) = blocks.end {
        r.seek(end)?;
    }

    // fN (i64), fEntriesToProcess (i64), then fTreeName / fFileName.
    let (mut tree_name, mut file_name) = (String::new(), String::new());
    if r.be_i64().is_ok() && r.be_i64().is_ok() {
        if let Ok(name) = r.string() {
            tree_name = name;
        }
        if let Ok(name) = r.string() {
            file_name = name;
        }
    }

    entries.sort_unstable();
    entries.dedup();
    Ok(TEntryList {
        name: named.name,
        tree_name,
        file_name,
        entries,
    })
}

/// Decode one `TEntryListBlock` (the object header already consumed), appending
/// its passing entries — offset by `block_index * K_BLOCK_SIZE` — to `out`.
fn read_block(r: &mut RBuffer, block_index: u64, out: &mut Vec<u64>) -> Result<()> {
    let vh = r.read_version()?; // TEntryListBlock version header
    read_tobject(r)?;
    let n_passed = r.be_i32()?.max(0) as usize;
    let n = r.be_i32()?.max(0) as usize; // number of UShort_t words in fIndices
    r.u8()?; // the counted-array "is present" flag
    let mut indices = Vec::with_capacity(n.min(r.remaining() / 2));
    for _ in 0..n {
        indices.push(r.be_u16()?);
    }
    let f_type = r.be_i32()?;
    let f_passing = r.u8()? != 0;

    let base = block_index * K_BLOCK_SIZE;
    if f_type == 0 && f_passing {
        // Bit array: bit b of word i ⇒ entry base + 16*i + b.
        for (i, &word) in indices.iter().enumerate() {
            for bit in 0..16u64 {
                if (word >> bit) & 1 == 1 {
                    out.push(base + 16 * i as u64 + bit);
                }
            }
        }
    } else if f_type != 0 {
        // Explicit list: fIndices holds the passing entries' offsets directly.
        for &offset in indices.iter().take(n_passed) {
            out.push(base + offset as u64);
        }
    }
    // (f_type == 0 && !f_passing — the complement encoding ROOT uses when most
    // entries pass — is rare and left for later; such a block contributes
    // nothing here rather than risking a wrong decode.)

    if let Some(end) = vh.end {
        r.seek(end)?;
    }
    Ok(())
}
