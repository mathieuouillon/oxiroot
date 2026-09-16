//! RNTuple files carry the streamer info ROOT writes: the `ROOT::RNTuple` anchor
//! class, plus each user class with plain-number members. The entries must be
//! the ones ROOT wrote for the same classes in `rntuple_user_uncompressed.root`.

use std::path::PathBuf;

use oxiroot_io_core::{Compression, RFile, StreamerRegistry};
use oxiroot_rntuple::{Column, Field, Ntuple, NtupleFile, RNTupleWriter};

fn root_written() -> StreamerRegistry {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/rntuple_user_uncompressed.root");
    RFile::open(path).unwrap().streamer_registry().unwrap()
}

fn hit(ids: Vec<i32>) -> Column {
    let energy = ids.iter().map(|&i| f64::from(i) + 0.5).collect();
    Column::Object {
        type_name: "Hit".into(),
        members: vec![
            ("id".into(), Column::I32(ids)),
            ("energy".into(), Column::F64(energy)),
        ],
    }
}

fn registry(bytes: Vec<u8>) -> StreamerRegistry {
    RFile::from_bytes(bytes)
        .unwrap()
        .streamer_registry()
        .unwrap()
}

#[test]
fn one_shot_file_describes_the_anchor_and_user_classes_like_root() {
    let fields = vec![
        Field::new("hit", hit(vec![0, 1])),
        // The same class again, inside a vector: described once.
        Field::new(
            "vhit",
            Column::Nested {
                offsets: vec![1, 3],
                items: Box::new(hit(vec![0, 1, 2])),
            },
        ),
        Field::new("x", Column::F32(vec![1.0, 2.0])),
    ];
    let bytes = Ntuple::new("ntpl", fields)
        .to_root_bytes("t.root", Compression::Zstd(5))
        .unwrap();
    let ours = registry(bytes);
    let root = root_written();
    assert_eq!(ours.class_names(), ["ROOT::RNTuple", "Hit"]);
    assert_eq!(ours.get("ROOT::RNTuple"), root.get("ROOT::RNTuple"));
    assert_eq!(ours.get("Hit"), root.get("Hit"));
}

#[test]
fn streamed_file_describes_the_anchor_and_user_classes() {
    let path = std::env::temp_dir().join("oxiroot_rntuple_stream_streamers.root");
    let mut w = RNTupleWriter::create(&path, "ntpl", Compression::None).unwrap();
    for k in 0..2 {
        w.write_batch(&[Field::new("hit", hit(vec![k, k + 1]))])
            .unwrap();
    }
    w.finish().unwrap();
    let ours = RFile::open(&path).unwrap().streamer_registry().unwrap();
    let root = root_written();
    assert_eq!(ours.class_names(), ["ROOT::RNTuple", "Hit"]);
    assert_eq!(ours.get("Hit"), root.get("Hit"));
}

#[test]
fn classes_with_other_members_are_not_described() {
    // A string member has no plain-number streamer element here, so the class is
    // left to the reader's dictionary; the anchor class is still described.
    let tagged = Column::Object {
        type_name: "Tagged".into(),
        members: vec![
            ("id".into(), Column::I32(vec![1])),
            ("label".into(), Column::Str(vec!["a".into()])),
        ],
    };
    let bytes = NtupleFile::new()
        .add(Ntuple::new("a", vec![Field::new("t", tagged)]))
        .dir("d", |d| {
            d.add(Ntuple::new("b", vec![Field::new("hit", hit(vec![3]))]))
        })
        .to_root_bytes("t.root", Compression::None)
        .unwrap();
    assert_eq!(registry(bytes).class_names(), ["ROOT::RNTuple", "Hit"]);
}

#[test]
fn schema_extended_files_are_described_too() {
    let path = std::env::temp_dir().join("oxiroot_rntuple_ext_streamers.root");
    Ntuple::new("ext", vec![Field::new("hit", hit(vec![1, 2]))])
        .write_root_extended(
            &path,
            &[(1, Field::new("y", Column::F64(vec![0.5])))],
            Compression::None,
        )
        .unwrap();
    let ours = RFile::open(&path).unwrap().streamer_registry().unwrap();
    assert_eq!(ours.class_names(), ["ROOT::RNTuple", "Hit"]);
}
