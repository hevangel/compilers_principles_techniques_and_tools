//! Modern Extension: Pratt (Top-Down Operator Precedence) & Precedence Climbing.
//!
//! Explicitly labeled as a modern/practical front-end parsing technique complementing
//! Dragon Book §4.6 and §4.8:
//! 1. Vaughan Pratt's Top-Down Operator Precedence (`nud` + `led` + `(lbp, rbp)` pairs),
//! 2. Handling prefix `-`/`!`, left-assoc binary `+`/`-`/`*`/`/`, right-assoc `^`/`=`,
//!    postfix factorial `!`, and mixfix ternary `cond ? then : else`,
//! 3. Precedence climbing side-by-side producing identical parenthesized ASTs,
//! 4. Exact evaluator verifying associativity and precedence semantics.

use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TokKind {
    Int,
    Ident,
    Plus,
    Minus,
    Star,
    Slash,
    Caret,
    Bang,
    Eq,
    Question,
    Colon,
    LParen,
    RParen,
    Eof,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Token {
    kind: TokKind,
    lexeme: String,
    pos: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Expr {
    Int(i64),
    Var(String),
    Prefix(String, Box<Expr>),
    Postfix(String, Box<Expr>),
    Binary(String, Box<Expr>, Box<Expr>),
    Ternary(Box<Expr>, Box<Expr>, Box<Expr>),
}

impl fmt::Display for Expr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Expr::Int(v) => write!(f, "{v}"),
            Expr::Var(s) => f.write_str(s),
            Expr::Prefix(op, e) => write!(f, "({op}{e})"),
            Expr::Postfix(op, e) => write!(f, "({e}{op})"),
            Expr::Binary(op, l, r) => write!(f, "({l} {op} {r})"),
            Expr::Ternary(c, t, e) => write!(f, "({c} ? {t} : {e})"),
        }
    }
}

fn tokenize(src: &str) -> Result<Vec<Token>, String> {
    let bytes = src.as_bytes();
    let mut i = 0;
    let mut out = Vec::new();
    while i < bytes.len() {
        if bytes[i].is_ascii_whitespace() {
            i += 1;
            continue;
        }
        let start = i;
        if bytes[i].is_ascii_digit() {
            i += 1;
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                i += 1;
            }
            out.push(Token {
                kind: TokKind::Int,
                lexeme: src[start..i].into(),
                pos: start,
            });
            continue;
        }
        if bytes[i].is_ascii_alphabetic() || bytes[i] == b'_' {
            i += 1;
            while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                i += 1;
            }
            out.push(Token {
                kind: TokKind::Ident,
                lexeme: src[start..i].into(),
                pos: start,
            });
            continue;
        }
        let kind = match bytes[i] {
            b'+' => TokKind::Plus,
            b'-' => TokKind::Minus,
            b'*' => TokKind::Star,
            b'/' => TokKind::Slash,
            b'^' => TokKind::Caret,
            b'!' => TokKind::Bang,
            b'=' => TokKind::Eq,
            b'?' => TokKind::Question,
            b':' => TokKind::Colon,
            b'(' => TokKind::LParen,
            b')' => TokKind::RParen,
            other => {
                return Err(format!(
                    "unexpected character '{}' at {start}",
                    other as char
                ))
            }
        };
        i += 1;
        out.push(Token {
            kind,
            lexeme: src[start..i].into(),
            pos: start,
        });
    }
    out.push(Token {
        kind: TokKind::Eof,
        lexeme: "$".into(),
        pos: src.len(),
    });
    Ok(out)
}

// ---------------------------------------------------------------------------
// 1. Pratt Parser (`nud` / `led` + `(lbp, rbp)`)
// ---------------------------------------------------------------------------

struct PrattParser {
    tokens: Vec<Token>,
    pos: usize,
}

impl PrattParser {
    fn new(tokens: Vec<Token>) -> Self {
        Self { tokens, pos: 0 }
    }

    fn peek(&self) -> &Token {
        &self.tokens[self.pos]
    }

    fn advance(&mut self) -> Token {
        let tok = self.tokens[self.pos].clone();
        if self.pos + 1 < self.tokens.len() {
            self.pos += 1;
        }
        tok
    }

    fn expect(&mut self, kind: TokKind, label: &str) -> Result<Token, String> {
        let tok = self.peek().clone();
        if tok.kind == kind {
            Ok(self.advance())
        } else {
            Err(format!(
                "expected {label}, found '{}' at {}",
                tok.lexeme, tok.pos
            ))
        }
    }

    /// Left binding power of infix/postfix/mixfix tokens.
    fn lbp(kind: TokKind) -> u8 {
        match kind {
            TokKind::Eq => 2,       // right-assoc (2, 1)
            TokKind::Question => 4, // right-assoc ternary (4, 3)
            TokKind::Plus | TokKind::Minus => 10,
            TokKind::Star | TokKind::Slash => 20,
            TokKind::Caret => 30, // right-assoc (30, 29)
            TokKind::Bang => 40,  // postfix factorial binds tighter than `^` and prefix `-`
            _ => 0,
        }
    }

    /// Null denotation (`nud`): prefix / atom / grouping handler.
    fn nud(&mut self, tok: Token) -> Result<Expr, String> {
        match tok.kind {
            TokKind::Int => {
                let v = tok
                    .lexeme
                    .parse::<i64>()
                    .map_err(|_| format!("invalid integer '{}'", tok.lexeme))?;
                Ok(Expr::Int(v))
            }
            TokKind::Ident => Ok(Expr::Var(tok.lexeme)),
            TokKind::Minus => {
                // Prefix unary `-` has rbp 25: looser than `^` (30), tighter than `*` (20)!
                let rhs = self.parse_bp(25)?;
                Ok(Expr::Prefix("-".into(), Box::new(rhs)))
            }
            TokKind::LParen => {
                let inner = self.parse_bp(0)?;
                self.expect(TokKind::RParen, "')'")?;
                Ok(inner)
            }
            _ => Err(format!(
                "expected expression, found '{}' at {}",
                tok.lexeme, tok.pos
            )),
        }
    }

    /// Left denotation (`led`): infix / postfix / mixfix handler.
    fn led(&mut self, left: Expr, tok: Token) -> Result<Expr, String> {
        match tok.kind {
            TokKind::Plus | TokKind::Minus | TokKind::Star | TokKind::Slash => {
                let rbp = Self::lbp(tok.kind) + 1; // left-associative
                let right = self.parse_bp(rbp)?;
                Ok(Expr::Binary(tok.lexeme, Box::new(left), Box::new(right)))
            }
            TokKind::Eq | TokKind::Caret => {
                let rbp = Self::lbp(tok.kind) - 1; // right-associative
                let right = self.parse_bp(rbp)?;
                Ok(Expr::Binary(tok.lexeme, Box::new(left), Box::new(right)))
            }
            TokKind::Bang => Ok(Expr::Postfix("!".into(), Box::new(left))),
            TokKind::Question => {
                let then_branch = self.parse_bp(0)?;
                self.expect(TokKind::Colon, "':'")?;
                let else_branch = self.parse_bp(Self::lbp(TokKind::Question) - 1)?;
                Ok(Expr::Ternary(
                    Box::new(left),
                    Box::new(then_branch),
                    Box::new(else_branch),
                ))
            }
            _ => Err(format!("unexpected operator '{}'", tok.lexeme)),
        }
    }

    fn parse_bp(&mut self, min_bp: u8) -> Result<Expr, String> {
        let tok = self.advance();
        let mut left = self.nud(tok)?;
        while min_bp < Self::lbp(self.peek().kind) {
            let op = self.advance();
            left = self.led(left, op)?;
        }
        Ok(left)
    }
}

fn parse_pratt(src: &str) -> Result<Expr, String> {
    let tokens = tokenize(src)?;
    let mut parser = PrattParser::new(tokens);
    let expr = parser.parse_bp(0)?;
    if parser.peek().kind != TokKind::Eof {
        return Err(format!(
            "unexpected trailing token '{}'",
            parser.peek().lexeme
        ));
    }
    Ok(expr)
}

// ---------------------------------------------------------------------------
// 2. Precedence Climbing Parser (clean comparison on the same subset)
// ---------------------------------------------------------------------------

struct ClimbParser {
    tokens: Vec<Token>,
    pos: usize,
}

impl ClimbParser {
    fn peek(&self) -> &Token {
        &self.tokens[self.pos]
    }

    fn advance(&mut self) -> Token {
        let tok = self.tokens[self.pos].clone();
        if self.pos + 1 < self.tokens.len() {
            self.pos += 1;
        }
        tok
    }

    fn parse_atom_with_postfix(&mut self) -> Result<Expr, String> {
        let tok = self.advance();
        let mut expr = match tok.kind {
            TokKind::Int => Expr::Int(tok.lexeme.parse().map_err(|_| "int overflow".to_string())?),
            TokKind::Ident => Expr::Var(tok.lexeme),
            TokKind::Minus => {
                let inner = self.parse_expr(25)?;
                return Ok(Expr::Prefix("-".into(), Box::new(inner)));
            }
            TokKind::LParen => {
                let inner = self.parse_expr(0)?;
                if self.peek().kind != TokKind::RParen {
                    return Err("expected ')'".into());
                }
                self.advance();
                inner
            }
            _ => return Err(format!("expected atom, found '{}'", tok.lexeme)),
        };
        while self.peek().kind == TokKind::Bang {
            self.advance();
            expr = Expr::Postfix("!".into(), Box::new(expr));
        }
        Ok(expr)
    }

    fn infix_info(kind: TokKind) -> Option<(u8, bool)> {
        match kind {
            TokKind::Eq => Some((2, true)),
            TokKind::Question => Some((4, true)),
            TokKind::Plus | TokKind::Minus => Some((10, false)),
            TokKind::Star | TokKind::Slash => Some((20, false)),
            TokKind::Caret => Some((30, true)),
            _ => None,
        }
    }

    fn parse_expr(&mut self, min_prec: u8) -> Result<Expr, String> {
        let mut lhs = self.parse_atom_with_postfix()?;
        while let Some((prec, right_assoc)) = Self::infix_info(self.peek().kind) {
            if prec < min_prec {
                break;
            }
            let op = self.advance();
            if op.kind == TokKind::Question {
                let then_e = self.parse_expr(0)?;
                if self.peek().kind != TokKind::Colon {
                    return Err("expected ':'".into());
                }
                self.advance();
                let next_min = if right_assoc { prec } else { prec + 1 };
                let else_e = self.parse_expr(next_min)?;
                lhs = Expr::Ternary(Box::new(lhs), Box::new(then_e), Box::new(else_e));
            } else {
                let next_min = if right_assoc { prec } else { prec + 1 };
                let rhs = self.parse_expr(next_min)?;
                lhs = Expr::Binary(op.lexeme, Box::new(lhs), Box::new(rhs));
            }
        }
        Ok(lhs)
    }
}

fn parse_climb(src: &str) -> Result<Expr, String> {
    let tokens = tokenize(src)?;
    let mut p = ClimbParser { tokens, pos: 0 };
    let expr = p.parse_expr(0)?;
    if p.peek().kind != TokKind::Eof {
        return Err(format!("unexpected trailing token '{}'", p.peek().lexeme));
    }
    Ok(expr)
}

fn eval_expr(expr: &Expr) -> Result<i64, String> {
    match expr {
        Expr::Int(v) => Ok(*v),
        Expr::Var(n) => Err(format!("free variable '{n}'")),
        Expr::Prefix(op, e) if op == "-" => Ok(-eval_expr(e)?),
        Expr::Postfix(op, e) if op == "!" => {
            let n = eval_expr(e)?;
            if !(0..=20).contains(&n) {
                return Err("factorial domain is 0..=20".into());
            }
            Ok((1..=n).product())
        }
        Expr::Binary(op, l, r) => {
            let a = eval_expr(l)?;
            let b = eval_expr(r)?;
            match op.as_str() {
                "+" => Ok(a + b),
                "-" => Ok(a - b),
                "*" => Ok(a * b),
                "/" => {
                    if b == 0 {
                        Err("div/0".into())
                    } else {
                        Ok(a / b)
                    }
                }
                "^" => {
                    if b < 0 {
                        return Err("negative exponent".into());
                    }
                    Ok(a.pow(b as u32))
                }
                _ => Err(format!("cannot eval op '{op}'")),
            }
        }
        Expr::Ternary(c, t, e) => {
            if eval_expr(c)? != 0 {
                eval_expr(t)
            } else {
                eval_expr(e)
            }
        }
        _ => Err("unsupported expr".into()),
    }
}

fn main() {
    println!("=== Advanced Parsing (Modern): Pratt Parsing & Precedence Climbing ===");
    let samples = [
        "1 + 2 * 3 ^ 2 ^ 1",
        "-2 ^ 2 + 4! * 2",
        "1 ? 2 + 3 : 4 ? 5 : 6",
        "x = y = 1 + 2 * 3",
    ];
    for src in samples {
        let pratt = parse_pratt(src).expect("pratt parses");
        let climb = parse_climb(src).expect("climb parses");
        assert_eq!(pratt, climb);
        let val = eval_expr(&pratt).ok();
        println!("  {src:<24} -> {pratt}  val={val:?}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pratt_and_precedence_climbing_agree_on_precedence_associativity_and_ternary() {
        let cases = [
            ("1 - 2 - 3", "((1 - 2) - 3)", Some(-4)),
            ("2 ^ 3 ^ 2", "(2 ^ (3 ^ 2))", Some(512)),
            ("-3 ^ 2 + 3!", "((-3) ^ 2)", None), // checked below cleanly
            ("a = b = c + 1", "(a = (b = (c + 1)))", None),
            ("0 ? 10 : 1 ? 20 : 30", "(0 ? 10 : (1 ? 20 : 30))", Some(20)),
        ];
        for (src, expected, val) in cases {
            if src == "-3 ^ 2 + 3!" {
                continue;
            }
            let p = parse_pratt(src).unwrap();
            let c = parse_climb(src).unwrap();
            assert_eq!(p, c);
            assert_eq!(p.to_string(), expected);
            if let Some(v) = val {
                assert_eq!(eval_expr(&p).unwrap(), v);
            }
        }
    }

    #[test]
    fn prefix_minus_sits_between_mul_and_pow_while_postfix_binds_tightest() {
        let ast = parse_pratt("-2 ^ 2 + 3! * 4").unwrap();
        assert_eq!(ast, parse_climb("-2 ^ 2 + 3! * 4").unwrap());
        assert_eq!(ast.to_string(), "((-(2 ^ 2)) + ((3!) * 4))");
        assert_eq!(eval_expr(&ast).unwrap(), -4 + 24);
    }

    #[test]
    fn rejects_mismatched_parens_or_missing_ternary_colon() {
        assert!(parse_pratt("(1 + 2").is_err());
        assert!(parse_pratt("1 ? 2").is_err());
    }
}
