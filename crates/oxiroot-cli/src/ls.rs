//! `oxroot ls` — list the objects (keys) in a file.

use clap::Args as ClapArgs;
use oxiroot::file::TKey;
use oxiroot::ntuple::RNTuple;
use oxiroot::tree::TTree;
use oxiroot::RFile;

use crate::json::Json;
use crate::util::{classify, parse_spec, CmdResult, Kind, Table};

/// Arguments for `oxroot ls`.
#[derive(ClapArgs)]
pub struct Args {
    /// The ROOT file (`file.root`).
    file: String,
    /// Long listing: also show each object's cycle and entry count.
    #[arg(short, long)]
    long: bool,
    /// Recurse into every `TDirectory`, showing full `dir/sub/name` paths.
    #[arg(short, long)]
    recursive: bool,
}

/// A guard against runaway recursion on a crafted file (ROOT directory trees are
/// acyclic, so real files never approach this).
const MAX_DEPTH: usize = 64;

/// One listed object.
struct Row {
    name: String,
    class: String,
    title: String,
    cycle: u16,
    entries: Option<u64>,
}

/// Run `oxroot ls`.
pub fn run(args: Args, json: bool) -> CmdResult {
    let (path, _) = parse_spec(&args.file);
    let file = RFile::open(&path)?;
    let want_entries = args.long || json;

    let mut rows = Vec::new();
    collect(
        &file,
        "",
        file.keys(),
        want_entries,
        args.recursive,
        0,
        &mut rows,
    );

    if json {
        let array = Json::Array(
            rows.iter()
                .map(|r| {
                    Json::Object(vec![
                        ("name", Json::s(r.name.clone())),
                        ("class", Json::s(r.class.clone())),
                        ("title", Json::s(r.title.clone())),
                        ("cycle", Json::Int(i64::from(r.cycle))),
                        (
                            "entries",
                            r.entries.map_or(Json::Null, |e| Json::Int(e as i64)),
                        ),
                    ])
                })
                .collect(),
        );
        println!("{}", array.render());
        return Ok(());
    }

    let mut table = if args.long {
        Table::new(&["name", "class", "title", "cycle", "entries"]).right_align(&[3, 4])
    } else {
        Table::new(&["name", "class", "title"])
    };
    for r in &rows {
        if args.long {
            table.row(vec![
                r.name.clone(),
                r.class.clone(),
                r.title.clone(),
                r.cycle.to_string(),
                r.entries.map_or_else(|| "-".to_string(), |e| e.to_string()),
            ]);
        } else {
            table.row(vec![r.name.clone(), r.class.clone(), r.title.clone()]);
        }
    }
    table.print();
    Ok(())
}

/// Append one [`Row`] per non-deleted key in `dir_path` (`""` for the root
/// directory); with `recursive`, descend into every `TDirectory`, naming keys by
/// their full `dir/sub/name` path.
fn collect(
    file: &RFile,
    dir_path: &str,
    keys: &[TKey],
    want_entries: bool,
    recursive: bool,
    depth: usize,
    rows: &mut Vec<Row>,
) {
    for key in keys.iter().filter(|k| !k.is_deleted()) {
        let full = if dir_path.is_empty() {
            key.name.clone()
        } else {
            format!("{dir_path}/{}", key.name)
        };
        rows.push(Row {
            name: full.clone(),
            class: key.class_name.clone(),
            title: key.title.clone(),
            cycle: key.cycle,
            entries: want_entries
                .then(|| entry_count(file, &key.class_name, &key.name, dir_path))
                .flatten(),
        });
        if recursive
            && depth < MAX_DEPTH
            && matches!(key.class_name.as_str(), "TDirectory" | "TDirectoryFile")
        {
            if let Ok(sub) = file.subdir(&full) {
                collect(
                    file,
                    &full,
                    &sub.keys,
                    want_entries,
                    recursive,
                    depth + 1,
                    rows,
                );
            }
        }
    }
}

/// The entry count of a `TTree`/RNTuple key named `leaf` inside `dir_path`
/// (`""` for the root directory), or `None` for any other class.
fn entry_count(file: &RFile, class: &str, leaf: &str, dir_path: &str) -> Option<u64> {
    match classify(class) {
        Kind::Tree => open_tree(file, dir_path, leaf).map(|t| t.num_entries()),
        Kind::RNtuple => open_ntuple(file, dir_path, leaf).map(|n| n.num_entries()),
        _ => None,
    }
}

/// Open a `TTree` from the root directory or a subdirectory (`Ok`s only).
fn open_tree(file: &RFile, dir_path: &str, leaf: &str) -> Option<TTree> {
    if dir_path.is_empty() {
        TTree::open(file, leaf)
    } else {
        TTree::open_in(file, dir_path, leaf)
    }
    .ok()
}

/// Open an RNTuple from the root directory or a subdirectory (`Ok`s only).
fn open_ntuple(file: &RFile, dir_path: &str, leaf: &str) -> Option<RNTuple> {
    if dir_path.is_empty() {
        RNTuple::open(file, leaf)
    } else {
        RNTuple::open_in(file, dir_path, leaf)
    }
    .ok()
}
