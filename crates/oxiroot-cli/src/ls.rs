//! `oxroot ls` — list the objects (keys) in a file.

use clap::Args as ClapArgs;
use oxiroot::file::TKey;
use oxiroot::ntuple::RNTuple;
use oxiroot::tree::TTree;
use oxiroot::RFile;

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

/// Run `oxroot ls`.
pub fn run(args: Args) -> CmdResult {
    let (path, _) = parse_spec(&args.file);
    let file = RFile::open(&path)?;

    let mut table = if args.long {
        Table::new(&["name", "class", "title", "cycle", "entries"]).right_align(&[3, 4])
    } else {
        Table::new(&["name", "class", "title"])
    };

    add_rows(&file, "", file.keys(), args.long, &mut table);

    if args.recursive {
        for key in file.keys().iter().filter(|k| !k.is_deleted()) {
            if matches!(key.class_name.as_str(), "TDirectory" | "TDirectoryFile") {
                if let Ok(dir) = file.subdir(&key.name) {
                    add_rows(
                        &file,
                        &format!("{}/", key.name),
                        &dir.keys,
                        args.long,
                        &mut table,
                    );
                }
            }
        }
    }

    table.print();
    Ok(())
}

/// Append one row per non-deleted key, names prefixed with `prefix`.
fn add_rows(file: &RFile, prefix: &str, keys: &[TKey], long: bool, table: &mut Table) {
    for key in keys.iter().filter(|k| !k.is_deleted()) {
        let name = format!("{prefix}{}", key.name);
        if long {
            table.row(vec![
                name,
                key.class_name.clone(),
                key.title.clone(),
                key.cycle.to_string(),
                entry_count(file, &key.class_name, &key.name),
            ]);
        } else {
            table.row(vec![name, key.class_name.clone(), key.title.clone()]);
        }
    }
}

/// The entry count of a `TTree`/RNTuple key (a dash for anything else, or if it
/// cannot be opened).
fn entry_count(file: &RFile, class: &str, name: &str) -> String {
    let dash = || "-".to_string();
    match classify(class) {
        Kind::Tree => {
            TTree::open(file, name).map_or_else(|_| dash(), |t| t.num_entries().to_string())
        }
        Kind::RNtuple => {
            RNTuple::open(file, name).map_or_else(|_| dash(), |n| n.num_entries().to_string())
        }
        _ => dash(),
    }
}
