//! A quick-construction builder for histograms, modelled on scikit-hep
//! [`hist`](https://github.com/scikit-hep/hist)'s `Hist.new` API: chain the axes,
//! then a storage finalizer.
//!
//! ```
//! use oxiroot_hist::Hist;
//! // hist:  Hist.new.Reg(50, 0, 100, name="pt", label="$p_T$").Weight()
//! let h = Hist::reg(50, 0.0, 100.0).name("pt").label("$p_T$ [GeV]").weight();
//! // 2-D, mixing a regular and a variable axis:
//! let h2 = Hist::reg(40, -4.0, 4.0).label("x").var(&[0.0, 1.0, 4.0, 10.0]).label("y").double();
//! ```
//!
//! The storage finalizers map onto ROOT's histogram classes, so everything the
//! builder produces reads and writes as ordinary ROOT histograms:
//!
//! | `hist` storage | builder | ROOT class | notes |
//! |----------------|---------|------------|-------|
//! | `Double()`     | [`double`](Build1D::double) | `TH1D` | |
//! | `Weight()`     | [`weight`](Build1D::weight) | `TH1D` + `Sumw2` | value **and** variance |
//! | `Int64()`      | [`int64`](Build1D::int64)   | `TH1L` | 64-bit integer bins |
//! | (ROOT)         | [`float`](Build1D::float)   | `TH1F` | 32-bit float bins |
//! | (ROOT)         | [`int32`](Build1D::int32)   | `TH1I` | 32-bit integer bins |
//! | (ROOT)         | [`int16`](Build1D::int16)   | `TH1S` | 16-bit integer bins |
//! | (ROOT)         | [`int8`](Build1D::int8)     | `TH1C` | 8-bit integer bins |
//! | `Mean()`       | [`profile`](Build1D::profile) | `Profile1D` | per-bin mean (1-, 2-, 3-D) |

use crate::base::BinContentType;
use crate::hist1d::Hist1D;
use crate::hist2d::Hist2D;
use crate::hist3d::Hist3D;
use crate::profile1d::Profile1D;
use crate::profile2d::Profile2D;
use crate::profile3d::Profile3D;

/// One axis of the builder: a regular range or explicit edges, plus a label.
#[derive(Debug, Clone)]
struct AxisSpec {
    nbins: i32,
    lo: f64,
    hi: f64,
    edges: Option<Vec<f64>>,
    label: String,
}

impl AxisSpec {
    fn reg(nbins: i32, lo: f64, hi: f64) -> AxisSpec {
        AxisSpec {
            nbins,
            lo,
            hi,
            edges: None,
            label: String::new(),
        }
    }

    fn var(edges: &[f64]) -> AxisSpec {
        let n = edges.len().saturating_sub(1) as i32;
        AxisSpec {
            nbins: n,
            lo: edges.first().copied().unwrap_or(0.0),
            hi: edges.last().copied().unwrap_or(1.0),
            edges: Some(edges.to_vec()),
            label: String::new(),
        }
    }

    fn is_regular(&self) -> bool {
        self.edges.is_none()
    }

    /// The explicit `nbins + 1` bin edges (computed for a regular axis).
    fn edge_vec(&self) -> Vec<f64> {
        match &self.edges {
            Some(e) => e.clone(),
            None => {
                let n = self.nbins.max(1);
                (0..=n)
                    .map(|i| self.lo + (self.hi - self.lo) * i as f64 / n as f64)
                    .collect()
            }
        }
    }
}

/// Apply the chosen storage and the shared name/title to a freshly built
/// histogram. Implemented for `Hist1D`/`Hist2D`/`Hist3D` so the finalizers are one line.
trait Finish: Sized {
    fn sumw2(&mut self);
    fn set_bin_content_type(self, p: BinContentType) -> Self;
    fn set_name(self, name: String) -> Self;
    fn set_title(self, title: String) -> Self;

    fn finish(
        self,
        name: String,
        title: String,
        content_type: BinContentType,
        weight: bool,
    ) -> Self {
        let mut h = self
            .set_bin_content_type(content_type)
            .set_name(name)
            .set_title(title);
        if weight {
            h.sumw2();
        }
        h
    }
}

macro_rules! impl_finish {
    ($t:ty) => {
        impl Finish for $t {
            fn sumw2(&mut self) {
                <$t>::sumw2(self);
            }
            fn set_bin_content_type(self, p: BinContentType) -> Self {
                self.with_bin_content_type(p)
            }
            fn set_name(self, name: String) -> Self {
                self.named(name)
            }
            fn set_title(self, title: String) -> Self {
                self.titled(title)
            }
        }
    };
}
impl_finish!(Hist1D);
impl_finish!(Hist2D);
impl_finish!(Hist3D);

/// The entry point of the quick-construction builder (mirrors `hist`'s
/// `Hist.new`). Start an axis with [`reg`](Hist::reg) or [`var`](Hist::var).
pub struct Hist;

impl Hist {
    /// Begin with a regular axis of `nbins` uniform bins over `[lo, hi)`
    /// (`hist`'s `Reg`).
    #[must_use]
    pub fn reg(nbins: i32, lo: f64, hi: f64) -> Build1D {
        Build1D {
            ax: AxisSpec::reg(nbins, lo, hi),
            name: String::new(),
            title: String::new(),
        }
    }

    /// Begin with a variable-width axis from explicit `edges` (`hist`'s `Var`).
    #[must_use]
    pub fn var(edges: &[f64]) -> Build1D {
        Build1D {
            ax: AxisSpec::var(edges),
            name: String::new(),
            title: String::new(),
        }
    }
}

/// Generate, per axis count, the builder struct: per-axis-label `label`, shared
/// `name`/`title`, the `reg`/`var` chaining to the next axis, and the storage
/// finalizers that build the ROOT histogram.
macro_rules! builder {
    ($name:ident, $hist:ty, $build:ident) => {
        impl $name {
            /// Set the histogram's key name (`fName`).
            #[must_use]
            pub fn name(mut self, name: impl Into<String>) -> Self {
                self.name = name.into();
                self
            }
            /// Set the histogram's title (`fTitle`).
            #[must_use]
            pub fn title(mut self, title: impl Into<String>) -> Self {
                self.title = title.into();
                self
            }
            /// Set the label of the most recently added axis (its ROOT `fTitle`).
            #[must_use]
            pub fn label(mut self, label: impl Into<String>) -> Self {
                self.last_axis().label = label.into();
                self
            }
            /// Build with `Double` storage (`TH1D`/`TH2D`/`TH3D`).
            #[must_use]
            pub fn double(self) -> $hist {
                self.$build(BinContentType::F64, false)
            }
            /// Build with `Float` storage (`TH1F`/`TH2F`/`TH3F`).
            #[must_use]
            pub fn float(self) -> $hist {
                self.$build(BinContentType::F32, false)
            }
            /// Build with 64-bit integer storage (`TH1L`/`TH2L`/`TH3L`).
            #[must_use]
            pub fn int64(self) -> $hist {
                self.$build(BinContentType::I64, false)
            }
            /// Build with 32-bit integer storage (`TH1I`/`TH2I`/`TH3I`).
            #[must_use]
            pub fn int32(self) -> $hist {
                self.$build(BinContentType::I32, false)
            }
            /// Build with 16-bit integer storage (`TH1S`/`TH2S`/`TH3S`).
            #[must_use]
            pub fn int16(self) -> $hist {
                self.$build(BinContentType::I16, false)
            }
            /// Build with 8-bit integer storage (`TH1C`/`TH2C`/`TH3C`).
            #[must_use]
            pub fn int8(self) -> $hist {
                self.$build(BinContentType::I8, false)
            }
            /// Build with `Weight` storage — `Double` plus per-bin variances
            /// (ROOT `Sumw2`), for weighted fills.
            #[must_use]
            pub fn weight(self) -> $hist {
                self.$build(BinContentType::F64, true)
            }
        }
    };
}

/// A one-axis builder → [`Hist1D`].
pub struct Build1D {
    ax: AxisSpec,
    name: String,
    title: String,
}

impl Build1D {
    fn last_axis(&mut self) -> &mut AxisSpec {
        &mut self.ax
    }

    /// Add a regular second axis, producing a 2-D builder.
    #[must_use]
    pub fn reg(self, nbins: i32, lo: f64, hi: f64) -> Build2D {
        Build2D {
            axes: [self.ax, AxisSpec::reg(nbins, lo, hi)],
            name: self.name,
            title: self.title,
        }
    }
    /// Add a variable-width second axis, producing a 2-D builder.
    #[must_use]
    pub fn var(self, edges: &[f64]) -> Build2D {
        Build2D {
            axes: [self.ax, AxisSpec::var(edges)],
            name: self.name,
            title: self.title,
        }
    }

    fn build1(self, content_type: BinContentType, weight: bool) -> Hist1D {
        let mut h = if self.ax.is_regular() {
            Hist1D::new(self.ax.nbins, self.ax.lo, self.ax.hi)
        } else {
            Hist1D::new_variable(&self.ax.edge_vec())
        };
        h.xaxis.title = self.ax.label;
        h.finish(self.name, self.title, content_type, weight)
    }

    /// Build a [`Profile1D`] — `hist`'s `Mean` storage on a 1-D axis. Fill it with
    /// `(x, y)` pairs (`profile.fill(x, y)`); each bin then holds the mean `y`
    /// and its error, instead of a count.
    #[must_use]
    pub fn profile(self) -> Profile1D {
        // Profile1D has only a regular-axis constructor; overlay explicit edges
        // for a variable axis (the bin count already matches).
        let mut p = Profile1D::new(self.ax.nbins, self.ax.lo, self.ax.hi);
        if let Some(e) = &self.ax.edges {
            p.xaxis.xbins = e.clone();
        }
        p.xaxis.title = self.ax.label;
        p.named(self.name).titled(self.title)
    }
}
builder!(Build1D, Hist1D, build1);

/// A two-axis builder → [`Hist2D`].
pub struct Build2D {
    axes: [AxisSpec; 2],
    name: String,
    title: String,
}

impl Build2D {
    fn last_axis(&mut self) -> &mut AxisSpec {
        &mut self.axes[1]
    }

    /// Add a third axis (regular), producing a 3-D builder.
    #[must_use]
    pub fn reg(self, nbins: i32, lo: f64, hi: f64) -> Build3D {
        let [x, y] = self.axes;
        Build3D {
            axes: [x, y, AxisSpec::reg(nbins, lo, hi)],
            name: self.name,
            title: self.title,
        }
    }
    /// Add a third axis (variable-width), producing a 3-D builder.
    #[must_use]
    pub fn var(self, edges: &[f64]) -> Build3D {
        let [x, y] = self.axes;
        Build3D {
            axes: [x, y, AxisSpec::var(edges)],
            name: self.name,
            title: self.title,
        }
    }

    fn build2(self, content_type: BinContentType, weight: bool) -> Hist2D {
        let [x, y] = &self.axes;
        let mut h = if x.is_regular() && y.is_regular() {
            Hist2D::new(x.nbins, x.lo, x.hi, y.nbins, y.lo, y.hi)
        } else {
            Hist2D::new_variable(&x.edge_vec(), &y.edge_vec())
        };
        h.xaxis.title = x.label.clone();
        h.yaxis.title = y.label.clone();
        h.finish(self.name, self.title, content_type, weight)
    }

    /// Build a [`Profile2D`] — `hist`'s `Mean` storage over two axes. Fill it
    /// with `(x, y, z)` triples (`profile.fill(x, y, z)`); each bin holds the
    /// mean `z` and its error, instead of a count.
    #[must_use]
    pub fn profile(self) -> Profile2D {
        let [x, y] = &self.axes;
        // Profile2D has only a regular-axis constructor; overlay explicit edges
        // for any variable axis (the bin counts already match).
        let mut p = Profile2D::new(x.nbins, x.lo, x.hi, y.nbins, y.lo, y.hi);
        if let Some(e) = &x.edges {
            p.xaxis.xbins = e.clone();
        }
        if let Some(e) = &y.edges {
            p.yaxis.xbins = e.clone();
        }
        p.xaxis.title = x.label.clone();
        p.yaxis.title = y.label.clone();
        p.named(self.name).titled(self.title)
    }
}
builder!(Build2D, Hist2D, build2);

/// A three-axis builder → [`Hist3D`].
pub struct Build3D {
    axes: [AxisSpec; 3],
    name: String,
    title: String,
}

impl Build3D {
    fn last_axis(&mut self) -> &mut AxisSpec {
        &mut self.axes[2]
    }

    fn build3(self, content_type: BinContentType, weight: bool) -> Hist3D {
        let [x, y, z] = &self.axes;
        // Hist3D has no variable-axis constructor; build a regular `Hist3D` with the
        // right bin counts/ranges, then overlay explicit edges on any variable
        // axis (a populated `fXbins` is exactly how ROOT marks a variable axis,
        // and the cell counts already match).
        let mut h = Hist3D::new(
            x.nbins, x.lo, x.hi, y.nbins, y.lo, y.hi, z.nbins, z.lo, z.hi,
        );
        if let Some(e) = &x.edges {
            h.xaxis.xbins = e.clone();
        }
        if let Some(e) = &y.edges {
            h.yaxis.xbins = e.clone();
        }
        if let Some(e) = &z.edges {
            h.zaxis.xbins = e.clone();
        }
        h.xaxis.title = x.label.clone();
        h.yaxis.title = y.label.clone();
        h.zaxis.title = z.label.clone();
        h.finish(self.name, self.title, content_type, weight)
    }

    /// Build a [`Profile3D`] — `hist`'s `Mean` storage over three axes. Fill it
    /// with `(x, y, z, t)` (`profile.fill(x, y, z, t)`); each bin holds the mean
    /// `t` and its error, instead of a count.
    #[must_use]
    pub fn profile(self) -> Profile3D {
        let [x, y, z] = &self.axes;
        let mut p = Profile3D::new(
            x.nbins, x.lo, x.hi, y.nbins, y.lo, y.hi, z.nbins, z.lo, z.hi,
        );
        if let Some(e) = &x.edges {
            p.xaxis.xbins = e.clone();
        }
        if let Some(e) = &y.edges {
            p.yaxis.xbins = e.clone();
        }
        if let Some(e) = &z.edges {
            p.zaxis.xbins = e.clone();
        }
        p.xaxis.title = x.label.clone();
        p.yaxis.title = y.label.clone();
        p.zaxis.title = z.label.clone();
        p.named(self.name).titled(self.title)
    }
}
builder!(Build3D, Hist3D, build3);
