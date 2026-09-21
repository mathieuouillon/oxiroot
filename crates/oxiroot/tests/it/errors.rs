//! Every error the facade's API returns converts into `oxiroot::Error`, so `?`
//! works in one function that mixes file IO, statistics, formulas and plotting.

use std::error::Error as _;

use oxiroot::stat::{pearsonr, StatError};

fn correlate(x: &[f64], y: &[f64]) -> oxiroot::Result<f64> {
    let (r, _) = pearsonr(x, y)?;
    Ok(r)
}

#[test]
fn statistics_errors_convert() {
    let err = correlate(&[1.0, 2.0, 3.0], &[1.0, 2.0]).unwrap_err();
    assert_eq!(
        err,
        oxiroot::Error::Stat(StatError::LengthMismatch { left: 3, right: 2 })
    );
    assert_eq!(
        err.to_string(),
        "statistics: paired samples must have the same length, got 3 and 2"
    );
    assert!(err.source().is_some());
    assert!((correlate(&[1.0, 2.0, 3.0], &[2.0, 4.0, 6.0]).unwrap() - 1.0).abs() < 1e-12);
}

#[cfg(feature = "fit")]
#[test]
fn formula_errors_convert() {
    fn model(formula: &str) -> oxiroot::Result<oxiroot::fit::Model> {
        Ok(oxiroot::fit::Model::from_formula("m", formula)?)
    }
    let err = model("[0]*(x").unwrap_err();
    assert!(matches!(err, oxiroot::Error::Formula(_)), "{err:?}");
    assert!(err.to_string().starts_with("invalid formula: "), "{err}");
    assert!(err.source().is_some());
    assert!(model("[0]*x").is_ok());
}

#[cfg(feature = "plot")]
#[test]
fn plotting_errors_convert() {
    fn save(path: &std::path::Path) -> oxiroot::Result<()> {
        let mut ax = oxiroot::plot::Axes::new();
        ax.plot(&[0.0, 1.0], &[0.0, 1.0]);
        ax.save(path)?;
        Ok(())
    }
    let dir = std::env::temp_dir();
    // An unknown extension is a plotting error, carried with its message.
    let err = save(&dir.join("oxiroot_errors_test.bmp")).unwrap_err();
    assert_eq!(
        err,
        oxiroot::Error::Plot("unknown image format `bmp` (use a .png, .svg, or .pdf path)".into())
    );
    // An I/O failure keeps its kind.
    let err = save(&dir.join("oxiroot-no-such-dir/x/y.svg")).unwrap_err();
    assert!(
        matches!(
            err,
            oxiroot::Error::Io {
                kind: std::io::ErrorKind::NotFound,
                ..
            }
        ),
        "{err:?}"
    );
    assert!(save(&dir.join("oxiroot_errors_test.svg")).is_ok());
}
