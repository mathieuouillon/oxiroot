//! Adding and merging the 2-D and 3-D profiles.
//!
//! `add` and `Merge` used to exist only for `TH1`/`TH2`/`TH3`/`TProfile`, so a
//! file merge copied `TProfile2D`/`TProfile3D` from the first input where ROOT's
//! `hadd` sums them. The defining property checked here is the one `Merge`
//! documents: merging two profiles must give exactly the profile you would get by
//! filling one with all of their data. Every value below is a small dyadic
//! rational, so that equality is exact whatever order the sums happen in.

use oxiroot_hist::{
    merge_histogram_files, Hist, Merge, ReadRoot, RootFile, TProfile, TProfile2D, TProfile3D,
    ThreadedHist, WriteRoot,
};
use oxiroot_io_core::{Compression, Error, RFile};

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

// ------------------------------------------------------ add is a combined fill

#[test]
fn adding_2d_profiles_equals_one_combined_fill() {
    let mut merged = profile_2d(&[&SET_A_2D]);
    merged.add(&profile_2d(&[&SET_B_2D]), 1.0).unwrap();
    let want = profile_2d(&[&SET_A_2D, &SET_B_2D]);
    assert_eq!(merged, want);
    // The weighted fills are reflected in the error bookkeeping, not dropped.
    assert!(!merged.bin_sumw2.is_empty());
}

#[test]
fn adding_3d_profiles_equals_one_combined_fill() {
    let mut merged = profile_3d(&[&SET_A_3D]);
    merged.add(&profile_3d(&[&SET_B_3D]), 1.0).unwrap();
    assert_eq!(merged, profile_3d(&[&SET_A_3D, &SET_B_3D]));
    assert!(!merged.bin_sumw2.is_empty());
}

#[test]
fn addition_order_does_not_matter() {
    let mut ab = profile_2d(&[&SET_A_2D]);
    ab.add(&profile_2d(&[&SET_B_2D]), 1.0).unwrap();
    let mut ba = profile_2d(&[&SET_B_2D]);
    ba.add(&profile_2d(&[&SET_A_2D]), 1.0).unwrap();
    assert_eq!(ab, ba);
}

#[test]
fn scaled_2d_add_is_linear_except_squared_weights() {
    let (mut dst, src) = (profile_2d(&[&SET_A_2D]), profile_2d(&[&SET_B_2D]));
    let before = dst.clone();
    let c = 3.0;
    dst.add(&src, c).unwrap();

    for (name, got, a, b) in [
        ("tsumw", dst.tsumw, before.tsumw, src.tsumw),
        ("tsumwx2", dst.tsumwx2, before.tsumwx2, src.tsumwx2),
        ("tsumwxy", dst.tsumwxy, before.tsumwxy, src.tsumwxy),
        ("tsumwz", dst.tsumwz, before.tsumwz, src.tsumwz),
        ("tsumwz2", dst.tsumwz2, before.tsumwz2, src.tsumwz2),
    ] {
        assert_eq!(got, a + c * b, "{name} must scale by c");
    }
    assert_eq!(dst.tsumw2, before.tsumw2 + c * c * src.tsumw2);

    // `before` had only unit weights, so its seeded Σw² equals its Σw.
    let cell = 1 + 5; // (x-bin 1, y-bin 1): stride nx + 2 = 5
    assert_eq!(
        dst.bin_sumw2[cell],
        before.bin_entries[cell] + c * c * src.bin_sumw2[cell]
    );
    assert_eq!(
        dst.bin_entries[cell],
        before.bin_entries[cell] + c * src.bin_entries[cell]
    );
}

#[test]
fn scaled_3d_add_scales_its_extra_moments() {
    let (mut dst, src) = (profile_3d(&[&SET_A_3D]), profile_3d(&[&SET_B_3D]));
    let before = dst.clone();
    let c = 0.5;
    dst.add(&src, c).unwrap();
    for (name, got, a, b) in [
        ("tsumwxz", dst.tsumwxz, before.tsumwxz, src.tsumwxz),
        ("tsumwyz", dst.tsumwyz, before.tsumwyz, src.tsumwyz),
        ("tsumwt", dst.tsumwt, before.tsumwt, src.tsumwt),
        ("tsumwt2", dst.tsumwt2, before.tsumwt2, src.tsumwt2),
    ] {
        assert_eq!(got, a + c * b, "{name} must scale by c");
    }
    assert_eq!(dst.tsumw2, before.tsumw2 + c * c * src.tsumw2);
}

#[test]
fn mismatched_binning_is_rejected_and_leaves_the_profile_unchanged() {
    let mut p = profile_2d(&[&SET_A_2D]);
    let before = p.clone();
    let other = Hist::reg(2, 0.0, 2.0).reg(2, 0.0, 2.0).profile(); // 2 x-bins, not 3
    assert!(matches!(
        p.add(&other, 1.0),
        Err(Error::BinningMismatch { .. })
    ));
    assert_eq!(p, before);

    let mut q = profile_3d(&[&SET_A_3D]);
    let other3 = Hist::reg(2, 0.0, 2.0)
        .reg(2, 0.0, 2.0)
        .reg(3, 0.0, 3.0) // z differs
        .profile();
    assert!(q.add(&other3, 1.0).is_err());
    assert_eq!(q, profile_3d(&[&SET_A_3D]));
}

/// A file may store a profile's `fSumw2` (Σw·v²) empty, meaning all zeros. The
/// 1-D `add` used to pair that array up to the shorter length, so an empty
/// receiving array silently dropped the other profile's values.
#[test]
fn an_empty_value_squared_array_is_treated_as_zeros() {
    let mut a = Hist::reg(2, 0.0, 2.0).profile();
    a.fill(0.5, 2.0);
    a.sumy2.clear(); // as stored by a file that omitted it
    let mut b = Hist::reg(2, 0.0, 2.0).profile();
    b.fill(0.5, 3.0);

    a.add(&b, 1.0).unwrap();
    assert_eq!(a.sumy2.len(), a.sums.len());
    assert_eq!(a.sumy2[1], 9.0, "b's Σw·y² must survive the merge");

    // And an empty array on the other side adds nothing.
    let mut c = Hist::reg(2, 0.0, 2.0).profile();
    c.fill(0.5, 2.0);
    let mut d: TProfile = c.clone();
    d.sumy2.clear();
    c.add(&d, 1.0).unwrap();
    assert_eq!(c.sumy2[1], 4.0);
}

// ------------------------------------------------------------------- Merge

#[test]
fn merge_all_folds_2d_and_3d_profiles() {
    let got = TProfile2D::merge_all([profile_2d(&[&SET_A_2D]), profile_2d(&[&SET_B_2D])])
        .unwrap()
        .expect("two items");
    assert_eq!(got, profile_2d(&[&SET_A_2D, &SET_B_2D]));

    let got = TProfile3D::merge_all([profile_3d(&[&SET_A_3D]), profile_3d(&[&SET_B_3D])])
        .unwrap()
        .expect("two items");
    assert_eq!(got, profile_3d(&[&SET_A_3D, &SET_B_3D]));
}

#[test]
fn threaded_2d_and_3d_fills_match_a_serial_fill() {
    let acc2 = ThreadedHist::new(empty_2d());
    let acc3 = ThreadedHist::new(empty_3d());
    std::thread::scope(|s| {
        let (acc2, acc3) = (&acc2, &acc3);
        s.spawn(move || {
            for &(x, y, z, w) in &SET_A_2D {
                acc2.fill_weight(x, y, z, w);
            }
            for &(x, y, z, t, w) in &SET_A_3D {
                acc3.fill_weight(x, y, z, t, w);
            }
        });
        s.spawn(move || {
            for &(x, y, z, w) in &SET_B_2D {
                acc2.fill_weight(x, y, z, w);
            }
            for &(x, y, z, t, w) in &SET_B_3D {
                acc3.fill_weight(x, y, z, t, w);
            }
        });
    });
    assert_eq!(acc2.merge().unwrap(), profile_2d(&[&SET_A_2D, &SET_B_2D]));
    assert_eq!(acc3.merge().unwrap(), profile_3d(&[&SET_A_3D, &SET_B_3D]));
}

#[test]
fn threaded_unit_fill_shortcuts_exist_for_2d_and_3d() {
    let acc2 = ThreadedHist::new(empty_2d());
    acc2.fill(0.5, 0.5, 1.0);
    assert_eq!(acc2.merge().unwrap().entries, 1.0);
    let acc3 = ThreadedHist::new(empty_3d());
    acc3.fill(0.5, 0.5, 0.5, 1.0);
    assert_eq!(acc3.merge().unwrap().entries, 1.0);
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

// ------------------------------------------------------- negative scale factor
//
// ROOT's `TProfileHelper::Add` scales the weights by |c| and only the weighted
// value sums by the signed c, so subtracting a profile flips its values while
// keeping every weight non-negative. The expected numbers below transcribe those
// formulas.

#[test]
fn subtracting_a_1d_profile_follows_root() {
    let mut a = Hist::reg(2, 0.0, 2.0).profile();
    a.fill(0.5, 1.0);
    let mut b = Hist::reg(2, 0.0, 2.0).profile();
    b.fill(0.5, 1.0);
    b.fill(0.5, 1.0);

    a.add(&b, -1.0).unwrap();
    assert_eq!(a.sums[1], 1.0 - 2.0);
    assert_eq!(a.bin_entries[1], 1.0 + 2.0);
    assert_eq!(a.sumy2[1], 1.0 + 2.0);
    assert_eq!(a.entries, 3.0);
    assert_eq!((a.tsumw, a.tsumwy, a.tsumwy2), (3.0, 3.0, 3.0));
    assert_eq!(a.tsumw2, 3.0);
    // |c| == 1 keeps unit weights, so Σw² still equals Σw and is not allocated.
    assert!(a.bin_sumw2.is_empty());
    assert_eq!(a.values()[0], -1.0 / 3.0);
    // mean = -1/3, spread² = 1 - 1/9 = 8/9, neff = 3.
    assert!((a.bin_error(1) - (8.0_f64 / 27.0).sqrt()).abs() < 1e-15);
}

#[test]
fn subtracting_a_profile_from_itself_keeps_its_weights() {
    let mut p = Hist::reg(2, 0.0, 2.0).reg(2, 0.0, 2.0).profile();
    p.fill(0.5, 0.5, 1.0);
    p.fill(0.5, 0.5, 3.0);
    let cell = 1 + 4;
    let orig = p.clone();
    p.add(&orig, -1.0).unwrap();
    assert_eq!(p.sums[cell], 0.0);
    assert_eq!(p.bin_entries[cell], 4.0);
    assert_eq!(p.sumz2[cell], 20.0);
    assert_eq!((p.entries, p.tsumw), (4.0, 4.0));
    assert!(p.bin_sumw2.is_empty());
    // mean 0, spread² = 20/4 = 5, neff = 4: the error is √(5/4), not 0.
    assert!((p.bin_error(cell) - 1.25_f64.sqrt()).abs() < 1e-15);

    let mut q = profile_3d(&[&SET_A_3D]);
    let orig3 = q.clone();
    q.add(&orig3, -1.0).unwrap();
    assert!(q.sums.iter().all(|&v| v == 0.0));
    let doubled = |v: &[f64]| v.iter().map(|x| 2.0 * x).collect::<Vec<_>>();
    assert_eq!(q.bin_entries, doubled(&orig3.bin_entries));
    assert_eq!(q.sumt2, doubled(&orig3.sumt2));
    assert_eq!(q.entries, 2.0 * orig3.entries);
    assert_eq!(q.tsumwt2, 2.0 * orig3.tsumwt2);
}

#[test]
fn negative_scale_of_weighted_profiles_squares_the_weights() {
    let (mut dst, src) = (profile_2d(&[&SET_B_2D]), profile_2d(&[&SET_B_2D]));
    let before = dst.clone();
    dst.add(&src, -2.0).unwrap();
    let cell = 1 + 5;
    assert_eq!(dst.sums[cell], before.sums[cell] - 2.0 * src.sums[cell]);
    assert_eq!(
        dst.bin_entries[cell],
        before.bin_entries[cell] + 2.0 * src.bin_entries[cell]
    );
    assert_eq!(
        dst.bin_sumw2[cell],
        before.bin_sumw2[cell] + 4.0 * src.bin_sumw2[cell]
    );
    assert_eq!(dst.tsumwz, before.tsumwz + 2.0 * src.tsumwz);
    assert_eq!(dst.tsumw2, before.tsumw2 + 4.0 * src.tsumw2);
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
        let summed_h = oxiroot_hist::TH1::read_root(&RFile::open(out).unwrap(), "h").unwrap();
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
