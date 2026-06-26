//! TEfficiency: read a ROOT-written fixture + self-round-trip (ROOT C++ oracle;
//! uproot can't read TEfficiency).

use std::path::PathBuf;

use oxiroot_hist::{read_tefficiency, write_tefficiency_file, TEfficiency};
use oxiroot_io_core::{Compression, RFile};

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures")
        .join(name)
}

fn sample() -> TEfficiency {
    let mut e = TEfficiency::new("eff", "my eff;x;#epsilon", 4, 0.0, 4.0);
    e.fill(true, 0.5);
    e.fill(false, 0.5); // bin1 1/2
    e.fill(true, 1.5);
    e.fill(true, 1.5); // bin2 2/2
    e.fill(false, 2.5); // bin3 0/1
    e.fill(true, 3.5);
    e.fill(true, 3.5);
    e.fill(true, 3.5); // bin4 3/3
    e
}

#[test]
fn reads_root_written_tefficiency() {
    let f = RFile::open(fixture("tefficiency.root")).expect("open");
    assert_eq!(f.key("eff").unwrap().class_name, "TEfficiency");
    let e = read_tefficiency(&f, "eff").expect("read");
    assert_eq!(e.efficiency(1), 0.5);
    assert_eq!(e.efficiency(2), 1.0);
    assert_eq!(e.efficiency(3), 0.0);
    assert_eq!(e.efficiency(4), 1.0);
    assert!((e.conf_level - 0.682689492137086).abs() < 1e-12);
}

#[test]
fn tefficiency_round_trips() {
    let e = sample();
    assert_eq!(e.efficiency(1), 0.5);
    let out = PathBuf::from("/tmp/oxiroot_tefficiency.root");
    write_tefficiency_file(&out, &e, Compression::None).expect("write");
    let f = RFile::open(&out).expect("reopen");
    assert_eq!(read_tefficiency(&f, "eff").unwrap(), e, "round-trips");
}
