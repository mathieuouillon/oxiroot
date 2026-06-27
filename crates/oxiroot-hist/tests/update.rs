//! M6: append histograms to an existing ROOT file (update mode). The rewritten
//! file must hold the original objects plus the new ones, readable by our reader
//! (and, as separately verified, by official ROOT and uproot).

use std::path::PathBuf;

use oxiroot_hist::{ReadRoot, RootFile, WriteRoot, TH1, TH2};
use oxiroot_io_core::RFile;

#[test]
fn appends_objects_to_an_existing_file() {
    let out = PathBuf::from("/tmp/rootrs_update.root");

    // Start with a one-histogram file.
    let mut a = TH1::new("a", "first", 4, 0.0, 4.0);
    a.fill(0.5);
    a.fill(2.5);
    a.write_root(&out, oxiroot_io_core::Compression::None)
        .expect("initial write");

    // Append two more histograms (a TH1D and a TH2D).
    let mut b = TH1::new("b", "second", 3, 0.0, 3.0);
    b.fill(1.5);
    let mut c = TH2::new("c", "third", 2, 0.0, 2.0, 2, 0.0, 2.0);
    c.fill(0.5, 1.5);
    RootFile::open(&out)
        .expect("open for append")
        .add(&b)
        .add(&c)
        .write(oxiroot_io_core::Compression::None)
        .expect("append");

    // All three are present and intact.
    let f = RFile::open(&out).expect("reopen");
    let names: Vec<&str> = f.keys().iter().map(|k| k.name.as_str()).collect();
    assert!(
        names.contains(&"a") && names.contains(&"b") && names.contains(&"c"),
        "{names:?}"
    );
    assert_eq!(TH1::read_root(&f, "a").unwrap(), a, "original survived");
    assert_eq!(TH1::read_root(&f, "b").unwrap(), b);
    assert_eq!(TH2::read_root(&f, "c").unwrap(), c);

    // The embedded streamer info is preserved across the update.
    let reg = f.streamer_registry().expect("streamer info");
    assert!(reg.class_names().contains(&"TH2D"));
}

#[test]
fn re_adding_a_name_bumps_the_cycle() {
    let out = PathBuf::from("/tmp/rootrs_update_cycle.root");
    let mut v1 = TH1::new("h", "v1", 4, 0.0, 4.0);
    v1.fill(0.5);
    v1.write_root(&out, oxiroot_io_core::Compression::None)
        .expect("write v1");

    // Re-add "h" with different contents; ROOT keeps both at different cycles,
    // newest (highest cycle) wins for a plain lookup.
    let mut v2 = TH1::new("h", "v2", 4, 0.0, 4.0);
    v2.fill(1.5);
    v2.fill(1.5);
    RootFile::open(&out)
        .expect("open for append")
        .add(&v2)
        .write(oxiroot_io_core::Compression::None)
        .expect("append v2");

    let f = RFile::open(&out).expect("reopen");
    let cycles: Vec<u16> = f
        .keys()
        .iter()
        .filter(|k| k.name == "h")
        .map(|k| k.cycle)
        .collect();
    assert_eq!(cycles.len(), 2, "both cycles present: {cycles:?}");
    assert!(cycles.contains(&1) && cycles.contains(&2), "{cycles:?}");
    // Our reader returns the highest cycle (newest) -> v2.
    assert_eq!(TH1::read_root(&f, "h").unwrap(), v2, "newest cycle wins");
}
