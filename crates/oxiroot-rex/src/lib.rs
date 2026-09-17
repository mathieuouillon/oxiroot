//! A vendored subset of [ReX](https://github.com/KenyC/ReX), a TeX math layout
//! engine, used by `oxiroot-plot` to typeset `$…$` label spans.
//!
//! Upstream: KenyC/ReX at `aeccdba38f3fa54195c469319b65c423e17a77ae` (version 0.1.2),
//! with its `unicode-math` path dependency folded in as a private module. Only the
//! `ttf-parser` font backend is kept; the renderers, the `font` backend and the
//! `serde` derives are removed. See `README.md` and `LICENSE-3rdparty`.
//!
//! This crate is an implementation detail of `oxiroot-plot`. Its API is ReX's and
//! is not covered by oxiroot's semver guarantees.








#[macro_use]
mod macros;

#[deny(missing_docs)]
mod geometry;
#[deny(missing_docs)]
pub mod error;
#[deny(missing_docs)]
pub mod dimensions;
#[deny(missing_docs)]
pub mod layout;
#[warn(missing_docs)]
pub mod parser;
#[deny(missing_docs)]
pub mod render;

pub mod font;
mod unicode_math;

use font::MathFont;
use layout::engine::LayoutBuilder;
pub use render::*;

use crate::parser::parse;


/// Render a LateX formula to a given a surface `backend`, given a math font provided by `font_context`.
pub fn render<F : MathFont, B : Backend<F>>(formula : &str, backend : &mut B, font: &F) -> Result<(), crate::error::Error> {
    let parse_nodes = parse(formula)?;



    let layout = LayoutBuilder::new(font).layout(&parse_nodes)?;


    let renderer = Renderer::new();
    renderer.render(&layout, backend);
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::{font::backend::ttf_parser::TtfMathFont, layout::engine::LayoutBuilder, parser::parse};

    const GARAMOND_MATH_FONT : &[u8] = include_bytes!("../resources/Garamond_Math.otf");


    /// If the font's coverage of mathematical alphanumeric characters is exhaustive in all styles (as with Garamond-Math.otf, a.o.),
    /// then the library should not fail parsing and laying out on any of these.
    /// Test for bugs like [https://github.com/KenyC/ReX/issues/6](https://github.com/KenyC/ReX/issues/6)
    #[test]
    fn all_alphanumeric_style_combinations_must_work() {
        let font = ttf_parser::Face::parse(GARAMOND_MATH_FONT, 0).unwrap();
        let font = TtfMathFont::new(font).unwrap();

        let layout_engine = LayoutBuilder::new(&font).font_size(10.0).build();

        let alphanumeric : Vec<_> =
            (0 .. 0x7F)
            .filter_map(|i| std::primitive::char::from_u32(i))
            .filter(|c| c.is_alphanumeric())
            .collect();

        let envs = vec![
            None,
            Some("mathcal"),
            Some("mathrm"),
            Some("mathfrak"),
            Some("mathbb"),
        ];

        for env in envs {
            for character in alphanumeric.iter() {
                let formula; 
                if let Some(env) = env {
                    formula = format!(r"\{}{{{}}}", env, character)
                }
                else {
                    formula = character.to_string();
                }

                println!("{}", formula);
                let parse_nodes = parse(&formula).unwrap();
                layout_engine.layout(&parse_nodes).unwrap();
            }
        }
    }
}