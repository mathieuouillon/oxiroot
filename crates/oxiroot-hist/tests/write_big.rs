//! The 64-bit ("big") TFile container form for the one-shot object writers, used
//! automatically once a file would exceed 2 GiB. The big-file threshold is
//! lowered here (via `RootFile::write_threshold`) so the wide header/directory/
//! key path is exercised without producing a 2 GiB file; the result must read
//! back through our own reader with every object intact.

use std::path::PathBuf;

use oxiroot_hist::{Hist, ReadRoot, RootFile, TH1, TH2};
use oxiroot_io_core::RFile;

fn th1(name: &str) -> TH1 {
    let mut h = Hist::reg(4, 0.0, 4.0).double().named(name).titled("1-D");
    for x in [0.5, 1.5, 1.5, 3.5] {
        h.fill(x);
    }
    h
}

#[test]
fn big_container_flat_round_trips() {
    let out = PathBuf::from("/tmp/oxiroot_big_flat.root");
    let h1 = th1("hx");
    let mut h2 = Hist::reg(2, 0.0, 2.0)
        .reg(2, 0.0, 2.0)
        .double()
        .named("hxy")
        .titled("2-D");
    h2.fill(0.5, 0.5);
    h2.fill(1.5, 1.5);

    // Threshold 0 forces the 64-bit container form even for this tiny file.
    RootFile::create(&out)
        .add(&h1)
        .add(&h2)
        .write_threshold(oxiroot_io_core::Compression::None, 0)
        .expect("write big");

    let f = RFile::open(&out).expect("reopen");
    assert!(
        f.header().is_big(),
        "forced-big file must use fVersion>=1e6"
    );
    assert_eq!(f.header().units, 8, "big-format fUnits is 8");
    let keys: Vec<(&str, &str)> = f
        .keys()
        .iter()
        .map(|k| (k.name.as_str(), k.class_name.as_str()))
        .collect();
    assert_eq!(keys, vec![("hx", "TH1D"), ("hxy", "TH2D")]);
    assert_eq!(TH1::read_root(&f, "hx").expect("read hx"), h1);
    assert_eq!(TH2::read_root(&f, "hxy").expect("read hxy"), h2);
}

#[test]
fn big_container_with_subdirs_round_trips() {
    let out = PathBuf::from("/tmp/oxiroot_big_dirs.root");
    let top = th1("top");
    let inner = th1("inner");

    RootFile::create(&out)
        .add(&top)
        .dir("region_a", |d| d.add(&inner))
        .write_threshold(oxiroot_io_core::Compression::None, 0)
        .expect("write big dirs");

    let f = RFile::open(&out).expect("reopen");
    assert!(f.header().is_big());
    assert_eq!(TH1::read_root(&f, "top").expect("top"), top);
    // The histogram inside the big-format subdirectory reads back too.
    assert_eq!(
        TH1::read_root_in(&f, "region_a", "inner").expect("inner"),
        inner
    );
}

#[test]
fn append_crossing_into_big_round_trips() {
    // A small file, then an append whose result is forced past the (lowered)
    // threshold: the appended file must switch to the 64-bit form in place —
    // keeping the first object untouched — and read back both objects.
    let out = PathBuf::from("/tmp/oxiroot_big_append.root");
    let first = th1("h_first");
    RootFile::create(&out)
        .add(&first)
        .write(oxiroot_io_core::Compression::None) // normal small file
        .expect("write first");
    // Sanity: the base file is small.
    assert!(!RFile::open(&out).unwrap().header().is_big());

    let second = th1("h_second");
    RootFile::open(&out)
        .expect("open")
        .add(&second)
        .write_threshold(oxiroot_io_core::Compression::None, 0) // force big output
        .expect("append big");

    let f = RFile::open(&out).expect("reopen");
    assert!(
        f.header().is_big(),
        "the appended file must be the big form"
    );
    assert_eq!(f.header().units, 8);
    // Both the pre-existing (untouched, small on-disk) key and the appended one
    // read back through the big key list.
    assert_eq!(TH1::read_root(&f, "h_first").expect("first"), first);
    assert_eq!(TH1::read_root(&f, "h_second").expect("second"), second);
}

#[test]
fn append_crossing_into_big_works_for_a_renamed_root_file() {
    // The top directory's reserved record size is measured from the name stored
    // in the file, so a ROOT-written file copied under a longer name can still
    // be widened in place.
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures");
    let out = std::env::temp_dir().join("oxiroot_big_append_a_much_longer_file_name.root");
    std::fs::copy(fixture.join("th1d_uncompressed.root"), &out).expect("copy fixture");
    let original = TH1::read_root(&RFile::open(&out).unwrap(), "h1").expect("fixture h1");

    let extra = th1("extra");
    RootFile::open(&out)
        .expect("open")
        .add(&extra)
        .write_threshold(oxiroot_io_core::Compression::None, 0) // force big
        .expect("append big");

    let f = RFile::open(&out).expect("reopen");
    assert!(f.header().is_big());
    assert_eq!(TH1::read_root(&f, "h1").expect("h1"), original);
    assert_eq!(TH1::read_root(&f, "extra").expect("extra"), extra);
}

#[test]
fn append_to_already_big_file_round_trips() {
    // Appending to a file that is *already* the 64-bit form stays big and keeps
    // every object.
    let out = PathBuf::from("/tmp/oxiroot_big_append2.root");
    let a = th1("a");
    RootFile::create(&out)
        .add(&a)
        .write_threshold(oxiroot_io_core::Compression::None, 0) // big from the start
        .expect("write big base");
    assert!(RFile::open(&out).unwrap().header().is_big());

    let b = th1("b");
    RootFile::open(&out)
        .expect("open")
        .add(&b)
        .write_threshold(oxiroot_io_core::Compression::None, 0)
        .expect("append to big");

    let f = RFile::open(&out).expect("reopen");
    assert!(f.header().is_big());
    assert_eq!(TH1::read_root(&f, "a").expect("a"), a);
    assert_eq!(TH1::read_root(&f, "b").expect("b"), b);
}

#[test]
fn append_crossing_into_big_preserves_existing_subdir() {
    // The point of in-place append: an existing subdirectory (its keys hold
    // absolute offsets) survives the switch to the 64-bit form untouched.
    let out = PathBuf::from("/tmp/oxiroot_big_append_subdir.root");
    let top = th1("top");
    let inner = th1("inner");
    RootFile::create(&out)
        .add(&top)
        .dir("region", |d| d.add(&inner))
        .write(oxiroot_io_core::Compression::None) // small file with a subdir
        .expect("write base");
    assert!(!RFile::open(&out).unwrap().header().is_big());

    let extra = th1("extra");
    RootFile::open(&out)
        .expect("open")
        .add(&extra)
        .write_threshold(oxiroot_io_core::Compression::None, 0) // force big
        .expect("append big");

    let f = RFile::open(&out).expect("reopen");
    assert!(f.header().is_big());
    assert_eq!(TH1::read_root(&f, "top").expect("top"), top);
    assert_eq!(TH1::read_root(&f, "extra").expect("extra"), extra);
    // The untouched subdirectory (at its original offsets) still resolves.
    assert_eq!(
        TH1::read_root_in(&f, "region", "inner").expect("inner"),
        inner
    );
}
