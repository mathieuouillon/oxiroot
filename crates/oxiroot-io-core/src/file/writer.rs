//! Writing a minimal, ROOT-compatible TFile container.
//!
//! Produces a small-format (32-bit) file: header, the root directory's name
//! key + `TDirectory` record, one `TKey` + object per supplied object, an
//! optional streamer-info record, and the directory key list. The free list is
//! kept empty. All offsets are back-patched once the layout is known.
//! [`update_root_file`] re-reads an existing file and rewrites it with extra
//! objects appended.

use crate::buffer::WBuffer;
use crate::error::{Error, Result};

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
pub fn write_root_file(file_name: &str, objects: &[ObjectRecord], compression: u32) -> Vec<u8> {
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
) -> Vec<u8> {
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

    let mut w = WBuffer::new();

    // --- File header (100 bytes; pointers patched at the end). ---
    w.bytes(b"root");
    w.be_u32(FILE_VERSION);
    w.be_u32(100); // fBEGIN
    let p_end = w.reserve(4);
    let p_seek_free = w.reserve(4);
    let p_nbytes_free = w.reserve(4);
    let p_nfree = w.reserve(4);
    let p_nbytes_name = w.reserve(4);
    w.u8(4); // fUnits
    w.be_u32(compression); // fCompress
    let p_seek_info = w.reserve(4);
    let p_nbytes_info = w.reserve(4);
    w.be_u16(1); // fUUID version
    w.bytes(&[0u8; 16]); // fUUID
    while w.len() < 100 {
        w.u8(0);
    }

    // --- Root directory name key + object (at fBEGIN = 100). ---
    let first_klen = key_len(DIR_CLASS, file_name, "");
    let name_title_len = (1 + file_name.len()) + 1; // object name=file_name, title=""
    let f_nbytes_name = first_klen as usize + name_title_len; // dir record starts here
    let dir_record_len = 30 + 18; // TDirectory fields (30) + UUID (18)
    let first_obj_len = (name_title_len + dir_record_len) as u32;

    write_key_header(
        &mut w,
        DIR_CLASS,
        file_name,
        "",
        first_obj_len,
        first_obj_len,
        100,
        0,
    );
    w.string(file_name); // object: name
    w.string(""); // object: title
                  // TDirectory record.
    w.be_i16(5); // version
    w.be_u32(DATIME); // fDatimeC
    w.be_u32(DATIME); // fDatimeM
    let p_dir_nbytes_keys = w.reserve(4);
    w.be_i32(f_nbytes_name as i32); // fNbytesName
    w.be_u32(100); // fSeekDir
    w.be_u32(0); // fSeekParent
    let p_dir_seek_keys = w.reserve(4);
    w.be_u16(1); // UUID version
    w.bytes(&[0u8; 16]); // UUID

    // --- One key + object per stored object. ---
    let mut seeks = Vec::with_capacity(objects.len());
    for (i, obj) in objects.iter().enumerate() {
        let seek = w.len();
        write_key_header_cycle(
            &mut w,
            &obj.class_name,
            &obj.name,
            &obj.title,
            obj.object.len() as u32,
            payloads[i].len() as u32,
            seek as u64,
            100,
            cycles[i],
        );
        w.bytes(&payloads[i]);
        seeks.push(seek);
    }

    // --- Streamer-info record (TList<TStreamerInfo>), referenced by fSeekInfo
    // only (not listed as a directory key). ---
    let (seek_info, nbytes_info) = match (streamer_info, &streamer_payload) {
        (Some(object), Some(payload)) => {
            let seek = w.len();
            write_key_header(
                &mut w,
                TLIST_CLASS,
                STREAMER_INFO_NAME,
                STREAMER_INFO_TITLE,
                object.len() as u32,
                payload.len() as u32,
                seek as u64,
                100,
            );
            w.bytes(payload);
            let klen = key_len(TLIST_CLASS, STREAMER_INFO_NAME, STREAMER_INFO_TITLE) as u32;
            (seek as u32, klen + payload.len() as u32)
        }
        _ => (0, 0),
    };

    // --- Directory key list: a key, then nkeys, then a header per object. ---
    let keylist_seek = w.len();
    let keylist_obj_len = {
        let headers: usize = objects
            .iter()
            .map(|o| key_len(&o.class_name, &o.name, &o.title) as usize)
            .sum();
        (4 + headers) as u32
    };
    write_key_header(
        &mut w,
        DIR_CLASS,
        file_name,
        "",
        keylist_obj_len,
        keylist_obj_len,
        keylist_seek as u64,
        100,
    );
    w.be_i32(objects.len() as i32); // nkeys
    for (i, obj) in objects.iter().enumerate() {
        write_key_header_cycle(
            &mut w,
            &obj.class_name,
            &obj.name,
            &obj.title,
            obj.object.len() as u32,
            payloads[i].len() as u32,
            seeks[i] as u64,
            100,
            cycles[i],
        );
    }
    let keylist_nbytes = key_len(DIR_CLASS, file_name, "") as u32 + keylist_obj_len;
    let f_end = w.len() as u32;

    // --- Back-patch header + directory pointers. ---
    w.patch_be_u32(p_end, f_end);
    w.patch_be_u32(p_seek_free, 0);
    w.patch_be_u32(p_nbytes_free, 0);
    w.patch_be_u32(p_nfree, 0);
    w.patch_be_u32(p_nbytes_name, f_nbytes_name as u32);
    w.patch_be_u32(p_seek_info, seek_info);
    w.patch_be_u32(p_nbytes_info, nbytes_info);
    w.patch_be_u32(p_dir_nbytes_keys, keylist_nbytes);
    w.patch_be_u32(p_dir_seek_keys, keylist_seek as u32);

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
/// This rewrites the whole file rather than appending in place, so it does not
/// support files containing an RNTuple (whose anchor stores absolute file
/// offsets that a rewrite would invalidate); such files return an error.
pub fn update_root_file(
    existing: &[u8],
    file_name: &str,
    new_objects: &[ObjectRecord],
    compression: u32,
    streamer_info: Option<&[u8]>,
) -> Result<Vec<u8>> {
    let file = RFile::from_bytes(existing.to_vec())?;

    // Copy existing objects, ordered by ascending cycle so the cycle-by-position
    // assignment in the rewrite reproduces (or extends) their cycles. Deleted
    // keys (negative fNbytes) point at freed space and must be skipped.
    let mut keys: Vec<&TKey> = file.keys().iter().filter(|k| !k.is_deleted()).collect();
    keys.sort_by_key(|k| k.cycle);

    let mut objects: Vec<ObjectRecord> = Vec::with_capacity(keys.len() + new_objects.len());
    for key in keys {
        if key.class_name == "ROOT::RNTuple" {
            return Err(Error::Format(
                "updating a file that contains an RNTuple is not supported \
                 (its anchor holds absolute file offsets)"
                    .into(),
            ));
        }
        if key.class_name == "TDirectory" || key.class_name == "TDirectoryFile" {
            return Err(Error::Format(
                "updating a file that contains subdirectories is not supported \
                 (the subdirectory record and its keys are not relocated)"
                    .into(),
            ));
        }
        let payload = key.payload(file.data())?;
        let object = oxiroot_compress::decompress(payload, key.obj_len as usize)
            .map_err(|e| Error::Format(format!("decompressing {:?}: {e}", key.name)))?;
        objects.push(ObjectRecord {
            class_name: key.class_name.clone(),
            name: key.name.clone(),
            title: key.title.clone(),
            object,
        });
    }
    for o in new_objects {
        objects.push(ObjectRecord {
            class_name: o.class_name.clone(),
            name: o.name.clone(),
            title: o.title.clone(),
            object: o.object.clone(),
        });
    }

    let existing_si = file.streamer_info_object()?;
    let si = streamer_info.or(existing_si.as_deref());

    Ok(write_root_file_with_streamers(
        file_name,
        &objects,
        compression,
        si,
    ))
}

/// A subdirectory to create in the file's root directory, holding its own
/// objects (one level of nesting).
pub struct Subdir {
    /// Subdirectory name (becomes a `TDirectory` key in the root directory).
    pub name: String,
    /// Objects stored directly in this subdirectory.
    pub objects: Vec<ObjectRecord>,
}

/// Write the small-format `TDirectory` record an object-or-subdirectory uses.
/// Returns the (reserved nbytesKeys, reserved seekKeys) patch handles to fill in
/// once that directory's key list has been written. `seek_dir` is this
/// directory's own offset; `nbytes_name` is the size of its name record (the
/// root's name key + name/title, or a subdirectory key's `KeyLen`).
fn write_dir_record(
    w: &mut WBuffer,
    seek_dir: u64,
    seek_parent: u64,
    nbytes_name: u32,
) -> (crate::buffer::Patch, crate::buffer::Patch) {
    w.be_i16(5); // version (small format)
    w.be_u32(DATIME); // fDatimeC
    w.be_u32(DATIME); // fDatimeM
    let p_nbytes_keys = w.reserve(4);
    w.be_i32(nbytes_name as i32);
    w.be_u32(seek_dir as u32);
    w.be_u32(seek_parent as u32);
    let p_seek_keys = w.reserve(4);
    w.be_u16(1); // UUID version
    w.bytes(&[0u8; 16]); // UUID
    (p_nbytes_keys, p_seek_keys)
}

/// Size of the `TDirectory` record written by [`write_dir_record`].
const DIR_RECORD_LEN: u32 = 48;

/// Write a directory's key list (a wrapping `TKey` whose payload is an `i32`
/// count followed by a `TKey` header per entry). `entries` are `(class, name,
/// title, obj_len, payload_len, seek_key)` tuples. Returns `(seek, nbytes)`.
fn write_key_list(
    w: &mut WBuffer,
    dir_class: &str,
    dir_name: &str,
    dir_title: &str,
    seek_pdir: u64,
    entries: &[(&str, &str, &str, u32, u32, u64)],
) -> (u64, u32) {
    let seek = w.len() as u64;
    let headers: usize = entries
        .iter()
        .map(|(c, n, t, _, _, _)| key_len(c, n, t) as usize)
        .sum();
    let obj_len = (4 + headers) as u32;
    write_key_header(
        w, dir_class, dir_name, dir_title, obj_len, obj_len, seek, seek_pdir,
    );
    w.be_i32(entries.len() as i32);
    for (c, n, t, ol, pl, sk) in entries {
        write_key_header(w, c, n, t, *ol, *pl, *sk, seek_pdir);
    }
    let nbytes = key_len(dir_class, dir_name, dir_title) as u32 + obj_len;
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
) -> Vec<u8> {
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

    let mut w = WBuffer::new();

    // --- File header. ---
    w.bytes(b"root");
    w.be_u32(FILE_VERSION);
    w.be_u32(100);
    let p_end = w.reserve(4);
    let p_seek_free = w.reserve(4);
    let p_nbytes_free = w.reserve(4);
    let p_nfree = w.reserve(4);
    let p_nbytes_name = w.reserve(4);
    w.u8(4);
    w.be_u32(compression);
    let p_seek_info = w.reserve(4);
    let p_nbytes_info = w.reserve(4);
    w.be_u16(1);
    w.bytes(&[0u8; 16]);
    while w.len() < 100 {
        w.u8(0);
    }

    // --- Root directory name key + record (at fBEGIN = 100). ---
    let first_klen = key_len(DIR_CLASS, file_name, "");
    let name_title_len = (1 + file_name.len()) + 1;
    let f_nbytes_name = (first_klen as usize + name_title_len) as u32;
    let first_obj_len = name_title_len as u32 + DIR_RECORD_LEN;
    write_key_header(
        &mut w,
        DIR_CLASS,
        file_name,
        "",
        first_obj_len,
        first_obj_len,
        100,
        0,
    );
    w.string(file_name);
    w.string("");
    let (p_root_nbk, p_root_sk) = write_dir_record(&mut w, 100, 0, f_nbytes_name);

    // --- Root objects. ---
    let mut root_seeks = Vec::with_capacity(root_objects.len());
    for (i, o) in root_objects.iter().enumerate() {
        let s = w.len() as u64;
        write_key_header(
            &mut w,
            &o.class_name,
            &o.name,
            &o.title,
            o.object.len() as u32,
            root_pl[i].len() as u32,
            s,
            100,
        );
        w.bytes(&root_pl[i]);
        root_seeks.push(s);
    }

    // --- Streamer-info record (referenced only by fSeekInfo). ---
    let (seek_info, nbytes_info) = match (streamer_info, &streamer_pl) {
        (Some(object), Some(payload)) => {
            let s = w.len() as u64;
            write_key_header(
                &mut w,
                TLIST_CLASS,
                STREAMER_INFO_NAME,
                STREAMER_INFO_TITLE,
                object.len() as u32,
                payload.len() as u32,
                s,
                100,
            );
            w.bytes(payload);
            let klen = key_len(TLIST_CLASS, STREAMER_INFO_NAME, STREAMER_INFO_TITLE) as u32;
            (s as u32, klen + payload.len() as u32)
        }
        _ => (0, 0),
    };

    // --- Subdirectories: each = TDirectory key + record, its objects, its key list. ---
    let mut sub_seeks = Vec::with_capacity(subdirs.len());
    for (si, sub) in subdirs.iter().enumerate() {
        let sub_klen = key_len("TDirectory", &sub.name, &sub.name);
        let s_sub = w.len() as u64;
        write_key_header(
            &mut w,
            "TDirectory",
            &sub.name,
            &sub.name,
            DIR_RECORD_LEN,
            DIR_RECORD_LEN,
            s_sub,
            100,
        );
        let (p_sub_nbk, p_sub_sk) = write_dir_record(&mut w, s_sub, 100, sub_klen as u32);

        let mut obj_seeks = Vec::with_capacity(sub.objects.len());
        for (j, o) in sub.objects.iter().enumerate() {
            let s = w.len() as u64;
            write_key_header(
                &mut w,
                &o.class_name,
                &o.name,
                &o.title,
                o.object.len() as u32,
                sub_pl[si][j].len() as u32,
                s,
                s_sub,
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
        let (sub_kl_seek, sub_kl_nbytes) =
            write_key_list(&mut w, "TDirectory", &sub.name, &sub.name, s_sub, &entries);
        w.patch_be_u32(p_sub_nbk, sub_kl_nbytes);
        w.patch_be_u32(p_sub_sk, sub_kl_seek as u32);
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
            DIR_RECORD_LEN,
            DIR_RECORD_LEN,
            sub_seeks[si],
        ));
    }
    let (root_kl_seek, root_kl_nbytes) =
        write_key_list(&mut w, DIR_CLASS, file_name, "", 100, &entries);
    w.patch_be_u32(p_root_nbk, root_kl_nbytes);
    w.patch_be_u32(p_root_sk, root_kl_seek as u32);

    let f_end = w.len() as u32;
    w.patch_be_u32(p_end, f_end);
    w.patch_be_u32(p_seek_free, 0);
    w.patch_be_u32(p_nbytes_free, 0);
    w.patch_be_u32(p_nfree, 0);
    w.patch_be_u32(p_nbytes_name, f_nbytes_name);
    w.patch_be_u32(p_seek_info, seek_info);
    w.patch_be_u32(p_nbytes_info, nbytes_info);

    w.into_vec()
}
