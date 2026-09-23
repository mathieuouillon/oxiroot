//! Merging histogram files — the histogram half of [`merge_files`](super::merge_files).
//!
//! [`merge_histogram_files`] combines several ROOT files whose keys are all
//! histogram-family objects. Everything ROOT's `hadd` merges is merged the same
//! way: `TH1`/`TH2`/`TH3`, the `TProfile`s, `TH2Poly` and `THnSparse` bin by
//! bin, a `TEfficiency`'s passed and total histograms, a `THStack`'s histograms
//! by name, a `TParameter`'s value, and the graphs by appending their points.
//! What ROOT's `hadd` does not merge is copied from the first file that holds
//! it: `TF1`/`TF2`/`TF3`, `TGraph2D`, `TGraphMultiErrors`, `TMultiGraph`,
//! strings, maps and matrices. (ROOT writes one key per input for those, which
//! oxiroot cannot do: it rejects two objects of the same name in one directory.)
//! Objects of a class oxiroot cannot read *and* write are skipped and listed in
//! the returned report rather than silently dropped.
//!
//! The output is written through the same typed [`FileWriter`] builder as any
//! other oxiroot write, so its `TStreamerInfo` matches the bytes exactly (no
//! reliance on the inputs' streamer versions). [`merge_files`](super::merge_files)
//! uses it when a fileset contains no `TTree` or RNTuple.

use std::collections::HashSet;
use std::path::Path;

use oxiroot_io_core::{Compression, Error, FileReader, Result};

use oxiroot_linalg::{TMatrixD, TMatrixDSym, TVectorD};

use oxiroot_hist::{
    FileWriter, Mergeable, ReadRoot, TEfficiency, TGraph, TGraph2D, TGraphMultiErrors, TH2Poly,
    THStack, THnSparse, TMap, TMultiGraph, TObjString, TParameter, TProfile, TProfile2D,
    TProfile3D, WriteRoot, TH1, TH2, TH3,
};
use oxiroot_hist_func::{TF1, TF2, TF3};

/// What [`merge_histogram_files`] did with each key: the names that were summed
/// bin-by-bin, the names copied from the first file, and the `(name, reason)`
/// pairs skipped because oxiroot cannot merge or reproduce that class.
#[derive(Debug, Clone, Default)]
pub struct HistMergeOutcome {
    /// Keys summed across all inputs (`TH1`/`TH2`/`TH3` and the 1-, 2- and 3-D
    /// profiles).
    pub summed: Vec<String>,
    /// Keys copied from the first file that holds them (not summed).
    pub copied: Vec<String>,
    /// Keys skipped, with the reason (an unmergeable / unreadable class).
    pub skipped: Vec<(String, String)>,
}

/// The dimension of a summable histogram class (`TH1D` → 1, `TH2F` → 2,
/// `TH3I` → 3), or `None` for anything else (including `TH2Poly`, `THnSparse`,
/// and `THStack`, which are not summed here).
fn summable_hist_dim(class: &str) -> Option<u8> {
    let b = class.as_bytes();
    if b.len() == 4 && &b[..2] == b"TH" && matches!(b[3], b'C' | b'S' | b'I' | b'F' | b'D' | b'L') {
        return match b[2] {
            b'1' => Some(1),
            b'2' => Some(2),
            b'3' => Some(3),
            _ => None,
        };
    }
    None
}

/// Merge histogram-family files into `output`.
///
/// The union of the inputs' top-level keys (in first-seen order) is merged:
/// each summable histogram is added across every input that holds it (keeping
/// the first file's name, title, and binning), and every other supported object
/// is taken from the first file that holds it. `inputs` are already-opened files
/// (the caller opens them); it must be non-empty.
///
/// # Errors
///
/// Returns an error if two histograms with the same key have incompatible
/// binning (as ROOT's `hadd` also refuses; the error names the key), or if
/// writing the output fails. Classes oxiroot cannot merge, and objects it cannot
/// read from any input, are recorded in the returned
/// [`HistMergeOutcome::skipped`] rather than erroring; a key is never written as
/// a partial sum.
pub fn merge_histogram_files(
    output: &Path,
    inputs: &[FileReader],
    compression: Compression,
) -> Result<HistMergeOutcome> {
    // Union of top-level key names, in first-seen order, with the class of the
    // first file that holds each.
    let mut order: Vec<(String, String)> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    for file in inputs {
        for key in file.keys() {
            if key.is_deleted() {
                continue;
            }
            if seen.insert(key.name.clone()) {
                order.push((key.name.clone(), key.class_name.clone()));
            }
        }
    }

    let mut outcome = HistMergeOutcome::default();
    let mut objects: Vec<Box<dyn WriteRoot>> = Vec::new();

    for (name, class) in &order {
        // Inputs that hold this key with a matching class, in file order.
        let contributors: Vec<&FileReader> = inputs
            .iter()
            .filter(|f| f.key(name).is_some_and(|k| k.class_name == *class))
            .collect();
        if contributors.is_empty() {
            continue;
        }

        match build_object(class, name, &contributors)? {
            Built::Summed(obj) => {
                outcome.summed.push(name.clone());
                objects.push(obj);
            }
            Built::Copied(obj) => {
                outcome.copied.push(name.clone());
                objects.push(obj);
            }
            Built::Skipped(reason) => outcome.skipped.push((name.clone(), reason)),
        }
    }

    let mut file = FileWriter::create(output);
    for obj in &objects {
        file = file.add(&**obj);
    }
    file.write(compression)?;

    Ok(outcome)
}

/// The result of merging one key.
enum Built {
    Summed(Box<dyn WriteRoot>),
    Copied(Box<dyn WriteRoot>),
    Skipped(String),
}

/// Sum a summable histogram type across `contributors` (first as the
/// accumulator), or copy a supported non-summable type from the first
/// contributor. Unknown classes become [`Built::Skipped`].
fn build_object(class: &str, name: &str, contributors: &[&FileReader]) -> Result<Built> {
    // Add every contributor, read as `$T`, onto the first. If any of them cannot
    // be read, the whole key is skipped with the reason, as `copied!` does, so
    // one unreadable object neither aborts the merge nor leaves a partial sum.
    macro_rules! summed {
        ($T:ty) => {{
            let mut acc: Option<$T> = None;
            let mut unreadable = None;
            for (i, f) in contributors.iter().enumerate() {
                let h = match <$T>::read_root(f, name) {
                    Ok(h) => h,
                    Err(e) => {
                        unreadable = Some(format!(
                            "cannot read {} from input {} of {}: {e}",
                            stringify!($T),
                            i + 1,
                            contributors.len()
                        ));
                        break;
                    }
                };
                match acc.as_mut() {
                    None => acc = Some(h),
                    // Through `Mergeable`, so a type that cannot be merged
                    // cannot be routed here.
                    Some(a) => a.merge(&h).map_err(|e| with_key(name, e))?,
                }
            }
            match (unreadable, acc) {
                (Some(reason), _) => Built::Skipped(reason),
                (None, Some(a)) => Built::Summed(Box::new(a)),
                (None, None) => Built::Skipped("no input holds it".into()),
            }
        }};
    }
    // Copy `$T` from the first contributor; a read failure becomes a skip so one
    // odd object never aborts the whole merge.
    macro_rules! copied {
        ($T:ty) => {
            match <$T>::read_root(contributors[0], name) {
                Ok(obj) => Built::Copied(Box::new(obj)),
                Err(e) => Built::Skipped(format!("cannot read {}: {e}", stringify!($T))),
            }
        };
    }

    if let Some(dim) = summable_hist_dim(class) {
        return Ok(match dim {
            1 => summed!(TH1),
            2 => summed!(TH2),
            _ => summed!(TH3),
        });
    }

    let built = match class {
        "TProfile" => summed!(TProfile),
        "TProfile2D" => summed!(TProfile2D),
        "TProfile3D" => summed!(TProfile3D),
        // Summed, as ROOT's hadd does: the efficiency's two histograms, the
        // poly's bins, the sparse histogram's filled bins, the graphs' points
        // (appended), the stack's histograms by name, and the parameter's value.
        "TEfficiency" => summed!(TEfficiency),
        "TH2Poly" => summed!(TH2Poly),
        "TGraph" | "TGraphErrors" | "TGraphAsymmErrors" => summed!(TGraph),
        "THStack" => summed!(THStack),
        c if c.starts_with("THnSparse") => summed!(THnSparse),
        c if c.starts_with("TParameter") => summed!(TParameter),
        // Copied from the first file: ROOT's hadd does not merge these either.
        // It writes one key per input instead, which oxiroot cannot do, since
        // it rejects two objects of the same name in one directory.
        "TF1" => copied!(TF1),
        "TF2" => copied!(TF2),
        "TF3" => copied!(TF3),
        "TGraph2D" => copied!(TGraph2D),
        // ROOT 6.40's hadd crashes merging this one.
        "TGraphMultiErrors" => copied!(TGraphMultiErrors),
        "TObjString" => copied!(TObjString),
        "TMultiGraph" => copied!(TMultiGraph),
        "TMap" => copied!(TMap),
        c if c.starts_with("TVectorT") => copied!(TVectorD),
        c if c.starts_with("TMatrixTSym") => copied!(TMatrixDSym),
        c if c.starts_with("TMatrixT") => copied!(TMatrixD),
        other => Built::Skipped(format!("class {other:?} is not mergeable by oxiroot")),
    };
    Ok(built)
}

/// Name the key a binning mismatch came from, so a failed merge says which
/// object was incompatible.
fn with_key(name: &str, e: Error) -> Error {
    match e {
        Error::BinningMismatch { detail } => Error::BinningMismatch {
            detail: format!("{name:?}: {detail}"),
        },
        other => other,
    }
}
