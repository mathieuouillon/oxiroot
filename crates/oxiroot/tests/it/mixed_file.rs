//! Histograms, a `TTree` and an RNTuple in one file, through one `FileWriter`.

use oxiroot::prelude::*;
use oxiroot::Error;

fn hist() -> TH1 {
    let mut h = Hist::reg(4, 0.0, 4.0).double().named("pt");
    h.fill(0.5);
    h.fill(2.5);
    h
}

#[test]
fn histograms_trees_and_rntuples_share_a_file() {
    let path = std::env::temp_dir().join("oxiroot_mixed_all.root");
    FileWriter::create(&path)
        .add(&hist())
        .put(Tree::new(
            "Events",
            vec![
                Branch::f64("energy", vec![10.5, 20.1, 5.0]),
                Branch::jagged_i32("hits", vec![vec![1], vec![], vec![2, 3]]),
            ],
        ))
        .put(Ntuple::new("ntuple", vec![Field::i32("x", vec![1, 2])]))
        .dir("cal", |d| {
            d.add(&TParameter::f64("gain", 1.5))
                .put(Tree::new("Pedestals", vec![Branch::i32("adc", vec![3, 4])]))
        })
        .write(Compression::Zstd(5))
        .unwrap();

    let f = FileReader::open(&path).unwrap();
    assert_eq!(TH1::read_root(&f, "pt").unwrap(), hist());
    let tree = TreeReader::open(&f, "Events").unwrap();
    assert_eq!(
        tree.read_branch(&f, "energy").unwrap(),
        BranchValues::F64(vec![10.5, 20.1, 5.0])
    );
    assert_eq!(
        tree.read_branch(&f, "hits").unwrap(),
        BranchValues::VecI32(vec![vec![1], vec![], vec![2, 3]])
    );
    assert_eq!(
        NtupleReader::open(&f, "ntuple")
            .unwrap()
            .read_field(&f, "x")
            .unwrap(),
        FieldValues::I32(vec![1, 2])
    );
    assert_eq!(
        TParameter::read_root_in(&f, "cal", "gain")
            .unwrap()
            .value()
            .as_f64(),
        1.5
    );
    let ped = TreeReader::open_in(&f, "cal", "Pedestals").unwrap();
    assert_eq!(
        ped.read_branch(&f, "adc").unwrap(),
        BranchValues::I32(vec![3, 4])
    );

    // One streamer-info record describes all of it.
    let registry = f.streamer_registry().unwrap();
    for class in [
        "TH1D",
        "TTree",
        "TBranch",
        "ROOT::RNTuple",
        "TParameter<double>",
    ] {
        assert!(registry.get(class).is_some(), "{class} is described");
    }
}

#[test]
fn a_tree_and_a_histogram_cannot_share_a_name() {
    let err = FileWriter::create("clash.root")
        .add(&hist())
        .put(Tree::new("pt", vec![Branch::i32("x", vec![1])]))
        .to_bytes(Compression::None);
    assert!(matches!(err, Err(Error::DuplicateName { .. })), "{err:?}");
}

#[test]
fn a_tree_can_be_appended_to_a_histogram_file() {
    let path = std::env::temp_dir().join("oxiroot_mixed_append_tree.root");
    hist().write_root(&path, Compression::None).unwrap();
    FileWriter::open(&path)
        .unwrap()
        .put(Tree::new("T", vec![Branch::u8("flag", vec![1, 0])]))
        .write(Compression::None)
        .unwrap();
    let f = FileReader::open(&path).unwrap();
    assert_eq!(TH1::read_root(&f, "pt").unwrap(), hist());
    assert_eq!(
        TreeReader::open(&f, "T")
            .unwrap()
            .read_branch(&f, "flag")
            .unwrap(),
        BranchValues::U8(vec![1, 0])
    );
}

/// A `TTree` is more than its key — its baskets sit elsewhere in the file — so
/// compacting a file that holds one is refused, with the alternative named,
/// rather than writing a file whose tree has lost its data.
#[test]
fn compacting_a_file_that_holds_a_tree_is_refused() {
    let path = std::env::temp_dir().join("oxiroot_compact_with_tree.root");
    FileWriter::create(&path)
        .put(Tree::new("T", vec![Branch::i32("x", vec![1, 2, 3])]))
        .write(Compression::None)
        .expect("write");

    let err = FileWriter::open(&path)
        .expect("open")
        .compact()
        .write(Compression::None)
        .expect_err("compacting a tree file must be refused");
    let message = err.to_string();
    assert!(
        message.contains("TTree") && message.contains("hadd"),
        "{err}"
    );

    // The file is untouched: the tree still reads.
    let file = FileReader::open(&path).expect("reopen");
    let tree = TreeReader::open(&file, "T").expect("tree");
    assert_eq!(tree.num_entries(), 3);
}
