//! Append mode for a file that contains an RNTuple. The RNTuple's anchor and
//! page locators hold absolute file offsets, so append-in-place must not move
//! it; only new keys are written after the existing content. Both the RNTuple
//! and the appended histogram read back (separately verified against ROOT C++
//! and uproot).

use oxiroot::prelude::*;
use oxiroot::RFile;

#[test]
fn appends_a_histogram_to_an_rntuple_file() {
    let out = std::env::temp_dir().join("oxiroot_append_rntuple.root");

    let fields = vec![
        Field::f64("mass", vec![91.2, 125.0, 80.4]),
        Field::i32("n", vec![1, 2, 3]),
    ];
    Ntuple::new("events", fields)
        .write_root(&out, Compression::None)
        .expect("write rntuple");

    let mut h = TH1::new(4, 0.0, 4.0).named("h").titled("appended");
    h.fill(0.5);
    h.fill(2.5);
    RootFile::open(&out)
        .expect("open for append")
        .add(&h)
        .write(Compression::None)
        .expect("append");

    let f = RFile::open(&out).expect("reopen");
    let names: Vec<&str> = f.keys().iter().map(|k| k.name.as_str()).collect();
    assert!(
        names.contains(&"events") && names.contains(&"h"),
        "{names:?}"
    );

    // The RNTuple still reads — its anchor/page offsets were not relocated.
    let nt = RNTuple::open(&f, "events").expect("rntuple survived the append");
    assert_eq!(nt.num_entries(), 3);
    assert_eq!(
        nt.read_field(&f, "mass").unwrap(),
        FieldValues::F64(vec![91.2, 125.0, 80.4])
    );
    assert_eq!(
        nt.read_field(&f, "n").unwrap(),
        FieldValues::I32(vec![1, 2, 3])
    );
    assert_eq!(TH1::read_root(&f, "h").unwrap(), h, "appended histogram");

    let _ = std::fs::remove_file(&out);
}
