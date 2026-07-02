//! `oxroot stat` — a one-screen summary of a ROOT file.

use clap::Args as ClapArgs;
use oxiroot::RFile;

use crate::util::{compression_label, parse_spec, root_version, CmdResult};

/// Arguments for `oxroot stat`.
#[derive(ClapArgs)]
pub struct Args {
    /// The ROOT file (`file.root`).
    file: String,
}

/// Run `oxroot stat`.
pub fn run(args: Args) -> CmdResult {
    let (path, _) = parse_spec(&args.file);
    let size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
    let file = RFile::open(&path)?;
    let header = file.header();
    let keys = file.keys().iter().filter(|k| !k.is_deleted()).count();

    println!("file         {path}");
    println!("size         {}", human_size(size));
    println!("ROOT version {}", root_version(header.version));
    println!("compression  {}", compression_label(header.compress));
    println!("objects      {keys}");

    if let Ok(registry) = file.streamer_registry() {
        let infos = registry.infos();
        if !infos.is_empty() {
            println!("streamers    {} classes", infos.len());
            let mut classes: Vec<String> = infos
                .iter()
                .map(|i| format!("{} (v{})", i.class_name, i.class_version))
                .collect();
            classes.sort();
            for c in classes {
                println!("    {c}");
            }
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
