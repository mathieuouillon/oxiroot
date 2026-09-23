//! Deleting, purging and compacting in update mode (`FileWriter::open`).

use std::path::PathBuf;

use oxiroot_hist::{FileWriter, Hist, ReadRoot, TH1};
use oxiroot_io_core::{Compression, FileReader};

fn hist(name: &str, content: f64) -> TH1 {
    let mut h = Hist::reg(1, 0.0, 1.0).double().named(name);
    h.fill_weight(0.5, content);
    h
}

/// `name;cycle` for every key on file, in the order the directory lists them.
fn keys(path: &PathBuf) -> Vec<String> {
    FileReader::open(path)
        .expect("reopen")
        .keys()
        .iter()
        .filter(|k| !k.is_deleted())
        .map(|k| format!("{};{}", k.name, k.cycle))
        .collect()
}

/// A file with three cycles of `h`, plus `keep` and `scratch`.
fn cycles_file(path: &PathBuf) {
    FileWriter::create(path)
        .add(&hist("h", 1.0))
        .add(&hist("keep", 100.0))
        .add(&hist("scratch", 7.0))
        .write(Compression::None)
        .expect("write");
    for cycle in 2..=3 {
        FileWriter::open(path)
            .expect("open")
            .add(&hist("h", f64::from(cycle)))
            .write(Compression::None)
            .expect("append");
    }
}

#[test]
fn deletes_one_cycle_and_a_whole_name() {
    let out = std::env::temp_dir().join("oxiroot_delete_keys.root");
    cycles_file(&out);
    assert_eq!(keys(&out), ["h;1", "keep;1", "scratch;1", "h;2", "h;3"]);

    FileWriter::open(&out)
        .expect("open")
        .delete("h;1") // one cycle
        .delete("scratch") // every cycle of a name
        .write(Compression::None)
        .expect("delete");

    assert_eq!(keys(&out), ["keep;1", "h;2", "h;3"]);
    // What is left still reads, and the newest cycle is still what a name means.
    let f = FileReader::open(&out).unwrap();
    assert_eq!(TH1::read_root(&f, "h").unwrap().integral(), 3.0);
    assert_eq!(TH1::read_root(&f, "h;2").unwrap().integral(), 2.0);
    assert!(TH1::read_root(&f, "scratch").is_err());
    assert!(TH1::read_root(&f, "h;1").is_err());
}

#[test]
fn purge_keeps_the_current_cycle_of_each_name() {
    let out = std::env::temp_dir().join("oxiroot_purge_keys.root");
    cycles_file(&out);

    FileWriter::open(&out)
        .expect("open")
        .purge()
        .write(Compression::None)
        .expect("purge");

    assert_eq!(keys(&out), ["keep;1", "scratch;1", "h;3"]);
    let f = FileReader::open(&out).unwrap();
    assert_eq!(TH1::read_root(&f, "h").unwrap().integral(), 3.0);
}

#[test]
fn an_object_written_after_a_delete_takes_the_next_free_cycle() {
    let out = std::env::temp_dir().join("oxiroot_delete_then_add.root");
    cycles_file(&out);

    FileWriter::open(&out)
        .expect("open")
        .delete("h;3")
        .add(&hist("h", 9.0))
        .write(Compression::None)
        .expect("delete and add");

    // h;3 went, so the added one takes 3 again — the newest is what was added.
    assert_eq!(keys(&out), ["h;1", "keep;1", "scratch;1", "h;2", "h;3"]);
    let f = FileReader::open(&out).unwrap();
    assert_eq!(TH1::read_root(&f, "h").unwrap().integral(), 9.0);
}

#[test]
fn compact_gives_up_the_space_the_dropped_objects_held() {
    let out = std::env::temp_dir().join("oxiroot_compact.root");
    cycles_file(&out);
    let before = std::fs::metadata(&out).unwrap().len();

    FileWriter::open(&out)
        .expect("open")
        .purge()
        .compact()
        .write(Compression::None)
        .expect("compact");

    let after = std::fs::metadata(&out).unwrap().len();
    assert!(
        after < before,
        "compacted {after} is not smaller than {before}"
    );
    assert_eq!(keys(&out), ["keep;1", "scratch;1", "h;1"]);
    let f = FileReader::open(&out).unwrap();
    assert_eq!(TH1::read_root(&f, "h").unwrap().integral(), 3.0);
    assert_eq!(TH1::read_root(&f, "keep").unwrap().integral(), 100.0);
    // The rewritten file describes the classes it holds, so it reads on its own.
    assert!(f.get_value("h").unwrap().class().is_some());
}

#[test]
fn compact_keeps_the_subdirectories() {
    let out = std::env::temp_dir().join("oxiroot_compact_dirs.root");
    FileWriter::create(&out)
        .add(&hist("top", 1.0))
        .dir("a", |d| {
            d.add(&hist("in_a", 2.0))
                .dir("b", |d| d.add(&hist("in_b", 3.0)))
        })
        .write(Compression::None)
        .expect("write");

    FileWriter::open(&out)
        .expect("open")
        .compact()
        .write(Compression::None)
        .expect("compact");

    let f = FileReader::open(&out).unwrap();
    assert_eq!(TH1::read_root(&f, "top").unwrap().integral(), 1.0);
    assert_eq!(TH1::read_root_in(&f, "a", "in_a").unwrap().integral(), 2.0);
    assert_eq!(
        TH1::read_root_in(&f, "a/b", "in_b").unwrap().integral(),
        3.0
    );
}

#[test]
fn what_cannot_be_done_is_refused() {
    let out = std::env::temp_dir().join("oxiroot_delete_refusals.root");
    FileWriter::create(&out)
        .add(&hist("h", 1.0))
        .write(Compression::None)
        .expect("write");

    // A name the file does not hold: a typo is not a quiet no-op.
    let err = FileWriter::open(&out)
        .unwrap()
        .delete("nope")
        .write(Compression::None)
        .expect_err("deleting a missing key must be refused");
    assert!(err.to_string().contains("nope"), "{err}");

    // Deleting and compacting need a file to work on.
    let fresh = std::env::temp_dir().join("oxiroot_delete_on_create.root");
    let err = FileWriter::create(&fresh)
        .add(&hist("h", 1.0))
        .delete("h")
        .write(Compression::None)
        .expect_err("delete on a fresh file must be refused");
    assert!(err.to_string().contains("FileWriter::open"), "{err}");
}
