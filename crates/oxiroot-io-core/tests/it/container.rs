//! `ContainerWriter`: the TFile layout every oxiroot writer goes through. Each
//! file is read back with `FileReader`, in both the 32-bit and 64-bit forms.

use std::io::Cursor;

use oxiroot_io_core::streamer_gen::{basic, streamer_info_list, Cls};
use oxiroot_io_core::{compress_if_smaller, Compression, ContainerWriter, DirId, FileReader, Key};

/// Build a file in the small form (`big = false`) or the big form.
fn build(
    big: bool,
    layout: impl FnMut(&mut ContainerWriter<Cursor<Vec<u8>>>) -> oxiroot_io_core::Result<()>,
) -> FileReader {
    let threshold = if big { 0 } else { u64::MAX };
    let bytes =
        ContainerWriter::build("t.root", Compression::None, threshold, layout).expect("build");
    let f = FileReader::from_bytes(bytes).expect("parse");
    assert_eq!(f.header().is_big(), big);
    f
}

/// The payload stored under `name` in `dir` ("" for the top directory).
fn payload(f: &FileReader, dir: &str, name: &str) -> Vec<u8> {
    if dir.is_empty() {
        let key = f.key(name).expect("key");
        f.key_payload(key).expect("payload").to_vec()
    } else {
        f.object_in(dir, name).expect("object").1
    }
}

#[test]
fn nested_directories_round_trip_in_both_forms() {
    for big in [false, true] {
        let f = build(big, |c| {
            c.place_key(DirId::TOP, "TObjString", "top", "t", b"top-bytes")?;
            let a = c.mkdir(DirId::TOP, "a")?;
            c.place_key(a, "TObjString", "in_a", "", b"a-bytes")?;
            let b = c.mkdir(a, "b")?;
            c.place_key(b, "TObjString", "in_b", "", b"b-bytes")?;
            // `b` is left open: finish closes it before its parent.
            c.close_dir(a)?;
            let empty = c.mkdir(DirId::TOP, "empty")?;
            c.close_dir(empty)
        });
        let names: Vec<&str> = f.keys().iter().map(|k| k.name.as_str()).collect();
        assert_eq!(names, ["top", "a", "empty"], "big={big}");
        assert_eq!(f.key("top").unwrap().title, "t");
        assert_eq!(payload(&f, "", "top"), b"top-bytes");
        assert_eq!(payload(&f, "a", "in_a"), b"a-bytes");
        assert_eq!(payload(&f, "a/b", "in_b"), b"b-bytes");
        assert!(f.subdir("empty").unwrap().keys.is_empty());

        let a = f.subdir("a").unwrap();
        assert_eq!(a.keys.len(), 2, "in_a and the subdirectory b");
        assert_eq!(a.version > 1000, big, "subdirectory record form");
        // Keys point back at the directory that lists them.
        let b = f.subdir("a/b").unwrap();
        assert_eq!(b.keys[0].seek_pdir, b.seek_dir);
        assert_eq!(b.seek_parent, a.seek_dir);
    }
}

#[test]
fn a_repeated_name_gets_the_next_cycle() {
    let f = build(false, |c| {
        c.place_key(DirId::TOP, "TObjString", "x", "", b"first")?;
        c.place_key(DirId::TOP, "TObjString", "y", "", b"other")?;
        c.place_key(DirId::TOP, "TObjString", "x", "", b"second")
            .map(drop)
    });
    let cycles: Vec<(&str, u16)> = f
        .keys()
        .iter()
        .map(|k| (k.name.as_str(), k.cycle))
        .collect();
    assert_eq!(cycles, [("x", 1), ("y", 1), ("x", 2)]);
    assert_eq!(payload(&f, "", "x"), b"second", "the highest cycle wins");
}

#[test]
fn long_names_and_titles_get_the_long_string_form() {
    // ROOT strings of 255+ bytes use a five-byte length prefix; the key length
    // must count it, or readers look for the payload four bytes early.
    let name = "n".repeat(300);
    let title = "t".repeat(1000);
    let dir = "d".repeat(260);
    for big in [false, true] {
        let f = build(big, |c| {
            c.place_key(DirId::TOP, "TObjString", &name, &title, b"payload")?;
            let d = c.mkdir(DirId::TOP, &dir)?;
            c.place_key(d, "TObjString", &name, "", b"inner")?;
            c.close_dir(d)
        });
        let key = f.key(&name).expect("long key");
        assert_eq!(key.title, title);
        assert_eq!(payload(&f, "", &name), b"payload");
        assert_eq!(payload(&f, &dir, &name), b"inner");
    }
}

#[test]
fn oversized_key_strings_are_rejected() {
    let huge = "x".repeat(70_000);
    let err = ContainerWriter::build("t.root", Compression::None, u64::MAX, |c| {
        c.place_key(DirId::TOP, "TObjString", &huge, "", b"")
            .map(drop)
    });
    assert!(
        err.is_err(),
        "a key header over 65535 bytes cannot be written"
    );
}

#[test]
fn blobs_land_at_the_returned_offset() {
    let mut offsets = Vec::new();
    let bytes = ContainerWriter::build("t.root", Compression::None, u64::MAX, |c| {
        offsets.clear();
        offsets.push(c.place_blob(b"first blob")?);
        c.place_key(DirId::TOP, "TObjString", "k", "", b"between")?;
        offsets.push(c.position());
        offsets.push(c.place_blob(b"second")?);
        Ok(())
    })
    .unwrap();
    assert_eq!(
        offsets[1], offsets[2],
        "position() is where the next write goes"
    );
    assert_eq!(&bytes[offsets[0] as usize..][..10], b"first blob");
    assert_eq!(&bytes[offsets[2] as usize..][..6], b"second");
    let f = FileReader::from_bytes(bytes).unwrap();
    assert_eq!(f.keys().len(), 1, "blobs are not directory entries");
}

#[test]
fn threshold_picks_the_form_from_the_finished_size() {
    let layout = |c: &mut ContainerWriter<Cursor<Vec<u8>>>| {
        c.place_key(DirId::TOP, "TObjString", "k", "", &[7u8; 500])
            .map(drop)
    };
    let small = ContainerWriter::build("t.root", Compression::None, u64::MAX, layout).unwrap();
    assert!(!FileReader::from_bytes(small.clone())
        .unwrap()
        .header()
        .is_big());
    let at_size =
        ContainerWriter::build("t.root", Compression::None, small.len() as u64, layout).unwrap();
    assert_eq!(
        at_size, small,
        "a file exactly at the threshold stays small"
    );
    let over = ContainerWriter::build("t.root", Compression::None, small.len() as u64 - 1, layout)
        .unwrap();
    assert!(FileReader::from_bytes(over).unwrap().header().is_big());
}

/// A one-member class, for streamer-info lists.
fn class(name: &str) -> Cls<'_> {
    Cls {
        name: name.into(),
        version: 1,
        checksum: 7,
        elements: vec![basic("x", 3, 4, "int")],
    }
}

/// The class names in a file's streamer info.
fn described(f: &FileReader) -> Vec<String> {
    let registry = f.streamer_registry().unwrap();
    registry
        .class_names()
        .into_iter()
        .map(String::from)
        .collect()
}

/// The `KeyLen` of a file's streamer-info record.
fn streamer_key_len(f: &FileReader) -> u16 {
    let h = f.header();
    let record = f.read_at(h.seek_info, h.nbytes_info as usize).unwrap();
    Key::read(&mut oxiroot_io_core::RBuffer::new(&record))
        .unwrap()
        .key_len
}

#[test]
fn streamer_info_is_referenced_from_the_header() {
    for big in [false, true] {
        let f = build(big, |c| {
            c.place_key(DirId::TOP, "TObjString", "k", "", b"x")?;
            c.place_streamer_info(&streamer_info_list(&[class("A")]), &[class("B")])
        });
        assert_eq!(described(&f), ["A", "B"], "extra classes follow the list");
        assert_eq!(f.keys().len(), 1, "streamer info is not a directory entry");
        // Lists captured from ROOT refer back into themselves by offset within
        // the key, so the key keeps ROOT's small-form length in both forms.
        assert_eq!(streamer_key_len(&f), 64, "big={big}");
    }
}

#[test]
fn streamer_info_is_compressed_with_the_file() {
    let bytes = ContainerWriter::build("t.root", Compression::Zstd(5), u64::MAX, |c| {
        c.place_streamer_info(&[0u8; 400], &[])
    })
    .unwrap();
    let f = FileReader::from_bytes(bytes).unwrap();
    assert_eq!(f.header().compress, 505);
    assert!(f.header().nbytes_info < 400);
    assert_eq!(f.streamer_info_object().unwrap().unwrap(), vec![0u8; 400]);
}

#[test]
fn streaming_to_a_sink_matches_the_in_memory_build() {
    let layout = |c: &mut ContainerWriter<Cursor<Vec<u8>>>| {
        c.place_key(DirId::TOP, "TObjString", "k", "", b"payload")?;
        let d = c.mkdir(DirId::TOP, "d")?;
        c.place_key(d, "TObjString", "k2", "", b"payload2")
            .map(drop)
    };
    for big in [false, true] {
        let threshold = if big { 0 } else { u64::MAX };
        let built = ContainerWriter::build("t.root", Compression::None, threshold, layout).unwrap();
        let mut c = ContainerWriter::new(
            Cursor::<Vec<u8>>::default(),
            "t.root",
            Compression::None,
            big,
        )
        .unwrap();
        layout(&mut c).unwrap();
        let streamed = c.finish().unwrap().into_inner();
        assert_eq!(streamed, built, "big={big}");
    }
}

#[test]
fn misuse_is_an_error() {
    ContainerWriter::build("t.root", Compression::None, u64::MAX, |c| {
        let d = c.mkdir(DirId::TOP, "d")?;
        c.close_dir(d)?;
        assert!(c.place_key(d, "TObjString", "late", "", b"").is_err());
        assert!(c.close_dir(d).is_err(), "closing twice");
        assert!(c.mkdir(d, "child").is_err(), "a closed parent");
        assert!(c.close_dir(DirId::TOP).is_err(), "finish closes the top");
        Ok(())
    })
    .unwrap();

    // A directory handle from another file.
    let mut other = ContainerWriter::new(
        Cursor::<Vec<u8>>::default(),
        "o.root",
        Compression::None,
        false,
    )
    .unwrap();
    let foreign = other.mkdir(DirId::TOP, "x").unwrap();
    let mut c = ContainerWriter::new(
        Cursor::<Vec<u8>>::default(),
        "t.root",
        Compression::None,
        false,
    )
    .unwrap();
    assert!(c.place_key(foreign, "TObjString", "k", "", b"").is_err());
}

/// A small file with two objects and a subdirectory, to append to.
fn existing_file() -> Vec<u8> {
    existing_file_with(&streamer_info_list(&[class("A")]))
}

fn existing_file_with(streamer_info: &[u8]) -> Vec<u8> {
    ContainerWriter::build("orig.root", Compression::None, u64::MAX, |c| {
        c.place_key(DirId::TOP, "TObjString", "a", "", b"alpha")?;
        c.place_key(DirId::TOP, "TObjString", "b", "", b"beta")?;
        let d = c.mkdir(DirId::TOP, "sub")?;
        c.place_key(d, "TObjString", "inner", "", b"gamma")?;
        c.close_dir(d)?;
        c.place_streamer_info(streamer_info, &[])
    })
    .unwrap()
}

#[test]
fn append_keeps_existing_bytes_and_adds_keys() {
    let existing = existing_file();
    for big in [false, true] {
        let threshold = if big { 0 } else { u64::MAX };
        // A different (longer) file name than the one stored: the top directory's
        // reserved record is measured from the stored name.
        let bytes = ContainerWriter::build_append(
            &existing,
            "a-much-longer-name-than-before.root",
            Compression::None,
            threshold,
            |c| {
                c.place_key(DirId::TOP, "TObjString", "a", "", b"alpha v2")?;
                c.place_key(DirId::TOP, "TObjString", "c", "", b"new")?;
                // The file's own list is kept; only the class it lacks is added.
                c.place_streamer_info(
                    &streamer_info_list(&[class("Z")]),
                    &[class("A"), class("B")],
                )
            },
        )
        .unwrap();
        let end = FileReader::from_bytes(existing.clone())
            .unwrap()
            .header()
            .end as usize;
        if !big {
            // Everything but the patched header and top record is untouched.
            let record = 100
                + FileReader::from_bytes(existing.clone())
                    .unwrap()
                    .header()
                    .nbytes_name as usize;
            assert_eq!(bytes[record + 60..end], existing[record + 60..end]);
        }
        let f = FileReader::from_bytes(bytes).unwrap();
        assert_eq!(f.header().is_big(), big);
        let keys: Vec<(&str, u16)> = f
            .keys()
            .iter()
            .map(|k| (k.name.as_str(), k.cycle))
            .collect();
        assert_eq!(keys, [("a", 1), ("b", 1), ("sub", 1), ("a", 2), ("c", 1)]);
        assert_eq!(payload(&f, "", "a"), b"alpha v2");
        assert_eq!(payload(&f, "", "b"), b"beta");
        assert_eq!(payload(&f, "sub", "inner"), b"gamma");
        assert_eq!(described(&f), ["A", "B"]);
        assert_eq!(streamer_key_len(&f), 64);
    }
}

#[test]
fn append_keeps_complete_or_unreadable_streamer_info_as_is() {
    // Nothing missing: the original record stays in place.
    let existing = existing_file();
    let before = FileReader::from_bytes(existing.clone())
        .unwrap()
        .header()
        .seek_info;
    let bytes =
        ContainerWriter::build_append(&existing, "orig.root", Compression::None, u64::MAX, |c| {
            c.place_streamer_info(&[], &[class("A")])
        })
        .unwrap();
    assert_eq!(
        FileReader::from_bytes(bytes).unwrap().header().seek_info,
        before
    );

    // A record that does not parse is kept rather than extended.
    let existing = existing_file_with(b"not a streamer info list");
    let bytes =
        ContainerWriter::build_append(&existing, "orig.root", Compression::None, u64::MAX, |c| {
            c.place_streamer_info(&[], &[class("B")])
        })
        .unwrap();
    let f = FileReader::from_bytes(bytes).unwrap();
    assert_eq!(
        f.streamer_info_object().unwrap().unwrap(),
        b"not a streamer info list"
    );
}

#[test]
fn a_big_file_stays_big_when_appended() {
    let big = ContainerWriter::build("b.root", Compression::None, 0, |c| {
        c.place_key(DirId::TOP, "TObjString", "a", "", b"alpha")
            .map(drop)
    })
    .unwrap();
    let appended =
        ContainerWriter::build_append(&big, "b.root", Compression::None, u64::MAX, |c| {
            c.place_key(DirId::TOP, "TObjString", "b", "", b"beta")
                .map(drop)
        })
        .unwrap();
    let f = FileReader::from_bytes(appended).unwrap();
    assert!(f.header().is_big());
    assert_eq!(payload(&f, "", "a"), b"alpha");
    assert_eq!(payload(&f, "", "b"), b"beta");

    // Continuing it in the small form is refused.
    let file = FileReader::from_bytes(big.clone()).unwrap();
    let mut sink = Cursor::new(big);
    sink.set_position(file.header().end);
    assert!(ContainerWriter::append(sink, &file, "b.root", Compression::None, false).is_err());
}

#[test]
fn append_rejects_a_truncated_file() {
    let existing = existing_file();
    let err = ContainerWriter::build_append(
        &existing[..existing.len() - 1],
        "orig.root",
        Compression::None,
        u64::MAX,
        |_| Ok(()),
    );
    assert!(err.is_err());
}

#[test]
fn compress_if_smaller_keeps_incompressible_bytes() {
    let noise: Vec<u8> = (0..64u32)
        .map(|i| (i.wrapping_mul(2_654_435_761) >> 24) as u8)
        .collect();
    assert!(matches!(
        compress_if_smaller(&noise, Compression::Zstd(5).setting()),
        std::borrow::Cow::Borrowed(_)
    ));
    assert!(matches!(
        compress_if_smaller(&[0u8; 1000], 0),
        std::borrow::Cow::Borrowed(_)
    ));
    let zeros = compress_if_smaller(&[0u8; 1000], Compression::Zlib(1).setting());
    assert!(zeros.len() < 1000);
    assert_eq!(
        oxiroot_compress::decompress(&zeros, 1000).unwrap(),
        vec![0u8; 1000]
    );
}
