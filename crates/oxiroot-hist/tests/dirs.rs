//! Item 4: write histograms organized into subdirectories, then read them back
//! through our reader (and, separately, validate with official ROOT + uproot).

use std::path::PathBuf;

use oxiroot_hist::{Hist, ReadRoot, RootFile, TH1};
use oxiroot_io_core::RFile;

#[test]
fn writes_histograms_into_subdirectories() {
    let mut top = Hist::reg(3, 0.0, 3.0)
        .double()
        .named("top")
        .titled("top-level");
    top.fill(0.5);

    let mut sr = Hist::reg(4, 0.0, 4.0)
        .double()
        .named("mll")
        .titled("signal region");
    sr.fill(1.5);
    sr.fill(2.5);
    let mut cr = Hist::reg(4, 0.0, 4.0)
        .double()
        .named("mll")
        .titled("control region");
    cr.fill(0.5);

    let out = PathBuf::from("/tmp/rootrs_dirs.root");
    RootFile::create(&out)
        .add(&top)
        .dir("signal", |d| d.add(&sr))
        .dir("control", |d| d.add(&cr))
        .write(oxiroot_io_core::Compression::None)
        .expect("write");

    let f = RFile::open(&out).expect("reopen");

    // The root directory lists the top histogram and the two subdirectories.
    let root_keys: Vec<(&str, &str)> = f
        .keys()
        .iter()
        .map(|k| (k.name.as_str(), k.class_name.as_str()))
        .collect();
    assert!(root_keys.contains(&("top", "TH1D")));
    assert!(root_keys.contains(&("signal", "TDirectory")));
    assert!(root_keys.contains(&("control", "TDirectory")));

    // The top-level histogram and both subdirectory histograms read back.
    assert_eq!(TH1::read_root(&f, "top").unwrap(), top);
    assert_eq!(TH1::read_root_in(&f, "signal", "mll").unwrap(), sr);
    assert_eq!(TH1::read_root_in(&f, "control", "mll").unwrap(), cr);

    // The subdirectory's own key list is navigable.
    let signal = f.subdir("signal").expect("signal dir");
    assert_eq!(
        signal
            .keys
            .iter()
            .map(|k| k.name.as_str())
            .collect::<Vec<_>>(),
        ["mll"]
    );
}
