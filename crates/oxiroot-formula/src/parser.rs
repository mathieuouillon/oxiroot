//! A precedence-climbing (Pratt) parser turning a token stream into an [`Expr`],
//! including ROOT's `gaus`/`expo`/`polN` shortcut expansion.

use crate::expr::{BinOp, Expr, Func};
use crate::lexer::Token;
use crate::ParseError;

/// Parse result: the AST plus the parameter count and dimensionality inferred
/// from the highest `[i]` and the highest variable (`x`/`y`/`z`) used.
pub(crate) struct Parsed {
    pub(crate) expr: Expr,
    pub(crate) npar: usize,
    pub(crate) ndim: usize,
}

pub(crate) fn parse(tokens: Vec<Token>) -> Result<Parsed, ParseError> {
    let mut p = Parser {
        toks: tokens,
        pos: 0,
        max_par: -1,
        named: Vec::new(),
        max_var: 0,
        any_var: false,
    };
    let expr = p.expr_ternary()?;
    if p.pos != p.toks.len() {
        return Err(ParseError::new("trailing tokens after expression", p.pos));
    }
    let npar = (p.max_par + 1).max(p.named.len() as i64) as usize;
    let ndim = if p.any_var { p.max_var + 1 } else { 1 };
    Ok(Parsed { expr, npar, ndim })
}

struct Parser {
    toks: Vec<Token>,
    pos: usize,
    max_par: i64,
    named: Vec<String>,
    max_var: usize,
    any_var: bool,
}

impl Parser {
    fn peek(&self) -> Option<&Token> {
        self.toks.get(self.pos)
    }
    fn bump(&mut self) -> Option<Token> {
        let t = self.toks.get(self.pos).cloned();
        if t.is_some() {
            self.pos += 1;
        }
        t
    }
    fn eat(&mut self, t: &Token) -> Result<(), ParseError> {
        if self.peek() == Some(t) {
            self.pos += 1;
            Ok(())
        } else {
            Err(ParseError::new("unexpected token", self.pos))
        }
    }

    /// The lowest precedence: the right-associative ternary `cond ? a : b`.
    fn expr_ternary(&mut self) -> Result<Expr, ParseError> {
        let cond = self.expr_bp(0)?;
        if self.peek() == Some(&Token::Question) {
            self.pos += 1;
            let then = self.expr_ternary()?;
            self.eat(&Token::Colon)?;
            let otherwise = self.expr_ternary()?;
            Ok(Expr::Cond(
                Box::new(cond),
                Box::new(then),
                Box::new(otherwise),
            ))
        } else {
            Ok(cond)
        }
    }

    /// Precedence-climbing over the binary operators.
    fn expr_bp(&mut self, min_bp: u8) -> Result<Expr, ParseError> {
        let mut lhs = self.prefix()?;
        while let Some((op, l_bp, r_bp)) = self.peek().and_then(binop) {
            if l_bp < min_bp {
                break;
            }
            self.pos += 1;
            let rhs = self.expr_bp(r_bp)?;
            lhs = Expr::Bin(op, Box::new(lhs), Box::new(rhs));
        }
        Ok(lhs)
    }

    /// A unary prefix (`-`/`+`) or a primary.
    fn prefix(&mut self) -> Result<Expr, ParseError> {
        match self.peek() {
            Some(Token::Minus) => {
                self.pos += 1;
                Ok(Expr::Neg(Box::new(self.expr_bp(12)?)))
            }
            Some(Token::Plus) => {
                self.pos += 1;
                self.expr_bp(12)
            }
            _ => self.primary(),
        }
    }

    fn primary(&mut self) -> Result<Expr, ParseError> {
        match self.bump() {
            Some(Token::Num(v)) => Ok(Expr::Num(v)),
            Some(Token::Par(n)) => {
                self.max_par = self.max_par.max(n as i64);
                Ok(Expr::Par(n))
            }
            Some(Token::ParName(name)) => {
                let idx = self
                    .named
                    .iter()
                    .position(|n| n == &name)
                    .unwrap_or_else(|| {
                        self.named.push(name);
                        self.named.len() - 1
                    });
                Ok(Expr::Par(idx))
            }
            Some(Token::LParen) => {
                let e = self.expr_ternary()?;
                self.eat(&Token::RParen)?;
                Ok(e)
            }
            Some(Token::Ident(name)) => self.ident(&name),
            _ => Err(ParseError::new("expected an expression", self.pos)),
        }
    }

    /// Resolve an identifier: a variable, a ROOT shortcut, a `pi`, or a function.
    fn ident(&mut self, name: &str) -> Result<Expr, ParseError> {
        if let Some(v) = var_index(name) {
            self.max_var = self.max_var.max(v);
            self.any_var = true;
            return Ok(Expr::Var(v));
        }
        if let Some(e) = self.shortcut(name)? {
            return Ok(e);
        }
        if let Some((f, arity)) = Func::resolve(name) {
            // A zero-arg function (`pi`) may appear bare or as `pi()`.
            if arity == 0 {
                if self.peek() == Some(&Token::LParen) {
                    self.pos += 1;
                    self.eat(&Token::RParen)?;
                }
                return Ok(Expr::Call(f, Vec::new()));
            }
            self.eat(&Token::LParen)?;
            let mut args = vec![self.expr_ternary()?];
            while self.peek() == Some(&Token::Comma) {
                self.pos += 1;
                args.push(self.expr_ternary()?);
            }
            self.eat(&Token::RParen)?;
            if args.len() != arity {
                return Err(ParseError::new(
                    "wrong number of function arguments",
                    self.pos,
                ));
            }
            return Ok(Expr::Call(f, args));
        }
        Err(ParseError::new("unknown identifier", self.pos))
    }

    /// Expand a ROOT shortcut (`gaus`, `expo`, `polN`) if `name` is one, reading
    /// an optional `(offset)` for the first parameter index. Returns `None` when
    /// `name` is not a shortcut.
    fn shortcut(&mut self, name: &str) -> Result<Option<Expr>, ParseError> {
        let kind = if name == "gaus" {
            Shortcut::Gaus
        } else if name == "expo" {
            Shortcut::Expo
        } else if let Some(deg) = name
            .strip_prefix("pol")
            .and_then(|d| d.parse::<usize>().ok())
        {
            Shortcut::Pol(deg)
        } else {
            return Ok(None);
        };
        // Optional `(offset)`.
        let offset = if self.peek() == Some(&Token::LParen) {
            self.pos += 1;
            let off = match self.bump() {
                Some(Token::Num(v)) if v >= 0.0 && v.fract() == 0.0 => v as usize,
                _ => {
                    return Err(ParseError::new(
                        "shortcut offset must be a non-negative integer",
                        self.pos,
                    ))
                }
            };
            self.eat(&Token::RParen)?;
            off
        } else {
            0
        };
        self.any_var = true;
        let x = || Expr::Var(0);
        let par = |k: usize| Expr::Par(k);
        let count;
        let expr = match kind {
            Shortcut::Gaus => {
                // [k]*exp(-0.5*((x-[k+1])/[k+2])^2)
                count = 3;
                let z = Expr::Bin(
                    BinOp::Div,
                    Box::new(Expr::Bin(
                        BinOp::Sub,
                        Box::new(x()),
                        Box::new(par(offset + 1)),
                    )),
                    Box::new(par(offset + 2)),
                );
                let exponent = Expr::Bin(
                    BinOp::Mul,
                    Box::new(Expr::Num(-0.5)),
                    Box::new(Expr::Bin(BinOp::Pow, Box::new(z), Box::new(Expr::Num(2.0)))),
                );
                Expr::Bin(
                    BinOp::Mul,
                    Box::new(par(offset)),
                    Box::new(Expr::Call(Func::Exp, vec![exponent])),
                )
            }
            Shortcut::Expo => {
                // exp([k]+[k+1]*x)
                count = 2;
                let inner = Expr::Bin(
                    BinOp::Add,
                    Box::new(par(offset)),
                    Box::new(Expr::Bin(
                        BinOp::Mul,
                        Box::new(par(offset + 1)),
                        Box::new(x()),
                    )),
                );
                Expr::Call(Func::Exp, vec![inner])
            }
            Shortcut::Pol(deg) => {
                // [k] + [k+1]*x + [k+2]*x^2 + … + [k+deg]*x^deg  (Horner)
                count = deg + 1;
                let mut acc = par(offset + deg);
                for j in (0..deg).rev() {
                    acc = Expr::Bin(
                        BinOp::Add,
                        Box::new(par(offset + j)),
                        Box::new(Expr::Bin(BinOp::Mul, Box::new(x()), Box::new(acc))),
                    );
                }
                acc
            }
        };
        self.max_par = self.max_par.max((offset + count - 1) as i64);
        Ok(Some(expr))
    }
}

enum Shortcut {
    Gaus,
    Expo,
    Pol(usize),
}

/// The variable index for `x`/`y`/`z` (ROOT's first three dimensions).
fn var_index(name: &str) -> Option<usize> {
    match name {
        "x" => Some(0),
        "y" => Some(1),
        "z" => Some(2),
        _ => None,
    }
}

/// `(operator, left_bp, right_bp)` for a binary token, or `None` if it is not a
/// binary operator. Right-associative operators (`^`) have `right_bp < left_bp`.
fn binop(t: &Token) -> Option<(BinOp, u8, u8)> {
    Some(match t {
        Token::Or => (BinOp::Or, 1, 2),
        Token::And => (BinOp::And, 3, 4),
        Token::Lt => (BinOp::Lt, 5, 6),
        Token::Gt => (BinOp::Gt, 5, 6),
        Token::Le => (BinOp::Le, 5, 6),
        Token::Ge => (BinOp::Ge, 5, 6),
        Token::EqEq => (BinOp::Eq, 5, 6),
        Token::Ne => (BinOp::Ne, 5, 6),
        Token::Plus => (BinOp::Add, 7, 8),
        Token::Minus => (BinOp::Sub, 7, 8),
        Token::Star => (BinOp::Mul, 9, 10),
        Token::Slash => (BinOp::Div, 9, 10),
        Token::Caret => (BinOp::Pow, 14, 13),
        _ => return None,
    })
}
