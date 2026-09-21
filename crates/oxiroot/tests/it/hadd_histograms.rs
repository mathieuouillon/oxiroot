//! Summing histogram files with [`merge_histogram_files`]: 2-D and 3-D profiles
//! add up to one combined fill, and a key that cannot be merged is skipped or
//! named rather than written as a partial sum.

use oxiroot::hadd::merge_histogram_files;
use oxiroot::prelude::*;
use oxiroot::Error;

// --------------------------------------------------------------------- data

/// A 2-D profile fill: `(x, y, z, weight)`.
type Point2 = (f64, f64, f64, f64);
/// A 3-D profile fill: `(x, y, z, t, weight)`.
type Point3 = (f64, f64, f64, f64, f64);

/// Four unit-weight 2-D fills, so this profile never tracks Σw² itself. Repeats
/// a cell and includes an x-underflow point, which fills a flow cell but no
/// moment sum.
const SET_A_2D: [Point2; 4] = [
    (0.5, 0.5, 1.0, 1.0),
    (1.5, 0.5, 2.0, 1.0),
    (0.5, 0.5, 3.0, 1.0),
    (-1.0, 0.5, 4.0, 1.0),
];
/// Mixes unit and non-unit weights, starting with a unit fill, so the Σw²
/// tracking its first weighted fill turns on must be seeded.
const SET_B_2D: [Point2; 4] = [
    (0.5, 0.5, 5.0, 1.0),
    (1.5, 1.5, 2.0, 2.0),
    (2.5, 0.5, 1.0, 0.5),
    (0.5, 0.5, 1.0, 2.0),
];

/// Three 3-D fills.
const SET_A_3D: [Point3; 3] = [
    (0.5, 0.5, 0.5, 1.0, 1.0),
    (1.5, 0.5, 1.5, 2.0, 1.0),
    (0.5, 0.5, 0.5, 3.0, 1.0),
];
const SET_B_3D: [Point3; 4] = [
    (0.5, 0.5, 0.5, 4.0, 1.0),
    (1.5, 1.5, 0.5, 2.0, 2.0),
    (0.5, 1.5, 1.5, 1.0, 0.5),
    (5.0, 0.5, 0.5, 1.0, 2.0), // x overflow
];

fn empty_2d() -> TProfile2D {
    Hist::reg(3, 0.0, 3.0)
        .reg(2, 0.0, 2.0)
        .profile()
        .named("p2")
}

fn empty_3d() -> TProfile3D {
    Hist::reg(2, 0.0, 2.0)
        .reg(2, 0.0, 2.0)
        .reg(2, 0.0, 2.0)
        .profile()
        .named("p3")
}

fn fill_2d(p: &mut TProfile2D, points: &[Point2]) {
    for &(x, y, z, w) in points {
        p.fill_weight(x, y, z, w);
    }
}

fn fill_3d(p: &mut TProfile3D, points: &[Point3]) {
    for &(x, y, z, t, w) in points {
        p.fill_weight(x, y, z, t, w);
    }
}

fn profile_2d(sets: &[&[Point2]]) -> TProfile2D {
    let mut p = empty_2d();
    for set in sets {
        fill_2d(&mut p, set);
    }
    p
}

fn profile_3d(sets: &[&[Point3]]) -> TProfile3D {
    let mut p = empty_3d();
    for set in sets {
        fill_3d(&mut p, set);
    }
    p
}

// ------------------------------------------------------------- file merging

#[test]
fn file_merge_sums_2d_and_3d_profiles() {
    let dir = std::env::temp_dir();
    let tag = std::process::id();
    let in1 = dir.join(format!("oxiroot_pmerge_in1_{tag}.root"));
    let in2 = dir.join(format!("oxiroot_pmerge_in2_{tag}.root"));
    let out = dir.join(format!("oxiroot_pmerge_out_{tag}.root"));

    let (a2, a3) = (profile_2d(&[&SET_A_2D]), profile_3d(&[&SET_A_3D]));
    let (b2, b3) = (profile_2d(&[&SET_B_2D]), profile_3d(&[&SET_B_3D]));
    RootFile::create(&in1)
        .add(&a2)
        .add(&a3)
        .write(Compression::None)
        .unwrap();
    RootFile::create(&in2)
        .add(&b2)
        .add(&b3)
        .write(Compression::None)
        .unwrap();

    let inputs = [RFile::open(&in1).unwrap(), RFile::open(&in2).unwrap()];
    let outcome = merge_histogram_files(&out, &inputs, Compression::None).unwrap();

    let result = (|| {
        let f = RFile::open(&out)?;
        Ok::<_, Error>((
            TProfile2D::read_root(&f, "p2")?,
            TProfile3D::read_root(&f, "p3")?,
        ))
    })();
    for p in [&in1, &in2, &out] {
        let _ = std::fs::remove_file(p);
    }
    let (got2, got3) = result.unwrap();

    assert_eq!(outcome.summed, ["p2", "p3"], "{outcome:?}");
    assert!(outcome.copied.is_empty(), "{outcome:?}");
    assert!(outcome.skipped.is_empty(), "{outcome:?}");
    assert_eq!(got2, profile_2d(&[&SET_A_2D, &SET_B_2D]));
    assert_eq!(got3, profile_3d(&[&SET_A_3D, &SET_B_3D]));
}

// -------------------------------------------------------- merge failure policy

#[test]
fn an_unreadable_profile_is_skipped_not_fatal() {
    let dir = std::env::temp_dir();
    let tag = std::process::id();
    let good = dir.join(format!("oxiroot_pskip_good_{tag}.root"));
    let bad = dir.join(format!("oxiroot_pskip_bad_{tag}.root"));
    let out_a = dir.join(format!("oxiroot_pskip_out_a_{tag}.root"));
    let out_b = dir.join(format!("oxiroot_pskip_out_b_{tag}.root"));

    let mut h = Hist::reg(4, 0.0, 4.0).double().named("h");
    h.fill(0.5);
    let p2 = profile_2d(&[&SET_A_2D]);
    let mut broken = p2.clone();
    broken.bin_sumw2 = vec![1.0; 3]; // not one per cell: the reader rejects it
    RootFile::create(&good)
        .add(&h)
        .add(&p2)
        .write(Compression::None)
        .unwrap();
    RootFile::create(&bad)
        .add(&h)
        .add(&broken)
        .write(Compression::None)
        .unwrap();

    let run = |inputs: [&std::path::Path; 2], out: &std::path::Path| {
        let files = inputs.map(|p| RFile::open(p).unwrap());
        let outcome = merge_histogram_files(out, &files, Compression::None).unwrap();
        let summed_h = TH1::read_root(&RFile::open(out).unwrap(), "h").unwrap();
        (outcome, summed_h)
    };
    // The bad object last, then first: either way the key is skipped whole.
    let (late, h_late) = run([good.as_path(), bad.as_path()], &out_a);
    let (early, h_early) = run([bad.as_path(), good.as_path()], &out_b);
    for p in [&good, &bad, &out_a, &out_b] {
        let _ = std::fs::remove_file(p);
    }

    for (outcome, merged_h, bad_index) in [(late, h_late, 2), (early, h_early, 1)] {
        assert_eq!(outcome.summed, ["h"], "{outcome:?}");
        assert_eq!(outcome.skipped.len(), 1, "{outcome:?}");
        let (name, reason) = &outcome.skipped[0];
        assert_eq!(name, "p2");
        assert!(
            reason.contains(&format!("input {bad_index} of 2")),
            "{reason}"
        );
        assert_eq!(merged_h.contents[1], 2.0, "the other key still sums");
    }
}

#[test]
fn a_binning_mismatch_names_the_key() {
    let dir = std::env::temp_dir();
    let tag = std::process::id();
    let f1 = dir.join(format!("oxiroot_pmis_1_{tag}.root"));
    let f2 = dir.join(format!("oxiroot_pmis_2_{tag}.root"));
    let out = dir.join(format!("oxiroot_pmis_out_{tag}.root"));
    Hist::reg(4, 0.0, 4.0)
        .double()
        .named("pt")
        .write_root(&f1, Compression::None)
        .unwrap();
    Hist::reg(5, 0.0, 4.0)
        .double()
        .named("pt")
        .write_root(&f2, Compression::None)
        .unwrap();
    let files = [RFile::open(&f1).unwrap(), RFile::open(&f2).unwrap()];
    let result = merge_histogram_files(&out, &files, Compression::None);
    for p in [&f1, &f2, &out] {
        let _ = std::fs::remove_file(p);
    }
    match result {
        Err(Error::BinningMismatch { detail }) => {
            assert!(detail.contains("\"pt\""), "{detail}")
        }
        other => panic!("expected a binning mismatch, got {other:?}"),
    }
}
