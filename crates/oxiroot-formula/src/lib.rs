//! A ROOT [`TFormula`](https://root.cern/doc/master/classTFormula.html)
//! expression engine — parse an arbitrary formula string and evaluate it, plus
//! numerical integration and differentiation.
//!
//! Dependency-free leaf crate. It powers oxiroot's `Func1D`/`Func2D`/`Func3D` function
//! objects (evaluation, `Integral`, `Derivative`) and `oxiroot_fit`'s
//! `Model::from_formula`, so any ROOT formula can be evaluated and fitted.
//!
//! ```
//! use oxiroot_formula::Formula;
//! // parameters are [0], [1], …; variables are x (and y, z for 2-/3-D).
//! let f = Formula::parse("[0]*sin([1]*x) + [2]").unwrap();
//! assert_eq!(f.npar(), 3);
//! assert_eq!(f.ndim(), 1);
//! let y = f.eval1(1.0, &[2.0, 1.5, 0.5]); // 2*sin(1.5) + 0.5
//! assert!((y - 2.494_990).abs() < 1e-6);
//! ```
//!
//! Supported syntax: the operators `+ - * / ^` (and `**`) with the usual
//! precedence, comparisons/`&&`/`||` and a `?:` ternary (each comparison yields
//! `1.0`/`0.0`), the constants `pi`/`e`, the functions `sin cos tan asin acos
//! atan atan2 sinh cosh tanh exp log log10 log2 sqrt abs pow min max …` (with or
//! without a `TMath::` prefix), and ROOT's shortcuts `gaus`/`gaus(n)`,
//! `expo`/`expo(n)`, and `pol0`…`polN` (which expand to parameterised templates).

mod expr;
mod lexer;
mod numeric;
mod parser;

pub use numeric::{derivative, integrate};

use expr::Expr;

/// A parsed formula: an expression tree plus the parameter count and
/// dimensionality inferred from the highest `[i]` and variable it uses.
///
/// Two formulas compare equal when their canonical `[pN]` forms and
/// dimensionality match (so a formula built from `"[0]*x"` equals one read back
/// as `"[p0]*x"`).
#[derive(Debug, Clone)]
pub struct Formula {
    expr: Expr,
    npar: usize,
    ndim: usize,
    source: String,
    root_formula: String,
    param_names: Vec<String>,
}

impl Formula {
    /// Parse a ROOT formula string.
    ///
    /// # Errors
    /// Returns a [`ParseError`] if the string is not a valid formula.
    pub fn parse(source: &str) -> Result<Formula, ParseError> {
        let tokens = lexer::lex(source)?;
        let parsed = parser::parse(tokens)?;
        let param_names = (0..parsed.npar)
            .map(|i| format!("p{i}"))
            .collect::<Vec<_>>();
        Ok(Formula {
            expr: parsed.expr,
            npar: parsed.npar,
            ndim: parsed.ndim,
            source: source.to_string(),
            root_formula: normalize(source),
            param_names,
        })
    }

    /// Evaluate the formula. `vars` supplies `x` (`[x]`), `x,y` (`[x, y]`), or
    /// `x,y,z`; `params` supplies `[0], [1], …`. Missing entries read as `0`.
    #[must_use]
    pub fn eval(&self, vars: &[f64], params: &[f64]) -> f64 {
        self.expr.eval(vars, params)
    }

    /// Evaluate a 1-D formula at `x` (convenience for `eval(&[x], params)`).
    #[must_use]
    pub fn eval1(&self, x: f64, params: &[f64]) -> f64 {
        self.expr.eval(&[x], params)
    }

    /// The number of parameters (`highest [i] + 1`).
    #[must_use]
    pub fn npar(&self) -> usize {
        self.npar
    }

    /// The dimensionality: `1`/`2`/`3` for a formula in `x`, `x,y`, or `x,y,z`
    /// (a constant formula is 1-D).
    #[must_use]
    pub fn ndim(&self) -> usize {
        self.ndim
    }

    /// The original formula string, as passed to [`parse`](Formula::parse) —
    /// ROOT stores this as the function's title (`fTitle`).
    #[must_use]
    pub fn source(&self) -> &str {
        &self.source
    }

    /// The formula in ROOT's canonical `[pN]` parameter form with whitespace
    /// removed — what ROOT stores as `TFormula::fFormula`.
    #[must_use]
    pub fn root_formula(&self) -> &str {
        &self.root_formula
    }

    /// The parameter names in index order (`p0`, `p1`, …) — ROOT's
    /// `TFormula::fParams` keys.
    #[must_use]
    pub fn param_names(&self) -> &[String] {
        &self.param_names
    }
}

/// Normalize a formula to ROOT's `fFormula` form: drop whitespace and rewrite
/// each positional parameter `[k]` (all-digit) to `[pk]`. Named parameters are
/// left as written.
fn normalize(src: &str) -> String {
    let mut out = String::with_capacity(src.len());
    let bytes = src.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b' ' | b'\t' | b'\n' | b'\r' => i += 1,
            b'[' => {
                if let Some(rel) = src[i + 1..].find(']') {
                    let end = i + 1 + rel;
                    let inner = src[i + 1..end].trim();
                    if !inner.is_empty() && inner.bytes().all(|c| c.is_ascii_digit()) {
                        out.push_str("[p");
                        out.push_str(inner);
                        out.push(']');
                    } else {
                        out.push('[');
                        out.push_str(inner);
                        out.push(']');
                    }
                    i = end + 1;
                } else {
                    out.push('[');
                    i += 1;
                }
            }
            c => {
                out.push(c as char);
                i += 1;
            }
        }
    }
    out
}

/// A formula parse error, with a byte offset into the source string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    message: &'static str,
    position: usize,
}

impl ParseError {
    pub(crate) fn new(message: &'static str, position: usize) -> ParseError {
        ParseError { message, position }
    }
    /// The byte offset into the source where parsing failed.
    #[must_use]
    pub fn position(&self) -> usize {
        self.position
    }
}

impl PartialEq for Formula {
    fn eq(&self, other: &Self) -> bool {
        self.root_formula == other.root_formula && self.ndim == other.ndim
    }
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} (at position {})", self.message, self.position)
    }
}

impl std::error::Error for ParseError {}

#[cfg(test)]
mod tests {
    use super::*;

    const EPS: f64 = 1e-9;

    #[test]
    fn parses_and_evaluates_against_root() {
        // ROOT: Func1D f("f","[0]*sin([1]*x) + [2]"); SetParameters(2,1.5,0.5)
        let f = Formula::parse("[0]*sin([1]*x) + [2]").unwrap();
        assert_eq!(f.npar(), 3);
        assert_eq!(f.ndim(), 1);
        assert_eq!(f.root_formula(), "[p0]*sin([p1]*x)+[p2]");
        let p = [2.0, 1.5, 0.5];
        assert!((f.eval1(1.0, &p) - 2.494_990).abs() < 1e-6); // ROOT Eval(1)
                                                              // ROOT Integral(0, pi) = 2.904130 ; Derivative(1) = 0.212212
        assert!(
            (integrate(|x| f.eval1(x, &p), 0.0, std::f64::consts::PI) - 2.904_130).abs() < 1e-5
        );
        assert!((derivative(|x| f.eval1(x, &p), 1.0) - 3.0 * 1.5_f64.cos()).abs() < EPS);
    }

    #[test]
    fn operator_precedence_and_functions() {
        let f = Formula::parse("2 + 3 * 4 ^ 2").unwrap();
        assert_eq!(f.eval1(0.0, &[]), 2.0 + 3.0 * 16.0); // ^ before *, * before +
        assert_eq!(Formula::parse("-x^2").unwrap().eval1(3.0, &[]), -9.0); // -(x^2)
        assert_eq!(Formula::parse("2**3").unwrap().eval1(0.0, &[]), 8.0); // ** is pow
        let g = Formula::parse("sqrt(x) + pow(x, 3) + TMath::Abs(-x)").unwrap();
        assert!((g.eval1(4.0, &[]) - (2.0 + 64.0 + 4.0)).abs() < EPS);
        // comparison yields 1/0, ternary picks a branch.
        assert_eq!(Formula::parse("(x > 0) * 5").unwrap().eval1(2.0, &[]), 5.0);
        assert_eq!(Formula::parse("(x > 0) * 5").unwrap().eval1(-2.0, &[]), 0.0);
        assert_eq!(
            Formula::parse("x > 0 ? x : -x").unwrap().eval1(-3.0, &[]),
            3.0
        );
        assert!((Formula::parse("pi").unwrap().eval1(0.0, &[]) - std::f64::consts::PI).abs() < EPS);
    }

    #[test]
    fn root_shortcuts_expand() {
        // gaus: [0]*exp(-0.5*((x-[1])/[2])^2)
        let g = Formula::parse("gaus").unwrap();
        assert_eq!(g.npar(), 3);
        assert_eq!(g.eval1(0.0, &[2.0, 0.0, 1.0]), 2.0);
        assert!((g.eval1(1.0, &[2.0, 0.0, 1.0]) - 2.0 * (-0.5f64).exp()).abs() < EPS);
        // expo: exp([0]+[1]*x)
        let e = Formula::parse("expo").unwrap();
        assert_eq!(e.npar(), 2);
        assert!((e.eval1(1.0, &[0.0, 1.0]) - std::f64::consts::E).abs() < EPS);
        // pol2: [0]+[1]*x+[2]*x^2
        let p = Formula::parse("pol2").unwrap();
        assert_eq!(p.npar(), 3);
        assert_eq!(p.eval1(2.0, &[1.0, 2.0, 3.0]), 1.0 + 4.0 + 12.0);
        // offsets: gaus(0) + pol1(3) uses [0..2] and [3..4]
        let sum = Formula::parse("gaus(0) + pol1(3)").unwrap();
        assert_eq!(sum.npar(), 5);
    }

    #[test]
    fn multidimensional_matches_root() {
        // ROOT Func2D f2("f2","[0]*sin(x) + [1]*y*y"); Eval(1,1) = 1.962206
        let f2 = Formula::parse("[0]*sin(x) + [1]*y*y").unwrap();
        assert_eq!(f2.ndim(), 2);
        assert!((f2.eval(&[1.0, 1.0], &[1.5, 0.7]) - 1.962_206).abs() < 1e-5);
        // ROOT Func3D f3("f3","[0]*x + y*z"); Eval(1,1,1) = 3.0
        let f3 = Formula::parse("[0]*x + y*z").unwrap();
        assert_eq!(f3.ndim(), 3);
        assert_eq!(f3.eval(&[1.0, 1.0, 1.0], &[2.0]), 3.0);
    }

    #[test]
    fn integrate_and_derivative_exact_cases() {
        // ∫₀¹ x² dx = 1/3 ; d/dx x³ at 2 = 12
        assert!((integrate(|x| x * x, 0.0, 1.0) - 1.0 / 3.0).abs() < EPS);
        assert!((integrate(|x| x.sin(), 0.0, std::f64::consts::PI) - 2.0).abs() < EPS);
        assert!((derivative(|x| x * x * x, 2.0) - 12.0).abs() < 1e-6);
        // reversed limits negate.
        assert!((integrate(|x| x, 1.0, 0.0) + 0.5).abs() < EPS);
    }

    #[test]
    fn rejects_bad_formulas() {
        assert!(Formula::parse("1 +").is_err());
        assert!(Formula::parse("sin(").is_err());
        assert!(Formula::parse("[0]*bogus(x)").is_err());
        assert!(Formula::parse("1 2").is_err());
        assert!(Formula::parse("[]").is_err());
    }
}
