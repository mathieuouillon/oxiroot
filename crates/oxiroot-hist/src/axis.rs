//! `TAxis` — a histogram axis.

use oxiroot_io_core::buffer::RBuffer;
use oxiroot_io_core::error::Result;
use oxiroot_io_core::streamer::{read_tnamed, skip_versioned};

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
        let n = r.be_i32()?.max(0) as usize;
        let mut xbins = Vec::with_capacity(n);
        for _ in 0..n {
            xbins.push(r.be_f64()?);
        }

        // Skip the rest (fFirst, fLast, fBits2, fTimeDisplay, fTimeFormat,
        // fLabels, fModLabs) via the axis byte count.
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
        })
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
}
