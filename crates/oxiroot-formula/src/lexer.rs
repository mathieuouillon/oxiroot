//! Tokenizer for ROOT `TFormula` expressions.

use crate::ParseError;

/// A lexical token.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Token {
    /// A numeric literal (`1.5`, `2e-3`, `.5`).
    Num(f64),
    /// An identifier: a variable (`x`/`y`/`z`), constant (`pi`/`e`), or function
    /// name (`sin`, `TMath::Sqrt`). Kept verbatim (case-sensitive match later).
    Ident(String),
    /// A positional parameter `[0]`, `[1]`, …
    Par(usize),
    /// A named parameter `[mean]` — resolved to an index by first appearance.
    ParName(String),
    Plus,
    Minus,
    Star,
    Slash,
    /// Exponentiation, from `^` or `**`.
    Caret,
    LParen,
    RParen,
    Comma,
    Lt,
    Gt,
    Le,
    Ge,
    EqEq,
    Ne,
    And,
    Or,
    Question,
    Colon,
}

/// Turn `src` into a token stream, or a [`ParseError`] on an illegal character.
pub(crate) fn lex(src: &str) -> Result<Vec<Token>, ParseError> {
    let bytes = src.as_bytes();
    let mut i = 0;
    let mut out = Vec::new();
    while i < bytes.len() {
        let c = bytes[i];
        match c {
            b' ' | b'\t' | b'\n' | b'\r' => i += 1,
            b'+' => {
                out.push(Token::Plus);
                i += 1;
            }
            b'-' => {
                out.push(Token::Minus);
                i += 1;
            }
            b'*' => {
                if bytes.get(i + 1) == Some(&b'*') {
                    out.push(Token::Caret); // `**` is power, like `^`
                    i += 2;
                } else {
                    out.push(Token::Star);
                    i += 1;
                }
            }
            b'/' => {
                out.push(Token::Slash);
                i += 1;
            }
            b'^' => {
                out.push(Token::Caret);
                i += 1;
            }
            b'(' => {
                out.push(Token::LParen);
                i += 1;
            }
            b')' => {
                out.push(Token::RParen);
                i += 1;
            }
            b',' => {
                out.push(Token::Comma);
                i += 1;
            }
            b'<' => {
                if bytes.get(i + 1) == Some(&b'=') {
                    out.push(Token::Le);
                    i += 2;
                } else {
                    out.push(Token::Lt);
                    i += 1;
                }
            }
            b'>' => {
                if bytes.get(i + 1) == Some(&b'=') {
                    out.push(Token::Ge);
                    i += 2;
                } else {
                    out.push(Token::Gt);
                    i += 1;
                }
            }
            b'=' if bytes.get(i + 1) == Some(&b'=') => {
                out.push(Token::EqEq);
                i += 2;
            }
            b'!' if bytes.get(i + 1) == Some(&b'=') => {
                out.push(Token::Ne);
                i += 2;
            }
            b'&' if bytes.get(i + 1) == Some(&b'&') => {
                out.push(Token::And);
                i += 2;
            }
            b'|' if bytes.get(i + 1) == Some(&b'|') => {
                out.push(Token::Or);
                i += 2;
            }
            b'?' => {
                out.push(Token::Question);
                i += 1;
            }
            b':' if bytes.get(i + 1) == Some(&b':') => {
                // `::` inside `TMath::Sin` — glue it onto the current identifier
                // if one is being built; otherwise it is illegal.
                return Err(ParseError::new("unexpected `::`", i));
            }
            b':' => {
                out.push(Token::Colon);
                i += 1;
            }
            b'[' => {
                let start = i + 1;
                let end = src[start..]
                    .find(']')
                    .map(|p| start + p)
                    .ok_or_else(|| ParseError::new("unclosed `[` in parameter", i))?;
                let inner = src[start..end].trim();
                if inner.is_empty() {
                    return Err(ParseError::new("empty parameter `[]`", i));
                }
                if let Ok(n) = inner.parse::<usize>() {
                    out.push(Token::Par(n));
                } else {
                    out.push(Token::ParName(inner.to_string()));
                }
                i = end + 1;
            }
            b'0'..=b'9' | b'.' => {
                let start = i;
                let mut seen_dot = false;
                let mut seen_exp = false;
                while i < bytes.len() {
                    match bytes[i] {
                        b'0'..=b'9' => i += 1,
                        b'.' if !seen_dot && !seen_exp => {
                            seen_dot = true;
                            i += 1;
                        }
                        b'e' | b'E' if !seen_exp => {
                            seen_exp = true;
                            i += 1;
                            if matches!(bytes.get(i), Some(b'+' | b'-')) {
                                i += 1;
                            }
                        }
                        _ => break,
                    }
                }
                let text = &src[start..i];
                let val = text
                    .parse::<f64>()
                    .map_err(|_| ParseError::new("invalid number", start))?;
                out.push(Token::Num(val));
            }
            b'A'..=b'Z' | b'a'..=b'z' | b'_' => {
                let start = i;
                while i < bytes.len() {
                    match bytes[i] {
                        b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'_' => i += 1,
                        // Absorb `::` so `TMath::Sin` is one identifier.
                        b':' if bytes.get(i + 1) == Some(&b':') => i += 2,
                        _ => break,
                    }
                }
                out.push(Token::Ident(src[start..i].to_string()));
            }
            _ => return Err(ParseError::new("illegal character", i)),
        }
    }
    Ok(out)
}
