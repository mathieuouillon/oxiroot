//! `THnSparse` — an N-dimensional *sparse* histogram. Only filled bins are
//! stored: ROOT keeps them in `THnSparseArrayChunk`s, each holding the filled
//! bins' packed global coordinates (`char*`) and contents (`TArrayD`). On disk
//! the class is `THnSparseT<TArrayD>` → `THnSparse` → `THnBase`.
//!
//! The compact coordinate of a filled bin is its global linear index across all
//! axes, with `bits[d] = bitlength(nbins[d] + 2)` bits per axis (so axis `d`'s
//! stride is `2^Σ_{e<d} bits[e]`), packed big-endian into
//! `fSingleCoordinateSize = ceil(Σ bits / 8)` bytes.

use oxiroot_io_core::buffer::RBuffer;
use oxiroot_io_core::error::Result;
use oxiroot_io_core::streamer::{read_tnamed, read_tobject};
use oxiroot_io_core::RFile;

use crate::axis::TAxis;
use crate::base::{object_bytes, object_bytes_in, read_tarray, Precision};

/// A filled cell: one (per-axis, flow-inclusive) bin index per dimension, and its
/// content.
#[derive(Debug, Clone, PartialEq)]
pub struct SparseBin {
    /// Per-axis bin index (0 = underflow … `nbins+1` = overflow) for each dimension.
    pub coords: Vec<i32>,
    /// Bin content.
    pub content: f64,
}

/// An N-dimensional sparse histogram (ROOT `THnSparse`/`THnSparseT<TArrayD>`).
#[derive(Debug, Clone, PartialEq)]
pub struct THnSparse {
    /// Name (`fName`).
    pub name: String,
    /// Title (`fTitle`).
    pub title: String,
    /// One axis per dimension (`fAxes`).
    pub axes: Vec<TAxis>,
    /// Number of entries (`fEntries`).
    pub entries: f64,
    /// Sum of weights (`fTsumw`).
    pub tsumw: f64,
    /// Sum of squared weights (`fTsumw2`; ROOT writes `-1` when not tracked).
    pub tsumw2: f64,
    /// Per-axis `Σ(w·x)` (`fTsumwx`).
    pub tsumwx: Vec<f64>,
    /// Per-axis `Σ(w·x²)` (`fTsumwx2`).
    pub tsumwx2: Vec<f64>,
    /// The filled cells (sparse storage).
    pub bins: Vec<SparseBin>,
}

impl THnSparse {
    /// Create an empty `THnSparse` over the given per-dimension uniform axes,
    /// `(nbins, xmin, xmax)`.
    pub fn new(name: &str, title: &str, axes: &[(i32, f64, f64)]) -> THnSparse {
        let n = axes.len();
        THnSparse {
            name: name.to_string(),
            title: title.to_string(),
            axes: axes
                .iter()
                .enumerate()
                .map(|(i, &(nb, lo, hi))| TAxis::new(&format!("axis{i}"), nb, lo, hi))
                .collect(),
            entries: 0.0,
            tsumw: 0.0,
            tsumw2: -1.0, // ROOT's "moments not tracked" marker
            tsumwx: vec![0.0; n],
            tsumwx2: vec![0.0; n],
            bins: Vec::new(),
        }
    }

    /// Number of dimensions.
    pub fn ndim(&self) -> usize {
        self.axes.len()
    }

    /// Fill the cell containing `coords` (one value per dimension) with unit
    /// weight, accumulating into an existing filled cell or creating a new one.
    pub fn fill(&mut self, coords: &[f64]) {
        let bins: Vec<i32> = self
            .axes
            .iter()
            .zip(coords)
            .map(|(ax, &x)| ax.find_bin(x) as i32)
            .collect();
        self.entries += 1.0;
        if let Some(b) = self.bins.iter_mut().find(|b| b.coords == bins) {
            b.content += 1.0;
        } else {
            self.bins.push(SparseBin {
                coords: bins,
                content: 1.0,
            });
        }
    }

    /// Per-axis bit widths used to pack a compact coordinate
    /// (`bits[d] = bitlength(nbins[d] + 2)`).
    pub(crate) fn axis_bits(&self) -> Vec<u32> {
        self.axes
            .iter()
            .map(|ax| {
                let cells = (ax.nbins.max(0) + 2) as u32;
                u32::BITS - cells.leading_zeros()
            })
            .collect()
    }

    /// Pack per-axis bins into the compact global coordinate.
    pub(crate) fn pack(&self, bins: &[i32], bits: &[u32]) -> u64 {
        let mut coord = 0u64;
        let mut shift = 0u32;
        for (&b, &nb) in bins.iter().zip(bits) {
            coord |= (b as u64) << shift;
            shift += nb;
        }
        coord
    }

    /// Unpack a compact global coordinate into per-axis bins.
    fn unpack(&self, mut coord: u64, bits: &[u32]) -> Vec<i32> {
        bits.iter()
            .map(|&nb| {
                let mask = (1u64 << nb) - 1;
                let b = (coord & mask) as i32;
                coord >>= nb;
                b
            })
            .collect()
    }

    pub(crate) fn read(r: &mut RBuffer) -> Result<THnSparse> {
        let tt = r.read_version()?; // THnSparseT<TArrayD> wrapper
        let _ts = r.read_version()?; // THnSparse v3
        let thnbase = r.read_version()?; // THnBase v1

        let named = read_tnamed(r)?;
        let ndim = r.be_i32()? as usize;
        let axes = read_axis_array(r)?;
        let entries = r.be_f64()?;
        let tsumw = r.be_f64()?;
        let tsumw2 = r.be_f64()?;
        let tsumwx = read_tarray(r, Precision::Double)?;
        let tsumwx2 = read_tarray(r, Precision::Double)?;
        if let Some(end) = thnbase.end {
            r.seek(end)?;
        }

        let _chunk_size = r.be_i32()?;
        let _filled_bins = r.be_i64()?;
        let bins = read_bin_content(r, ndim)?;

        if let Some(end) = tt.end {
            r.seek(end)?;
        }

        let mut h = THnSparse {
            name: named.name,
            title: named.title,
            axes,
            entries,
            tsumw,
            tsumw2,
            tsumwx,
            tsumwx2,
            bins: Vec::new(),
        };
        let bit_widths = h.axis_bits();
        h.bins = bins
            .into_iter()
            .map(|(coord, content)| SparseBin {
                coords: h.unpack(coord, &bit_widths),
                content,
            })
            .collect();
        Ok(h)
    }
}

/// Consume an object-pointer header (`{byte count}{class tag}`), leaving the
/// cursor at the object body. Returns `false` for a null pointer.
fn enter_object(r: &mut RBuffer) -> Result<bool> {
    let bc = r.be_i32()? as u32;
    if bc == 0 {
        return Ok(false); // null pointer
    }
    let tag = r.be_i32()? as u32;
    if tag == 0xFFFF_FFFF {
        while r.u8()? != 0 {} // kNewClassTag: NUL-terminated class name
    }
    Ok(true)
}

/// Read a `TObjArray` header, returning the element count.
fn read_objarray_len(r: &mut RBuffer) -> Result<usize> {
    let _oa = r.read_version()?; // TObjArray version 3
    read_tobject(r)?;
    let _name = r.string()?;
    let size = r.be_i32()? as usize;
    let _lower_bound = r.be_i32()?;
    Ok(size)
}

/// Read `fAxes`: a `TObjArray` of `TAxis` object pointers.
fn read_axis_array(r: &mut RBuffer) -> Result<Vec<TAxis>> {
    let n = read_objarray_len(r)?;
    (0..n)
        .map(|_| {
            enter_object(r)?;
            TAxis::read(r)
        })
        .collect()
}

/// Read `fBinContent`: a `TObjArray` of `THnSparseArrayChunk`. Returns each
/// filled bin as `(compact coordinate, content)`.
fn read_bin_content(r: &mut RBuffer, _ndim: usize) -> Result<Vec<(u64, f64)>> {
    let nchunks = read_objarray_len(r)?;
    let mut out = Vec::new();
    for _ in 0..nchunks {
        if !enter_object(r)? {
            continue;
        }
        let _chunk = r.read_version()?; // THnSparseArrayChunk v1
        read_tobject(r)?;
        let single = r.be_i32()? as usize; // fSingleCoordinateSize
        let coords_size = r.be_i32()? as usize; // fCoordinatesSize
        let _marker = r.u8()?; // char* presence flag
        let coords = r.bytes(coords_size)?.to_vec();
        // fContent: a TArrayD* of the contents (sized to the chunk capacity).
        let content = if enter_object(r)? {
            read_tarray(r, Precision::Double)?
        } else {
            Vec::new()
        };
        // fSumw2: a TArrayD* (often null); skip it.
        if enter_object(r)? {
            read_tarray(r, Precision::Double)?;
        }
        let n_filled = coords_size.checked_div(single).unwrap_or(0);
        for i in 0..n_filled {
            let mut c = 0u64;
            for b in &coords[i * single..(i + 1) * single] {
                c = (c << 8) | *b as u64; // big-endian
            }
            out.push((c, content.get(i).copied().unwrap_or(0.0)));
        }
    }
    Ok(out)
}

/// Read a `THnSparse` named `name` from `file`.
pub(crate) fn read_thnsparse(file: &RFile, name: &str) -> Result<THnSparse> {
    THnSparse::read(&mut RBuffer::new(&object_bytes(
        file,
        name,
        "THnSparseT<TArrayD>",
    )?))
}

/// Read a `THnSparseT<TArrayD>` from subdirectory `subdir`.
pub(crate) fn read_thnsparse_in(file: &RFile, subdir: &str, name: &str) -> Result<THnSparse> {
    THnSparse::read(&mut RBuffer::new(&object_bytes_in(
        file,
        subdir,
        name,
        "THnSparseT<TArrayD>",
    )?))
}
