//! Font configuration: the text family + math font used to render a figure.
//!
//! A [`FontSet`] bundles a text font (regular/bold/italic, drawn as glyph
//! outlines) and an OpenType **MATH** font used by the ReX engine to typeset
//! `$…$` spans. The default is **STIX Two** — a LaTeX-like serif paired with STIX
//! Two Math — giving plots a publication ("LaTeX") look out of the box. The
//! matplotlib sans-serif look is available as `FontSet::dejavu`, and any
//! TrueType/OpenType font can be supplied with `FontSet::from_font` /
//! `FontSet::from_path`.

use std::path::Path;
use std::sync::{Arc, OnceLock};

use ab_glyph::FontVec;

use crate::error::{Error, Result};
use crate::text::FontStyle;

// Bundled OFL fonts (see the matching `*-LICENSE.txt` in `assets/`).
static STIX_REGULAR: &[u8] = include_bytes!("../assets/STIXTwoText-Regular.otf");
static STIX_BOLD: &[u8] = include_bytes!("../assets/STIXTwoText-Bold.otf");
static STIX_ITALIC: &[u8] = include_bytes!("../assets/STIXTwoText-Italic.otf");
static STIX_MATH: &[u8] = include_bytes!("../assets/STIXTwoMath-Regular.otf");
static DEJAVU_REGULAR: &[u8] = include_bytes!("../assets/DejaVuSans.ttf");
static DEJAVU_BOLD: &[u8] = include_bytes!("../assets/DejaVuSans-Bold.ttf");
static DEJAVU_OBLIQUE: &[u8] = include_bytes!("../assets/DejaVuSans-Oblique.ttf");

/// The fonts used to draw a figure: a text family (regular/bold/italic) and an
/// OpenType MATH font for `$…$` spans.
///
/// Defaults to [`FontSet::stix`] (a LaTeX-like serif). It is cheap to clone
/// (the parsed faces are shared behind an `Arc`), so it lives on
/// [`Style`](crate::Style) and is carried by every panel.
///
/// # Examples
/// ```no_run
/// use oxiroot_plot::{Axes, FontSet};
/// // The matplotlib sans-serif look:
/// let mut ax = Axes::new();
/// ax.fonts(FontSet::dejavu());
/// // A custom text font from disk (STIX Two Math for the `$…$` spans):
/// let custom = FontSet::from_path("/path/to/MyFont.otf").unwrap();
/// ax.fonts(custom);
/// ```
#[derive(Clone)]
pub struct FontSet {
    inner: Arc<Inner>,
}

struct Inner {
    regular: FontVec,
    bold: FontVec,
    italic: FontVec,
    /// Raw bytes of the OpenType MATH font (parsed per math run by ReX).
    math: Vec<u8>,
}

impl std::fmt::Debug for FontSet {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FontSet").finish_non_exhaustive()
    }
}

impl Default for FontSet {
    fn default() -> Self {
        FontSet::stix()
    }
}

impl FontSet {
    /// The default LaTeX-like set: **STIX Two Text** + **STIX Two Math**.
    ///
    /// The parsed faces are built once and shared, so repeated calls are cheap.
    #[must_use]
    pub fn stix() -> FontSet {
        static CELL: OnceLock<FontSet> = OnceLock::new();
        CELL.get_or_init(|| from_static(STIX_REGULAR, STIX_BOLD, STIX_ITALIC, STIX_MATH))
            .clone()
    }

    /// The matplotlib look: **DejaVu Sans** text with STIX Two Math for `$…$`.
    #[must_use]
    pub fn dejavu() -> FontSet {
        static CELL: OnceLock<FontSet> = OnceLock::new();
        CELL.get_or_init(|| from_static(DEJAVU_REGULAR, DEJAVU_BOLD, DEJAVU_OBLIQUE, STIX_MATH))
            .clone()
    }

    /// A custom text font (used for regular/bold/italic) with the default STIX
    /// Two Math for `$…$` spans.
    ///
    /// # Errors
    /// If the bytes are not a valid TrueType/OpenType font.
    pub fn from_font(text: &[u8]) -> Result<FontSet> {
        Self::from_fonts(text, STIX_MATH)
    }

    /// A custom text font plus a custom math font (an OpenType font with a `MATH`
    /// table, e.g. STIX Two Math, XITS Math, Latin Modern Math).
    ///
    /// # Errors
    /// If either font cannot be parsed (the math font must have a `MATH` table).
    pub fn from_fonts(text: &[u8], math: &[u8]) -> Result<FontSet> {
        let load = |b: &[u8]| {
            FontVec::try_from_vec(b.to_vec())
                .map_err(|e| Error::Font(format!("invalid text font: {e}")))
        };
        // Validate the math font parses as a face (ReX re-reads the bytes later).
        ttf_parser::Face::parse(math, 0)
            .map_err(|e| Error::Font(format!("invalid math font: {e}")))?;
        Ok(FontSet {
            inner: Arc::new(Inner {
                regular: load(text)?,
                bold: load(text)?,
                italic: load(text)?,
                math: math.to_vec(),
            }),
        })
    }

    /// Load a custom text font from a file (STIX Two Math for `$…$` spans).
    ///
    /// # Errors
    /// On I/O failure or if the font cannot be parsed.
    pub fn from_path(text: impl AsRef<Path>) -> Result<FontSet> {
        let bytes = std::fs::read(text)?;
        Self::from_font(&bytes)
    }

    /// Load a custom text font and a custom math font from files.
    ///
    /// # Errors
    /// On I/O failure or if either font cannot be parsed.
    pub fn from_paths(text: impl AsRef<Path>, math: impl AsRef<Path>) -> Result<FontSet> {
        let text = std::fs::read(text)?;
        let math = std::fs::read(math)?;
        Self::from_fonts(&text, &math)
    }

    pub(crate) fn face(&self, style: FontStyle) -> &FontVec {
        match style {
            FontStyle::Regular => &self.inner.regular,
            FontStyle::Bold => &self.inner.bold,
            FontStyle::Italic => &self.inner.italic,
        }
    }

    pub(crate) fn math_bytes(&self) -> &[u8] {
        &self.inner.math
    }
}

fn from_static(r: &[u8], b: &[u8], i: &[u8], math: &[u8]) -> FontSet {
    let load = |bytes: &[u8]| FontVec::try_from_vec(bytes.to_vec()).expect("bundled font is valid");
    FontSet {
        inner: Arc::new(Inner {
            regular: load(r),
            bold: load(b),
            italic: load(i),
            math: math.to_vec(),
        }),
    }
}
