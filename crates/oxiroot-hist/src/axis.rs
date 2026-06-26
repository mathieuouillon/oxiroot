//! `TAxis` — a histogram axis.

use oxiroot_io_core::buffer::RBuffer;
use oxiroot_io_core::error::Result;
use oxiroot_io_core::streamer::{read_tnamed, read_tobject, skip_versioned};

/// A ROOT histogram axis (`TAxis`).
#[derive(Debug, Clone, PartialEq)]
pub struct TAxis {
    /// Axis name (`fName`, e.g. "xaxis").
    pub name: String,
    /// Axis title (`fTitle`).
    pub title: String,
    /// Number of bins (`fNbins`).
    pub nbins: i32,
    /// Low edge of the axis range (`fXmin`).
    pub xmin: f64,
    /// High edge of the axis range (`fXmax`).
    pub xmax: f64,
    /// Variable bin edges (`fXbins`); empty when the axis is uniform.
    pub xbins: Vec<f64>,
    /// Alphanumeric bin labels (`fLabels`), one per bin (`labels[i]` labels bin
    /// `i + 1`). Empty for an ordinary numeric axis; an unlabelled bin in an
    /// otherwise-labelled axis holds an empty string.
    pub labels: Vec<String>,
}

impl TAxis {
    /// Create a uniform axis of `nbins` bins over `[xmin, xmax)`.
    pub fn new(name: &str, nbins: i32, xmin: f64, xmax: f64) -> TAxis {
        TAxis {
            name: name.to_string(),
            title: String::new(),
            nbins,
            xmin,
            xmax,
            xbins: Vec::new(),
            labels: Vec::new(),
        }
    }

    /// Create a variable-width axis from `edges` (the `nbins + 1` bin
    /// boundaries, ascending). Panics if fewer than two edges are given or if
    /// they are not strictly ascending (which would silently corrupt binning).
    pub fn variable(name: &str, edges: &[f64]) -> TAxis {
        assert!(edges.len() >= 2, "a variable axis needs at least two edges");
        assert!(
            edges.windows(2).all(|w| w[0] < w[1]),
            "variable axis edges must be strictly ascending"
        );
        TAxis {
            name: name.to_string(),
            title: String::new(),
            nbins: (edges.len() - 1) as i32,
            xmin: edges[0],
            xmax: edges[edges.len() - 1],
            xbins: edges.to_vec(),
            labels: Vec::new(),
        }
    }

    /// Find the bin for value `x`: 0 = underflow, `1..=nbins` = in range,
    /// `nbins + 1` = overflow. Handles uniform and variable-width axes. `NaN`
    /// goes to overflow, matching ROOT's `TAxis::FindBin`.
    pub fn find_bin(&self, x: f64) -> usize {
        let n = self.nbins.max(0) as usize;
        if n == 0 {
            return 0;
        }
        if x < self.xmin {
            return 0;
        }
        // Route `NaN` to overflow (ROOT does the same): a plain `x >= xmax`
        // would let it fall through to the in-range arithmetic below.
        if x.is_nan() || x >= self.xmax {
            return n + 1;
        }
        if self.xbins.is_empty() {
            let width = (self.xmax - self.xmin) / n as f64;
            // `x` is in range here, so clamp to `n` (the last in-range bin):
            // float rounding at the top edge must never spill into overflow.
            (1 + ((x - self.xmin) / width).floor() as usize).min(n)
        } else {
            // Variable edges: largest i with xbins[i] <= x, giving bin i+1.
            match self.xbins.partition_point(|&edge| edge <= x) {
                0 => 0,
                i => i.min(n),
            }
        }
    }

    /// Whether two axes describe the same binning — identical bin count and
    /// edges, ignoring name/title. This is the precondition for bin-by-bin
    /// histogram arithmetic (`add`/`multiply`/`divide`) and merging.
    pub fn same_binning(&self, other: &TAxis) -> bool {
        self.nbins == other.nbins && self.edges() == other.edges()
    }

    /// Read a `TAxis` from `r` (positioned at the axis's `{byte-count, version}`
    /// header), leaving the cursor at the axis's end.
    pub fn read(r: &mut RBuffer) -> Result<TAxis> {
        let vh = r.read_version()?; // TAxis (e.g. version 10)
        let named = read_tnamed(r)?; // TNamed base
        skip_versioned(r)?; // TAttAxis base (drawing attributes — not needed)

        let nbins = r.be_i32()?;
        let xmin = r.be_f64()?;
        let xmax = r.be_f64()?;

        // fXbins is a TArrayD member: a count followed by that many doubles.
        // Cap the reservation at the buffer size so a forged count can't drive a
        // huge allocation before the element reads fail.
        let n = r.be_i32()?.max(0) as usize;
        let mut xbins = Vec::with_capacity(n.min(r.remaining()));
        for _ in 0..n {
            xbins.push(r.be_f64()?);
        }

        // fFirst, fLast, fBits2, fTimeDisplay, fTimeFormat precede the labels.
        let _first = r.be_i32()?;
        let _last = r.be_i32()?;
        let _bits2 = r.be_u16()?;
        let _time_display = r.u8()?;
        let _time_format = r.string()?;
        let labels = read_labels(r, nbins.max(0) as usize)?; // fLabels (THashList*)

        // Skip the remainder (fModLabs) via the axis byte count.
        if let Some(end) = vh.end {
            r.seek(end)?;
        }

        Ok(TAxis {
            name: named.name,
            title: named.title,
            nbins,
            xmin,
            xmax,
            xbins,
            labels,
        })
    }

    /// Alphanumeric label for bin `bin` (1-based), or `None` for an unlabelled
    /// bin / numeric axis.
    pub fn bin_label(&self, bin: usize) -> Option<&str> {
        self.labels
            .get(bin.checked_sub(1)?)
            .filter(|s| !s.is_empty())
            .map(String::as_str)
    }

    /// The 1-based bin carrying label `label`, if any.
    pub fn find_label(&self, label: &str) -> Option<usize> {
        self.labels.iter().position(|l| l == label).map(|i| i + 1)
    }

    /// Whether the axis carries alphanumeric bin labels (`fLabels`).
    pub fn is_labelled(&self) -> bool {
        self.labels.iter().any(|l| !l.is_empty())
    }

    /// The `nbins + 1` bin edges, low to high (uniform when `xbins` is empty).
    pub fn edges(&self) -> Vec<f64> {
        if !self.xbins.is_empty() {
            return self.xbins.clone();
        }
        let n = self.nbins.max(0) as usize;
        if n == 0 {
            return vec![self.xmin, self.xmax];
        }
        let step = (self.xmax - self.xmin) / n as f64;
        (0..=n).map(|i| self.xmin + step * i as f64).collect()
    }

    /// Low edge of bin `bin` (1-based; bin 1 starts at `xmin`). Out-of-range bins
    /// clamp to the nearest edge.
    pub fn bin_low_edge(&self, bin: usize) -> f64 {
        let e = self.edges();
        e[bin.clamp(1, e.len().max(1)) - 1]
    }

    /// Width of bin `bin` (1-based). `0.0` for an out-of-range bin index.
    pub fn bin_width(&self, bin: usize) -> f64 {
        let e = self.edges();
        if (1..e.len()).contains(&bin) {
            e[bin] - e[bin - 1]
        } else {
            0.0
        }
    }

    /// Center of bin `bin` (1-based). `0.0` for an out-of-range bin index.
    pub fn bin_center(&self, bin: usize) -> f64 {
        let e = self.edges();
        if (1..e.len()).contains(&bin) {
            0.5 * (e[bin - 1] + e[bin])
        } else {
            0.0
        }
    }
}

/// Consume an object pointer's class tag (already past the byte count): a
/// `kNewClassTag` (`0xFFFFFFFF`) is followed by a NUL-terminated class name; a
/// class back-reference is the bare 4-byte tag. We never need to *resolve* the
/// class — context fixes the type — so we just advance past whichever form.
fn skip_class_tag(r: &mut RBuffer) -> Result<()> {
    if r.be_u32()? == 0xFFFF_FFFF {
        while r.u8()? != 0 {}
    }
    Ok(())
}

/// Read the `fLabels` member: a `THashList*` of `TObjString`, each carrying its
/// 1-based bin number in `fUniqueID` and the label text in `fString`. Returns a
/// `Vec` of length `nbins` (empty strings for unlabelled bins), or empty when
/// the pointer is null (an ordinary numeric axis).
fn read_labels(r: &mut RBuffer, nbins: usize) -> Result<Vec<String>> {
    if r.be_u32()? == 0 {
        return Ok(Vec::new()); // null fLabels pointer
    }
    skip_class_tag(r)?; // THashList class tag
    r.read_version()?; // THashList (a TList, version 5)
    read_tobject(r)?;
    let _name = r.string()?; // fName (empty)
    let size = r.be_i32()?.max(0) as usize;

    let mut labels = vec![String::new(); nbins];
    for _ in 0..size {
        if r.be_u32()? == 0 {
            continue; // null entry
        }
        skip_class_tag(r)?; // TObjString class tag
        let body = r.read_version()?; // TObjString body {byte count, version}
        let obj = read_tobject(r)?; // fUniqueID = 1-based bin number
        let label = r.string()?; // fString
        let bin = obj.unique_id as usize;
        if (1..=nbins).contains(&bin) {
            labels[bin - 1] = label;
        }
        if let Some(end) = body.end {
            r.seek(end)?;
        }
        let _option = r.string()?; // per-element option string (empty), as in TList
    }
    if labels.iter().all(String::is_empty) {
        return Ok(Vec::new());
    }
    Ok(labels)
}
