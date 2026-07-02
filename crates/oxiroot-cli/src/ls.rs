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
    /// Recurse one level into `TDirectory` subdirectories.
    #[arg(short, long)]
    recursive: bool,
}

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
    collect(&file, "", file.keys(), want_entries, &mut rows);
    if args.recursive {
        for key in file.keys().iter().filter(|k| !k.is_deleted()) {
            if matches!(key.class_name.as_str(), "TDirectory" | "TDirectoryFile") {
                if let Ok(dir) = file.subdir(&key.name) {
                    collect(
                        &file,
                        &format!("{}/", key.name),
                        &dir.keys,
                        want_entries,
                        &mut rows,
                    );
                }
            }
        }
    }

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

/// Append one [`Row`] per non-deleted key, names prefixed with `prefix`.
fn collect(file: &RFile, prefix: &str, keys: &[TKey], want_entries: bool, rows: &mut Vec<Row>) {
    for key in keys.iter().filter(|k| !k.is_deleted()) {
        rows.push(Row {
            name: format!("{prefix}{}", key.name),
            class: key.class_name.clone(),
            title: key.title.clone(),
            cycle: key.cycle,
            entries: want_entries
                .then(|| entry_count(file, &key.class_name, &key.name))
                .flatten(),
        });
    }
}

/// The entry count of a top-level `TTree`/RNTuple key, or `None` otherwise.
fn entry_count(file: &RFile, class: &str, name: &str) -> Option<u64> {
    match classify(class) {
        Kind::Tree => TTree::open(file, name).ok().map(|t| t.num_entries()),
        Kind::RNtuple => RNTuple::open(file, name).ok().map(|n| n.num_entries()),
        _ => None,
    }
}
