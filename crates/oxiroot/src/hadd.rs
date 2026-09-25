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
//! [`FileWriter`](oxiroot_io_core::FileWriter) can hold all three, but the merger
//! concatenates each tree or RNTuple on its own path). So a fileset must be one
//! of:
//!
//! * **all histogram-family objects** — everything ROOT's `hadd` merges is
//!   merged the same way (`Hist1D`/`Hist2D`/`Hist3D`, the profiles, `PolyHist` and
//!   `SparseHist` bin by bin, an `Efficiency`'s histograms, a `HistStack`'s
//!   histograms by name, a `Parameter`'s value, and the graphs by appending
//!   their points); what it does not merge is copied from the first file
//!   (`Func1D`/`2`/`3`, `Graph2D`, `MultiErrorGraph`, `GraphStack`, strings,
//!   maps, matrices); unknown classes are skipped and reported;
//! * **a single `TTree`** (and nothing else) — entries concatenated;
//! * **a single RNTuple** (and nothing else) — entries concatenated.
//!
//! Anything else — a `TTree` or RNTuple alongside histograms, or more than one
//! of them — is refused with an error that names the keys, rather than writing a
//! partial file.
//!
//! The inputs are read on demand rather than loaded whole. A tree or RNTuple is
//! streamed to the output one input at a time (one batch of baskets, or one
//! cluster, per input), so memory holds a single input's entries. A large
//! merge is written in ROOT's 64-bit container form. The output must not be
//! one of the inputs. For finer control, merge the pieces yourself with
//! [`merge_histogram_files`], [`oxiroot_tree::concat_trees`], or
//! [`oxiroot_rntuple::concat_ntuples`].

use std::fmt;
use std::path::{Path, PathBuf};

use oxiroot_io_core::{Compression, Error, FileReader, Result, KSTART_BIG_FILE};

mod histograms;
pub use histograms::{merge_histogram_files, HistMergeOutcome};
use oxiroot_rntuple::{append_ntuples, concat_ntuples, NtupleReader, NtupleWriter, ANCHOR_CLASS};
use oxiroot_tree::{append_trees, concat_trees, TreeReader, TreeWriter};

/// Inputs larger than this in total are merged straight into the 64-bit
/// container form; smaller ones switch to it only if the output turns out not to
/// fit the 32-bit form.
const LARGE_INPUT_BYTES: u64 = KSTART_BIG_FILE / 2;

/// What kind of merge [`merge_files`] performed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MergeKind {
    /// A histogram-family fileset: histograms summed, other objects copied.
    Histograms,
    /// A single `TTree`, its entries concatenated. Holds the tree name.
    Tree(String),
    /// A single RNTuple, its entries concatenated. Holds the RNTuple name.
    Ntuple(String),
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
            MergeKind::Tree(name) | MergeKind::Ntuple(name) => {
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
        return Err(Error::InvalidInput("merge_files: no input files".into()));
    }

    // The inputs are read on demand, so the output must not be one of them.
    let out_path = std::fs::canonicalize(output).ok();
    for input in inputs {
        if out_path.is_some() && std::fs::canonicalize(input).ok() == out_path {
            return Err(Error::InvalidInput(format!(
                "merge_files: the output {} is also an input",
                output.display()
            )));
        }
    }
    // Positioned reads: only the objects and data a merge touches are read, one
    // input's worth of tree or RNTuple entries at a time.
    let files: Vec<FileReader> = inputs
        .iter()
        .map(FileReader::open_ranged)
        .collect::<Result<Vec<_>>>()?;
    let large = files.iter().map(FileReader::size).sum::<u64>() > LARGE_INPUT_BYTES;

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
        (1, 0, 0) => merge_tree(output, &files, &trees[0], inputs.len(), compression, large),
        // Exactly one RNTuple and nothing else.
        (0, 1, 0) => merge_rntuple(
            output,
            &files,
            &rntuples[0],
            inputs.len(),
            compression,
            large,
        ),
        // Anything mixed or plural: refuse loudly rather than write a partial file.
        _ => Err(Error::Unsupported(format!(
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
    files: &[FileReader],
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

/// Run `write` in the 32-bit container form unless `large`, and once more in the
/// 64-bit form if the 32-bit file turned out too large.
fn with_form_fallback(large: bool, mut write: impl FnMut(bool) -> Result<()>) -> Result<()> {
    match write(large) {
        Err(Error::FileTooLarge { .. }) if !large => write(true),
        other => other,
    }
}

fn merge_tree(
    output: &Path,
    files: &[FileReader],
    name: &str,
    inputs: usize,
    compression: Compression,
    large: bool,
) -> Result<MergeReport> {
    let trees: Vec<TreeReader> = files
        .iter()
        .map(|f| TreeReader::open(f, name))
        .collect::<Result<Vec<_>>>()?;
    let entries = trees.iter().map(TreeReader::num_entries).sum();
    let pairs: Vec<(&FileReader, &TreeReader)> = files.iter().zip(&trees).collect();

    if entries == 0 {
        // The streaming writer needs at least one entry; an empty tree is small.
        concat_trees(&pairs)?.write_root(output, compression)?;
    } else {
        // Stream one input at a time: one batch of baskets per input.
        let tree_name = trees[0].name();
        with_form_fallback(large, |big| {
            let mut writer = if big {
                TreeWriter::create_large(output, tree_name, compression)?
            } else {
                TreeWriter::create(output, tree_name, compression)?
            };
            append_trees(&mut writer, &pairs)?;
            writer.finish().map(drop)
        })?;
    }

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
    files: &[FileReader],
    name: &str,
    inputs: usize,
    compression: Compression,
    large: bool,
) -> Result<MergeReport> {
    let ntuples: Vec<NtupleReader> = files
        .iter()
        .map(|f| NtupleReader::open(f, name))
        .collect::<Result<Vec<_>>>()?;
    let entries = ntuples.iter().map(NtupleReader::num_entries).sum();
    let pairs: Vec<(&FileReader, &NtupleReader)> = files.iter().zip(&ntuples).collect();

    if entries == 0 {
        // The streaming writer needs at least one entry; an empty RNTuple is small.
        concat_ntuples(name, &pairs)?.write_root(output, compression)?;
    } else {
        // Stream one input at a time: one cluster per input.
        with_form_fallback(large, |big| {
            let mut writer = if big {
                NtupleWriter::create_large(output, name, compression)?
            } else {
                NtupleWriter::create(output, name, compression)?
            };
            append_ntuples(&mut writer, &pairs)?;
            writer.finish()
        })?;
    }

    Ok(MergeReport {
        output: output.to_path_buf(),
        inputs,
        kind: MergeKind::Ntuple(name.to_string()),
        merged: vec![name.to_string()],
        copied: Vec::new(),
        skipped: Vec::new(),
        entries: Some(entries),
    })
}

#[cfg(test)]
mod tests {
    use super::with_form_fallback;
    use oxiroot_io_core::Error;

    #[test]
    fn a_too_large_small_file_is_rewritten_in_the_large_form() {
        let mut forms = Vec::new();
        let result = with_form_fallback(false, |big| {
            forms.push(big);
            if big {
                Ok(())
            } else {
                Err(Error::FileTooLarge { size: 3 << 30 })
            }
        });
        assert!(result.is_ok());
        assert_eq!(forms, [false, true]);
    }

    #[test]
    fn other_errors_and_large_writes_are_not_retried() {
        let mut calls = 0;
        let result = with_form_fallback(false, |_| {
            calls += 1;
            Err(Error::Format("bad branch".into()))
        });
        assert!(result.is_err());
        assert_eq!(calls, 1);

        let mut forms = Vec::new();
        let result = with_form_fallback(true, |big| {
            forms.push(big);
            Err(Error::FileTooLarge { size: 1 })
        });
        assert!(matches!(result, Err(Error::FileTooLarge { .. })));
        assert_eq!(forms, [true]);
    }
}
