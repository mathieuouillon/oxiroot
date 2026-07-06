//! `oxroot stat` — a one-screen summary of a ROOT file.

use clap::Args as ClapArgs;

use crate::json::Json;
use crate::util::{compression_label, parse_spec, root_version, CmdResult};

/// Arguments for `oxroot stat`.
#[derive(ClapArgs)]
pub struct Args {
    /// The ROOT file (`file.root`).
    file: String,
}

/// Run `oxroot stat`.
pub fn run(args: Args, json: bool) -> CmdResult {
    let (path, _) = parse_spec(&args.file);
    let file = crate::util::open_root(&path)?;
    // The source's own length works for both local files and remote URLs (where
    // `std::fs::metadata` would fail).
    let size = file.size();
    let header = file.header();
    let keys = file.keys().iter().filter(|k| !k.is_deleted()).count();
    let mut streamers: Vec<(String, i32)> = file
        .streamer_registry()
        .map(|r| {
            r.infos()
                .iter()
                .map(|i| (i.class_name.clone(), i.class_version))
                .collect()
        })
        .unwrap_or_default();
    streamers.sort();

    if json {
        let streamers = Json::Array(
            streamers
                .iter()
                .map(|(class, version)| {
                    Json::Object(vec![
                        ("class", Json::s(class.clone())),
                        ("version", Json::Int(i64::from(*version))),
                    ])
                })
                .collect(),
        );
        let out = Json::Object(vec![
            ("file", Json::s(path)),
            ("size_bytes", Json::Int(size as i64)),
            ("root_version", Json::s(root_version(header.version))),
            ("compression", Json::s(compression_label(header.compress))),
            ("objects", Json::Int(keys as i64)),
            ("streamers", streamers),
        ]);
        println!("{}", out.render());
        return Ok(());
    }

    println!("file         {path}");
    println!("size         {}", human_size(size));
    println!("ROOT version {}", root_version(header.version));
    println!("compression  {}", compression_label(header.compress));
    println!("objects      {keys}");
    if !streamers.is_empty() {
        println!("streamers    {} classes", streamers.len());
        for (class, version) in &streamers {
            println!("    {class} (v{version})");
        }
    }
    Ok(())
}

/// Bytes as a compact human-readable size.
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
        format!("{value:.1} {} ({bytes} B)", UNITS[unit])
    }
}
