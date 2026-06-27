//! Integer-precision histogram write (`TH1C`/`TH1S`/`TH1I` and 2D/3D): write,
//! then read back through our own reader. The /tmp files are also checked by
//! uproot/ROOT C++ when run by hand.

use oxiroot_hist::{Precision, ReadRoot, WriteRoot, TH1, TH2, TH3};
use oxiroot_io_core::{Compression, RFile};

fn filled_th1() -> TH1 {
    let mut h = TH1::new("h", "int", 4, 0.0, 4.0);
    for (i, &n) in [1.0, 2.0, 3.0, 4.0].iter().enumerate() {
        for _ in 0..(n as usize) {
            h.fill(i as f64 + 0.5);
        }
    }
    h
}

fn check_th1(out: &str, cls: &str, h: &TH1) {
    let f = RFile::open(out).expect("reopen");
    assert_eq!(f.key("h").expect("key").class_name, cls);
    assert_eq!(
        TH1::read_root(&f, "h").expect("read back").values(),
        h.values()
    );
}

#[test]
fn th1_integer_variants_round_trip() {
    let h = filled_th1();
    h.clone()
        .with_precision(Precision::Char)
        .write_root("/tmp/oxiroot_th1c.root", Compression::None)
        .expect("write C");
    check_th1("/tmp/oxiroot_th1c.root", "TH1C", &h);
    h.clone()
        .with_precision(Precision::Short)
        .write_root("/tmp/oxiroot_th1s.root", Compression::None)
        .expect("write S");
    check_th1("/tmp/oxiroot_th1s.root", "TH1S", &h);
    h.clone()
        .with_precision(Precision::Int)
        .write_root("/tmp/oxiroot_th1i.root", Compression::None)
        .expect("write I");
    check_th1("/tmp/oxiroot_th1i.root", "TH1I", &h);
    h.clone()
        .with_precision(Precision::Long)
        .write_root("/tmp/oxiroot_th1l.root", Compression::None)
        .expect("write L");
    check_th1("/tmp/oxiroot_th1l.root", "TH1L", &h);
}

#[test]
fn th2i_th3i_round_trip() {
    let mut h2 = TH2::new("h2", "i2", 2, 0.0, 2.0, 2, 0.0, 2.0);
    h2.fill(0.5, 0.5);
    h2.fill(1.5, 1.5);
    let out = std::path::PathBuf::from("/tmp/oxiroot_th2i.root");
    h2.clone()
        .with_precision(Precision::Int)
        .write_root(&out, Compression::None)
        .expect("write");
    let f = RFile::open(&out).expect("reopen");
    assert_eq!(f.key("h2").expect("key").class_name, "TH2I");
    assert_eq!(TH2::read_root(&f, "h2").unwrap().values(), h2.values());

    let mut h3 = TH3::new("h3", "i3", 2, 0.0, 2.0, 2, 0.0, 2.0, 2, 0.0, 2.0);
    h3.fill(0.5, 0.5, 0.5);
    let out = std::path::PathBuf::from("/tmp/oxiroot_th3i.root");
    h3.clone()
        .with_precision(Precision::Int)
        .write_root(&out, Compression::None)
        .expect("write");
    let f = RFile::open(&out).expect("reopen");
    assert_eq!(f.key("h3").expect("key").class_name, "TH3I");
    assert_eq!(TH3::read_root(&f, "h3").unwrap().values(), h3.values());
}
