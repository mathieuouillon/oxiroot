//! Float-precision histogram write (`TH1F`/`TH2F`/`TH3F`): write, then read back
//! through our own reader. The files in /tmp are also checked by uproot/ROOT C++
//! in the interop job.

use oxiroot_hist::{Hist, Precision, ReadRoot, WriteRoot, TH1, TH2, TH3};
use oxiroot_io_core::{Compression, RFile};

#[test]
fn th1f_write_read_round_trips() {
    let mut h = Hist::reg(4, 0.0, 4.0).double().named("h1").titled("float");
    for (i, &n) in [1.0, 2.0, 3.0, 4.0].iter().enumerate() {
        for _ in 0..(n as usize) {
            h.fill(i as f64 + 0.5);
        }
    }
    let out = std::path::PathBuf::from("/tmp/oxiroot_th1f.root");
    h.clone()
        .with_precision(Precision::Float)
        .write_root(&out, Compression::None)
        .expect("write");

    let f = RFile::open(&out).expect("reopen");
    assert_eq!(f.key("h1").expect("key").class_name, "TH1F");
    let back = TH1::read_root(&f, "h1").expect("read TH1F");
    assert_eq!(back.values(), h.values());
}

#[test]
fn th2f_write_read_round_trips() {
    let mut h = Hist::reg(2, 0.0, 2.0)
        .reg(2, 0.0, 2.0)
        .double()
        .named("h2")
        .titled("float2");
    h.fill(0.5, 0.5);
    h.fill(1.5, 1.5);
    h.fill(1.5, 1.5);
    let out = std::path::PathBuf::from("/tmp/oxiroot_th2f.root");
    h.clone()
        .with_precision(Precision::Float)
        .write_root(&out, Compression::None)
        .expect("write");

    let f = RFile::open(&out).expect("reopen");
    assert_eq!(f.key("h2").expect("key").class_name, "TH2F");
    let back = TH2::read_root(&f, "h2").expect("read TH2F");
    assert_eq!(back.values(), h.values());
}

#[test]
fn th3f_write_read_round_trips() {
    let mut h = Hist::reg(2, 0.0, 2.0)
        .reg(2, 0.0, 2.0)
        .reg(2, 0.0, 2.0)
        .double()
        .named("h3")
        .titled("float3");
    h.fill(0.5, 0.5, 0.5);
    h.fill(1.5, 1.5, 1.5);
    let out = std::path::PathBuf::from("/tmp/oxiroot_th3f.root");
    h.clone()
        .with_precision(Precision::Float)
        .write_root(&out, Compression::None)
        .expect("write");

    let f = RFile::open(&out).expect("reopen");
    assert_eq!(f.key("h3").expect("key").class_name, "TH3F");
    let back = TH3::read_root(&f, "h3").expect("read TH3F");
    assert_eq!(back.values(), h.values());
}
