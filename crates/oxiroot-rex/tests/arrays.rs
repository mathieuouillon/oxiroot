//! Array and matrix environments whose rows do not match their column count.
//! These are local patches (see README.md): upstream dropped cells of a ragged
//! matrix, and a debug build panicked when the column format and the widest row
//! disagreed.

use oxiroot_rex::font::backend::ttf_parser::TtfMathFont;
use oxiroot_rex::layout::engine::LayoutBuilder;
use oxiroot_rex::parser::error::ParseError;
use oxiroot_rex::parser::parse;

fn font() -> TtfMathFont<'static> {
    let path = format!("{}/resources/XITS_Math.otf", env!("CARGO_MANIFEST_DIR"));
    let bytes: &'static [u8] = std::fs::read(path).expect("read font").leak();
    TtfMathFont::new(ttf_parser::Face::parse(bytes, 0).expect("parse font")).expect("MATH table")
}

/// The laid-out width of `tex`.
fn width(font: &TtfMathFont<'_>, tex: &str) -> f64 {
    let nodes = parse(tex).expect("parse");
    let layout = LayoutBuilder::new(font)
        .font_size(12.0)
        .build()
        .layout(&nodes)
        .expect("layout");
    layout.size().width
}

#[test]
fn a_ragged_matrix_keeps_every_cell() {
    // The column count used to come from the last row, so `b` was dropped.
    let font = font();
    let ragged = width(&font, r"\begin{pmatrix} a & b \\ c \end{pmatrix}");
    let padded = width(&font, r"\begin{pmatrix} a & b \\ c & {} \end{pmatrix}");
    let one_column = width(&font, r"\begin{pmatrix} a \\ c \end{pmatrix}");
    assert!((ragged - padded).abs() < 1e-9, "{ragged} vs {padded}");
    assert!(ragged > one_column);
}

#[test]
fn an_array_row_with_too_many_cells_is_an_error() {
    assert_eq!(
        parse(r"\begin{array}{l} a & b \end{array}").err(),
        Some(ParseError::TooManyCellsInArrayRow {
            declared: 1,
            found: 2
        })
    );
    // A row with fewer cells than columns is fine; the rest are empty.
    let font = font();
    let short = width(&font, r"\begin{array}{ccc} a \end{array}");
    let full = width(&font, r"\begin{array}{ccc} a & {} & {} \end{array}");
    assert!((short - full).abs() < 1e-9, "{short} vs {full}");
}

#[test]
fn column_separators_count_with_the_declared_columns() {
    let font = font();
    for tex in [
        r"\begin{array}{c|c} a \end{array}",
        r"\begin{array}{c@{x}c} a \\ b & c \end{array}",
        r"\begin{aligned} a &= b \\ c \end{aligned}",
        r"\begin{matrix} \end{matrix}",
    ] {
        width(&font, tex);
    }
}
