//! `FileReader::open_ranged` (positioned local reads, no full slurp) must parse and
//! read byte-for-byte identically to the resident `FileReader::open`.

use std::path::PathBuf;

use oxiroot_io_core::FileReader;

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures")
        .join(name)
}

#[test]
fn open_ranged_matches_open() {
    for name in ["th1d_uncompressed.root", "th1d_zstd.root"] {
        let path = fixture(name);
        let resident = FileReader::open(&path).expect("open");
        let ranged = FileReader::open_ranged(&path).expect("open_ranged");

        assert_eq!(ranged.size(), resident.size(), "{name}: same length");

        let names =
            |f: &FileReader| -> Vec<String> { f.keys().iter().map(|k| k.name.clone()).collect() };
        assert_eq!(names(&ranged), names(&resident), "{name}: same keys");

        // Every key's payload bytes must match, fetched positionally vs sliced.
        for k in resident.keys() {
            assert_eq!(
                ranged.key_payload(k).unwrap(),
                resident.key_payload(k).unwrap(),
                "{name}: key {:?} payload",
                k.name
            );
        }

        // The whole file, read range-by-range, equals the resident buffer.
        let whole = |f: &FileReader| f.read_at(0, f.size() as usize).unwrap();
        assert_eq!(whole(&ranged), whole(&resident), "{name}: whole-file bytes");
    }
}
