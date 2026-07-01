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
//! | `Double()`     | [`double`](H1::double) | `TH1D` | |
//! | `Weight()`     | [`weight`](H1::weight) | `TH1D` + `Sumw2` | value **and** variance |
//! | `Int64()`      | [`int64`](H1::int64)   | `TH1L` | 64-bit integer bins |
//! | (ROOT)         | [`float`](H1::float)   | `TH1F` | 32-bit float bins |
//! | (ROOT)         | [`int32`](H1::int32)   | `TH1I` | 32-bit integer bins |
//! | (ROOT)         | [`int16`](H1::int16)   | `TH1S` | 16-bit integer bins |
//! | (ROOT)         | [`int8`](H1::int8)     | `TH1C` | 8-bit integer bins |
//! | `Mean()`       | [`profile`](H1::profile) | `TProfile` | per-bin mean (1-, 2-, 3-D) |

use crate::base::Precision;
use crate::th1::TH1;
use crate::th2::TH2;
use crate::th3::TH3;
use crate::tprofile::TProfile;
use crate::tprofile2d::TProfile2D;
use crate::tprofile3d::TProfile3D;

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
/// histogram. Implemented for `TH1`/`TH2`/`TH3` so the finalizers are one line.
trait Finish: Sized {
    fn sumw2(&mut self);
    fn set_precision(self, p: Precision) -> Self;
    fn set_name(self, name: String) -> Self;
    fn set_title(self, title: String) -> Self;

    fn finish(self, name: String, title: String, prec: Precision, weight: bool) -> Self {
        let mut h = self.set_precision(prec).set_name(name).set_title(title);
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
            fn set_precision(self, p: Precision) -> Self {
                self.with_precision(p)
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
impl_finish!(TH1);
impl_finish!(TH2);
impl_finish!(TH3);

/// The entry point of the quick-construction builder (mirrors `hist`'s
/// `Hist.new`). Start an axis with [`reg`](Hist::reg) or [`var`](Hist::var).
pub struct Hist;

impl Hist {
    /// Begin with a regular axis of `nbins` uniform bins over `[lo, hi)`
    /// (`hist`'s `Reg`).
    #[must_use]
    pub fn reg(nbins: i32, lo: f64, hi: f64) -> H1 {
        H1 {
            ax: AxisSpec::reg(nbins, lo, hi),
            name: String::new(),
            title: String::new(),
        }
    }

    /// Begin with a variable-width axis from explicit `edges` (`hist`'s `Var`).
    #[must_use]
    pub fn var(edges: &[f64]) -> H1 {
        H1 {
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
                self.$build(Precision::Double, false)
            }
            /// Build with `Float` storage (`TH1F`/`TH2F`/`TH3F`).
            #[must_use]
            pub fn float(self) -> $hist {
                self.$build(Precision::Float, false)
            }
            /// Build with 64-bit integer storage (`TH1L`/`TH2L`/`TH3L`).
            #[must_use]
            pub fn int64(self) -> $hist {
                self.$build(Precision::Long, false)
            }
            /// Build with 32-bit integer storage (`TH1I`/`TH2I`/`TH3I`).
            #[must_use]
            pub fn int32(self) -> $hist {
                self.$build(Precision::Int, false)
            }
            /// Build with 16-bit integer storage (`TH1S`/`TH2S`/`TH3S`).
            #[must_use]
            pub fn int16(self) -> $hist {
                self.$build(Precision::Short, false)
            }
            /// Build with 8-bit integer storage (`TH1C`/`TH2C`/`TH3C`).
            #[must_use]
            pub fn int8(self) -> $hist {
                self.$build(Precision::Char, false)
            }
            /// Build with `Weight` storage — `Double` plus per-bin variances
            /// (ROOT `Sumw2`), for weighted fills.
            #[must_use]
            pub fn weight(self) -> $hist {
                self.$build(Precision::Double, true)
            }
        }
    };
}

/// A one-axis builder → [`TH1`].
pub struct H1 {
    ax: AxisSpec,
    name: String,
    title: String,
}

impl H1 {
    fn last_axis(&mut self) -> &mut AxisSpec {
        &mut self.ax
    }

    /// Add a regular second axis, producing a 2-D builder.
    #[must_use]
    pub fn reg(self, nbins: i32, lo: f64, hi: f64) -> H2 {
        H2 {
            axes: [self.ax, AxisSpec::reg(nbins, lo, hi)],
            name: self.name,
            title: self.title,
        }
    }
    /// Add a variable-width second axis, producing a 2-D builder.
    #[must_use]
    pub fn var(self, edges: &[f64]) -> H2 {
        H2 {
            axes: [self.ax, AxisSpec::var(edges)],
            name: self.name,
            title: self.title,
        }
    }

    fn build1(self, prec: Precision, weight: bool) -> TH1 {
        let mut h = if self.ax.is_regular() {
            TH1::new(self.ax.nbins, self.ax.lo, self.ax.hi)
        } else {
            TH1::new_variable(&self.ax.edge_vec())
        };
        h.xaxis.title = self.ax.label;
        h.finish(self.name, self.title, prec, weight)
    }

    /// Build a [`TProfile`] — `hist`'s `Mean` storage on a 1-D axis. Fill it with
    /// `(x, y)` pairs (`profile.fill(x, y)`); each bin then holds the mean `y`
    /// and its error, instead of a count.
    #[must_use]
    pub fn profile(self) -> TProfile {
        // TProfile has only a regular-axis constructor; overlay explicit edges
        // for a variable axis (the bin count already matches).
        let mut p = TProfile::new(self.ax.nbins, self.ax.lo, self.ax.hi);
        if let Some(e) = &self.ax.edges {
            p.xaxis.xbins = e.clone();
        }
        p.xaxis.title = self.ax.label;
        p.named(self.name).titled(self.title)
    }
}
builder!(H1, TH1, build1);

/// A two-axis builder → [`TH2`].
pub struct H2 {
    axes: [AxisSpec; 2],
    name: String,
    title: String,
}

impl H2 {
    fn last_axis(&mut self) -> &mut AxisSpec {
        &mut self.axes[1]
    }

    /// Add a third axis (regular), producing a 3-D builder.
    #[must_use]
    pub fn reg(self, nbins: i32, lo: f64, hi: f64) -> H3 {
        let [x, y] = self.axes;
        H3 {
            axes: [x, y, AxisSpec::reg(nbins, lo, hi)],
            name: self.name,
            title: self.title,
        }
    }
    /// Add a third axis (variable-width), producing a 3-D builder.
    #[must_use]
    pub fn var(self, edges: &[f64]) -> H3 {
        let [x, y] = self.axes;
        H3 {
            axes: [x, y, AxisSpec::var(edges)],
            name: self.name,
            title: self.title,
        }
    }

    fn build2(self, prec: Precision, weight: bool) -> TH2 {
        let [x, y] = &self.axes;
        let mut h = if x.is_regular() && y.is_regular() {
            TH2::new(x.nbins, x.lo, x.hi, y.nbins, y.lo, y.hi)
        } else {
            TH2::new_variable(&x.edge_vec(), &y.edge_vec())
        };
        h.xaxis.title = x.label.clone();
        h.yaxis.title = y.label.clone();
        h.finish(self.name, self.title, prec, weight)
    }

    /// Build a [`TProfile2D`] — `hist`'s `Mean` storage over two axes. Fill it
    /// with `(x, y, z)` triples (`profile.fill(x, y, z)`); each bin holds the
    /// mean `z` and its error, instead of a count.
    #[must_use]
    pub fn profile(self) -> TProfile2D {
        let [x, y] = &self.axes;
        // TProfile2D has only a regular-axis constructor; overlay explicit edges
        // for any variable axis (the bin counts already match).
        let mut p = TProfile2D::new(x.nbins, x.lo, x.hi, y.nbins, y.lo, y.hi);
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
builder!(H2, TH2, build2);

/// A three-axis builder → [`TH3`].
pub struct H3 {
    axes: [AxisSpec; 3],
    name: String,
    title: String,
}

impl H3 {
    fn last_axis(&mut self) -> &mut AxisSpec {
        &mut self.axes[2]
    }

    fn build3(self, prec: Precision, weight: bool) -> TH3 {
        let [x, y, z] = &self.axes;
        // TH3 has no variable-axis constructor; build a regular `TH3` with the
        // right bin counts/ranges, then overlay explicit edges on any variable
        // axis (a populated `fXbins` is exactly how ROOT marks a variable axis,
        // and the cell counts already match).
        let mut h = TH3::new(
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
        h.finish(self.name, self.title, prec, weight)
    }

    /// Build a [`TProfile3D`] — `hist`'s `Mean` storage over three axes. Fill it
    /// with `(x, y, z, t)` (`profile.fill(x, y, z, t)`); each bin holds the mean
    /// `t` and its error, instead of a count.
    #[must_use]
    pub fn profile(self) -> TProfile3D {
        let [x, y, z] = &self.axes;
        let mut p = TProfile3D::new(
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
builder!(H3, TH3, build3);
