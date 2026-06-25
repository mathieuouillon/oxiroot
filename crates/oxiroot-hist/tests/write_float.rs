//! Float-precision histogram write (`TH1F`/`TH2F`/`TH3F`): write, then read back
//! through our own reader. The files in /tmp are also checked by uproot/ROOT C++
//! in the interop job.

use oxiroot_hist::{
    read_th1f, read_th2f, read_th3f, write_th1f_file, write_th2f_file, write_th3f_file, TH1, TH2,
    TH3,
};
use oxiroot_io_core::{Compression, RFile};

#[test]
fn th1f_write_read_round_trips() {
    let mut h = TH1::new("h1", "float", 4, 0.0, 4.0);
    for (i, &n) in [1.0, 2.0, 3.0, 4.0].iter().enumerate() {
        for _ in 0..(n as usize) {
            h.fill(i as f64 + 0.5);
        }
    }
    let out = std::path::PathBuf::from("/tmp/oxiroot_th1f.root");
    write_th1f_file(&out, &h, Compression::None).expect("write");

    let f = RFile::open(&out).expect("reopen");
    assert_eq!(f.key("h1").expect("key").class_name, "TH1F");
    let back = read_th1f(&f, "h1").expect("read TH1F");
    assert_eq!(back.values(), h.values());
}

#[test]
fn th2f_write_read_round_trips() {
    let mut h = TH2::new("h2", "float2", 2, 0.0, 2.0, 2, 0.0, 2.0);
    h.fill(0.5, 0.5);
    h.fill(1.5, 1.5);
    h.fill(1.5, 1.5);
    let out = std::path::PathBuf::from("/tmp/oxiroot_th2f.root");
    write_th2f_file(&out, &h, Compression::None).expect("write");

    let f = RFile::open(&out).expect("reopen");
    assert_eq!(f.key("h2").expect("key").class_name, "TH2F");
    let back = read_th2f(&f, "h2").expect("read TH2F");
    assert_eq!(back.values(), h.values());
}

#[test]
fn th3f_write_read_round_trips() {
    let mut h = TH3::new("h3", "float3", 2, 0.0, 2.0, 2, 0.0, 2.0, 2, 0.0, 2.0);
    h.fill(0.5, 0.5, 0.5);
    h.fill(1.5, 1.5, 1.5);
    let out = std::path::PathBuf::from("/tmp/oxiroot_th3f.root");
    write_th3f_file(&out, &h, Compression::None).expect("write");

    let f = RFile::open(&out).expect("reopen");
    assert_eq!(f.key("h3").expect("key").class_name, "TH3F");
    let back = read_th3f(&f, "h3").expect("read TH3F");
    assert_eq!(back.values(), h.values());
}
