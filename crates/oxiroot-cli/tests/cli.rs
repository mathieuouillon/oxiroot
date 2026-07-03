//! Smoke tests for the `oxroot` binary, run over the shared fixtures.

use std::path::PathBuf;
use std::process::Command;

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures")
        .join(name)
}

/// Run `oxroot` with `args`; return its stdout and whether it succeeded.
fn oxroot(args: &[&str]) -> (String, bool) {
    let out = Command::new(env!("CARGO_BIN_EXE_oxroot"))
        .args(args)
        .output()
        .expect("run oxroot");
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        out.status.success(),
    )
}

fn spec(name: &str, object: &str) -> String {
    format!("{}:{object}", fixture(name).display())
}

#[test]
fn stat_reports_version_and_streamers() {
    let (out, ok) = oxroot(&["stat", fixture("tree_flat.root").to_str().unwrap()]);
    assert!(ok, "{out}");
    assert!(out.contains("ROOT version 6."), "{out}");
    assert!(out.contains("streamers"), "{out}");
}

#[test]
fn show_lists_tree_branches_with_types() {
    let (out, ok) = oxroot(&["show", &spec("tree_flat.root", "Events")]);
    assert!(ok, "{out}");
    assert!(out.contains("6 branches"), "{out}");
    assert!(out.contains("f8") && out.contains("double"), "{out}");
}

#[test]
fn show_lists_rntuple_fields_with_types() {
    let (out, ok) = oxroot(&["show", &spec("rntuple_scalars_uncompressed.root", "ntpl")]);
    assert!(ok, "{out}");
    assert!(out.contains("fields"), "{out}");
    assert!(out.contains("std::vector<float>"), "{out}");
}

#[test]
fn dump_prints_tree_entries() {
    let (out, ok) = oxroot(&["dump", &spec("tree_flat.root", "Events"), "-n", "2"]);
    assert!(ok, "{out}");
    assert!(out.contains("showing 2"), "{out}");
    // The header row lists the branches.
    assert!(out.contains("i4") && out.contains("f8"), "{out}");
}

#[test]
fn dump_histogram_shows_stats_and_bins() {
    let (out, ok) = oxroot(&["dump", &spec("th1d_uncompressed.root", "h1")]);
    assert!(ok, "{out}");
    assert!(out.contains("mean") && out.contains("integral"), "{out}");
    assert!(out.contains("content"), "{out}");
}

#[test]
fn ls_lists_keys() {
    let (out, ok) = oxroot(&["ls", fixture("graphs.root").to_str().unwrap()]);
    assert!(ok, "{out}");
    assert!(out.contains("TGraphErrors"), "{out}");
}

#[test]
fn ls_recurse_descends_into_every_subdirectory() {
    // fixtures/tree_subdir.root nests the tree three levels deep: cal/run2/Events.
    let (out, ok) = oxroot(&["ls", fixture("tree_subdir.root").to_str().unwrap(), "-r"]);
    assert!(ok, "{out}");
    assert!(out.contains("cal/run2/Events"), "{out}");

    // With -l, the nested tree's entry count is resolved (5), not a dash.
    let (out, ok) = oxroot(&[
        "ls",
        fixture("tree_subdir.root").to_str().unwrap(),
        "-l",
        "-r",
    ]);
    assert!(ok, "{out}");
    let line = out
        .lines()
        .find(|l| l.starts_with("cal/run2/Events"))
        .unwrap_or("");
    assert!(line.contains('5'), "{out}");
}

#[test]
fn a_missing_object_is_an_error() {
    let (_out, ok) = oxroot(&["show", &spec("tree_flat.root", "NoSuch")]);
    assert!(!ok);
}

#[test]
fn json_mode_emits_json() {
    // stat: a JSON object with the file summary.
    let (out, ok) = oxroot(&[
        "stat",
        fixture("tree_flat.root").to_str().unwrap(),
        "--json",
    ]);
    assert!(ok, "{out}");
    assert!(out.trim_start().starts_with('{'), "{out}");
    assert!(
        out.contains("\"root_version\"") && out.contains("\"streamers\""),
        "{out}"
    );

    // dump: typed rows with a columns list.
    let (out, ok) = oxroot(&[
        "dump",
        &spec("tree_flat.root", "Events"),
        "-n",
        "2",
        "--json",
    ]);
    assert!(ok, "{out}");
    assert!(
        out.contains("\"columns\"") && out.contains("\"rows\""),
        "{out}"
    );

    // ls: a JSON array.
    let (out, ok) = oxroot(&["ls", fixture("graphs.root").to_str().unwrap(), "--json"]);
    assert!(ok, "{out}");
    assert!(out.trim_start().starts_with('['), "{out}");
}

#[test]
fn dump_shows_a_function_and_falls_back_for_undumpable_classes() {
    // A TF1: formula + params.
    let (out, ok) = oxroot(&["dump", &spec("tf1.root", "myfunc")]);
    assert!(ok, "{out}");
    assert!(out.contains("formula") && out.contains("params"), "{out}");

    // A readable-but-undumpable class reports itself instead of erroring.
    let (out, ok) = oxroot(&["dump", &spec("thnsparse.root", "hs")]);
    assert!(ok, "{out}");
    assert!(out.contains("no dedicated `dump` view"), "{out}");
}

#[test]
fn shows_and_dumps_a_tree_in_a_nested_subdirectory() {
    // fixtures/tree_subdir.root holds the tree at cal/run2/Events.
    let (out, ok) = oxroot(&["show", &spec("tree_subdir.root", "cal/run2/Events")]);
    assert!(ok, "{out}");
    assert!(out.contains("2 branches"), "{out}");

    let (out, ok) = oxroot(&[
        "dump",
        &spec("tree_subdir.root", "cal/run2/Events"),
        "-n",
        "2",
    ]);
    assert!(ok, "{out}");
    assert!(out.contains("showing 2"), "{out}");
}
