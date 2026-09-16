//! Reading an RNTuple *streamer* field (the `kStreamer` structural role): a
//! class stored unsplit as one serialized blob per entry, rather than split into
//! a record of member columns. oxiroot interprets each blob with the class
//! `TStreamerInfo` carried in the file and surfaces it as a struct-of-arrays
//! record. Grounded against ROOT C++ only — uproot does not support unsplit
//! fields (`NotImplementedError`), so it can't read this file.

use std::path::PathBuf;

use oxiroot_io_core::RFile;
use oxiroot_rntuple::{FieldValues, RNTuple, StructRole};

fn open(name: &str) -> RFile {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures")
        .join(name);
    RFile::open(path).expect("open fixture")
}

#[test]
fn reads_streamer_field() {
    let file = open("rntuple_streamer.root");
    let ntpl = RNTuple::open(&file, "ntpl").expect("open ntpl");
    assert_eq!(ntpl.num_entries(), 3);

    // The field is the kStreamer role, typed with the class name.
    let blob = &ntpl.header().fields[0];
    assert_eq!(blob.struct_role, StructRole::Streamer);
    assert_eq!(blob.type_name, "Blob");

    // Each blob `Blob { int32 id; double value; std::string tag }` is decoded
    // member by member from the class TStreamerInfo.
    assert_eq!(
        ntpl.read_field(&file, "blob").unwrap(),
        FieldValues::Record(vec![
            ("id".into(), FieldValues::I32(vec![7, 42, -1])),
            ("value".into(), FieldValues::F64(vec![3.25, -1.5, 0.0])),
            (
                "tag".into(),
                FieldValues::Str(vec!["hello".into(), "world!!".into(), "".into()]),
            ),
        ])
    );
}
