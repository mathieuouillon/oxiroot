//! Serializing a `TH1D` to ROOT's on-disk object layout.
//!
//! Reproduces the exact byte layout ROOT writes (validated by byte-comparison
//! against a ROOT-written fixture), filling the data-bearing members from a
//! [`TH1`] and the cosmetic/auxiliary members with ROOT's defaults.

use std::path::Path;

use root_io_core::buffer::WBuffer;
use root_io_core::streamer::{write_tnamed, write_tobject};
use root_io_core::{write_root_file, ObjectRecord};

use crate::axis::TAxis;
use crate::th1::TH1;

/// Write a single `TH1D` into a new ROOT file at `path`. `compression` is a
/// ROOT setting (`algorithm*100 + level`, 0 = none; e.g. 505 = Zstd level 5).
pub fn write_th1d_file(path: &Path, h: &TH1, compression: u32) -> std::io::Result<()> {
    let file_name = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("file.root");
    let record = ObjectRecord {
        class_name: "TH1D".to_string(),
        name: h.name.clone(),
        title: h.title.clone(),
        object: th1d_to_bytes(h),
    };
    std::fs::write(path, write_root_file(file_name, &[record], compression))
}

// `fBits` values ROOT writes for the embedded TObjects in a fresh histogram.
const HIST_BITS: u32 = 0x0300_0008;
const AXIS_BITS: u32 = 0x0300_0000;
const TLIST_BITS: u32 = 0x0301_0000;

/// Serialize a `TH1D` object (including its leading byte-count/version header)
/// into `w`, byte-for-byte as ROOT writes it.
pub fn write_th1d(w: &mut WBuffer, h: &TH1) {
    let th1d = w.begin_object(3); // TH1D version 3
    write_th1_base(w, h);
    write_tarrayd(w, &h.contents); // TArrayD base: bin contents, inline
    w.end_object(th1d);
}

/// Serialize a `TH1D` object to a fresh byte vector.
pub fn th1d_to_bytes(h: &TH1) -> Vec<u8> {
    let mut w = WBuffer::new();
    write_th1d(&mut w, h);
    w.into_vec()
}

fn write_th1_base(w: &mut WBuffer, h: &TH1) {
    let th1 = w.begin_object(8); // TH1 version 8

    write_tnamed(w, HIST_BITS, &h.name, &h.title);
    write_attline(w);
    write_attfill(w);
    write_attmarker(w);

    w.be_i32(h.ncells);
    write_taxis(w, &h.xaxis);
    write_taxis(w, &h.yaxis);
    write_taxis(w, &h.zaxis);
    w.be_i16(0); // fBarOffset
    w.be_i16(1000); // fBarWidth
    w.be_f64(h.entries);
    w.be_f64(h.tsumw);
    w.be_f64(h.tsumw2);
    w.be_f64(h.tsumwx);
    w.be_f64(h.tsumwx2);
    w.be_f64(-1111.0); // fMaximum
    w.be_f64(-1111.0); // fMinimum
    w.be_f64(0.0); // fNormFactor
    write_tarrayd(w, &[]); // fContour
    write_tarrayd(w, &[]); // fSumw2
    w.string(""); // fOption
    write_empty_tlist(w); // fFunctions
    w.be_i32(0); // fBufferSize
    w.u8(0); // fBuffer (null pointer-to-array marker)
    w.be_i32(0); // fBinStatErrOpt
    w.be_i32(2); // fStatOverflows

    w.end_object(th1);
}

fn write_attline(w: &mut WBuffer) {
    let t = w.begin_object(2);
    w.be_i16(602); // fLineColor
    w.be_i16(1); // fLineStyle
    w.be_i16(1); // fLineWidth
    w.end_object(t);
}

fn write_attfill(w: &mut WBuffer) {
    let t = w.begin_object(2);
    w.be_i16(0); // fFillColor
    w.be_i16(1001); // fFillStyle
    w.end_object(t);
}

fn write_attmarker(w: &mut WBuffer) {
    let t = w.begin_object(2);
    w.be_i16(1); // fMarkerColor
    w.be_i16(1); // fMarkerStyle
    w.be_f32(1.0); // fMarkerSize
    w.end_object(t);
}

fn write_taxis(w: &mut WBuffer, ax: &TAxis) {
    let t = w.begin_object(10); // TAxis version 10
    write_tnamed(w, AXIS_BITS, &ax.name, &ax.title);

    // TAttAxis base (drawing defaults).
    let att = w.begin_object(4);
    w.be_i32(510); // fNdivisions
    w.be_i16(1); // fAxisColor
    w.be_i16(1); // fLabelColor
    w.be_i16(42); // fLabelFont
    w.be_f32(0.005); // fLabelOffset
    w.be_f32(0.035); // fLabelSize
    w.be_f32(0.03); // fTickLength
    w.be_f32(1.0); // fTitleOffset
    w.be_f32(0.035); // fTitleSize
    w.be_i16(1); // fTitleColor
    w.be_i16(42); // fTitleFont
    w.end_object(att);

    w.be_i32(ax.nbins);
    w.be_f64(ax.xmin);
    w.be_f64(ax.xmax);
    write_tarrayd(w, &ax.xbins); // fXbins
    w.be_i32(0); // fFirst
    w.be_i32(0); // fLast
    w.be_u16(0); // fBits2
    w.u8(0); // fTimeDisplay
    w.string(""); // fTimeFormat
    w.be_u32(0); // fLabels (null THashList*)
    w.be_u32(0); // fModLabs (null TList*)
    w.end_object(t);
}

fn write_empty_tlist(w: &mut WBuffer) {
    let t = w.begin_object(5); // TList version 5
    write_tobject(w, TLIST_BITS);
    w.string(""); // fName
    w.be_i32(0); // fSize
    w.end_object(t);
}

/// Write a `TArrayD` base inline (a count followed by that many doubles).
fn write_tarrayd(w: &mut WBuffer, data: &[f64]) {
    w.be_i32(data.len() as i32);
    for &d in data {
        w.be_f64(d);
    }
}
