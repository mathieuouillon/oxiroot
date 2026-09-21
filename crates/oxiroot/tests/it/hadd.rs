//! End-to-end `hadd`-style file merging via [`oxiroot::hadd`].
use oxiroot::hadd::{merge_files, MergeKind, Merger};
use oxiroot::prelude::*;

fn tmp(name: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("oxiroot_hadd_{name}.root"))
}

#[test]
fn sums_histograms_and_copies_other_objects() {
    // Two files, each a TH1 "h" (2 in-range fills) plus a TObjString "meta".
    for (tag, xs) in [("hist_a", [0.5, 1.5]), ("hist_b", [2.5, 3.5])] {
        let mut h = Hist::reg(4, 0.0, 4.0).double().named("h").titled("h");
        for x in xs {
            h.fill(x);
        }
        FileWriter::create(tmp(tag))
            .add(&h)
            .add(&TObjString::new("provenance").named("meta"))
            .write(Compression::None)
            .unwrap();
    }

    let out = tmp("hist_out");
    let report = merge_files(&out, &[tmp("hist_a"), tmp("hist_b")], Compression::None).unwrap();

    assert_eq!(report.kind, MergeKind::Histograms);
    assert!(report.merged.contains(&"h".to_string()), "{report}");
    assert!(report.copied.contains(&"meta".to_string()), "{report}");
    assert!(report.skipped.is_empty(), "{report}");

    // The merged histogram is the bin-by-bin sum: 4 in-range entries total.
    let fo = FileReader::open(&out).unwrap();
    let h = TH1::read_root(&fo, "h").unwrap();
    assert_eq!(h.integral(), 4.0);
    // The non-summable object was carried over verbatim.
    assert!(TObjString::read_root(&fo, "meta").is_ok());
}

#[test]
fn concatenates_tree_files() {
    Tree::new("Events", vec![Branch::i32("i", vec![0, 1])])
        .write_root(tmp("tree_a"), Compression::None)
        .unwrap();
    Tree::new("Events", vec![Branch::i32("i", vec![2, 3, 4])])
        .write_root(tmp("tree_b"), Compression::None)
        .unwrap();

    let out = tmp("tree_out");
    let report = Merger::new()
        .input(tmp("tree_a"))
        .input(tmp("tree_b"))
        .compression(Compression::None)
        .merge(&out)
        .unwrap();

    assert_eq!(report.kind, MergeKind::Tree("Events".into()));
    assert_eq!(report.entries, Some(5));

    let fo = FileReader::open(&out).unwrap();
    let t = TreeReader::open(&fo, "Events").unwrap();
    assert_eq!(t.num_entries(), 5);
    assert_eq!(
        t.read_branch(&fo, "i").unwrap(),
        BranchValues::I32(vec![0, 1, 2, 3, 4])
    );
}

#[test]
fn concatenates_rntuple_files() {
    Ntuple::new("ntpl", vec![Field::f64("x", vec![0.0, 1.0])])
        .write_root(tmp("rn_a"), Compression::None)
        .unwrap();
    Ntuple::new("ntpl", vec![Field::f64("x", vec![2.0, 3.0, 4.0])])
        .write_root(tmp("rn_b"), Compression::None)
        .unwrap();

    let out = tmp("rn_out");
    let report = merge_files(&out, &[tmp("rn_a"), tmp("rn_b")], Compression::None).unwrap();

    assert_eq!(report.kind, MergeKind::Ntuple("ntpl".into()));
    assert_eq!(report.entries, Some(5));

    let fo = FileReader::open(&out).unwrap();
    let nt = NtupleReader::open(&fo, "ntpl").unwrap();
    assert_eq!(nt.num_entries(), 5);
    assert_eq!(
        nt.read_field(&fo, "x").unwrap(),
        FieldValues::F64(vec![0.0, 1.0, 2.0, 3.0, 4.0])
    );
}

#[test]
fn refuses_a_fileset_mixing_a_tree_with_histograms() {
    Tree::new("Events", vec![Branch::i32("i", vec![0, 1])])
        .write_root(tmp("mix_tree"), Compression::None)
        .unwrap();
    let mut h = Hist::reg(4, 0.0, 4.0).double().named("h");
    h.fill(1.5);
    h.write_root(tmp("mix_hist"), Compression::None).unwrap();

    let out = tmp("mix_out");
    let Err(err) = merge_files(&out, &[tmp("mix_tree"), tmp("mix_hist")], Compression::None) else {
        panic!("expected a mixed-fileset error");
    };
    assert!(err.to_string().contains("cannot combine"), "{err}");
}

#[test]
fn a_tree_merge_streams_one_batch_per_input() {
    let inputs: Vec<_> = (0..3)
        .map(|k| {
            let path = tmp(&format!("stream_tree_{k}"));
            let n = k + 1;
            Tree::new(
                "Events",
                vec![
                    Branch::i32("i", (0..n).map(|x| k * 10 + x).collect()),
                    Branch::jagged_f32(
                        "hits",
                        (0..n).map(|x| vec![x as f32; x as usize]).collect(),
                    ),
                    Branch::strings("tag", (0..n).map(|x| format!("t{k}{x}")).collect()),
                    Branch::vector_f64("vec", (0..n).map(|x| vec![f64::from(x); 2]).collect()),
                ],
            )
            .write_root(&path, Compression::Zstd(3))
            .unwrap();
            path
        })
        .collect();

    let out = tmp("stream_tree_out");
    let report = merge_files(&out, &inputs, Compression::Zstd(3)).unwrap();
    assert_eq!(report.entries, Some(6));

    let fo = FileReader::open(&out).unwrap();
    let t = TreeReader::open(&fo, "Events").unwrap();
    assert_eq!(
        t.read_branch(&fo, "i").unwrap(),
        BranchValues::I32(vec![0, 10, 11, 20, 21, 22])
    );
    assert_eq!(
        t.read_branch(&fo, "hits").unwrap(),
        BranchValues::VecF32(vec![
            vec![],
            vec![],
            vec![1.0],
            vec![],
            vec![1.0],
            vec![2.0, 2.0],
        ])
    );
    assert_eq!(
        t.read_branch(&fo, "tag").unwrap(),
        BranchValues::Str(
            ["t00", "t10", "t11", "t20", "t21", "t22"]
                .map(String::from)
                .to_vec()
        )
    );
    assert_eq!(
        t.read_branch(&fo, "vec").unwrap(),
        BranchValues::VecF64(vec![
            vec![0.0; 2],
            vec![0.0; 2],
            vec![1.0; 2],
            vec![0.0; 2],
            vec![1.0; 2],
            vec![2.0; 2],
        ])
    );
}

#[test]
fn an_rntuple_merge_streams_one_cluster_per_input() {
    let inputs: Vec<_> = (0..3)
        .map(|k| {
            let path = tmp(&format!("stream_rn_{k}"));
            Ntuple::new(
                "ntpl",
                vec![
                    Field::i64("id", vec![k, k + 100]),
                    Field::strings("s", vec![format!("a{k}"), format!("b{k}")]),
                ],
            )
            .write_root(&path, Compression::None)
            .unwrap();
            path
        })
        .collect();

    let out = tmp("stream_rn_out");
    merge_files(&out, &inputs, Compression::Zstd(1)).unwrap();

    let fo = FileReader::open(&out).unwrap();
    let nt = NtupleReader::open(&fo, "ntpl").unwrap();
    assert_eq!(nt.footer().cluster_groups[0].num_clusters, 3);
    assert_eq!(
        nt.read_field(&fo, "id").unwrap(),
        FieldValues::I64(vec![0, 100, 1, 101, 2, 102])
    );
    assert_eq!(
        nt.read_field(&fo, "s").unwrap(),
        FieldValues::Str(
            ["a0", "b0", "a1", "b1", "a2", "b2"]
                .map(String::from)
                .to_vec()
        )
    );
}

#[test]
fn empty_inputs_merge_to_an_empty_tree() {
    for k in 0..2 {
        Tree::new("T", vec![Branch::f64("x", vec![])])
            .write_root(tmp(&format!("empty_tree_{k}")), Compression::None)
            .unwrap();
    }
    let out = tmp("empty_tree_out");
    let report = merge_files(
        &out,
        &[tmp("empty_tree_0"), tmp("empty_tree_1")],
        Compression::None,
    )
    .unwrap();
    assert_eq!(report.entries, Some(0));
    let fo = FileReader::open(&out).unwrap();
    assert_eq!(TreeReader::open(&fo, "T").unwrap().num_entries(), 0);
}

#[test]
fn the_output_cannot_be_an_input() {
    let path = tmp("self_merge");
    Tree::new("T", vec![Branch::i32("x", vec![1])])
        .write_root(&path, Compression::None)
        .unwrap();
    let err = merge_files(&path, &[&path], Compression::None).unwrap_err();
    assert!(err.to_string().contains("also an input"), "{err}");
    // The input is untouched.
    let f = FileReader::open(&path).unwrap();
    assert_eq!(TreeReader::open(&f, "T").unwrap().num_entries(), 1);
}
