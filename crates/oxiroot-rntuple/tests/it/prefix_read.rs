//! `NtupleReader::read_field_prefix(m)` must equal `read_field()` truncated to `m`,
//! for every field of every fixture and a spread of `m` (including 0, past the
//! end, and — for the multi-cluster fixture — values that fall mid-file).

use std::path::PathBuf;

use oxiroot_io_core::FileReader;
use oxiroot_rntuple::NtupleReader;

fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures")
            .join(name),
    )
    .expect("read fixture")
}

/// Every RNTuple fixture (all named `ntpl`): scalars, real precision, arrays,
/// std::set, nested vectors, records, a std::variant, a streamer blob, a
/// schema-extension file, and the multi-cluster vector file.
const FIXTURES: &[&str] = &[
    "rntuple_scalars_uncompressed.root",
    "rntuple_scalars_zstd.root",
    "rntuple_coltypes_uncompressed.root",
    "rntuple_realprec_uncompressed.root",
    "rntuple_stl_uncompressed.root",
    "rntuple_set_uncompressed.root",
    "rntuple_nested_uncompressed.root",
    "rntuple_user_uncompressed.root",
    "rntuple_variant_uncompressed.root",
    "rntuple_streamer.root",
    "rntuple_ext.root",
    "rntuple_multicluster_vec.root",
];

#[test]
fn prefix_read_equals_truncated_full_read() {
    for &fx in FIXTURES {
        let file = FileReader::from_bytes(fixture(fx)).expect("parse file");
        let ntpl = NtupleReader::open(&file, "ntpl").expect("open ntpl");
        let total = ntpl.num_entries() as usize;

        // 0, a few, mid-file (splits the multi-cluster file across clusters),
        // last, exactly all, and past the end.
        let ms = [
            0,
            1,
            2,
            total / 2,
            total.saturating_sub(1),
            total,
            total + 3,
        ];

        for field in ntpl.field_names() {
            let full = ntpl.read_field(&file, field).expect("full read");
            assert_eq!(full.len(), total, "{fx}: field {field:?} full length");

            for &m in &ms {
                let prefix = ntpl
                    .read_field_prefix(&file, field, m)
                    .expect("prefix read");
                let mut want = full.clone();
                want.truncate(m);

                assert_eq!(
                    prefix, want,
                    "{fx}: field {field:?} prefix({m}) != full.truncate({m})"
                );
                assert_eq!(
                    prefix.len(),
                    m.min(total),
                    "{fx}: field {field:?} prefix({m}) length"
                );
            }
        }
    }
}
