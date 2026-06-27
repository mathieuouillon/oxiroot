//! `TH2Poly` — a 2-D histogram whose bins are arbitrary polygons rather than a
//! regular grid.
//!
//! On disk a `TH2Poly` (v3) is a `TH2` base (bounding-box axes + global
//! statistics) followed by a spatial lookup grid (`fCells`) and the master bin
//! list (`fBins`). The polygon bins (`TH2PolyBin`) are **written in full the
//! first time they appear inside `fCells`**; every later reference — in another
//! grid cell, or in `fBins` — is an *object back-reference* into ROOT's
//! reference map. Reading the bins therefore goes through
//! [`TagReader`], the shared machinery that
//! resolves ROOT's `{byte-count, class-tag}` object protocol; a plain
//! offset-skipping reader can't, because it has no position→object map.
//!
//! `fCells` is a `TStreamerLoop` (`TList* //[fNCells]`): a 6-byte
//! `{byte-count, version}` header, then `fNCells` inline `TList` objects. Each
//! cell `TList` holds object pointers to the `TH2PolyBin`s overlapping that
//! cell, so walking `fCells` collects every bin exactly once (the first, full
//! occurrence). Each bin's `fPoly` is a `TGraph` giving the polygon vertices.
//!
//! **Writing** (via the [`WriteRoot`](crate::WriteRoot) trait) takes the
//! simpler-but-valid route: the bins are written in full inside `fBins` and the
//! `fCells` grid is left empty,
//! so there are no shared objects and hence no back-references to emit. ROOT and
//! uproot read the result correctly (verified bit-for-bit against ROOT C++); the
//! only thing not reconstructed is the spatial fast-fill grid. The reader handles
//! both layouts — when `fCells` yields no bins it falls back to reading `fBins`.

use oxiroot_io_core::buffer::RBuffer;
use oxiroot_io_core::error::{Error, Result};
use oxiroot_io_core::object::TagReader;
use oxiroot_io_core::streamer::{read_tobject, skip_versioned};
use oxiroot_io_core::RFile;

use crate::axis::TAxis;
use crate::base::{object_bytes_keyed, read_th1_base};

/// One polygon bin of a [`TH2Poly`] (ROOT `TH2PolyBin`).
#[derive(Debug, Clone, PartialEq)]
pub struct PolyBin {
    /// ROOT bin number (`fNumber`, 1-based in fill order).
    pub number: i32,
    /// Bin content (`fContent`).
    pub content: f64,
    /// Polygon area (`fArea`; ROOT computes it lazily, so often `0`).
    pub area: f64,
    /// Bounding-box minimum x (`fXmin`).
    pub xmin: f64,
    /// Bounding-box minimum y (`fYmin`).
    pub ymin: f64,
    /// Bounding-box maximum x (`fXmax`).
    pub xmax: f64,
    /// Bounding-box maximum y (`fYmax`).
    pub ymax: f64,
    /// Polygon vertex x coordinates (`fPoly`'s `TGraph` `fX`).
    pub x: Vec<f64>,
    /// Polygon vertex y coordinates (`fPoly`'s `TGraph` `fY`).
    pub y: Vec<f64>,
}

impl PolyBin {
    /// The polygon's geometric area via the shoelace formula.
    ///
    /// ROOT stores [`area`](Self::area) lazily (`0` until `TH2PolyBin::GetArea`
    /// is first called), so this matches ROOT's `GetArea()` for the common case
    /// where the stored value is `0`. Returns `0` for fewer than 3 vertices.
    pub fn polygon_area(&self) -> f64 {
        let n = self.x.len().min(self.y.len());
        if n < 3 {
            return 0.0;
        }
        let mut sum = 0.0;
        for i in 0..n {
            let j = (i + 1) % n;
            sum += self.x[i] * self.y[j] - self.x[j] * self.y[i];
        }
        (sum / 2.0).abs()
    }
}

/// A 2-D histogram with arbitrary polygon bins (ROOT `TH2Poly`).
#[derive(Debug, Clone, PartialEq)]
pub struct TH2Poly {
    /// Histogram name (`fName`).
    pub name: String,
    /// Histogram title (`fTitle`).
    pub title: String,
    /// X axis (the bins' overall bounding box).
    pub xaxis: TAxis,
    /// Y axis (the bins' overall bounding box).
    pub yaxis: TAxis,
    /// Number of entries (`fEntries`).
    pub entries: f64,
    /// Sum of weights (`fTsumw`).
    pub tsumw: f64,
    /// Sum of weight² (`fTsumw2`).
    pub tsumw2: f64,
    /// Sum of weight·x (`fTsumwx`).
    pub tsumwx: f64,
    /// Sum of weight·x² (`fTsumwx2`).
    pub tsumwx2: f64,
    /// Sum of weight·y (`fTsumwy`).
    pub tsumwy: f64,
    /// Sum of weight·y² (`fTsumwy2`).
    pub tsumwy2: f64,
    /// Sum of weight·x·y (`fTsumwxy`).
    pub tsumwxy: f64,
    /// The 9 over/underflow accumulators (`fOverflow`).
    pub overflow: [f64; 9],
    /// The polygon bins, sorted by `number`.
    pub bins: Vec<PolyBin>,
}

impl TH2Poly {
    /// Create an empty `TH2Poly` over the bounding box `[xlow, xup] × [ylow, yup]`,
    /// matching ROOT's `TH2Poly(name, title, xlow, xup, ylow, yup)` constructor.
    /// Add bins with [`add_bin`](Self::add_bin) / [`add_bin_rect`](Self::add_bin_rect).
    pub fn new(xlow: f64, xup: f64, ylow: f64, yup: f64) -> TH2Poly {
        TH2Poly {
            name: String::new(),
            title: String::new(),
            xaxis: TAxis::new("xaxis", 100, xlow, xup),
            yaxis: TAxis::new("yaxis", 100, ylow, yup),
            entries: 0.0,
            tsumw: 0.0,
            tsumw2: 0.0,
            tsumwx: 0.0,
            tsumwx2: 0.0,
            tsumwy: 0.0,
            tsumwy2: 0.0,
            tsumwxy: 0.0,
            overflow: [0.0; 9],
            bins: Vec::new(),
        }
    }

    /// Number of polygon bins.
    pub fn nbins(&self) -> usize {
        self.bins.len()
    }

    /// Look up a bin by its ROOT `fNumber`.
    pub fn bin(&self, number: i32) -> Option<&PolyBin> {
        self.bins.iter().find(|b| b.number == number)
    }

    /// Add a polygon bin from its vertices (`x[i]`, `y[i]`); returns the new
    /// bin's ROOT `number` (1-based, in insertion order). The bounding box is
    /// derived from the vertices.
    pub fn add_bin(&mut self, x: &[f64], y: &[f64]) -> i32 {
        let number = self.bins.len() as i32 + 1;
        let xmin = x.iter().copied().fold(f64::INFINITY, f64::min);
        let xmax = x.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        let ymin = y.iter().copied().fold(f64::INFINITY, f64::min);
        let ymax = y.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        self.bins.push(PolyBin {
            number,
            content: 0.0,
            area: 0.0,
            xmin,
            ymin,
            xmax,
            ymax,
            x: x.to_vec(),
            y: y.to_vec(),
        });
        number
    }

    /// Add a rectangular bin spanning `[xmin, xmax] × [ymin, ymax]`; returns its
    /// `number`. Convenience for ROOT's `AddBin(xmin, ymin, xmax, ymax)`.
    pub fn add_bin_rect(&mut self, xmin: f64, ymin: f64, xmax: f64, ymax: f64) -> i32 {
        self.add_bin(
            &[xmin, xmin, xmax, xmax, xmin],
            &[ymin, ymax, ymax, ymin, ymin],
        )
    }

    /// Set a bin's content by `number` (no-op if no such bin).
    pub fn set_bin_content(&mut self, number: i32, content: f64) {
        if let Some(b) = self.bins.iter_mut().find(|b| b.number == number) {
            b.content = content;
        }
    }

    /// Fill `(x, y)` with unit weight; see [`fill_weight`](Self::fill_weight).
    pub fn fill(&mut self, x: f64, y: f64) -> i32 {
        self.fill_weight(x, y, 1.0)
    }

    /// Fill `(x, y)` with weight `w`, adding it to the first bin that contains
    /// the point and accumulating the statistics. Returns the filled bin's
    /// `number`, or `0` if the point lay outside every bin (overflow).
    pub fn fill_weight(&mut self, x: f64, y: f64, w: f64) -> i32 {
        self.entries += 1.0;
        let hit = self
            .bins
            .iter()
            .position(|b| point_in_polygon(&b.x, &b.y, x, y));
        match hit {
            Some(i) => {
                self.bins[i].content += w;
                self.tsumw += w;
                self.tsumw2 += w * w;
                self.tsumwx += w * x;
                self.tsumwx2 += w * x * x;
                self.tsumwy += w * y;
                self.tsumwy2 += w * y * y;
                self.tsumwxy += w * x * y;
                self.bins[i].number
            }
            None => 0,
        }
    }

    pub(crate) fn read(r: &mut RBuffer, keylen: usize) -> Result<TH2Poly> {
        let mut tags = TagReader::new(keylen);

        let top = r.read_version()?; // TH2Poly v3
        let th2 = r.read_version()?; // TH2 v5 base
        let core = read_th1_base(r)?; // TH1 base (name/title/axes/stats)
        let _scalefactor = r.be_f64()?;
        let tsumwy = r.be_f64()?;
        let tsumwy2 = r.be_f64()?;
        let tsumwxy = r.be_f64()?;
        // Defensive: realign to the end of the TH2 base record.
        if let Some(end) = th2.end {
            r.seek(end)?;
        }

        let mut overflow = [0.0f64; 9];
        for o in overflow.iter_mut() {
            *o = r.be_f64()?;
        }
        let _cell_x = r.be_i32()?; // fCellX (grid columns)
        let _cell_y = r.be_i32()?; // fCellY (grid rows)
        let ncells = r.be_i32()?.max(0) as usize; // fNCells (grid cell count)

        // fCells (TStreamerLoop): a {byte-count, version} header then `ncells`
        // inline TLists. ROOT writes each bin in full the first time it appears
        // here; walking the cells collects them all (later repeats are back-refs).
        r.skip(6)?;
        let mut bins: Vec<PolyBin> = Vec::new();
        for _ in 0..ncells {
            read_cell(r, &mut tags, &mut bins)?;
        }

        // A ROOT-written file stores its bins in full inside fCells, so we have
        // them all and `fBins` holds only back-references — done. A file oxiroot
        // wrote leaves fCells empty and stores the bins in full in `fBins`, so
        // when fCells yielded nothing, walk the tail to `fBins` and read them.
        if bins.is_empty() {
            let _step_x = r.be_f64()?;
            let _step_y = r.be_f64()?;
            read_bool_pointer_array(r, ncells)?; // fIsEmpty
            read_bool_pointer_array(r, ncells)?; // fCompletelyInside
            let _float = r.u8()?;
            let header = tags.read_header(r)?;
            if header.class_name.is_some() {
                read_cell(r, &mut tags, &mut bins)?; // the fBins TList of full bins
            }
        }
        if let Some(end) = top.end {
            r.seek(end)?;
        }

        bins.sort_by_key(|b| b.number);
        bins.dedup_by_key(|b| b.number);
        Ok(TH2Poly {
            name: core.name,
            title: core.title,
            xaxis: core.xaxis,
            yaxis: core.yaxis,
            entries: core.entries,
            tsumw: core.tsumw,
            tsumw2: core.tsumw2,
            tsumwx: core.tsumwx,
            tsumwx2: core.tsumwx2,
            tsumwy,
            tsumwy2,
            tsumwxy,
            overflow,
            bins,
        })
    }
}

/// Ray-casting point-in-polygon test over vertices `(x[i], y[i])`. Used by
/// [`TH2Poly::fill_weight`] to find the bin containing a point.
fn point_in_polygon(x: &[f64], y: &[f64], px: f64, py: f64) -> bool {
    let n = x.len().min(y.len());
    if n < 3 {
        return false;
    }
    let mut inside = false;
    let mut j = n - 1;
    for i in 0..n {
        if (y[i] > py) != (y[j] > py) && px < (x[j] - x[i]) * (py - y[i]) / (y[j] - y[i]) + x[i] {
            inside = !inside;
        }
        j = i;
    }
    inside
}

/// Skip a `bool* //[n]` member: a 1-byte presence marker, then (if present)
/// `n` bytes — one per element.
fn read_bool_pointer_array(r: &mut RBuffer, n: usize) -> Result<()> {
    let present = r.u8()?;
    if present != 0 {
        r.skip(n)?;
    }
    Ok(())
}

/// Read one `fCells` entry: a full inline `TList` of `TH2PolyBin` object
/// pointers. New bins are read in full and pushed; repeats are object
/// back-references (resolved to "no object" — skipped).
fn read_cell(r: &mut RBuffer, tags: &mut TagReader, bins: &mut Vec<PolyBin>) -> Result<()> {
    let tlist = r.read_version()?; // TList v5
    read_tobject(r)?;
    let _name = r.string()?; // fName (empty)
    let size = r.be_i32()?.max(0);
    for _ in 0..size {
        let header = tags.read_header(r)?;
        if header.class_name.as_deref() == Some("TH2PolyBin") {
            bins.push(read_polybin(r, tags)?);
        }
        if let Some(end) = header.end {
            r.seek(end)?;
        }
        let _option = r.string()?; // per-element option string
    }
    if let Some(end) = tlist.end {
        r.seek(end)?;
    }
    Ok(())
}

/// Read a `TH2PolyBin` body, after its object-pointer header was consumed.
fn read_polybin(r: &mut RBuffer, tags: &mut TagReader) -> Result<PolyBin> {
    r.read_version()?; // TH2PolyBin v1
    read_tobject(r)?;
    let _changed = r.u8()?; // fChanged
    let number = r.be_i32()?; // fNumber
    let (x, y) = read_poly_graph(r, tags)?; // fPoly (a TGraph)
    let area = r.be_f64()?;
    let content = r.be_f64()?;
    let xmin = r.be_f64()?;
    let ymin = r.be_f64()?;
    let xmax = r.be_f64()?;
    let ymax = r.be_f64()?;
    Ok(PolyBin {
        number,
        content,
        area,
        xmin,
        ymin,
        xmax,
        ymax,
        x,
        y,
    })
}

/// Read a `TH2PolyBin`'s `fPoly` (a `TGraph` object pointer) and return the
/// polygon's `(fX, fY)` vertices, then seek past the rest of the graph.
fn read_poly_graph(r: &mut RBuffer, tags: &mut TagReader) -> Result<(Vec<f64>, Vec<f64>)> {
    let header = tags.read_header(r)?;
    if header.class_name.is_none() {
        // A null or back-referenced graph; nothing to read inline.
        if let Some(end) = header.end {
            r.seek(end)?;
        }
        return Ok((Vec::new(), Vec::new()));
    }
    let _graph = r.read_version()?; // TGraph v5
    skip_versioned(r)?; // TNamed
    skip_versioned(r)?; // TAttLine
    skip_versioned(r)?; // TAttFill
    skip_versioned(r)?; // TAttMarker
    let npoints = r.be_i32()?.max(0) as usize; // fNpoints
    let _x_marker = r.u8()?; // Double_t* //[fNpoints] presence marker
    let x = (0..npoints)
        .map(|_| r.be_f64())
        .collect::<Result<Vec<_>>>()?;
    let _y_marker = r.u8()?;
    let y = (0..npoints)
        .map(|_| r.be_f64())
        .collect::<Result<Vec<_>>>()?;
    // Skip fHistogram/fMinimum/fMaximum/fFunctions… via the object's byte count.
    if let Some(end) = header.end {
        r.seek(end)?;
    }
    Ok((x, y))
}

/// Read a `TH2Poly` named `name` from `file`.
pub(crate) fn read_th2poly(file: &RFile, name: &str) -> Result<TH2Poly> {
    let (object, keylen) = object_bytes_keyed(file, name, "TH2Poly")?;
    decode_th2poly(name, &object, keylen)
}

/// Read a `TH2Poly` from subdirectory `subdir`.
pub(crate) fn read_th2poly_in(file: &RFile, subdir: &str, name: &str) -> Result<TH2Poly> {
    let (class, object, keylen) = file.object_in_keyed(subdir, name)?;
    if class != "TH2Poly" {
        return Err(Error::Format(format!(
            "key {name:?} in {subdir:?} is a {class}, not TH2Poly"
        )));
    }
    decode_th2poly(name, &object, keylen)
}

fn decode_th2poly(name: &str, object: &[u8], keylen: usize) -> Result<TH2Poly> {
    TH2Poly::read(&mut RBuffer::new(object), keylen)
        .map_err(|e| Error::Format(format!("reading TH2Poly {name:?}: {e}")))
}
