//! Item 4: write histograms organized into subdirectories, then read them back
//! through our reader (and, separately, validate with official ROOT + uproot).

use std::path::PathBuf;

use oxiroot_hist::{FileWriter, Hist, ReadRoot, TH1};
use oxiroot_io_core::FileReader;

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
    FileWriter::create(&out)
        .add(&top)
        .dir("signal", |d| d.add(&sr))
        .dir("control", |d| d.add(&cr))
        .write(oxiroot_io_core::Compression::None)
        .expect("write");

    let f = FileReader::open(&out).expect("reopen");

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

/// Directories nest as deep as they are written: `dir` inside `dir`, with a
/// sibling at the second level. ROOT and uproot read every level of the file
/// this test writes (checked by hand against ROOT 6.40 and uproot 5.7.1; the
/// interop workflow covers it in CI).
#[test]
fn writes_directories_nested_several_levels_deep() {
    let named = |name: &str, weight: f64| {
        let mut h = Hist::reg(1, 0.0, 1.0).double().named(name);
        h.fill_weight(0.5, weight);
        h
    };
    let (top, a, b, c, sibling) = (
        named("top", 1.0),
        named("in_a", 2.0),
        named("in_b", 3.0),
        named("in_c", 4.0),
        named("in_sibling", 5.0),
    );

    let out = PathBuf::from("/tmp/oxiroot_nested_dirs.root");
    FileWriter::create(&out)
        .add(&top)
        .dir("a", |d| {
            d.add(&a)
                .dir("b", |d| d.add(&b).dir("c", |d| d.add(&c)))
                .dir("sibling", |d| d.add(&sibling))
        })
        .write(oxiroot_io_core::Compression::None)
        .expect("write");

    let f = FileReader::open(&out).expect("reopen");
    assert_eq!(TH1::read_root(&f, "top").unwrap(), top);
    assert_eq!(TH1::read_root_in(&f, "a", "in_a").unwrap(), a);
    assert_eq!(TH1::read_root_in(&f, "a/b", "in_b").unwrap(), b);
    assert_eq!(TH1::read_root_in(&f, "a/b/c", "in_c").unwrap(), c);
    assert_eq!(
        TH1::read_root_in(&f, "a/sibling", "in_sibling").unwrap(),
        sibling
    );

    // Each level lists what it holds: its objects and the directories below it.
    let names = |path: &str| {
        let mut names: Vec<String> = f
            .subdir(path)
            .expect("subdir")
            .keys
            .iter()
            .map(|k| format!("{}:{}", k.name, k.class_name))
            .collect();
        names.sort();
        names
    };
    assert_eq!(
        names("a"),
        ["b:TDirectory", "in_a:TH1D", "sibling:TDirectory"]
    );
    assert_eq!(names("a/b"), ["c:TDirectory", "in_b:TH1D"]);
    assert_eq!(names("a/b/c"), ["in_c:TH1D"]);
}

/// A name clash inside a nested directory is reported with the path that names
/// it, not silently written for a reader to trip over.
#[test]
fn a_clash_inside_a_nested_directory_names_its_path() {
    let h = Hist::reg(1, 0.0, 1.0).double().named("same");
    let err = FileWriter::create("/tmp/oxiroot_nested_clash.root")
        .dir("a", |d| d.dir("b", |d| d.add(&h).add(&h)))
        .to_bytes(oxiroot_io_core::Compression::None)
        .expect_err("a duplicate name must be refused");
    let message = err.to_string();
    assert!(message.contains("same") && message.contains("a/b"), "{err}");
}
