//! `oxroot` — a command-line inspector for ROOT files.
//!
//! Look into a `.root` file without ROOT or Python: list its objects, show the
//! structure of a `TTree` or RNTuple, dump entries / bins / points, or print a
//! file summary. Objects are addressed as `file.root:name` (or
//! `file.root:subdir/name`).

use std::process::ExitCode;

use clap::{Parser, Subcommand};

mod dump;
mod ls;
mod show;
mod stat;
mod util;

#[derive(Parser)]
#[command(
    name = "oxroot",
    about = "Inspect ROOT files — keys, TTree/RNTuple structure, and data.",
    version
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// List the objects (keys) in a file.
    Ls(ls::Args),
    /// Show the structure of a TTree or RNTuple.
    Show(show::Args),
    /// Print an object's data — TTree/RNTuple entries, histogram bins, graph
    /// points, or a scalar value.
    Dump(dump::Args),
    /// Summarize the file — size, compression, and streamer info.
    Stat(stat::Args),
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let result = match cli.command {
        Command::Ls(args) => ls::run(args),
        Command::Show(args) => show::run(args),
        Command::Dump(args) => dump::run(args),
        Command::Stat(args) => stat::run(args),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("oxroot: {e}");
            ExitCode::FAILURE
        }
    }
}
