//! The error a statistics function returns when it is given input it cannot use.

use std::fmt;

/// Why a statistics function rejected its input.
///
/// Only the functions that pair two samples element by element return this —
/// [`pearsonr`](crate::pearsonr), [`spearmanr`](crate::spearmanr),
/// [`chisquare`](crate::chisquare), [`wilcoxon`](crate::wilcoxon),
/// [`kl_divergence`](crate::kl_divergence),
/// [`weighted_mean`](crate::weighted_mean), [`weighted_std`](crate::weighted_std)
/// and [`combine_measurements`](crate::combine_measurements). Pairing up to the
/// shorter sample would silently compute a statistic from data that was never
/// paired, so they fail instead; each function documents where that is stricter
/// than `scipy`. A `NaN` *value* is not an error: it propagates to a `NaN`
/// result.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum StatError {
    /// Two samples that are paired element by element have different lengths.
    LengthMismatch {
        /// Length of the first sample.
        left: usize,
        /// Length of the second sample.
        right: usize,
    },
    /// The samples hold fewer observations than the statistic is defined for.
    TooFewObservations {
        /// The minimum number of observations the statistic needs.
        needed: usize,
        /// The number of observations supplied.
        got: usize,
    },
}

impl fmt::Display for StatError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StatError::LengthMismatch { left, right } => write!(
                f,
                "paired samples must have the same length, got {left} and {right}"
            ),
            StatError::TooFewObservations { needed, got } => {
                write!(f, "need at least {needed} observations, got {got}")
            }
        }
    }
}

impl std::error::Error for StatError {}

/// Reject two paired samples whose lengths differ.
pub(crate) fn check_paired(left: usize, right: usize) -> Result<(), StatError> {
    if left == right {
        Ok(())
    } else {
        Err(StatError::LengthMismatch { left, right })
    }
}
