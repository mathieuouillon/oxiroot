//! The data the plotting methods draw, as traits: [`Hist1dData`] for 1-D binned
//! values ([`Axes::hist`](crate::Axes::hist),
//! [`Axes::profile`](crate::Axes::profile)), [`Hist2dData`] for a 2-D grid
//! ([`Axes::hist2d`](crate::Axes::hist2d)), and [`PointData`] for points with
//! optional error bars ([`Axes::errorbar`](crate::Axes::errorbar)).
//!
//! The oxiroot histogram and graph types implement them (the `hist` feature, on
//! by default); any other data can implement them too.

/// A 1-D binned distribution: `n` in-range bins.
pub trait Hist1dData {
    /// The `n + 1` bin edges, ascending.
    fn edges(&self) -> Vec<f64>;
    /// The `n` in-range bin contents.
    fn values(&self) -> Vec<f64>;
    /// The uncertainty of in-range bin `i` (`0..n`).
    fn error(&self, i: usize) -> f64;
    /// The x axis's ROOT time format, when its values are times
    /// (`fTimeFormat` on a time axis). Plotting picks it up, so a histogram
    /// with a time axis draws dates and clock times instead of numbers.
    fn x_time_format(&self) -> Option<String> {
        None
    }
}

/// A 2-D binned distribution: `nx × ny` in-range bins.
pub trait Hist2dData {
    /// The `nx + 1` x edges, ascending.
    fn x_edges(&self) -> Vec<f64>;
    /// The `ny + 1` y edges, ascending.
    fn y_edges(&self) -> Vec<f64>;
    /// The in-range contents, indexed `[x][y]`.
    fn grid(&self) -> Vec<Vec<f64>>;
}

/// Points with optional error bars.
pub trait PointData {
    /// The x coordinates.
    fn xs(&self) -> Vec<f64>;
    /// The y coordinates, one per x.
    fn ys(&self) -> Vec<f64>;
    /// The `(low, high)` x errors, one pair of values per point, if any.
    fn x_errors(&self) -> Option<(Vec<f64>, Vec<f64>)> {
        None
    }
    /// The `(low, high)` y errors, one pair of values per point, if any.
    fn y_errors(&self) -> Option<(Vec<f64>, Vec<f64>)> {
        None
    }

    /// The x axis's ROOT time format, when its values are times (`fTimeFormat`
    /// on a time axis). Plotting picks it up, so a time axis draws as times.
    fn x_time_format(&self) -> Option<String> {
        None
    }
}

#[cfg(feature = "hist")]
mod hist_impls {
    use oxiroot_hist::{Graph, GraphErrors, Hist1D, Hist2D, Profile1D};

    use super::{Hist1dData, Hist2dData, PointData};

    impl Hist1dData for Hist1D {
        fn edges(&self) -> Vec<f64> {
            Hist1D::edges(self)
        }
        fn values(&self) -> Vec<f64> {
            Hist1D::values(self).to_vec()
        }
        fn error(&self, i: usize) -> f64 {
            self.bin_error(i + 1)
        }
        fn x_time_format(&self) -> Option<String> {
            self.xaxis
                .time_display
                .then(|| self.xaxis.time_format.clone())
        }
    }

    impl Hist1dData for Profile1D {
        fn edges(&self) -> Vec<f64> {
            Profile1D::edges(self)
        }
        fn values(&self) -> Vec<f64> {
            Profile1D::values(self)
        }
        fn error(&self, i: usize) -> f64 {
            self.bin_error(i + 1)
        }
    }

    impl Hist2dData for Hist2D {
        fn x_edges(&self) -> Vec<f64> {
            self.xaxis.edges()
        }
        fn y_edges(&self) -> Vec<f64> {
            self.yaxis.edges()
        }
        fn grid(&self) -> Vec<Vec<f64>> {
            self.values()
        }
    }

    /// Error arrays shorter than the graph are padded with zeros.
    fn padded(v: &[f64], n: usize) -> Vec<f64> {
        let mut out = v.to_vec();
        out.resize(n, 0.0);
        out
    }

    impl PointData for Graph {
        fn xs(&self) -> Vec<f64> {
            self.x[..self.len()].to_vec()
        }
        fn ys(&self) -> Vec<f64> {
            self.y[..self.len()].to_vec()
        }
        fn x_errors(&self) -> Option<(Vec<f64>, Vec<f64>)> {
            let n = self.len();
            match &self.errors {
                GraphErrors::Symmetric { ex, .. } => Some((padded(ex, n), padded(ex, n))),
                GraphErrors::Asymmetric {
                    ex_low, ex_high, ..
                } => Some((padded(ex_low, n), padded(ex_high, n))),
                _ => None,
            }
        }
        fn y_errors(&self) -> Option<(Vec<f64>, Vec<f64>)> {
            let n = self.len();
            match &self.errors {
                GraphErrors::Symmetric { ey, .. } => Some((padded(ey, n), padded(ey, n))),
                GraphErrors::Asymmetric {
                    ey_low, ey_high, ..
                } => Some((padded(ey_low, n), padded(ey_high, n))),
                _ => None,
            }
        }
        fn x_time_format(&self) -> Option<String> {
            // A graph's axes live on its display frame, as in ROOT.
            let axis = &self.histogram.as_ref()?.xaxis;
            axis.time_display.then(|| axis.time_format.clone())
        }
    }
}
