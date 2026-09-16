//! A split `TClonesArray` of a `TObject`-derived `Particle { float px, py; int
//! pid }`. ROOT splits it into per-member jagged sub-branches just like a
//! `std::vector`; oxiroot reads the data members (`parts.px`/`py`/`pid`) and the
//! `TObject` housekeeping `parts.fUniqueID`. The `parts.fBits` member uses the
//! special `kBits` encoding and is reported as unsupported (it is not user
//! data). Cross-checked against ROOT C++ and uproot.

use std::path::PathBuf;

use oxiroot_io_core::RFile;
use oxiroot_tree::{BranchValues, TTree};

fn fixture(name: &str) -> RFile {
    RFile::open(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures")
            .join(name),
    )
    .expect("open fixture")
}

#[test]
fn reads_tclonesarray_subbranches() {
    let f = fixture("tree_clones.root");
    let t = TTree::open(&f, "T").expect("open tree");
    assert_eq!(t.num_entries(), 3);

    assert_eq!(
        t.read_branch(&f, "parts.px").unwrap(),
        BranchValues::VecF32(vec![vec![0.0], vec![0.0, 1.0], vec![0.0, 1.0, 2.0]])
    );
    assert_eq!(
        t.read_branch(&f, "parts.py").unwrap(),
        BranchValues::VecF32(vec![vec![0.5], vec![0.5, 1.5], vec![0.5, 1.5, 2.5]])
    );
    assert_eq!(
        t.read_branch(&f, "parts.pid").unwrap(),
        BranchValues::VecI32(vec![vec![0], vec![0, 10], vec![0, 10, 20]])
    );

    // The TObject `fBits` member is the only unsupported sub-branch.
    let unsupported: Vec<&str> = t.unsupported_branches().iter().map(|(n, _)| *n).collect();
    assert_eq!(unsupported, vec!["parts.fBits"]);
}
