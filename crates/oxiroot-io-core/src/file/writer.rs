//! Writing a minimal, ROOT-compatible TFile container.
//!
//! Produces a small-format (32-bit) file: header, the root directory's name
//! key + `TDirectory` record, one `TKey` + object per supplied object, an
//! optional streamer-info record, and the directory key list. The free list is
//! kept empty. All offsets are back-patched once the layout is known.
//! [`update_root_file`] appends extra objects to an existing file *in place*,
//! leaving its existing bytes (objects, subdirectories, an RNTuple) untouched.

use crate::buffer::{RBuffer, WBuffer};
use crate::error::{Error, Result};

use super::header::{FileHeader, BIG_FILE_VERSION};
use super::key::TKey;
use super::rfile::RFile;

/// A fixed creation/modification timestamp (`TDatime`); readers don't validate it.
const DATIME: u32 = 0x7d7a_79ca;
/// On-disk file version (small format, < 1_000_000).
const FILE_VERSION: u32 = 62400;
/// Class used for the directory name key and key-list key.
const DIR_CLASS: &str = "TFile";

/// One object to store in the file: its class, name, title, and already-streamed
/// object bytes (including the object's own byte-count/version header).
pub struct ObjectRecord {
    /// ROOT class name (e.g. `"TH1D"`).
    pub class_name: String,
    /// Object name (the key name).
    pub name: String,
    /// Object title.
    pub title: String,
    /// Streamed object bytes.
    pub object: Vec<u8>,
}

/// Length of a small-format `TKey` header for the given strings.
pub fn key_len(class: &str, name: &str, title: &str) -> u16 {
    key_len_fmt(class, name, title, false)
}

/// `TKey` header length in either form. The fixed part is
/// Nbytes(4)+version(2)+ObjLen(4)+Datime(4)+KeyLen(2)+Cycle(2) plus the two seek
/// pointers — 4 bytes each in small form, 8 in big — then three length-prefixed
/// strings.
pub fn key_len_fmt(class: &str, name: &str, title: &str, big: bool) -> u16 {
    let fixed = if big { 34 } else { 26 };
    (fixed + (1 + class.len()) + (1 + name.len()) + (1 + title.len())) as u16
}

/// ROOT switches a file to the 64-bit ("big") on-disk form once any file pointer
/// would exceed this many bytes (`kStartBigFile`). Past this point keys, the
/// directory record, and the file header all widen their seek fields to 64 bits.
pub const KSTART_BIG_FILE: u64 = 2_000_000_000;

/// Reject a file whose total size would overflow the small format's 32-bit seek
/// pointers. These writers only emit the small (32-bit) `TFile` form, so beyond
/// [`KSTART_BIG_FILE`] the back-patched `fEND`/seek fields would silently wrap
/// and corrupt the file; return an error instead. (The RNTuple writer already
/// guards this; the streaming writers switch to the big format.)
pub fn guard_small_format(f_end: usize) -> Result<()> {
    if f_end as u64 > KSTART_BIG_FILE {
        return Err(Error::Format(format!(
            "file size {f_end} bytes exceeds the {KSTART_BIG_FILE}-byte small-format \
             TFile limit (64-bit seek pointers are not emitted by this writer)"
        )));
    }
    Ok(())
}

/// Write a zeroed file/directory seek field: 8 bytes in the big (64-bit) form,
/// 4 in the small form.
pub fn seek_zero(w: &mut WBuffer, big: bool) {
    if big {
        w.be_u64(0);
    } else {
        w.be_u32(0);
    }
}

/// Write a known seek value: 8 bytes in the big (64-bit) form, 4 in the small
/// form (truncated to 32 bits — the caller guarantees it fits when `!big`).
pub fn seek_value(w: &mut WBuffer, v: u64, big: bool) {
    if big {
        w.be_u64(v);
    } else {
        w.be_u32(v as u32);
    }
}

/// On-disk size (`obj_len`) of a `TDirectory` record including its 18-byte UUID:
/// 48 bytes in the small form, 60 in the big (64-bit seek) form. ROOT reserves
/// the big size unconditionally; these writers size it to the chosen form.
pub fn dir_record_total(big: bool) -> u32 {
    if big {
        60
    } else {
        48
    }
}

/// Write a `TDirectory` record (the body following a directory's name key) in the
/// small or big (64-bit seek) form. Returns the `(fNbytesKeys, fSeekKeys)` patch
/// handles to fill in once that directory's key list has been written. `fSeekKeys`
/// is reserved 8 bytes wide when `big`, so patch it with [`WBuffer::patch_be_u64`].
pub fn write_dir_record_fmt(
    w: &mut WBuffer,
    seek_dir: u64,
    seek_parent: u64,
    nbytes_name: u32,
    big: bool,
) -> (crate::buffer::Patch, crate::buffer::Patch) {
    w.be_i16(if big { 1005 } else { 5 }); // version (>1000 ⇒ 64-bit seeks)
    w.be_u32(DATIME); // fDatimeC
    w.be_u32(DATIME); // fDatimeM
    let p_nbytes_keys = w.reserve(4);
    w.be_i32(nbytes_name as i32); // fNbytesName
    seek_value(w, seek_dir, big); // fSeekDir
    seek_value(w, seek_parent, big); // fSeekParent
    let p_seek_keys = w.reserve(if big { 8 } else { 4 });
    w.be_u16(1); // UUID version
    w.bytes(&[0u8; 16]); // UUID
    (p_nbytes_keys, p_seek_keys)
}

/// Write the **root** (top) directory record, always reserving the big (60-byte)
/// on-disk size even when the file is small. When `!big` the 48-byte small record
/// is followed by 12 zero pad bytes, so the slot is 60 bytes either way. This
/// mirrors ROOT's `TDirectoryFile::Sizeof()` (which reserves the 64-bit-seek
/// width whenever the file version ≥ 40000) and lets [`update_root_file`] later
/// widen the record to the 64-bit form *in place* — without shifting the objects,
/// subdirectories, or RNTuple that follow it. The name key's `obj_len` for this
/// record must therefore be `name_title_len + dir_record_total(true)` (= +60).
pub fn write_root_dir_record_fmt(
    w: &mut WBuffer,
    seek_dir: u64,
    seek_parent: u64,
    nbytes_name: u32,
    big: bool,
) -> (crate::buffer::Patch, crate::buffer::Patch) {
    let handles = write_dir_record_fmt(w, seek_dir, seek_parent, nbytes_name, big);
    if !big {
        // Reserve the extra width the 64-bit form needs (three seeks widen 4→8).
        w.bytes(&[0u8; 12]);
    }
    handles
}

/// Key version written for the small (32-bit seek) form; the big form adds 1000,
/// which is what the reader keys on ([`crate::file::key`]).
const KEY_VERSION_SMALL: u16 = 4;

/// Write a small-format (32-bit seek) `TKey` header (no payload). `obj_len` is
/// the uncompressed object size (`ObjLen`); `payload_len` is the on-disk payload
/// size (equal to `obj_len` when stored uncompressed). `Nbytes = KeyLen +
/// payload_len`.
#[allow(clippy::too_many_arguments)]
pub fn write_key_header(
    w: &mut WBuffer,
    class: &str,
    name: &str,
    title: &str,
    obj_len: u32,
    payload_len: u32,
    seek_key: u64,
    seek_pdir: u64,
) {
    write_key_header_fmt(
        w,
        class,
        name,
        title,
        obj_len,
        payload_len,
        seek_key,
        seek_pdir,
        1,
        false,
    );
}

/// Like [`write_key_header`], but with an explicit `cycle` (ROOT bumps the cycle
/// when an object is rewritten under an existing name; the highest cycle wins).
#[allow(clippy::too_many_arguments)]
pub fn write_key_header_cycle(
    w: &mut WBuffer,
    class: &str,
    name: &str,
    title: &str,
    obj_len: u32,
    payload_len: u32,
    seek_key: u64,
    seek_pdir: u64,
    cycle: u16,
) {
    write_key_header_fmt(
        w,
        class,
        name,
        title,
        obj_len,
        payload_len,
        seek_key,
        seek_pdir,
        cycle,
        false,
    );
}

/// The full `TKey` header writer: small or big (64-bit seek) form per `big`. Used
/// directly by writers that produce files past [`KSTART_BIG_FILE`]; the small
/// helpers above delegate here with `big = false`.
#[allow(clippy::too_many_arguments)]
pub fn write_key_header_fmt(
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
    let klen = key_len_fmt(class, name, title, big);
    w.be_i32((klen as u32 + payload_len) as i32); // Nbytes = KeyLen + on-disk payload
    w.be_u16(if big {
        KEY_VERSION_SMALL + 1000
    } else {
        KEY_VERSION_SMALL
    });
    w.be_u32(obj_len);
    w.be_u32(DATIME);
    w.be_u16(klen);
    w.be_u16(cycle);
    if big {
        w.be_u64(seek_key);
        w.be_u64(seek_pdir);
    } else {
        w.be_u32(seek_key as u32);
        w.be_u32(seek_pdir as u32);
    }
    w.string(class);
    w.string(name);
    w.string(title);
}

/// The on-disk payload for an object: compressed when `compression != 0` and the
/// result is actually smaller, otherwise the raw object bytes.
fn on_disk_payload(object: &[u8], compression: u32) -> Vec<u8> {
    if compression == 0 {
        return object.to_vec();
    }
    match oxiroot_compress::compress(object, compression) {
        Ok(compressed) if compressed.len() < object.len() => compressed,
        _ => object.to_vec(),
    }
}

/// Class/name/title of the streamer-info key. The fixed strings give the key a
/// `KeyLen` of 64 bytes, which the baked `TList` blob's internal class-tag
/// references depend on (ROOT resolves them relative to `-KeyLen`).
const STREAMER_INFO_NAME: &str = "StreamerInfo";
const STREAMER_INFO_TITLE: &str = "Doubly linked list";
const TLIST_CLASS: &str = "TList";

/// Build a complete TFile holding `objects` in its root directory, optionally
/// compressing object payloads (`compression` = `algorithm*100 + level`, 0 = none).
pub fn write_root_file(
    file_name: &str,
    objects: &[ObjectRecord],
    compression: u32,
) -> Result<Vec<u8>> {
    write_root_file_with_streamers(file_name, objects, compression, None)
}

/// Like [`write_root_file`], but also embeds `streamer_info` (the already-streamed
/// `TList<TStreamerInfo>` object bytes) as the file's streamer-info record at
/// `fSeekInfo`, making the file self-describing for any ROOT reader.
pub fn write_root_file_with_streamers(
    file_name: &str,
    objects: &[ObjectRecord],
    compression: u32,
    streamer_info: Option<&[u8]>,
) -> Result<Vec<u8>> {
    write_root_file_with_streamers_threshold(
        file_name,
        objects,
        compression,
        streamer_info,
        KSTART_BIG_FILE,
    )
}

/// Like [`write_root_file_with_streamers`] but with the big-file threshold
/// injectable for tests. Builds the small (32-bit) container first; only if that
/// already exceeds `threshold` does it rebuild in the big (64-bit) form, so the
/// object payloads are re-copied only for genuinely large files.
#[doc(hidden)]
pub fn write_root_file_with_streamers_threshold(
    file_name: &str,
    objects: &[ObjectRecord],
    compression: u32,
    streamer_info: Option<&[u8]>,
    threshold: u64,
) -> Result<Vec<u8>> {
    let payloads: Vec<Vec<u8>> = objects
        .iter()
        .map(|o| on_disk_payload(&o.object, compression))
        .collect();
    let streamer_payload = streamer_info.map(|si| on_disk_payload(si, compression));

    // Per-object cycle: the n-th object sharing a name gets cycle n (1-based), so
    // re-adding an existing name yields a higher, newer cycle (as ROOT does).
    let mut seen: std::collections::HashMap<&str, u16> = std::collections::HashMap::new();
    let cycles: Vec<u16> = objects
        .iter()
        .map(|o| {
            let c = seen.entry(o.name.as_str()).or_insert(0);
            *c += 1;
            *c
        })
        .collect();

    let small = write_streamers_pass(
        file_name,
        objects,
        &payloads,
        streamer_info,
        streamer_payload.as_deref(),
        &cycles,
        compression,
        false,
    );
    if small.len() as u64 <= threshold {
        return Ok(small);
    }
    // The small container would overflow its 32-bit seek pointers; rebuild in the
    // big (64-bit) form. Big keys/records are larger, so a file that overflowed
    // the small form overflows it in the big form too — one rebuild converges.
    Ok(write_streamers_pass(
        file_name,
        objects,
        &payloads,
        streamer_info,
        streamer_payload.as_deref(),
        &cycles,
        compression,
        true,
    ))
}

/// One layout pass of [`write_root_file_with_streamers`] in the small (32-bit) or
/// big (64-bit) container form.
#[allow(clippy::too_many_arguments)]
fn write_streamers_pass(
    file_name: &str,
    objects: &[ObjectRecord],
    payloads: &[Vec<u8>],
    streamer_info: Option<&[u8]>,
    streamer_payload: Option<&[u8]>,
    cycles: &[u16],
    compression: u32,
    big: bool,
) -> Vec<u8> {
    let mut w = WBuffer::new();

    // --- File header (100 bytes; pointers patched at the end). ---
    w.bytes(b"root");
    w.be_u32(if big {
        FILE_VERSION + BIG_FILE_VERSION
    } else {
        FILE_VERSION
    });
    w.be_u32(100); // fBEGIN
    let p_end = w.reserve(if big { 8 } else { 4 });
    seek_zero(&mut w, big); // fSeekFree
    w.be_u32(0); // fNbytesFree
    w.be_u32(0); // nfree
    let p_nbytes_name = w.reserve(4);
    w.u8(if big { 8 } else { 4 }); // fUnits
    w.be_u32(compression); // fCompress
    let p_seek_info = w.reserve(if big { 8 } else { 4 });
    let p_nbytes_info = w.reserve(4);
    w.be_u16(1); // fUUID version
    w.bytes(&[0u8; 16]); // fUUID
    while w.len() < 100 {
        w.u8(0);
    }

    // --- Root directory name key + object (at fBEGIN = 100). The root record is
    // always reserved at the big (60-byte) size so the file can later be appended
    // into the 64-bit form in place (as ROOT reserves it). ---
    let first_klen = key_len_fmt(DIR_CLASS, file_name, "", big);
    let name_title_len = (1 + file_name.len()) + 1; // object name=file_name, title=""
    let f_nbytes_name = first_klen as usize + name_title_len; // dir record starts here
    let first_obj_len = name_title_len as u32 + dir_record_total(true);

    write_key_header_fmt(
        &mut w,
        DIR_CLASS,
        file_name,
        "",
        first_obj_len,
        first_obj_len,
        100,
        0,
        1,
        big,
    );
    w.string(file_name); // object: name
    w.string(""); // object: title
    let (p_dir_nbytes_keys, p_dir_seek_keys) =
        write_root_dir_record_fmt(&mut w, 100, 0, f_nbytes_name as u32, big);

    // --- One key + object per stored object. ---
    let mut seeks = Vec::with_capacity(objects.len());
    for (i, obj) in objects.iter().enumerate() {
        let seek = w.len();
        write_key_header_fmt(
            &mut w,
            &obj.class_name,
            &obj.name,
            &obj.title,
            obj.object.len() as u32,
            payloads[i].len() as u32,
            seek as u64,
            100,
            cycles[i],
            big,
        );
        w.bytes(&payloads[i]);
        seeks.push(seek);
    }

    // --- Streamer-info record (TList<TStreamerInfo>), referenced by fSeekInfo
    // only (not listed as a directory key). ---
    let (seek_info, nbytes_info) = match (streamer_info, streamer_payload) {
        (Some(object), Some(payload)) => {
            let seek = w.len();
            write_key_header_fmt(
                &mut w,
                TLIST_CLASS,
                STREAMER_INFO_NAME,
                STREAMER_INFO_TITLE,
                object.len() as u32,
                payload.len() as u32,
                seek as u64,
                100,
                1,
                big,
            );
            w.bytes(payload);
            let klen =
                key_len_fmt(TLIST_CLASS, STREAMER_INFO_NAME, STREAMER_INFO_TITLE, big) as u32;
            (seek as u64, klen + payload.len() as u32)
        }
        _ => (0, 0),
    };

    // --- Directory key list: a key, then nkeys, then a header per object. ---
    let keylist_seek = w.len();
    let keylist_obj_len = {
        let headers: usize = objects
            .iter()
            .map(|o| key_len_fmt(&o.class_name, &o.name, &o.title, big) as usize)
            .sum();
        (4 + headers) as u32
    };
    write_key_header_fmt(
        &mut w,
        DIR_CLASS,
        file_name,
        "",
        keylist_obj_len,
        keylist_obj_len,
        keylist_seek as u64,
        100,
        1,
        big,
    );
    w.be_i32(objects.len() as i32); // nkeys
    for (i, obj) in objects.iter().enumerate() {
        write_key_header_fmt(
            &mut w,
            &obj.class_name,
            &obj.name,
            &obj.title,
            obj.object.len() as u32,
            payloads[i].len() as u32,
            seeks[i] as u64,
            100,
            cycles[i],
            big,
        );
    }
    let keylist_nbytes = key_len_fmt(DIR_CLASS, file_name, "", big) as u32 + keylist_obj_len;
    let f_end = w.len();

    // --- Back-patch header + directory pointers. ---
    if big {
        w.patch_be_u64(p_end, f_end as u64);
    } else {
        w.patch_be_u32(p_end, f_end as u32);
    }
    w.patch_be_u32(p_nbytes_name, f_nbytes_name as u32);
    if big {
        w.patch_be_u64(p_seek_info, seek_info);
    } else {
        w.patch_be_u32(p_seek_info, seek_info as u32);
    }
    w.patch_be_u32(p_nbytes_info, nbytes_info);
    w.patch_be_u32(p_dir_nbytes_keys, keylist_nbytes);
    if big {
        w.patch_be_u64(p_dir_seek_keys, keylist_seek as u64);
    } else {
        w.patch_be_u32(p_dir_seek_keys, keylist_seek as u32);
    }

    w.into_vec()
}

/// Append `new_objects` to an existing ROOT file (`existing` bytes), returning a
/// new file holding the existing objects plus the new ones. `file_name` is the
/// root directory's name; `compression` applies to all object payloads.
///
/// Existing objects are copied (decompressed, then re-emitted); an added object
/// whose name matches an existing one gets a higher, newer cycle, as ROOT does.
/// The file's existing streamer info is preserved unless `streamer_info` is
/// given, in which case that replaces it.
///
/// This appends *in place*: the existing bytes are left untouched, the new
/// objects and a relocated root key list are written after them, and only the
/// file header and directory record are patched. Because nothing existing moves,
/// it preserves files that contain subdirectories and an RNTuple (whose anchor
/// and page locators hold absolute file offsets).
///
/// When the appended result would cross 2 GiB (or the existing file is already
/// the 64-bit form), the output is written in ROOT's big (64-bit) container form:
/// the new keys and the root key list use 64-bit seeks, and the file header and
/// root directory record are rewritten in place in big form. This needs the root
/// directory record to have been reserved at its 64-bit width (60 bytes) — every
/// oxiroot- or ROOT-written file is; a legacy file that reserved only 48 bytes
/// returns an error rather than corrupting the object that follows it.
///
/// The file's existing streamer info is kept (the appended standard objects are
/// read via ROOT/uproot's built-in dictionaries); `streamer_info` is only used
/// when the file has none.
pub fn update_root_file(
    existing: &[u8],
    file_name: &str,
    new_objects: &[ObjectRecord],
    compression: u32,
    streamer_info: Option<&[u8]>,
) -> Result<Vec<u8>> {
    update_root_file_threshold(
        existing,
        file_name,
        new_objects,
        compression,
        streamer_info,
        KSTART_BIG_FILE,
    )
}

/// Like [`update_root_file`] but with the big-file threshold injectable for tests.
#[doc(hidden)]
pub fn update_root_file_threshold(
    existing: &[u8],
    file_name: &str,
    new_objects: &[ObjectRecord],
    compression: u32,
    streamer_info: Option<&[u8]>,
    threshold: u64,
) -> Result<Vec<u8>> {
    let file = RFile::from_bytes(existing.to_vec())?;
    let header = file.header().clone();
    let end = header.end as usize;
    if existing.len() < end {
        return Err(Error::Format(format!(
            "file is truncated: fEND={end} but only {} bytes present",
            existing.len()
        )));
    }

    // Existing (non-deleted) root keys keep their byte positions and cycles; they
    // are relisted, pointing at their unchanged offsets, in the new key list.
    let existing_keys: Vec<&TKey> = file.keys().iter().filter(|k| !k.is_deleted()).collect();

    // An already-big file must stay big. Otherwise build the small form first and
    // only rebuild big if it would overflow the 32-bit seek pointers.
    if !header.is_big() {
        let small = append_pass(
            existing,
            &header,
            &existing_keys,
            file_name,
            new_objects,
            compression,
            streamer_info,
            false,
        )?;
        if small.len() as u64 <= threshold {
            return Ok(small);
        }
    }
    append_pass(
        existing,
        &header,
        &existing_keys,
        file_name,
        new_objects,
        compression,
        streamer_info,
        true,
    )
}

/// One append pass in the small (32-bit) or big (64-bit) container form: keep the
/// existing bytes, append the new keys + payloads + root key list, then patch the
/// header and root directory record in place.
#[allow(clippy::too_many_arguments)]
fn append_pass(
    existing: &[u8],
    header: &FileHeader,
    existing_keys: &[&TKey],
    file_name: &str,
    new_objects: &[ObjectRecord],
    compression: u32,
    streamer_info: Option<&[u8]>,
    big: bool,
) -> Result<Vec<u8>> {
    let begin = header.begin;
    let end = header.end as usize;

    // A new object whose name matches an existing key (or an earlier new object)
    // gets the next-higher cycle, as ROOT does.
    let mut max_cycle: std::collections::HashMap<&str, u16> = std::collections::HashMap::new();
    for k in existing_keys {
        let e = max_cycle.entry(k.name.as_str()).or_insert(0);
        *e = (*e).max(k.cycle);
    }

    // The output starts as the existing content (everything up to fEND); new
    // records are appended after it.
    let mut out = existing[..end].to_vec();

    // --- Append the new objects' keys + payloads. ---
    struct NewKey {
        class: String,
        name: String,
        title: String,
        obj_len: u32,
        payload_len: u32,
        seek: u64,
        cycle: u16,
    }
    let mut new_keys = Vec::with_capacity(new_objects.len());
    for obj in new_objects {
        let cycle = {
            let e = max_cycle.entry(obj.name.as_str()).or_insert(0);
            *e += 1;
            *e
        };
        let payload = on_disk_payload(&obj.object, compression);
        let seek = out.len() as u64;
        let mut w = WBuffer::new();
        write_key_header_fmt(
            &mut w,
            &obj.class_name,
            &obj.name,
            &obj.title,
            obj.object.len() as u32,
            payload.len() as u32,
            seek,
            begin,
            cycle,
            big,
        );
        out.extend_from_slice(&w.into_vec());
        out.extend_from_slice(&payload);
        new_keys.push(NewKey {
            class: obj.class_name.clone(),
            name: obj.name.clone(),
            title: obj.title.clone(),
            obj_len: obj.object.len() as u32,
            payload_len: payload.len() as u32,
            seek,
            cycle,
        });
    }

    // --- Append a streamer info record only if the file has none. ---
    let (seek_info, nbytes_info) = if header.seek_info != 0 {
        (header.seek_info, header.nbytes_info) // keep existing
    } else if let Some(si) = streamer_info {
        let payload = on_disk_payload(si, compression);
        let seek = out.len() as u64;
        let mut w = WBuffer::new();
        write_key_header_fmt(
            &mut w,
            TLIST_CLASS,
            STREAMER_INFO_NAME,
            STREAMER_INFO_TITLE,
            si.len() as u32,
            payload.len() as u32,
            seek,
            begin,
            1,
            big,
        );
        out.extend_from_slice(&w.into_vec());
        out.extend_from_slice(&payload);
        let klen = key_len_fmt(TLIST_CLASS, STREAMER_INFO_NAME, STREAMER_INFO_TITLE, big) as u32;
        (seek, klen + payload.len() as u32)
    } else {
        (0, 0)
    };

    // --- Append the new root key list (existing keys + new keys). A key-list
    // entry's format must match the actual on-disk key it describes (its `fKeyLen`
    // sets a reader's payload offset), so an existing key is relisted in its
    // ORIGINAL format — a small on-disk key stays a small entry even in a big
    // file's key list, which ROOT/uproot read fine. New keys were written on-disk
    // in `big` form, so their entries are `big`. ---
    let keylist_seek = out.len() as u64;
    let entry_headers: usize = existing_keys
        .iter()
        .map(|k| k.key_len as usize)
        .chain(
            new_keys
                .iter()
                .map(|k| key_len_fmt(&k.class, &k.name, &k.title, big) as usize),
        )
        .sum();
    let keylist_obj_len = (4 + entry_headers) as u32;
    let mut w = WBuffer::new();
    write_key_header_fmt(
        &mut w,
        DIR_CLASS,
        file_name,
        "",
        keylist_obj_len,
        keylist_obj_len,
        keylist_seek,
        begin,
        1,
        big,
    );
    let nkeys = existing_keys.len() + new_keys.len();
    w.be_i32(nkeys as i32);
    for k in existing_keys {
        let klen = k.key_len as u32;
        let payload_len = (k.nbytes as u32).saturating_sub(klen);
        write_key_header_fmt(
            &mut w,
            &k.class_name,
            &k.name,
            &k.title,
            k.obj_len,
            payload_len,
            k.seek_key,
            begin,
            k.cycle,
            k.version > 1000, // preserve the on-disk key's format
        );
    }
    for k in &new_keys {
        write_key_header_fmt(
            &mut w,
            &k.class,
            &k.name,
            &k.title,
            k.obj_len,
            k.payload_len,
            k.seek,
            begin,
            k.cycle,
            big,
        );
    }
    out.extend_from_slice(&w.into_vec());
    let keylist_nbytes = key_len_fmt(DIR_CLASS, file_name, "", big) as u32 + keylist_obj_len;

    let f_end = out.len();
    let dir_record = begin as usize + header.nbytes_name as usize;

    if !big {
        guard_small_format(f_end)?;
        // --- Patch the small-format header pointers + directory record in place.
        // Header offsets: fEND=12, fSeekFree=16, fNbytesFree=20, nfree=24,
        // fSeekInfo=37, fNbytesInfo=41; dir-record offsets: fNbytesKeys=10,
        // fSeekKeys=26.
        patch_be_u32(&mut out, 12, f_end as u32);
        patch_be_u32(&mut out, 16, 0);
        patch_be_u32(&mut out, 20, 0);
        patch_be_u32(&mut out, 24, 0);
        patch_be_u32(&mut out, 37, seek_info as u32);
        patch_be_u32(&mut out, 41, nbytes_info);
        patch_be_u32(&mut out, dir_record + 10, keylist_nbytes);
        patch_be_u32(&mut out, dir_record + 26, keylist_seek as u32);
        return Ok(out);
    }

    // --- Big output: rewrite the file header and root directory record in place.
    // The header always fits in `0..fBEGIN`; the directory record must have been
    // reserved at the 64-bit (60-byte) width, or its object would be overrun. ---
    let name_title_len = (1 + file_name.len()) + 1;
    let name_key = TKey::read(&mut RBuffer::new(&existing[begin as usize..]))?;
    let reserved = (name_key.obj_len as usize).saturating_sub(name_title_len);
    if reserved < dir_record_total(true) as usize {
        return Err(Error::Format(format!(
            "cannot append into the 64-bit form: this file's root directory record \
             reserves {reserved} bytes, but the big form needs {}. Rewrite the file \
             with RootFile::create (which reserves the 64-bit width) first.",
            dir_record_total(true)
        )));
    }

    // Rewrite the 100-byte header in big form (preserving name/compress/UUID).
    let ver = if header.is_big() {
        header.version
    } else {
        header.version + BIG_FILE_VERSION
    };
    let mut h = WBuffer::new();
    h.bytes(b"root");
    h.be_u32(ver);
    h.be_u32(begin as u32); // fBEGIN
    h.be_u64(f_end as u64); // fEND
    h.be_u64(0); // fSeekFree
    h.be_u32(0); // fNbytesFree
    h.be_u32(0); // nfree
    h.be_u32(header.nbytes_name); // fNbytesName
    h.u8(8); // fUnits
    h.be_u32(header.compress); // fCompress
    h.be_u64(seek_info); // fSeekInfo
    h.be_u32(nbytes_info); // fNbytesInfo
    h.be_u16(header.uuid.version);
    h.bytes(&header.uuid.bytes);
    while h.len() < begin as usize {
        h.u8(0);
    }
    let h = h.into_vec();
    out[..h.len()].copy_from_slice(&h);

    // Rewrite the root directory record in big form into its 60-byte slot.
    let mut d = WBuffer::new();
    d.be_i16(1005); // version (>1000 ⇒ 64-bit seeks)
    d.be_u32(DATIME); // fDatimeC
    d.be_u32(DATIME); // fDatimeM
    d.be_u32(keylist_nbytes); // fNbytesKeys
    d.be_i32(header.nbytes_name as i32); // fNbytesName
    d.be_u64(begin); // fSeekDir
    d.be_u64(0); // fSeekParent
    d.be_u64(keylist_seek); // fSeekKeys
    d.be_u16(1); // UUID version
    d.bytes(&[0u8; 16]); // UUID
    let d = d.into_vec();
    out[dir_record..dir_record + d.len()].copy_from_slice(&d);

    Ok(out)
}

/// Overwrite a big-endian `u32` at absolute `offset` in `buf`.
fn patch_be_u32(buf: &mut [u8], offset: usize, value: u32) {
    buf[offset..offset + 4].copy_from_slice(&value.to_be_bytes());
}

/// A subdirectory to create in the file's root directory, holding its own
/// objects (one level of nesting).
pub struct Subdir {
    /// Subdirectory name (becomes a `TDirectory` key in the root directory).
    pub name: String,
    /// Objects stored directly in this subdirectory.
    pub objects: Vec<ObjectRecord>,
}

/// Write a directory's key list (a wrapping `TKey` whose payload is an `i32`
/// count followed by a `TKey` header per entry), in the small or big (64-bit
/// seek) form. `entries` are `(class, name, title, obj_len, payload_len,
/// seek_key)` tuples. Returns `(seek, nbytes)`.
pub fn write_key_list_fmt(
    w: &mut WBuffer,
    dir_class: &str,
    dir_name: &str,
    dir_title: &str,
    seek_pdir: u64,
    entries: &[(&str, &str, &str, u32, u32, u64)],
    big: bool,
) -> (u64, u32) {
    let seek = w.len() as u64;
    let headers: usize = entries
        .iter()
        .map(|(c, n, t, _, _, _)| key_len_fmt(c, n, t, big) as usize)
        .sum();
    let obj_len = (4 + headers) as u32;
    write_key_header_fmt(
        w, dir_class, dir_name, dir_title, obj_len, obj_len, seek, seek_pdir, 1, big,
    );
    w.be_i32(entries.len() as i32);
    for (c, n, t, ol, pl, sk) in entries {
        write_key_header_fmt(w, c, n, t, *ol, *pl, *sk, seek_pdir, 1, big);
    }
    let nbytes = key_len_fmt(dir_class, dir_name, dir_title, big) as u32 + obj_len;
    (seek, nbytes)
}

/// Build a TFile whose root directory holds `root_objects` plus one level of
/// `subdirs`, each subdirectory holding its own objects. Optionally embeds
/// `streamer_info` and compresses object payloads. ROOT/uproot navigate the
/// subdirectories natively.
pub fn write_root_file_with_dirs(
    file_name: &str,
    root_objects: &[ObjectRecord],
    subdirs: &[Subdir],
    compression: u32,
    streamer_info: Option<&[u8]>,
) -> Result<Vec<u8>> {
    write_root_file_with_dirs_threshold(
        file_name,
        root_objects,
        subdirs,
        compression,
        streamer_info,
        KSTART_BIG_FILE,
    )
}

/// Like [`write_root_file_with_dirs`] but with the big-file threshold injectable
/// for tests. Builds the small (32-bit) container first; only if that already
/// exceeds `threshold` does it rebuild in the big (64-bit) form.
#[doc(hidden)]
#[allow(clippy::too_many_arguments)]
pub fn write_root_file_with_dirs_threshold(
    file_name: &str,
    root_objects: &[ObjectRecord],
    subdirs: &[Subdir],
    compression: u32,
    streamer_info: Option<&[u8]>,
    threshold: u64,
) -> Result<Vec<u8>> {
    let root_pl: Vec<Vec<u8>> = root_objects
        .iter()
        .map(|o| on_disk_payload(&o.object, compression))
        .collect();
    let sub_pl: Vec<Vec<Vec<u8>>> = subdirs
        .iter()
        .map(|s| {
            s.objects
                .iter()
                .map(|o| on_disk_payload(&o.object, compression))
                .collect()
        })
        .collect();
    let streamer_pl = streamer_info.map(|si| on_disk_payload(si, compression));

    let small = write_dirs_pass(
        file_name,
        root_objects,
        subdirs,
        &root_pl,
        &sub_pl,
        streamer_info,
        streamer_pl.as_deref(),
        compression,
        false,
    );
    if small.len() as u64 <= threshold {
        return Ok(small);
    }
    Ok(write_dirs_pass(
        file_name,
        root_objects,
        subdirs,
        &root_pl,
        &sub_pl,
        streamer_info,
        streamer_pl.as_deref(),
        compression,
        true,
    ))
}

/// One layout pass of [`write_root_file_with_dirs`] in the small (32-bit) or big
/// (64-bit) container form.
#[allow(clippy::too_many_arguments)]
fn write_dirs_pass(
    file_name: &str,
    root_objects: &[ObjectRecord],
    subdirs: &[Subdir],
    root_pl: &[Vec<u8>],
    sub_pl: &[Vec<Vec<u8>>],
    streamer_info: Option<&[u8]>,
    streamer_pl: Option<&[u8]>,
    compression: u32,
    big: bool,
) -> Vec<u8> {
    let dir_total = dir_record_total(big);
    let mut w = WBuffer::new();

    // --- File header. ---
    w.bytes(b"root");
    w.be_u32(if big {
        FILE_VERSION + BIG_FILE_VERSION
    } else {
        FILE_VERSION
    });
    w.be_u32(100);
    let p_end = w.reserve(if big { 8 } else { 4 });
    seek_zero(&mut w, big); // fSeekFree
    w.be_u32(0); // fNbytesFree
    w.be_u32(0); // nfree
    let p_nbytes_name = w.reserve(4);
    w.u8(if big { 8 } else { 4 });
    w.be_u32(compression);
    let p_seek_info = w.reserve(if big { 8 } else { 4 });
    let p_nbytes_info = w.reserve(4);
    w.be_u16(1);
    w.bytes(&[0u8; 16]);
    while w.len() < 100 {
        w.u8(0);
    }

    // --- Root directory name key + record (at fBEGIN = 100). Always reserved at
    // the big (60-byte) size so the file can later be appended into 64-bit form. ---
    let first_klen = key_len_fmt(DIR_CLASS, file_name, "", big);
    let name_title_len = (1 + file_name.len()) + 1;
    let f_nbytes_name = (first_klen as usize + name_title_len) as u32;
    let first_obj_len = name_title_len as u32 + dir_record_total(true);
    write_key_header_fmt(
        &mut w,
        DIR_CLASS,
        file_name,
        "",
        first_obj_len,
        first_obj_len,
        100,
        0,
        1,
        big,
    );
    w.string(file_name);
    w.string("");
    let (p_root_nbk, p_root_sk) = write_root_dir_record_fmt(&mut w, 100, 0, f_nbytes_name, big);

    // --- Root objects. ---
    let mut root_seeks = Vec::with_capacity(root_objects.len());
    for (i, o) in root_objects.iter().enumerate() {
        let s = w.len() as u64;
        write_key_header_fmt(
            &mut w,
            &o.class_name,
            &o.name,
            &o.title,
            o.object.len() as u32,
            root_pl[i].len() as u32,
            s,
            100,
            1,
            big,
        );
        w.bytes(&root_pl[i]);
        root_seeks.push(s);
    }

    // --- Streamer-info record (referenced only by fSeekInfo). ---
    let (seek_info, nbytes_info) = match (streamer_info, streamer_pl) {
        (Some(object), Some(payload)) => {
            let s = w.len() as u64;
            write_key_header_fmt(
                &mut w,
                TLIST_CLASS,
                STREAMER_INFO_NAME,
                STREAMER_INFO_TITLE,
                object.len() as u32,
                payload.len() as u32,
                s,
                100,
                1,
                big,
            );
            w.bytes(payload);
            let klen =
                key_len_fmt(TLIST_CLASS, STREAMER_INFO_NAME, STREAMER_INFO_TITLE, big) as u32;
            (s, klen + payload.len() as u32)
        }
        _ => (0, 0),
    };

    // --- Subdirectories: each = TDirectory key + record, its objects, its key list. ---
    let mut sub_seeks = Vec::with_capacity(subdirs.len());
    for (si, sub) in subdirs.iter().enumerate() {
        let sub_klen = key_len_fmt("TDirectory", &sub.name, &sub.name, big);
        let s_sub = w.len() as u64;
        write_key_header_fmt(
            &mut w,
            "TDirectory",
            &sub.name,
            &sub.name,
            dir_total,
            dir_total,
            s_sub,
            100,
            1,
            big,
        );
        let (p_sub_nbk, p_sub_sk) = write_dir_record_fmt(&mut w, s_sub, 100, sub_klen as u32, big);

        let mut obj_seeks = Vec::with_capacity(sub.objects.len());
        for (j, o) in sub.objects.iter().enumerate() {
            let s = w.len() as u64;
            write_key_header_fmt(
                &mut w,
                &o.class_name,
                &o.name,
                &o.title,
                o.object.len() as u32,
                sub_pl[si][j].len() as u32,
                s,
                s_sub,
                1,
                big,
            );
            w.bytes(&sub_pl[si][j]);
            obj_seeks.push(s);
        }

        let entries: Vec<(&str, &str, &str, u32, u32, u64)> = sub
            .objects
            .iter()
            .enumerate()
            .map(|(j, o)| {
                (
                    o.class_name.as_str(),
                    o.name.as_str(),
                    o.title.as_str(),
                    o.object.len() as u32,
                    sub_pl[si][j].len() as u32,
                    obj_seeks[j],
                )
            })
            .collect();
        let (sub_kl_seek, sub_kl_nbytes) = write_key_list_fmt(
            &mut w,
            "TDirectory",
            &sub.name,
            &sub.name,
            s_sub,
            &entries,
            big,
        );
        w.patch_be_u32(p_sub_nbk, sub_kl_nbytes);
        if big {
            w.patch_be_u64(p_sub_sk, sub_kl_seek);
        } else {
            w.patch_be_u32(p_sub_sk, sub_kl_seek as u32);
        }
        sub_seeks.push(s_sub);
    }

    // --- Root key list: root objects + a TDirectory entry per subdirectory. ---
    let mut entries: Vec<(&str, &str, &str, u32, u32, u64)> = root_objects
        .iter()
        .enumerate()
        .map(|(i, o)| {
            (
                o.class_name.as_str(),
                o.name.as_str(),
                o.title.as_str(),
                o.object.len() as u32,
                root_pl[i].len() as u32,
                root_seeks[i],
            )
        })
        .collect();
    for (si, sub) in subdirs.iter().enumerate() {
        entries.push((
            "TDirectory",
            sub.name.as_str(),
            sub.name.as_str(),
            dir_total,
            dir_total,
            sub_seeks[si],
        ));
    }
    let (root_kl_seek, root_kl_nbytes) =
        write_key_list_fmt(&mut w, DIR_CLASS, file_name, "", 100, &entries, big);
    w.patch_be_u32(p_root_nbk, root_kl_nbytes);
    if big {
        w.patch_be_u64(p_root_sk, root_kl_seek);
    } else {
        w.patch_be_u32(p_root_sk, root_kl_seek as u32);
    }

    let f_end = w.len();
    if big {
        w.patch_be_u64(p_end, f_end as u64);
    } else {
        w.patch_be_u32(p_end, f_end as u32);
    }
    w.patch_be_u32(p_nbytes_name, f_nbytes_name);
    if big {
        w.patch_be_u64(p_seek_info, seek_info);
    } else {
        w.patch_be_u32(p_seek_info, seek_info as u32);
    }
    w.patch_be_u32(p_nbytes_info, nbytes_info);

    w.into_vec()
}

#[cfg(test)]
mod tests {
    use super::{guard_small_format, KSTART_BIG_FILE};

    #[test]
    fn small_format_guard_boundary() {
        assert!(guard_small_format(0).is_ok());
        assert!(guard_small_format(KSTART_BIG_FILE as usize).is_ok());
        assert!(guard_small_format(KSTART_BIG_FILE as usize + 1).is_err());
    }
}
