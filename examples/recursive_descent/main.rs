//! Chapter 4 (§4.4.1): Recursive-Descent Parsing.
//!
//! Demonstrates handwritten top-down predictive parsing in direct 1-to-1
//! correspondence with an EBNF grammar (rewritten to eliminate left recursion):
//! - Mapping each nonterminal (`Program`, `Stmt`, `Expr`, `Term`, `Power`, `Factor`)
//!   to a recursive parsing function,
//! - Using `while` loops for left-associative repetition (`+`, `-`, `*`, `/`)
//!   without stack overflow, and right recursion for right-associative `^`,
//! - Comparing 1-token (`LL(1)`) lookahead vs. 2-token (`LL(2)`) lookahead to
//!   disambiguate `ident := expr;` (assignment) from `ident(args);` (call),
//! - Dangling-else resolution inside nested `if ... then ... else ...` blocks,
//! - Crisp error diagnostics, `;`/`}` panic-mode recovery across statements,
//!   and explicit recursion-depth limits against stack overflow.

use std::collections::BTreeMap;
use std::fmt;

// ---------------------------------------------------------------------------
// 1. Lexer (Characters -> Typed Tokens with Byte Offsets)
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq)]
enum TokKind {
    Let,
    If,
    Then,
    Else,
    Ident(String),
    Int(i64),
    Assign, // :=
    Plus,   // +
    Minus,  // -
    Star,   // *
    Slash,  // /
    Caret,  // ^
    LParen, // (
    RParen, // )
    LBrace, // {
    RBrace, // }
    Comma,  // ,
    Semi,   // ;
    Eof,
}

impl fmt::Display for TokKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TokKind::Let => write!(f, "'let'"),
            TokKind::If => write!(f, "'if'"),
            TokKind::Then => write!(f, "'then'"),
            TokKind::Else => write!(f, "'else'"),
            TokKind::Ident(s) => write!(f, "ident '{s}'"),
            TokKind::Int(n) => write!(f, "int {n}"),
            TokKind::Assign => write!(f, "':='"),
            TokKind::Plus => write!(f, "'+'"),
            TokKind::Minus => write!(f, "'-'"),
            TokKind::Star => write!(f, "'*'"),
            TokKind::Slash => write!(f, "'/'"),
            TokKind::Caret => write!(f, "'^'"),
            TokKind::LParen => write!(f, "'('"),
            TokKind::RParen => write!(f, "')'"),
            TokKind::LBrace => write!(f, "'{{'"),
            TokKind::RBrace => write!(f, "'}}'"),
            TokKind::Comma => write!(f, "','"),
            TokKind::Semi => write!(f, "';'"),
            TokKind::Eof => write!(f, "end-of-input"),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Token {
    kind: TokKind,
    pos: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Diagnostic {
    pos: usize,
    msg: String,
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "offset {}: {}", self.pos, self.msg)
    }
}

fn lex(src: &str) -> Result<Vec<Token>, Diagnostic> {
    let bytes = src.as_bytes();
    let mut i = 0;
    let mut out = Vec::new();

    while i < bytes.len() {
        let b = bytes[i];
        if b.is_ascii_whitespace() {
            i += 1;
            continue;
        }
        let start = i;
        if b.is_ascii_alphabetic() || b == b'_' {
            i += 1;
            while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                i += 1;
            }
            let word = &src[start..i];
            let kind = match word {
                "let" => TokKind::Let,
                "if" => TokKind::If,
                "then" => TokKind::Then,
                "else" => TokKind::Else,
                _ => TokKind::Ident(word.to_string()),
            };
            out.push(Token { kind, pos: start });
            continue;
        }
        if b.is_ascii_digit() {
            i += 1;
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                i += 1;
            }
            let val: i64 = src[start..i].parse().map_err(|_| Diagnostic {
                pos: start,
                msg: format!("integer literal out of range: '{}'", &src[start..i]),
            })?;
            out.push(Token {
                kind: TokKind::Int(val),
                pos: start,
            });
            continue;
        }
        let kind = match b {
            b':' if i + 1 < bytes.len() && bytes[i + 1] == b'=' => {
                i += 2;
                TokKind::Assign
            }
            b'+' => {
                i += 1;
                TokKind::Plus
            }
            b'-' => {
                i += 1;
                TokKind::Minus
            }
            b'*' => {
                i += 1;
                TokKind::Star
            }
            b'/' => {
                i += 1;
                TokKind::Slash
            }
            b'^' => {
                i += 1;
                TokKind::Caret
            }
            b'(' => {
                i += 1;
                TokKind::LParen
            }
            b')' => {
                i += 1;
                TokKind::RParen
            }
            b'{' => {
                i += 1;
                TokKind::LBrace
            }
            b'}' => {
                i += 1;
                TokKind::RBrace
            }
            b',' => {
                i += 1;
                TokKind::Comma
            }
            b';' => {
                i += 1;
                TokKind::Semi
            }
            _ => {
                return Err(Diagnostic {
                    pos: start,
                    msg: format!("unexpected character '{}'", b as char),
                });
            }
        };
        out.push(Token { kind, pos: start });
    }

    out.push(Token {
        kind: TokKind::Eof,
        pos: src.len(),
    });
    Ok(out)
}

// ---------------------------------------------------------------------------
// 2. AST Definitions
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq)]
enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Pow,
}

impl fmt::Display for BinOp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            BinOp::Add => "+",
            BinOp::Sub => "-",
            BinOp::Mul => "*",
            BinOp::Div => "/",
            BinOp::Pow => "^",
        };
        write!(f, "{s}")
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Expr {
    Int(i64),
    Var(String),
    Neg(Box<Expr>),
    Bin {
        op: BinOp,
        left: Box<Expr>,
        right: Box<Expr>,
    },
}

impl fmt::Display for Expr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Expr::Int(n) => write!(f, "{n}"),
            Expr::Var(name) => write!(f, "{name}"),
            Expr::Neg(inner) => write!(f, "(-{inner})"),
            Expr::Bin { op, left, right } => write!(f, "({left} {op} {right})"),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Stmt {
    Let {
        name: String,
        expr: Expr,
    },
    Assign {
        name: String,
        expr: Expr,
    },
    Call {
        callee: String,
        args: Vec<Expr>,
    },
    If {
        cond: Expr,
        then_br: Box<Stmt>,
        else_br: Option<Box<Stmt>>,
    },
    Block(Vec<Stmt>),
}

// ---------------------------------------------------------------------------
// 3. EBNF Grammar -> Handwritten Recursive-Descent Parser (§4.4.1)
// ---------------------------------------------------------------------------
//
// Rewritten (left-recursion-free) EBNF grammar implemented below:
//
//   Program -> Stmt* Eof
//   Stmt    -> 'let' Ident ':=' Expr ';'
//           |  Ident ':=' Expr ';'                  (LL(2): Ident followed by ':=')
//           |  Ident '(' [Expr (',' Expr)*] ')' ';' (LL(2): Ident followed by '(')
//           |  'if' Expr 'then' Stmt ['else' Stmt]  (dangling-else eager match)
//           |  '{' Stmt* '}'
//   Expr    -> Term   { ('+' | '-') Term }          (while loop -> left-assoc)
//   Term    -> Unary  { ('*' | '/') Unary }         (while loop -> left-assoc)
//   Unary   -> '-' Unary | Power                    (prefix negation)
//   Power   -> Atom   [ '^' Unary ]                 (right recursion -> right-assoc)
//   Atom    -> Int | Ident | '(' Expr ')'

struct Parser {
    tokens: Vec<Token>,
    pos: usize,
    depth: usize,
    max_depth: usize,
    lookahead_trace: Vec<String>,
    errors: Vec<Diagnostic>,
}

impl Parser {
    fn new(tokens: Vec<Token>, max_depth: usize) -> Self {
        Self {
            tokens,
            pos: 0,
            depth: 0,
            max_depth,
            lookahead_trace: Vec::new(),
            errors: Vec::new(),
        }
    }

    /// Look `k` tokens ahead (`k = 0` is standard LL(1); `k = 1` gives LL(2)).
    fn peek(&self, k: usize) -> &Token {
        let idx = (self.pos + k).min(self.tokens.len() - 1);
        &self.tokens[idx]
    }

    fn advance(&mut self) -> Token {
        let tok = self.tokens[self.pos].clone();
        if self.pos + 1 < self.tokens.len() {
            self.pos += 1;
        }
        tok
    }

    fn expect(&mut self, want: &TokKind, label: &str) -> Result<Token, Diagnostic> {
        let tok = self.peek(0).clone();
        if &tok.kind == want {
            Ok(self.advance())
        } else {
            Err(Diagnostic {
                pos: tok.pos,
                msg: format!("expected {label}, found {}", tok.kind),
            })
        }
    }

    fn expect_ident(&mut self) -> Result<String, Diagnostic> {
        let tok = self.peek(0).clone();
        match tok.kind {
            TokKind::Ident(name) => {
                self.advance();
                Ok(name)
            }
            _ => Err(Diagnostic {
                pos: tok.pos,
                msg: format!("expected identifier, found {}", tok.kind),
            }),
        }
    }

    fn enter(&mut self, rule: &str) -> Result<(), Diagnostic> {
        self.depth += 1;
        if self.depth > self.max_depth {
            let tok = self.peek(0);
            return Err(Diagnostic {
                pos: tok.pos,
                msg: format!(
                    "recursion depth limit ({}) exceeded while parsing {rule}",
                    self.max_depth
                ),
            });
        }
        Ok(())
    }

    fn leave(&mut self) {
        self.depth -= 1;
    }

    /// Panic-mode recovery: skip tokens until a statement boundary (`';'` or `'}'` or Eof).
    fn synchronize_to_stmt(&mut self) {
        while !matches!(
            self.peek(0).kind,
            TokKind::Semi | TokKind::RBrace | TokKind::Eof
        ) {
            self.advance();
        }
        if self.peek(0).kind == TokKind::Semi {
            self.advance();
        }
    }

    /// `Program -> Stmt* Eof`
    fn parse_program(&mut self) -> Vec<Stmt> {
        let mut stmts = Vec::new();
        while self.peek(0).kind != TokKind::Eof {
            match self.parse_stmt() {
                Ok(s) => stmts.push(s),
                Err(err) => {
                    self.errors.push(err);
                    self.synchronize_to_stmt();
                }
            }
        }
        stmts
    }

    /// `Stmt -> let | Ident:= (LL(2)) | Ident(...) (LL(2)) | if/then[/else] | { Stmt* }`
    fn parse_stmt(&mut self) -> Result<Stmt, Diagnostic> {
        self.enter("Stmt")?;
        let res = self.parse_stmt_inner();
        self.leave();
        res
    }

    fn parse_stmt_inner(&mut self) -> Result<Stmt, Diagnostic> {
        let tok0 = self.peek(0).clone();
        match &tok0.kind {
            TokKind::Let => {
                self.lookahead_trace
                    .push(format!("LL(1) peek(0)='let' -> Let stmt @{}", tok0.pos));
                self.advance();
                let name = self.expect_ident()?;
                self.expect(&TokKind::Assign, "':='")?;
                let expr = self.parse_expr()?;
                self.expect(&TokKind::Semi, "';'")?;
                Ok(Stmt::Let { name, expr })
            }
            TokKind::Ident(name) => {
                let name = name.clone();
                // LL(2) decision: peek(0) alone is Ident for BOTH assignment and call!
                // Checking peek(1) resolves `name := ...` vs `name(...)` with zero backtracking.
                let tok1 = self.peek(1).clone();
                match tok1.kind {
                    TokKind::Assign => {
                        self.lookahead_trace.push(format!(
                            "LL(2) peek(0)=ident '{name}', peek(1)=':=' -> Assign stmt @{}",
                            tok0.pos
                        ));
                        self.advance();
                        self.advance();
                        let expr = self.parse_expr()?;
                        self.expect(&TokKind::Semi, "';'")?;
                        Ok(Stmt::Assign { name, expr })
                    }
                    TokKind::LParen => {
                        self.lookahead_trace.push(format!(
                            "LL(2) peek(0)=ident '{name}', peek(1)='(' -> Call stmt @{}",
                            tok0.pos
                        ));
                        self.advance();
                        self.advance();
                        let mut args = Vec::new();
                        if self.peek(0).kind != TokKind::RParen {
                            loop {
                                args.push(self.parse_expr()?);
                                if self.peek(0).kind == TokKind::Comma {
                                    self.advance();
                                } else {
                                    break;
                                }
                            }
                        }
                        self.expect(&TokKind::RParen, "')'")?;
                        self.expect(&TokKind::Semi, "';'")?;
                        Ok(Stmt::Call { callee: name, args })
                    }
                    _ => Err(Diagnostic {
                        pos: tok1.pos,
                        msg: format!(
                            "expected ':=' or '(' after identifier '{name}', found {}",
                            tok1.kind
                        ),
                    }),
                }
            }
            TokKind::If => {
                self.lookahead_trace
                    .push(format!("LL(1) peek(0)='if' -> If stmt @{}", tok0.pos));
                self.advance();
                let cond = self.parse_expr()?;
                self.expect(&TokKind::Then, "'then'")?;
                let then_br = Box::new(self.parse_stmt()?);
                // Dangling-else rule: eagerly attach `else` to the innermost open `if`.
                let else_br = if self.peek(0).kind == TokKind::Else {
                    self.advance();
                    Some(Box::new(self.parse_stmt()?))
                } else {
                    None
                };
                Ok(Stmt::If {
                    cond,
                    then_br,
                    else_br,
                })
            }
            TokKind::LBrace => {
                self.lookahead_trace
                    .push(format!("LL(1) peek(0)='{{' -> Block stmt @{}", tok0.pos));
                self.advance();
                let mut body = Vec::new();
                while !matches!(self.peek(0).kind, TokKind::RBrace | TokKind::Eof) {
                    match self.parse_stmt() {
                        Ok(s) => body.push(s),
                        Err(err) => {
                            self.errors.push(err);
                            self.synchronize_to_stmt();
                        }
                    }
                }
                self.expect(&TokKind::RBrace, "'}'")?;
                Ok(Stmt::Block(body))
            }
            _ => Err(Diagnostic {
                pos: tok0.pos,
                msg: format!("unexpected token {} starting statement", tok0.kind),
            }),
        }
    }

    /// `Expr -> Term { ('+' | '-') Term }`
    /// Left recursion `Expr -> Expr + Term` was eliminated by factoring repetition
    /// into a `while` loop that folds left-to-right (`((a - b) - c)`).
    fn parse_expr(&mut self) -> Result<Expr, Diagnostic> {
        self.enter("Expr")?;
        let res = (|| {
            let mut acc = self.parse_term()?;
            while matches!(self.peek(0).kind, TokKind::Plus | TokKind::Minus) {
                let op = if self.advance().kind == TokKind::Plus {
                    BinOp::Add
                } else {
                    BinOp::Sub
                };
                let rhs = self.parse_term()?;
                acc = Expr::Bin {
                    op,
                    left: Box::new(acc),
                    right: Box::new(rhs),
                };
            }
            Ok(acc)
        })();
        self.leave();
        res
    }

    /// `Term -> Unary { ('*' | '/') Unary }`
    fn parse_term(&mut self) -> Result<Expr, Diagnostic> {
        self.enter("Term")?;
        let res = (|| {
            let mut acc = self.parse_unary()?;
            while matches!(self.peek(0).kind, TokKind::Star | TokKind::Slash) {
                let op = if self.advance().kind == TokKind::Star {
                    BinOp::Mul
                } else {
                    BinOp::Div
                };
                let rhs = self.parse_unary()?;
                acc = Expr::Bin {
                    op,
                    left: Box::new(acc),
                    right: Box::new(rhs),
                };
            }
            Ok(acc)
        })();
        self.leave();
        res
    }

    /// `Unary -> '-' Unary | Power`
    fn parse_unary(&mut self) -> Result<Expr, Diagnostic> {
        self.enter("Unary")?;
        let res = if self.peek(0).kind == TokKind::Minus {
            self.advance();
            Ok(Expr::Neg(Box::new(self.parse_unary()?)))
        } else {
            self.parse_power()
        };
        self.leave();
        res
    }

    /// `Power -> Atom [ '^' Unary ]`
    /// Right recursion on `'Unary'` makes `^` right-associative (`2^3^2 == 2^(3^2)`).
    fn parse_power(&mut self) -> Result<Expr, Diagnostic> {
        self.enter("Power")?;
        let res = (|| {
            let base = self.parse_atom()?;
            if self.peek(0).kind == TokKind::Caret {
                self.advance();
                let exp = self.parse_unary()?;
                Ok(Expr::Bin {
                    op: BinOp::Pow,
                    left: Box::new(base),
                    right: Box::new(exp),
                })
            } else {
                Ok(base)
            }
        })();
        self.leave();
        res
    }

    /// `Atom -> Int | Ident | '(' Expr ')'`
    fn parse_atom(&mut self) -> Result<Expr, Diagnostic> {
        self.enter("Atom")?;
        let res = {
            let tok = self.peek(0).clone();
            match tok.kind {
                TokKind::Int(n) => {
                    self.advance();
                    Ok(Expr::Int(n))
                }
                TokKind::Ident(name) => {
                    self.advance();
                    Ok(Expr::Var(name))
                }
                TokKind::LParen => {
                    self.advance();
                    let inner = self.parse_expr()?;
                    self.expect(&TokKind::RParen, "')'")?;
                    Ok(inner)
                }
                _ => Err(Diagnostic {
                    pos: tok.pos,
                    msg: format!("expected expression operand, found {}", tok.kind),
                }),
            }
        };
        self.leave();
        res
    }
}

fn parse_source(src: &str, max_depth: usize) -> (Vec<Stmt>, Vec<String>, Vec<Diagnostic>) {
    match lex(src) {
        Ok(tokens) => {
            let mut parser = Parser::new(tokens, max_depth);
            let stmts = parser.parse_program();
            (stmts, parser.lookahead_trace, parser.errors)
        }
        Err(diag) => (Vec::new(), Vec::new(), vec![diag]),
    }
}

// ---------------------------------------------------------------------------
// 4. AST Evaluator (Validates Precedence, Associativity, and Control Flow)
// ---------------------------------------------------------------------------

fn eval_expr(expr: &Expr, env: &BTreeMap<String, i64>) -> Result<i64, String> {
    match expr {
        Expr::Int(n) => Ok(*n),
        Expr::Var(name) => env
            .get(name)
            .copied()
            .ok_or_else(|| format!("undefined variable '{name}'")),
        Expr::Neg(inner) => Ok(-eval_expr(inner, env)?),
        Expr::Bin { op, left, right } => {
            let l = eval_expr(left, env)?;
            let r = eval_expr(right, env)?;
            match op {
                BinOp::Add => Ok(l + r),
                BinOp::Sub => Ok(l - r),
                BinOp::Mul => Ok(l * r),
                BinOp::Div => {
                    if r == 0 {
                        Err("division by zero".into())
                    } else {
                        Ok(l / r)
                    }
                }
                BinOp::Pow => {
                    if r < 0 {
                        Err("negative exponent".into())
                    } else {
                        Ok(l.pow(r as u32))
                    }
                }
            }
        }
    }
}

fn exec_stmts(
    stmts: &[Stmt],
    env: &mut BTreeMap<String, i64>,
    calls: &mut Vec<(String, Vec<i64>)>,
) -> Result<(), String> {
    for s in stmts {
        match s {
            Stmt::Let { name, expr } | Stmt::Assign { name, expr } => {
                let v = eval_expr(expr, env)?;
                env.insert(name.clone(), v);
            }
            Stmt::Call { callee, args } => {
                let vals = args
                    .iter()
                    .map(|a| eval_expr(a, env))
                    .collect::<Result<Vec<_>, _>>()?;
                calls.push((callee.clone(), vals));
            }
            Stmt::If {
                cond,
                then_br,
                else_br,
            } => {
                if eval_expr(cond, env)? != 0 {
                    exec_stmts(&[(*then_br.clone())], env, calls)?;
                } else if let Some(eb) = else_br {
                    exec_stmts(&[(*eb.clone())], env, calls)?;
                }
            }
            Stmt::Block(body) => exec_stmts(body, env, calls)?,
        }
    }
    Ok(())
}

fn main() {
    let sample = "\
let a := 10 - 3 - 2;
let p := 2 ^ 3 ^ 2;
a := a + 1 * 4;
if a - 9 then {
    log(a, p);
} else {
    log(0, p);
}
";
    let (stmts, trace, errors) = parse_source(sample, 64);
    assert!(errors.is_empty(), "unexpected diagnostics: {errors:?}");

    let mut env = BTreeMap::new();
    let mut calls = Vec::new();
    exec_stmts(&stmts, &mut env, &mut calls).unwrap();

    println!("=== Handwritten Recursive-Descent Parsing (§4.4.1) ===");
    println!("Parsed AST statements ({}):", stmts.len());
    for (i, s) in stmts.iter().enumerate() {
        println!("  [{i}] {s:?}");
    }
    println!("\nLookahead decisions (LL(1) vs LL(2)):");
    for item in &trace {
        println!("  {item}");
    }
    println!("\nExecution verification:");
    println!("  env={env:?}, calls={calls:?}");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn precedence_and_associativity_via_loops_and_right_recursion() {
        let src = "\
let left_sub := 10 - 3 - 2;
let right_pow := 2 ^ 3 ^ 2;
let mixed := 1 + 2 * 3 ^ 2 - 4 / 2;
";
        let (stmts, _, errors) = parse_source(src, 64);
        assert!(errors.is_empty());

        let Stmt::Let { expr: e_sub, .. } = &stmts[0] else {
            panic!()
        };
        let Stmt::Let { expr: e_pow, .. } = &stmts[1] else {
            panic!()
        };
        assert_eq!(e_sub.to_string(), "((10 - 3) - 2)");
        assert_eq!(e_pow.to_string(), "(2 ^ (3 ^ 2))");

        let mut env = BTreeMap::new();
        let mut calls = Vec::new();
        exec_stmts(&stmts, &mut env, &mut calls).unwrap();
        assert_eq!(env["left_sub"], 5);
        assert_eq!(env["right_pow"], 512);
        assert_eq!(env["mixed"], 17);
    }

    #[test]
    fn ll2_lookahead_and_dangling_else_nesting() {
        let src = "\
let x := 1;
x := 5;
if 0 then if 1 then emit(1); else emit(2);
emit(x);
";
        let (stmts, trace, errors) = parse_source(src, 64);
        assert!(errors.is_empty());
        assert!(trace
            .iter()
            .any(|t| t.contains("LL(2)") && t.contains("':='")));
        assert!(trace
            .iter()
            .any(|t| t.contains("LL(2)") && t.contains("'('")));

        let mut env = BTreeMap::new();
        let mut calls = Vec::new();
        exec_stmts(&stmts, &mut env, &mut calls).unwrap();
        // Because `else emit(2)` binds to inner `if 1`, outer `if 0` skips both!
        assert_eq!(calls, vec![("emit".into(), vec![5])]);
    }

    #[test]
    fn syntax_error_recovery_and_recursion_depth_limit() {
        let bad_src = "\
let ok1 := 1 + 2;
let broken := 3 + ;
let ok2 := 4 * 5;
";
        let (stmts, _, errors) = parse_source(bad_src, 64);
        assert_eq!(errors.len(), 1);
        assert!(errors[0].msg.contains("expected expression operand"));
        assert_eq!(stmts.len(), 2);

        // Deep nesting triggers clean depth limit error rather than stack overflow.
        let deep = format!("let d := {}1{};", "(".repeat(25), ")".repeat(25));
        let (_, _, deep_errs) = parse_source(&deep, 12);
        assert_eq!(deep_errs.len(), 1);
        assert!(deep_errs[0].msg.contains("recursion depth limit"));
    }
}
