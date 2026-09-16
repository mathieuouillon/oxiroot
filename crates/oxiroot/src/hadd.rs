//! Merging ROOT files — a pure-Rust `hadd`.
//!
//! [`merge_files`] (and the [`Merger`] builder) combine several ROOT files the
//! way ROOT's `hadd` command-line tool does: histograms are summed bin-by-bin,
//! and `TTree` / RNTuple entries are concatenated. It returns a [`MergeReport`]
//! describing exactly what each key became — so nothing is silently dropped.
//!
//! ```no_run
//! use oxiroot::hadd::merge_files;
//! use oxiroot::Compression;
//!
//! let report = merge_files("all.root", &["run1.root", "run2.root"], Compression::Zstd(5))?;
//! println!("{report}");
//! # Ok::<(), oxiroot::Error>(())
//! ```
//!
//! # What a fileset may contain
//!
//! One invocation writes **one** output file. The merger does not yet combine
//! histograms with a `TTree` or RNTuple in one output (a
//! [`RootFile`](oxiroot_io_core::RootFile) can hold all three, but the merger
//! concatenates each tree or RNTuple on its own path). So a fileset must be one
//! of:
//!
//! * **all histogram-family objects** — `TH1`/`TH2`/`TH3` and the 1-, 2- and
//!   3-D profiles are summed; graphs, efficiencies, functions, strings,
//!   matrices, … are copied from the first file; unknown classes are skipped and
//!   reported;
//! * **a single `TTree`** (and nothing else) — entries concatenated;
//! * **a single RNTuple** (and nothing else) — entries concatenated.
//!
//! Anything else — a `TTree` or RNTuple alongside histograms, or more than one
//! of them — is refused with an error that names the keys, rather than writing a
//! partial file. For finer control, merge the pieces yourself with
//! [`merge_histogram_files`], [`oxiroot_tree::concat_trees`], or
//! [`oxiroot_rntuple::concat_ntuples`].

use std::fmt;
use std::path::{Path, PathBuf};

use oxiroot_io_core::error::{Error, Result};
use oxiroot_io_core::{Compression, RFile};

mod histograms;
pub use histograms::{merge_histogram_files, HistMergeOutcome};
use oxiroot_rntuple::{concat_ntuples, RNTuple, ANCHOR_CLASS};
use oxiroot_tree::{concat_trees, TTree};

/// What kind of merge [`merge_files`] performed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MergeKind {
    /// A histogram-family fileset: histograms summed, other objects copied.
    Histograms,
    /// A single `TTree`, its entries concatenated. Holds the tree name.
    Tree(String),
    /// A single RNTuple, its entries concatenated. Holds the RNTuple name.
    RNTuple(String),
}

/// A summary of a [`merge_files`] run: what was written and how.
#[derive(Debug, Clone)]
pub struct MergeReport {
    /// The output file path.
    pub output: PathBuf,
    /// Number of input files merged.
    pub inputs: usize,
    /// Which merge path ran.
    pub kind: MergeKind,
    /// Keys summed / concatenated (histogram names, or the one tree/RNTuple).
    pub merged: Vec<String>,
    /// Keys copied from the first file (histogram path only).
    pub copied: Vec<String>,
    /// Keys skipped, with the reason (histogram path only).
    pub skipped: Vec<(String, String)>,
    /// Total entries written, for a `TTree`/RNTuple merge.
    pub entries: Option<u64>,
}

impl fmt::Display for MergeReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "merged {} file(s) into {}",
            self.inputs,
            self.output.display()
        )?;
        match &self.kind {
            MergeKind::Histograms => {
                write!(
                    f,
                    ": {} summed, {} copied, {} skipped",
                    self.merged.len(),
                    self.copied.len(),
                    self.skipped.len()
                )?;
            }
            MergeKind::Tree(name) | MergeKind::RNTuple(name) => {
                let what = if matches!(self.kind, MergeKind::Tree(_)) {
                    "TTree"
                } else {
                    "RNTuple"
                };
                write!(
                    f,
                    ": {what} {name:?} with {} entries",
                    self.entries.unwrap_or(0)
                )?;
            }
        }
        for (name, reason) in &self.skipped {
            write!(f, "\n  skipped {name:?}: {reason}")?;
        }
        Ok(())
    }
}

/// A builder for a file merge — collect inputs, choose compression, then
/// [`merge`](Merger::merge). Equivalent to [`merge_files`] but composable.
///
/// ```no_run
/// use oxiroot::hadd::Merger;
/// use oxiroot::Compression;
///
/// let report = Merger::new()
///     .input("run1.root")
///     .input("run2.root")
///     .compression(Compression::Zstd(5))
///     .merge("all.root")?;
/// # Ok::<(), oxiroot::Error>(())
/// ```
#[derive(Debug, Clone, Default)]
pub struct Merger {
    inputs: Vec<PathBuf>,
    compression: Option<Compression>,
}

impl Merger {
    /// A new, empty merger (default compression: Zstd level 5).
    #[must_use]
    pub fn new() -> Merger {
        Merger::default()
    }

    /// Add one input file.
    #[must_use]
    pub fn input(mut self, path: impl AsRef<Path>) -> Merger {
        self.inputs.push(path.as_ref().to_path_buf());
        self
    }

    /// Add several input files.
    #[must_use]
    pub fn inputs<P: AsRef<Path>>(mut self, paths: impl IntoIterator<Item = P>) -> Merger {
        self.inputs
            .extend(paths.into_iter().map(|p| p.as_ref().to_path_buf()));
        self
    }

    /// Set the output compression (default: Zstd level 5).
    #[must_use]
    pub fn compression(mut self, compression: Compression) -> Merger {
        self.compression = Some(compression);
        self
    }

    /// Merge the collected inputs into `output`.
    pub fn merge(self, output: impl AsRef<Path>) -> Result<MergeReport> {
        let compression = self.compression.unwrap_or(Compression::Zstd(5));
        merge_files(output, &self.inputs, compression)
    }
}

/// Merge `inputs` into `output`, dispatching on what the fileset contains (see
/// the [module docs](self)). Returns a [`MergeReport`] describing the result.
///
/// # Errors
///
/// Returns an error if `inputs` is empty, if a file cannot be read, if the
/// fileset mixes histograms with a `TTree`/RNTuple (or holds more than one of
/// them), or if the underlying histogram / tree / RNTuple merge fails (e.g.
/// incompatible histogram binnings, or a branch/field type oxiroot cannot write
/// back).
pub fn merge_files<P: AsRef<Path>>(
    output: impl AsRef<Path>,
    inputs: &[P],
    compression: Compression,
) -> Result<MergeReport> {
    let output = output.as_ref();
    if inputs.is_empty() {
        return Err(Error::Format("merge_files: no input files".into()));
    }

    let files: Vec<RFile> = inputs.iter().map(RFile::open).collect::<Result<Vec<_>>>()?;

    // Union of top-level key names (first-seen order) with each key's class.
    let mut seen = std::collections::HashSet::new();
    let mut trees = Vec::new();
    let mut rntuples = Vec::new();
    let mut others = 0usize;
    for file in &files {
        for key in file.keys() {
            if key.is_deleted() || !seen.insert(key.name.clone()) {
                continue;
            }
            match key.class_name.as_str() {
                "TTree" | "TNtuple" | "TNtupleD" => trees.push(key.name.clone()),
                ANCHOR_CLASS => rntuples.push(key.name.clone()),
                _ => others += 1,
            }
        }
    }

    match (trees.len(), rntuples.len(), others) {
        // No trees or RNTuples: a histogram-family fileset.
        (0, 0, _) => merge_histograms(output, &files, inputs.len(), compression),
        // Exactly one TTree and nothing else.
        (1, 0, 0) => merge_tree(output, &files, &trees[0], inputs.len(), compression),
        // Exactly one RNTuple and nothing else.
        (0, 1, 0) => merge_rntuple(output, &files, &rntuples[0], inputs.len(), compression),
        // Anything mixed or plural: refuse loudly rather than write a partial file.
        _ => Err(Error::Format(format!(
            "merge_files: this fileset mixes objects oxiroot cannot combine into one file yet \
             ({} TTree(s): {trees:?}; {} RNTuple(s): {rntuples:?}; {others} other object(s)). \
             oxiroot merges either an all-histogram fileset, a single TTree, or a single RNTuple \
             per call; merge the pieces separately with concat_trees / concat_ntuples / \
             merge_histogram_files.",
            trees.len(),
            rntuples.len(),
        ))),
    }
}

fn merge_histograms(
    output: &Path,
    files: &[RFile],
    inputs: usize,
    compression: Compression,
) -> Result<MergeReport> {
    let outcome = merge_histogram_files(output, files, compression)?;
    Ok(MergeReport {
        output: output.to_path_buf(),
        inputs,
        kind: MergeKind::Histograms,
        merged: outcome.summed,
        copied: outcome.copied,
        skipped: outcome.skipped,
        entries: None,
    })
}

fn merge_tree(
    output: &Path,
    files: &[RFile],
    name: &str,
    inputs: usize,
    compression: Compression,
) -> Result<MergeReport> {
    let trees: Vec<TTree> = files
        .iter()
        .map(|f| TTree::open(f, name))
        .collect::<Result<Vec<_>>>()?;
    let entries = trees.iter().map(TTree::num_entries).sum();
    let pairs: Vec<(&RFile, &TTree)> = files.iter().zip(&trees).collect();

    let merged = concat_trees(&pairs)?;
    merged.write_root(output, compression)?;

    Ok(MergeReport {
        output: output.to_path_buf(),
        inputs,
        kind: MergeKind::Tree(name.to_string()),
        merged: vec![name.to_string()],
        copied: Vec::new(),
        skipped: Vec::new(),
        entries: Some(entries),
    })
}

fn merge_rntuple(
    output: &Path,
    files: &[RFile],
    name: &str,
    inputs: usize,
    compression: Compression,
) -> Result<MergeReport> {
    let ntuples: Vec<RNTuple> = files
        .iter()
        .map(|f| RNTuple::open(f, name))
        .collect::<Result<Vec<_>>>()?;
    let entries = ntuples.iter().map(RNTuple::num_entries).sum();
    let pairs: Vec<(&RFile, &RNTuple)> = files.iter().zip(&ntuples).collect();

    let merged = concat_ntuples(name, &pairs)?;
    merged.write_root(output, compression)?;

    Ok(MergeReport {
        output: output.to_path_buf(),
        inputs,
        kind: MergeKind::RNTuple(name.to_string()),
        merged: vec![name.to_string()],
        copied: Vec::new(),
        skipped: Vec::new(),
        entries: Some(entries),
    })
}
