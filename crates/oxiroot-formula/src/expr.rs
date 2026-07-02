//! The expression AST and its evaluator.

/// A binary operator.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Pow,
    Lt,
    Gt,
    Le,
    Ge,
    Eq,
    Ne,
    And,
    Or,
}

/// A supported function, resolved from its name at parse time (so evaluation is
/// a cheap `match`, never a string lookup). ROOT's `TMath::` prefix and case are
/// normalized away by [`Func::resolve`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Func {
    Sin,
    Cos,
    Tan,
    ASin,
    ACos,
    ATan,
    SinH,
    CosH,
    TanH,
    ASinH,
    ACosH,
    ATanH,
    Exp,
    Expm1,
    Log,
    Log10,
    Log2,
    Sqrt,
    Cbrt,
    Abs,
    Floor,
    Ceil,
    Sign,
    ATan2,
    Pow,
    Hypot,
    Min,
    Max,
    Pi,
}

impl Func {
    /// Resolve a function name (after stripping a `TMath::` prefix and
    /// lowercasing) to a `(function, arity)`, or `None` if unknown.
    pub(crate) fn resolve(name: &str) -> Option<(Func, usize)> {
        let n = name
            .strip_prefix("TMath::")
            .unwrap_or(name)
            .to_ascii_lowercase();
        use Func::*;
        Some(match n.as_str() {
            "sin" => (Sin, 1),
            "cos" => (Cos, 1),
            "tan" => (Tan, 1),
            "asin" => (ASin, 1),
            "acos" => (ACos, 1),
            "atan" => (ATan, 1),
            "sinh" => (SinH, 1),
            "cosh" => (CosH, 1),
            "tanh" => (TanH, 1),
            "asinh" => (ASinH, 1),
            "acosh" => (ACosH, 1),
            "atanh" => (ATanH, 1),
            "exp" => (Exp, 1),
            "expm1" => (Expm1, 1),
            "log" | "ln" => (Log, 1),
            "log10" => (Log10, 1),
            "log2" => (Log2, 1),
            "sqrt" => (Sqrt, 1),
            "cbrt" => (Cbrt, 1),
            "abs" | "fabs" => (Abs, 1),
            "floor" => (Floor, 1),
            "ceil" => (Ceil, 1),
            "sign" => (Sign, 1),
            "atan2" => (ATan2, 2),
            "pow" | "power" => (Pow, 2),
            "hypot" => (Hypot, 2),
            "min" => (Min, 2),
            "max" => (Max, 2),
            "pi" => (Pi, 0),
            _ => return None,
        })
    }
}

/// An expression node. `Var(0/1/2)` is `x`/`y`/`z`; `Par(i)` is parameter `[i]`.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Expr {
    Num(f64),
    Var(usize),
    Par(usize),
    Neg(Box<Expr>),
    Bin(BinOp, Box<Expr>, Box<Expr>),
    Call(Func, Vec<Expr>),
    /// `cond ? then : otherwise`.
    Cond(Box<Expr>, Box<Expr>, Box<Expr>),
}

#[inline]
fn truthy(v: f64) -> bool {
    v != 0.0 && !v.is_nan()
}

impl Expr {
    /// Evaluate the node. `vars` is `[x]`/`[x,y]`/`[x,y,z]`; `params` holds the
    /// parameter values. Out-of-range variable/parameter indices read as 0.
    pub(crate) fn eval(&self, vars: &[f64], params: &[f64]) -> f64 {
        match self {
            Expr::Num(v) => *v,
            Expr::Var(i) => vars.get(*i).copied().unwrap_or(0.0),
            Expr::Par(i) => params.get(*i).copied().unwrap_or(0.0),
            Expr::Neg(e) => -e.eval(vars, params),
            Expr::Bin(op, a, b) => {
                let (l, r) = (a.eval(vars, params), b.eval(vars, params));
                match op {
                    BinOp::Add => l + r,
                    BinOp::Sub => l - r,
                    BinOp::Mul => l * r,
                    BinOp::Div => l / r,
                    BinOp::Pow => l.powf(r),
                    BinOp::Lt => (l < r) as u8 as f64,
                    BinOp::Gt => (l > r) as u8 as f64,
                    BinOp::Le => (l <= r) as u8 as f64,
                    BinOp::Ge => (l >= r) as u8 as f64,
                    BinOp::Eq => (l == r) as u8 as f64,
                    BinOp::Ne => (l != r) as u8 as f64,
                    BinOp::And => (truthy(l) && truthy(r)) as u8 as f64,
                    BinOp::Or => (truthy(l) || truthy(r)) as u8 as f64,
                }
            }
            Expr::Call(f, args) => {
                let a = |i: usize| args[i].eval(vars, params);
                use Func::*;
                match f {
                    Sin => a(0).sin(),
                    Cos => a(0).cos(),
                    Tan => a(0).tan(),
                    ASin => a(0).asin(),
                    ACos => a(0).acos(),
                    ATan => a(0).atan(),
                    SinH => a(0).sinh(),
                    CosH => a(0).cosh(),
                    TanH => a(0).tanh(),
                    ASinH => a(0).asinh(),
                    ACosH => a(0).acosh(),
                    ATanH => a(0).atanh(),
                    Exp => a(0).exp(),
                    Expm1 => a(0).exp_m1(),
                    Log => a(0).ln(),
                    Log10 => a(0).log10(),
                    Log2 => a(0).log2(),
                    Sqrt => a(0).sqrt(),
                    Cbrt => a(0).cbrt(),
                    Abs => a(0).abs(),
                    Floor => a(0).floor(),
                    Ceil => a(0).ceil(),
                    Sign => a(0).signum(),
                    ATan2 => a(0).atan2(a(1)),
                    Pow => a(0).powf(a(1)),
                    Hypot => a(0).hypot(a(1)),
                    Min => a(0).min(a(1)),
                    Max => a(0).max(a(1)),
                    Pi => std::f64::consts::PI,
                }
            }
            Expr::Cond(c, t, f) => {
                if truthy(c.eval(vars, params)) {
                    t.eval(vars, params)
                } else {
                    f.eval(vars, params)
                }
            }
        }
    }
}
