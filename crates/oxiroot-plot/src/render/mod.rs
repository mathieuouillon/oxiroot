//! Rendering backends consuming the shared [`crate::draw`] IR.

pub mod pdf;
#[cfg(feature = "png")]
pub mod raster;
pub mod svg;

/// PNG output without the `png` feature: an error that names the feature.
#[cfg(not(feature = "png"))]
pub mod raster {
    use crate::color::Color;
    use crate::draw::DrawGroup;
    use crate::error::{Error, Result};

    pub fn render_png(_: &[DrawGroup], _: u32, _: u32, _: Color) -> Result<Vec<u8>> {
        Err(Error::MissingFeature {
            output: "PNG",
            feature: "png",
        })
    }
}
