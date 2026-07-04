//! A programmatic mini `oxroot ls`: open arbitrary ROOT files we did *not* write
//! (bundled fixtures) and walk their key lists — printing each top-level object's
//! class, on-disk size, cycle and name, then descending one level into any
//! `TDirectory` — using only the `RFile` introspection surface (`keys()`,
//! `subdir()`, and the `TKey` fields). Reads fixtures; writes nothing.
//!
//! ```sh
//! cargo run -p oxiroot --example inspect
//! ```

use oxiroot::file::TKey;
use oxiroot::prelude::*;

/// A ROOT `TDirectory` can appear on disk under either class name (the in-memory
/// class oxiroot writes vs. the on-disk class official ROOT C++ records).
fn is_directory(class: &str) -> bool {
    matches!(class, "TDirectory" | "TDirectoryFile")
}

/// Bytes as a compact human-readable size (mirrors `oxroot stat`'s formatting).
fn human_size(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

/// Print one `ls`-style line for a key, indented by `depth` levels: the columns
/// are class, on-disk size, cycle, then `name  "title"`.
fn print_key(key: &TKey, depth: usize) {
    let indent = "  ".repeat(depth);
    // `total_bytes()` is the whole record (key header + payload) as stored on
    // disk; a title (fTitle) is optional, so only show it when present.
    println!(
        "  {indent}{:<18} {:>8}  cyc{:<3} {}{}",
        key.class_name,
        human_size(u64::from(key.total_bytes())),
        key.cycle,
        key.name,
        if key.title.is_empty() {
            String::new()
        } else {
            format!("  \"{}\"", key.title)
        },
    );
}

/// Walk one file like `oxroot ls`, descending one level into any `TDirectory`,
/// and return `(object_count, total_on_disk_bytes)`.
fn walk(file: &RFile) -> (usize, u64) {
    let (mut count, mut bytes) = (0usize, 0u64);
    // `keys()` yields the raw `TKey` records of the root directory; a negative
    // byte count marks freed space, so skip those, exactly as the CLI does.
    for key in file.keys().iter().filter(|k| !k.is_deleted()) {
        print_key(key, 0);
        count += 1;
        bytes += u64::from(key.total_bytes());

        // Show ONE level of nesting: if this key is a directory, list the keys it
        // directly contains via `subdir()`. (Recursing per level would be a full
        // walk — one level keeps the example focused and matches plain `ls`.)
        if is_directory(&key.class_name) {
            match file.subdir(&key.name) {
                Ok(dir) => {
                    for child in dir.keys.iter().filter(|k| !k.is_deleted()) {
                        print_key(child, 1);
                        count += 1;
                        bytes += u64::from(child.total_bytes());
                    }
                }
                // A directory we can't descend into isn't fatal for a listing.
                Err(e) => println!("    (could not read subdirectory {:?}: {e})", key.name),
            }
        }
    }
    (count, bytes)
}

/// Open one fixture (located relative to this crate, so the example runs from any
/// working directory), print a header, and list its contents.
fn inspect(name: &str, path: &str) -> Result<()> {
    // Opening only parses the file header and the root directory's key list;
    // object bodies are read lazily, on demand.
    let file = RFile::open(path)?;
    let header = file.header();
    println!("{name}");
    println!(
        "  ROOT format v{}, compression code {}",
        header.version, header.compress,
    );
    let (count, bytes) = walk(&file);
    println!("  -> {count} objects, {} on disk", human_size(bytes));
    println!();
    Ok(())
}

fn main() -> Result<()> {
    // `analysis.root`: a flat file of seven histograms — a rich, ordinary
    // listing (the common case: no subdirectories).
    inspect(
        "analysis.root (flat: several top-level objects)",
        concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/analysis.root"),
    )?;

    // `tree_subdir.root`: a file whose only top-level object is a `TDirectory`,
    // so the one-level descent above actually fires and lists the nested keys.
    let nested_path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../fixtures/tree_subdir.root"
    );
    inspect(
        "tree_subdir.root (nested: one TDirectory level)",
        nested_path,
    )?;

    // Beyond listing, the high-level accessors make targeted look-ups trivial.
    // `key()` returns the newest cycle of a name; `TTree::open_in` resolves an
    // object several directories deep — the sort of thing `oxroot ls -l` reports.
    let file = RFile::open(nested_path)?;
    if let Some(top) = file.key("cal") {
        println!(
            "newest cycle of top-level `{}` is cycle {} ({})",
            top.name, top.cycle, top.class_name,
        );
    }
    let events = TTree::open_in(&file, "cal/run2", "Events")?;
    println!(
        "resolved deeply-nested TTree `cal/run2/Events`: {} entries",
        events.num_entries(),
    );

    Ok(())
}
