//! `TAxis` — a histogram axis.

use root_io_core::buffer::RBuffer;
use root_io_core::error::Result;
use root_io_core::streamer::{read_tnamed, skip_versioned};

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
