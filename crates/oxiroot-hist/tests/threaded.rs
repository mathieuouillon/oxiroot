//! Multithreaded fill (`ThreadedHist`, `merge_all`, `fill_par`): a parallel fill
//! must equal the serial fill (bin contents and entries exactly; moment sums to
//! rounding, since summation order differs).

use oxiroot_hist::{merge_all, ThreadedHist, TH1};

fn data() -> Vec<f64> {
    // Deterministic, varied, all in-range over [0, 100).
    (0..1000).map(|i| ((i * 37) % 100) as f64 + 0.5).collect()
}

fn serial(data: &[f64]) -> TH1 {
    let mut h = TH1::new("h", "", 100, 0.0, 100.0);
    for &x in data {
        h.fill(x);
    }
    h
}

#[test]
fn threaded_fill_matches_serial() {
    let data = data();
    let want = serial(&data);

    let acc = ThreadedHist::new(TH1::new("h", "", 100, 0.0, 100.0));
    std::thread::scope(|s| {
        for chunk in data.chunks(data.len().div_ceil(4)) {
            let acc = &acc;
            s.spawn(move || {
                let mut h = acc.local();
                for &x in chunk {
                    h.fill(x);
                }
                acc.push(h);
            });
        }
    });
    assert_eq!(acc.num_slots(), 4, "four locals handed back");
    let got = acc.merge().expect("identical binning");

    // Bin contents and entry count are exact regardless of fill/merge order.
    assert_eq!(
        got.values(),
        want.values(),
        "bin contents match serial fill"
    );
    assert_eq!(got.entries, want.entries, "entry count matches");
    // Moment sums match to floating-point rounding.
    assert!((got.mean() - want.mean()).abs() < 1e-9, "mean matches");
}

#[test]
fn merge_with_no_work_returns_empty_prototype() {
    // No locals pushed → an empty histogram with the template's binning.
    let acc = ThreadedHist::new(TH1::new("h", "", 4, 0.0, 4.0));
    let h = acc.merge().unwrap();
    assert_eq!(h.entries, 0.0);
    assert_eq!(h.values(), &[0.0, 0.0, 0.0, 0.0]);
    assert_eq!(h.xaxis.nbins, 4, "binning preserved");
}

#[test]
fn merge_all_folds_or_none() {
    assert!(
        merge_all(Vec::<TH1>::new()).unwrap().is_none(),
        "empty → None"
    );

    let one = serial(&[0.5, 0.5]);
    let merged = merge_all(vec![one.clone()]).unwrap().unwrap();
    assert_eq!(merged, one, "single item → itself");

    let parts: Vec<TH1> = data().chunks(250).map(serial).collect();
    let merged = merge_all(parts).unwrap().unwrap();
    assert_eq!(
        merged.values(),
        serial(&data()).values(),
        "many → full fill"
    );
}

#[test]
fn merge_rejects_mismatched_binning() {
    let a = TH1::new("h", "", 4, 0.0, 4.0);
    let b = TH1::new("h", "", 5, 0.0, 5.0);
    assert!(
        merge_all(vec![a, b]).is_err(),
        "incompatible binnings error"
    );
}

#[cfg(feature = "rayon")]
#[test]
fn fill_par_matches_serial() {
    use oxiroot_hist::fill_par;
    let data = data();
    let want = serial(&data);
    let got = fill_par(&TH1::new("h", "", 100, 0.0, 100.0), &data, |h, &x| {
        h.fill(x)
    });
    assert_eq!(
        got.values(),
        want.values(),
        "fill_par contents match serial"
    );
    assert_eq!(got.entries, want.entries);
    assert!((got.mean() - want.mean()).abs() < 1e-9);
}
