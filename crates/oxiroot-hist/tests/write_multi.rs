//! #3: write several histograms (a TH1D and a TH2D) into a single ROOT file,
//! then read each back through our own reader.

use std::path::PathBuf;

use oxiroot_hist::{ReadRoot, RootFile, TH1, TH2};
use oxiroot_io_core::RFile;

#[test]
fn writes_multiple_histograms_into_one_file() {
    let mut h1 = TH1::new("hx", "1-D", 4, 0.0, 4.0);
    for x in [0.5, 1.5, 1.5, 3.5] {
        h1.fill(x);
    }
    let mut h2 = TH2::new("hxy", "2-D", 2, 0.0, 2.0, 2, 0.0, 2.0);
    h2.fill(0.5, 0.5);
    h2.fill(1.5, 1.5);
    h2.fill(1.5, 1.5);

    let out = PathBuf::from("/tmp/rootrs_multi_hist.root");
    RootFile::create(&out)
        .add(&h1)
        .add(&h2)
        .write(oxiroot_io_core::Compression::None)
        .expect("write");

    let f = RFile::open(&out).expect("reopen");
    let keys: Vec<(&str, &str)> = f
        .keys()
        .iter()
        .map(|k| (k.name.as_str(), k.class_name.as_str()))
        .collect();
    assert_eq!(keys, vec![("hx", "TH1D"), ("hxy", "TH2D")]);

    assert_eq!(TH1::read_root(&f, "hx").expect("read hx"), h1);
    assert_eq!(TH2::read_root(&f, "hxy").expect("read hxy"), h2);
}
