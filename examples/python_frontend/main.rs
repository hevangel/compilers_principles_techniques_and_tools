//! Appendix A extension: Python-subset compiler front end written in Rust.
//! Demonstrates off-side rule indentation lexing (`INDENT`/`DEDENT`/`NEWLINE` +
//! implicit line joining inside brackets), compile-time local-vs-global scope
//! pre-scan (`UnboundLocalError` semantics), `/` vs `//` division, negative list
//! indexing, lowering to labeled Three-Address Code (TAC), and a TAC VM.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

// ---------------------------------------------------------------------------
// 1. Off-Side Rule Indentation Lexer (INDENT / DEDENT / NEWLINE)
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq)]
enum TokKind {
    Def,
    Global,
    If,
    Elif,
    Else,
    While,
    Return,
    And,
    Or,
    Not,
    TrueKw,
    FalseKw,
    NoneKw,
    Emit,
    Len,
    Ident(String),
    IntLit(i64),
    FloatLit(f64),
    FloorDiv, // //
    EqEq,     // ==
    Ne,       // !=
    Le,       // <=
    Ge,       // >=
    Lt,
    Gt,
    Assign,
    Plus,
    Minus,
    Star,
    Slash,
    Colon,
    Comma,
    LParen,
    RParen,
    LBracket,
    RBracket,
    Newline,
    Indent,
    Dedent,
    Eof,
}

#[derive(Clone, Debug)]
struct Token {
    kind: TokKind,
    line: usize,
}

/// Tokenizes Python-subset source using an explicit indentation stack and
/// tracking `( )` / `[ ]` nesting to suppress NEWLINE/INDENT inside brackets.
fn lex(src: &str) -> Result<Vec<Token>, String> {
    let bytes = src.as_bytes();
    let mut i = 0usize;
    let mut line = 1usize;
    let mut indent_stack = vec![0usize];
    let mut bracket_depth = 0usize;
    let mut at_line_start = true;
    let mut out = Vec::new();

    while i < bytes.len() {
        if at_line_start {
            let mut col = 0usize;
            while i < bytes.len() {
                match bytes[i] {
                    b' ' => {
                        col += 1;
                        i += 1;
                    }
                    b'\r' => i += 1,
                    b'\t' => {
                        return Err(format!(
                            "line {line}: TabError: inconsistent use of tabs and spaces"
                        ));
                    }
                    _ => break,
                }
            }
            if i >= bytes.len() {
                break;
            }
            if bytes[i] == b'\n' {
                i += 1;
                line += 1;
                continue;
            }
            if bytes[i] == b'#' {
                while i < bytes.len() && bytes[i] != b'\n' {
                    i += 1;
                }
                continue;
            }
            at_line_start = false;
            if bracket_depth == 0 {
                let top = *indent_stack.last().unwrap();
                if col > top {
                    indent_stack.push(col);
                    out.push(Token {
                        kind: TokKind::Indent,
                        line,
                    });
                } else if col < top {
                    while col < *indent_stack.last().unwrap() {
                        indent_stack.pop();
                        out.push(Token {
                            kind: TokKind::Dedent,
                            line,
                        });
                    }
                    if col != *indent_stack.last().unwrap() {
                        return Err(format!(
                            "line {line}: IndentationError: unindent does not match any outer level"
                        ));
                    }
                }
            }
            continue;
        }

        let ch = bytes[i];
        if ch == b' ' || ch == b'\r' {
            i += 1;
            continue;
        }
        if ch == b'#' {
            while i < bytes.len() && bytes[i] != b'\n' {
                i += 1;
            }
            continue;
        }
        if ch == b'\n' {
            if bracket_depth == 0
                && out.last().is_some_and(|t: &Token| {
                    t.kind != TokKind::Newline && t.kind != TokKind::Dedent
                })
            {
                out.push(Token {
                    kind: TokKind::Newline,
                    line,
                });
            }
            i += 1;
            line += 1;
            at_line_start = true;
            continue;
        }

        if ch.is_ascii_alphabetic() || ch == b'_' {
            let start = i;
            i += 1;
            while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                i += 1;
            }
            let word = std::str::from_utf8(&bytes[start..i]).unwrap();
            let kind = match word {
                "def" => TokKind::Def,
                "global" => TokKind::Global,
                "if" => TokKind::If,
                "elif" => TokKind::Elif,
                "else" => TokKind::Else,
                "while" => TokKind::While,
                "return" => TokKind::Return,
                "and" => TokKind::And,
                "or" => TokKind::Or,
                "not" => TokKind::Not,
                "True" => TokKind::TrueKw,
                "False" => TokKind::FalseKw,
                "None" => TokKind::NoneKw,
                "emit" => TokKind::Emit,
                "len" => TokKind::Len,
                _ => TokKind::Ident(word.to_string()),
            };
            out.push(Token { kind, line });
            continue;
        }

        if ch.is_ascii_digit() {
            let start = i;
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                i += 1;
            }
            if bytes.get(i) == Some(&b'.') && bytes.get(i + 1).is_some_and(u8::is_ascii_digit) {
                i += 1;
                while i < bytes.len() && bytes[i].is_ascii_digit() {
                    i += 1;
                }
                let s = std::str::from_utf8(&bytes[start..i]).unwrap();
                let v: f64 = s
                    .parse()
                    .map_err(|_| format!("line {line}: invalid float '{s}'"))?;
                out.push(Token {
                    kind: TokKind::FloatLit(v),
                    line,
                });
            } else {
                let s = std::str::from_utf8(&bytes[start..i]).unwrap();
                let v: i64 = s
                    .parse()
                    .map_err(|_| format!("line {line}: invalid int '{s}'"))?;
                out.push(Token {
                    kind: TokKind::IntLit(v),
                    line,
                });
            }
            continue;
        }

        let two = bytes.get(i + 1).copied();
        let kind = match (ch, two) {
            (b'/', Some(b'/')) => {
                i += 2;
                TokKind::FloorDiv
            }
            (b'=', Some(b'=')) => {
                i += 2;
                TokKind::EqEq
            }
            (b'!', Some(b'=')) => {
                i += 2;
                TokKind::Ne
            }
            (b'<', Some(b'=')) => {
                i += 2;
                TokKind::Le
            }
            (b'>', Some(b'=')) => {
                i += 2;
                TokKind::Ge
            }
            (b'<', _) => {
                i += 1;
                TokKind::Lt
            }
            (b'>', _) => {
                i += 1;
                TokKind::Gt
            }
            (b'=', _) => {
                i += 1;
                TokKind::Assign
            }
            (b'+', _) => {
                i += 1;
                TokKind::Plus
            }
            (b'-', _) => {
                i += 1;
                TokKind::Minus
            }
            (b'*', _) => {
                i += 1;
                TokKind::Star
            }
            (b'/', _) => {
                i += 1;
                TokKind::Slash
            }
            (b':', _) => {
                i += 1;
                TokKind::Colon
            }
            (b',', _) => {
                i += 1;
                TokKind::Comma
            }
            (b'(', _) => {
                i += 1;
                bracket_depth += 1;
                TokKind::LParen
            }
            (b')', _) => {
                if bracket_depth == 0 {
                    return Err(format!("line {line}: unmatched ')'"));
                }
                i += 1;
                bracket_depth -= 1;
                TokKind::RParen
            }
            (b'[', _) => {
                i += 1;
                bracket_depth += 1;
                TokKind::LBracket
            }
            (b']', _) => {
                if bracket_depth == 0 {
                    return Err(format!("line {line}: unmatched ']'"));
                }
                i += 1;
                bracket_depth -= 1;
                TokKind::RBracket
            }
            _ => {
                return Err(format!(
                    "line {line}: SyntaxError: unexpected character '{}'",
                    ch as char
                ));
            }
        };
        out.push(Token { kind, line });
    }

    if bracket_depth > 0 {
        return Err(format!("line {line}: SyntaxError: unclosed '(' or '['"));
    }
    if out
        .last()
        .is_some_and(|t| t.kind != TokKind::Newline && t.kind != TokKind::Dedent)
    {
        out.push(Token {
            kind: TokKind::Newline,
            line,
        });
    }
    while indent_stack.len() > 1 {
        indent_stack.pop();
        out.push(Token {
            kind: TokKind::Dedent,
            line,
        });
    }
    out.push(Token {
        kind: TokKind::Eof,
        line,
    });
    Ok(out)
}

// ---------------------------------------------------------------------------
// 2. AST & Compile-Time Scope Pre-Scan (Local vs Global Binding Resolution)
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BinOp {
    Add,
    Sub,
    Mul,
    TrueDiv,  // /
    FloorDiv, // //
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RelOp {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

#[derive(Clone, Debug, PartialEq)]
enum PyVal {
    NoneVal,
    Int(i64),
    Float(f64),
    Bool(bool),
    List(Vec<PyVal>),
}

impl fmt::Display for PyVal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PyVal::NoneVal => write!(f, "None"),
            PyVal::Int(n) => write!(f, "{n}"),
            PyVal::Float(r) => write!(f, "{r:.4}"),
            PyVal::Bool(b) => write!(f, "{}", if *b { "True" } else { "False" }),
            PyVal::List(xs) => write!(f, "list(len={})", xs.len()),
        }
    }
}

#[derive(Clone, Debug)]
enum RawExpr {
    Lit(PyVal),
    Name(String),
    ListLit(Vec<RawExpr>),
    Index {
        base: Box<RawExpr>,
        idx: Box<RawExpr>,
    },
    Neg(Box<RawExpr>),
    Not(Box<RawExpr>),
    Bin {
        op: BinOp,
        left: Box<RawExpr>,
        right: Box<RawExpr>,
    },
    Rel {
        op: RelOp,
        left: Box<RawExpr>,
        right: Box<RawExpr>,
    },
    And(Box<RawExpr>, Box<RawExpr>),
    Or(Box<RawExpr>, Box<RawExpr>),
    Len(Box<RawExpr>),
    Call {
        name: String,
        args: Vec<RawExpr>,
    },
}

#[derive(Clone, Debug)]
enum RawStmt {
    Global(Vec<String>),
    Assign {
        name: String,
        rhs: RawExpr,
    },
    If {
        branches: Vec<(RawExpr, Vec<RawStmt>)>,
        else_body: Vec<RawStmt>,
    },
    While {
        cond: RawExpr,
        body: Vec<RawStmt>,
    },
    Return(Option<RawExpr>),
    Emit(RawExpr),
    Expr(RawExpr),
}

#[derive(Clone, Debug)]
struct RawFn {
    name: String,
    params: Vec<String>,
    body: Vec<RawStmt>,
}

// ---------------------------------------------------------------------------
// 3. Indentation-Driven Recursive-Descent Parser
// ---------------------------------------------------------------------------

struct Parser {
    toks: Vec<Token>,
    pos: usize,
}

impl Parser {
    fn new(toks: Vec<Token>) -> Self {
        Self { toks, pos: 0 }
    }

    fn peek(&self) -> &TokKind {
        &self.toks[self.pos].kind
    }

    fn peek_next(&self) -> &TokKind {
        self.toks
            .get(self.pos + 1)
            .map_or(&TokKind::Eof, |t| &t.kind)
    }

    fn line(&self) -> usize {
        self.toks[self.pos].line
    }

    fn err(&self, msg: &str) -> String {
        format!("line {}: {msg}", self.line())
    }

    fn expect(&mut self, kind: &TokKind) -> Result<(), String> {
        if self.peek() == kind {
            self.pos += 1;
            Ok(())
        } else {
            Err(self.err(&format!("expected {kind:?}, found {:?}", self.peek())))
        }
    }

    fn parse_module(&mut self) -> Result<(Vec<RawFn>, Vec<RawStmt>), String> {
        let mut fns = Vec::new();
        let mut top_stmts = Vec::new();
        while *self.peek() != TokKind::Eof {
            while *self.peek() == TokKind::Newline {
                self.pos += 1;
            }
            if *self.peek() == TokKind::Eof {
                break;
            }
            if *self.peek() == TokKind::Def {
                fns.push(self.parse_def()?);
            } else {
                top_stmts.push(self.parse_stmt()?);
            }
        }
        Ok((fns, top_stmts))
    }

    fn parse_def(&mut self) -> Result<RawFn, String> {
        self.expect(&TokKind::Def)?;
        let name = match self.peek().clone() {
            TokKind::Ident(s) => {
                self.pos += 1;
                s
            }
            _ => return Err(self.err("expected function name after 'def'")),
        };
        self.expect(&TokKind::LParen)?;
        let mut params = Vec::new();
        while *self.peek() != TokKind::RParen {
            let p = match self.peek().clone() {
                TokKind::Ident(s) => {
                    self.pos += 1;
                    s
                }
                _ => return Err(self.err("expected parameter name")),
            };
            params.push(p);
            if *self.peek() == TokKind::Comma {
                self.pos += 1;
            } else {
                break;
            }
        }
        self.expect(&TokKind::RParen)?;
        self.expect(&TokKind::Colon)?;
        let body = self.parse_suite()?;
        Ok(RawFn { name, params, body })
    }

    fn parse_suite(&mut self) -> Result<Vec<RawStmt>, String> {
        self.expect(&TokKind::Newline)?;
        self.expect(&TokKind::Indent)?;
        let mut stmts = Vec::new();
        while *self.peek() != TokKind::Dedent && *self.peek() != TokKind::Eof {
            while *self.peek() == TokKind::Newline {
                self.pos += 1;
            }
            if *self.peek() == TokKind::Dedent {
                break;
            }
            stmts.push(self.parse_stmt()?);
        }
        self.expect(&TokKind::Dedent)?;
        if stmts.is_empty() {
            return Err(self.err("expected non-empty indented block"));
        }
        Ok(stmts)
    }

    fn parse_stmt(&mut self) -> Result<RawStmt, String> {
        match self.peek().clone() {
            TokKind::Global => {
                self.pos += 1;
                let mut names = Vec::new();
                loop {
                    match self.peek().clone() {
                        TokKind::Ident(s) => {
                            self.pos += 1;
                            names.push(s);
                        }
                        _ => return Err(self.err("expected identifier after 'global'")),
                    }
                    if *self.peek() == TokKind::Comma {
                        self.pos += 1;
                    } else {
                        break;
                    }
                }
                self.expect(&TokKind::Newline)?;
                Ok(RawStmt::Global(names))
            }
            TokKind::If => {
                self.pos += 1;
                let cond = self.parse_expr()?;
                self.expect(&TokKind::Colon)?;
                let body = self.parse_suite()?;
                let mut branches = vec![(cond, body)];
                while *self.peek() == TokKind::Elif {
                    self.pos += 1;
                    let ec = self.parse_expr()?;
                    self.expect(&TokKind::Colon)?;
                    let eb = self.parse_suite()?;
                    branches.push((ec, eb));
                }
                let else_body = if *self.peek() == TokKind::Else {
                    self.pos += 1;
                    self.expect(&TokKind::Colon)?;
                    self.parse_suite()?
                } else {
                    Vec::new()
                };
                Ok(RawStmt::If {
                    branches,
                    else_body,
                })
            }
            TokKind::While => {
                self.pos += 1;
                let cond = self.parse_expr()?;
                self.expect(&TokKind::Colon)?;
                let body = self.parse_suite()?;
                Ok(RawStmt::While { cond, body })
            }
            TokKind::Return => {
                self.pos += 1;
                let val = if *self.peek() != TokKind::Newline {
                    Some(self.parse_expr()?)
                } else {
                    None
                };
                self.expect(&TokKind::Newline)?;
                Ok(RawStmt::Return(val))
            }
            TokKind::Emit => {
                self.pos += 1;
                self.expect(&TokKind::LParen)?;
                let e = self.parse_expr()?;
                self.expect(&TokKind::RParen)?;
                self.expect(&TokKind::Newline)?;
                Ok(RawStmt::Emit(e))
            }
            TokKind::Ident(name) if *self.peek_next() == TokKind::Assign => {
                self.pos += 2;
                let rhs = self.parse_expr()?;
                self.expect(&TokKind::Newline)?;
                Ok(RawStmt::Assign { name, rhs })
            }
            _ => {
                let e = self.parse_expr()?;
                self.expect(&TokKind::Newline)?;
                Ok(RawStmt::Expr(e))
            }
        }
    }

    fn parse_expr(&mut self) -> Result<RawExpr, String> {
        self.parse_or()
    }

    fn parse_or(&mut self) -> Result<RawExpr, String> {
        let mut lhs = self.parse_and()?;
        while *self.peek() == TokKind::Or {
            self.pos += 1;
            let rhs = self.parse_and()?;
            lhs = RawExpr::Or(Box::new(lhs), Box::new(rhs));
        }
        Ok(lhs)
    }

    fn parse_and(&mut self) -> Result<RawExpr, String> {
        let mut lhs = self.parse_not()?;
        while *self.peek() == TokKind::And {
            self.pos += 1;
            let rhs = self.parse_not()?;
            lhs = RawExpr::And(Box::new(lhs), Box::new(rhs));
        }
        Ok(lhs)
    }

    fn parse_not(&mut self) -> Result<RawExpr, String> {
        if *self.peek() == TokKind::Not {
            self.pos += 1;
            Ok(RawExpr::Not(Box::new(self.parse_not()?)))
        } else {
            self.parse_rel()
        }
    }

    fn parse_rel(&mut self) -> Result<RawExpr, String> {
        let lhs = self.parse_add()?;
        let op = match self.peek() {
            TokKind::EqEq => RelOp::Eq,
            TokKind::Ne => RelOp::Ne,
            TokKind::Lt => RelOp::Lt,
            TokKind::Le => RelOp::Le,
            TokKind::Gt => RelOp::Gt,
            TokKind::Ge => RelOp::Ge,
            _ => return Ok(lhs),
        };
        self.pos += 1;
        let rhs = self.parse_add()?;
        Ok(RawExpr::Rel {
            op,
            left: Box::new(lhs),
            right: Box::new(rhs),
        })
    }

    fn parse_add(&mut self) -> Result<RawExpr, String> {
        let mut lhs = self.parse_mul()?;
        loop {
            let op = match self.peek() {
                TokKind::Plus => BinOp::Add,
                TokKind::Minus => BinOp::Sub,
                _ => break,
            };
            self.pos += 1;
            let rhs = self.parse_mul()?;
            lhs = RawExpr::Bin {
                op,
                left: Box::new(lhs),
                right: Box::new(rhs),
            };
        }
        Ok(lhs)
    }

    fn parse_mul(&mut self) -> Result<RawExpr, String> {
        let mut lhs = self.parse_unary()?;
        loop {
            let op = match self.peek() {
                TokKind::Star => BinOp::Mul,
                TokKind::Slash => BinOp::TrueDiv,
                TokKind::FloorDiv => BinOp::FloorDiv,
                _ => break,
            };
            self.pos += 1;
            let rhs = self.parse_unary()?;
            lhs = RawExpr::Bin {
                op,
                left: Box::new(lhs),
                right: Box::new(rhs),
            };
        }
        Ok(lhs)
    }

    fn parse_unary(&mut self) -> Result<RawExpr, String> {
        if *self.peek() == TokKind::Minus {
            self.pos += 1;
            Ok(RawExpr::Neg(Box::new(self.parse_unary()?)))
        } else {
            self.parse_postfix()
        }
    }

    fn parse_postfix(&mut self) -> Result<RawExpr, String> {
        let mut base = self.parse_primary()?;
        while *self.peek() == TokKind::LBracket {
            self.pos += 1;
            let idx = self.parse_expr()?;
            self.expect(&TokKind::RBracket)?;
            base = RawExpr::Index {
                base: Box::new(base),
                idx: Box::new(idx),
            };
        }
        Ok(base)
    }

    fn parse_primary(&mut self) -> Result<RawExpr, String> {
        match self.peek().clone() {
            TokKind::IntLit(n) => {
                self.pos += 1;
                Ok(RawExpr::Lit(PyVal::Int(n)))
            }
            TokKind::FloatLit(r) => {
                self.pos += 1;
                Ok(RawExpr::Lit(PyVal::Float(r)))
            }
            TokKind::TrueKw => {
                self.pos += 1;
                Ok(RawExpr::Lit(PyVal::Bool(true)))
            }
            TokKind::FalseKw => {
                self.pos += 1;
                Ok(RawExpr::Lit(PyVal::Bool(false)))
            }
            TokKind::NoneKw => {
                self.pos += 1;
                Ok(RawExpr::Lit(PyVal::NoneVal))
            }
            TokKind::Len => {
                self.pos += 1;
                self.expect(&TokKind::LParen)?;
                let inner = self.parse_expr()?;
                self.expect(&TokKind::RParen)?;
                Ok(RawExpr::Len(Box::new(inner)))
            }
            TokKind::LBracket => {
                self.pos += 1;
                let mut elems = Vec::new();
                while *self.peek() != TokKind::RBracket {
                    elems.push(self.parse_expr()?);
                    if *self.peek() == TokKind::Comma {
                        self.pos += 1;
                    } else {
                        break;
                    }
                }
                self.expect(&TokKind::RBracket)?;
                Ok(RawExpr::ListLit(elems))
            }
            TokKind::LParen => {
                self.pos += 1;
                let inner = self.parse_expr()?;
                self.expect(&TokKind::RParen)?;
                Ok(inner)
            }
            TokKind::Ident(name) => {
                self.pos += 1;
                if *self.peek() == TokKind::LParen {
                    self.pos += 1;
                    let mut args = Vec::new();
                    while *self.peek() != TokKind::RParen {
                        args.push(self.parse_expr()?);
                        if *self.peek() == TokKind::Comma {
                            self.pos += 1;
                        } else {
                            break;
                        }
                    }
                    self.expect(&TokKind::RParen)?;
                    Ok(RawExpr::Call { name, args })
                } else {
                    Ok(RawExpr::Name(name))
                }
            }
            other => Err(self.err(&format!("SyntaxError: unexpected token {other:?}"))),
        }
    }
}

// ---------------------------------------------------------------------------
// 4. Symtable Pre-Scan & Three-Address Code (TAC) Lowering
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq)]
struct FnSymtable {
    name: String,
    params: Vec<String>,
    locals: BTreeSet<String>,
    globals: BTreeSet<String>,
}

fn collect_stmt_targets(
    stmts: &[RawStmt],
    assigned: &mut BTreeSet<String>,
    globals: &mut BTreeSet<String>,
) {
    for st in stmts {
        match st {
            RawStmt::Global(names) => {
                for n in names {
                    globals.insert(n.clone());
                }
            }
            RawStmt::Assign { name, .. } => {
                assigned.insert(name.clone());
            }
            RawStmt::If {
                branches,
                else_body,
            } => {
                for (_, body) in branches {
                    collect_stmt_targets(body, assigned, globals);
                }
                collect_stmt_targets(else_body, assigned, globals);
            }
            RawStmt::While { body, .. } => collect_stmt_targets(body, assigned, globals),
            RawStmt::Return(_) | RawStmt::Emit(_) | RawStmt::Expr(_) => {}
        }
    }
}

/// Pre-scans `def` bodies just like CPython's symtable pass: any assigned name not
/// listed under `global` becomes a function-local slot across the *entire* function.
fn analyze_fn_scope(f: &RawFn) -> FnSymtable {
    let mut assigned = BTreeSet::new();
    let mut globals = BTreeSet::new();
    collect_stmt_targets(&f.body, &mut assigned, &mut globals);
    let mut locals: BTreeSet<String> = f.params.iter().cloned().collect();
    for a in assigned {
        if !globals.contains(&a) {
            locals.insert(a);
        }
    }
    FnSymtable {
        name: f.name.clone(),
        params: f.params.clone(),
        locals,
        globals,
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Slot {
    Local(String),  // fn::name or temp
    Global(String), // global::name
}

impl fmt::Display for Slot {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Slot::Local(s) => write!(f, "{s}"),
            Slot::Global(s) => write!(f, "global::{s}"),
        }
    }
}

#[derive(Clone, Debug)]
enum Operand {
    Lit(PyVal),
    Slot(Slot),
}

impl fmt::Display for Operand {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Operand::Lit(v) => write!(f, "{v}"),
            Operand::Slot(s) => write!(f, "{s}"),
        }
    }
}

#[derive(Clone, Debug)]
enum TacInst {
    Label(String),
    Assign {
        dst: Slot,
        src: Operand,
    },
    MakeList {
        dst: Slot,
        elems: Vec<Operand>,
    },
    ListLen {
        dst: Slot,
        base: Operand,
    },
    ListLoad {
        dst: Slot,
        base: Operand,
        idx: Operand,
    },
    Neg {
        dst: Slot,
        src: Operand,
    },
    Not {
        dst: Slot,
        src: Operand,
    },
    Bin {
        dst: Slot,
        op: BinOp,
        left: Operand,
        right: Operand,
    },
    Rel {
        dst: Slot,
        op: RelOp,
        left: Operand,
        right: Operand,
    },
    Param(Operand),
    Call {
        dst: Slot,
        func: String,
        argc: usize,
    },
    IfFalse {
        cond: Operand,
        label: String,
    },
    Goto(String),
    Return(Option<Operand>),
    Emit(Operand),
}

impl fmt::Display for TacInst {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TacInst::Label(l) => write!(f, "{l}:"),
            TacInst::Assign { dst, src } => write!(f, "  {dst} = {src}"),
            TacInst::MakeList { dst, elems } => write!(f, "  {dst} = list({})", elems.len()),
            TacInst::ListLen { dst, base } => write!(f, "  {dst} = len({base})"),
            TacInst::ListLoad { dst, base, idx } => write!(f, "  {dst} = {base}[{idx}]"),
            TacInst::Neg { dst, src } => write!(f, "  {dst} = -{src}"),
            TacInst::Not { dst, src } => write!(f, "  {dst} = not {src}"),
            TacInst::Bin {
                dst,
                op,
                left,
                right,
            } => {
                let s = match op {
                    BinOp::Add => "+",
                    BinOp::Sub => "-",
                    BinOp::Mul => "*",
                    BinOp::TrueDiv => "/",
                    BinOp::FloorDiv => "//",
                };
                write!(f, "  {dst} = {left} {s} {right}")
            }
            TacInst::Rel {
                dst,
                op,
                left,
                right,
            } => {
                let s = match op {
                    RelOp::Eq => "==",
                    RelOp::Ne => "!=",
                    RelOp::Lt => "<",
                    RelOp::Le => "<=",
                    RelOp::Gt => ">",
                    RelOp::Ge => ">=",
                };
                write!(f, "  {dst} = {left} {s} {right}")
            }
            TacInst::Param(op) => write!(f, "  param {op}"),
            TacInst::Call { dst, func, argc } => write!(f, "  {dst} = call {func}/{argc}"),
            TacInst::IfFalse { cond, label } => write!(f, "  iffalse {cond} goto {label}"),
            TacInst::Goto(l) => write!(f, "  goto {l}"),
            TacInst::Return(Some(v)) => write!(f, "  return {v}"),
            TacInst::Return(None) => write!(f, "  return None"),
            TacInst::Emit(v) => write!(f, "  emit {v}"),
        }
    }
}

struct Emitter {
    next_temp: usize,
    next_lbl: usize,
    code: Vec<TacInst>,
}

impl Emitter {
    fn new() -> Self {
        Self {
            next_temp: 0,
            next_lbl: 0,
            code: Vec::new(),
        }
    }

    fn temp(&mut self) -> Slot {
        self.next_temp += 1;
        Slot::Local(format!("t{}", self.next_temp))
    }

    fn label(&mut self, prefix: &str) -> String {
        self.next_lbl += 1;
        format!("{prefix}_{}", self.next_lbl)
    }

    fn resolve(&self, name: &str, sym: Option<&FnSymtable>) -> Slot {
        match sym {
            Some(st) if st.locals.contains(name) => Slot::Local(format!("{}::{name}", st.name)),
            _ => Slot::Global(name.to_string()),
        }
    }

    fn gen_expr(&mut self, expr: &RawExpr, sym: Option<&FnSymtable>) -> Operand {
        match expr {
            RawExpr::Lit(v) => Operand::Lit(v.clone()),
            RawExpr::Name(name) => Operand::Slot(self.resolve(name, sym)),
            RawExpr::ListLit(elems) => {
                let ops: Vec<Operand> = elems.iter().map(|e| self.gen_expr(e, sym)).collect();
                let dst = self.temp();
                self.code.push(TacInst::MakeList {
                    dst: dst.clone(),
                    elems: ops,
                });
                Operand::Slot(dst)
            }
            RawExpr::Len(inner) => {
                let base = self.gen_expr(inner, sym);
                let dst = self.temp();
                self.code.push(TacInst::ListLen {
                    dst: dst.clone(),
                    base,
                });
                Operand::Slot(dst)
            }
            RawExpr::Index { base, idx } => {
                let b = self.gen_expr(base, sym);
                let i = self.gen_expr(idx, sym);
                let dst = self.temp();
                self.code.push(TacInst::ListLoad {
                    dst: dst.clone(),
                    base: b,
                    idx: i,
                });
                Operand::Slot(dst)
            }
            RawExpr::Neg(inner) => {
                let src = self.gen_expr(inner, sym);
                let dst = self.temp();
                self.code.push(TacInst::Neg {
                    dst: dst.clone(),
                    src,
                });
                Operand::Slot(dst)
            }
            RawExpr::Not(inner) => {
                let src = self.gen_expr(inner, sym);
                let dst = self.temp();
                self.code.push(TacInst::Not {
                    dst: dst.clone(),
                    src,
                });
                Operand::Slot(dst)
            }
            RawExpr::Bin { op, left, right } => {
                let l = self.gen_expr(left, sym);
                let r = self.gen_expr(right, sym);
                let dst = self.temp();
                self.code.push(TacInst::Bin {
                    dst: dst.clone(),
                    op: *op,
                    left: l,
                    right: r,
                });
                Operand::Slot(dst)
            }
            RawExpr::Rel { op, left, right } => {
                let l = self.gen_expr(left, sym);
                let r = self.gen_expr(right, sym);
                let dst = self.temp();
                self.code.push(TacInst::Rel {
                    dst: dst.clone(),
                    op: *op,
                    left: l,
                    right: r,
                });
                Operand::Slot(dst)
            }
            RawExpr::And(left, right) => {
                let dst = self.temp();
                let l_false = self.label("and_false");
                let l_end = self.label("and_end");
                let l = self.gen_expr(left, sym);
                self.code.push(TacInst::IfFalse {
                    cond: l,
                    label: l_false.clone(),
                });
                let r = self.gen_expr(right, sym);
                self.code.push(TacInst::Assign {
                    dst: dst.clone(),
                    src: r,
                });
                self.code.push(TacInst::Goto(l_end.clone()));
                self.code.push(TacInst::Label(l_false));
                self.code.push(TacInst::Assign {
                    dst: dst.clone(),
                    src: Operand::Lit(PyVal::Bool(false)),
                });
                self.code.push(TacInst::Label(l_end));
                Operand::Slot(dst)
            }
            RawExpr::Or(left, right) => {
                let dst = self.temp();
                let l_rhs = self.label("or_rhs");
                let l_end = self.label("or_end");
                let l = self.gen_expr(left, sym);
                self.code.push(TacInst::IfFalse {
                    cond: l,
                    label: l_rhs.clone(),
                });
                self.code.push(TacInst::Assign {
                    dst: dst.clone(),
                    src: Operand::Lit(PyVal::Bool(true)),
                });
                self.code.push(TacInst::Goto(l_end.clone()));
                self.code.push(TacInst::Label(l_rhs));
                let r = self.gen_expr(right, sym);
                self.code.push(TacInst::Assign {
                    dst: dst.clone(),
                    src: r,
                });
                self.code.push(TacInst::Label(l_end));
                Operand::Slot(dst)
            }
            RawExpr::Call { name, args } => {
                let ops: Vec<Operand> = args.iter().map(|a| self.gen_expr(a, sym)).collect();
                for op in &ops {
                    self.code.push(TacInst::Param(op.clone()));
                }
                let dst = self.temp();
                self.code.push(TacInst::Call {
                    dst: dst.clone(),
                    func: name.clone(),
                    argc: ops.len(),
                });
                Operand::Slot(dst)
            }
        }
    }

    fn gen_stmt(&mut self, stmt: &RawStmt, sym: Option<&FnSymtable>) {
        match stmt {
            RawStmt::Global(_) => {}
            RawStmt::Assign { name, rhs } => {
                let v = self.gen_expr(rhs, sym);
                let dst = self.resolve(name, sym);
                self.code.push(TacInst::Assign { dst, src: v });
            }
            RawStmt::If {
                branches,
                else_body,
            } => {
                let l_end = self.label("endif");
                for (cond, body) in branches {
                    let l_next = self.label("elif");
                    let c = self.gen_expr(cond, sym);
                    self.code.push(TacInst::IfFalse {
                        cond: c,
                        label: l_next.clone(),
                    });
                    for s in body {
                        self.gen_stmt(s, sym);
                    }
                    self.code.push(TacInst::Goto(l_end.clone()));
                    self.code.push(TacInst::Label(l_next));
                }
                for s in else_body {
                    self.gen_stmt(s, sym);
                }
                self.code.push(TacInst::Label(l_end));
            }
            RawStmt::While { cond, body } => {
                let l_top = self.label("while");
                let l_end = self.label("endwhile");
                self.code.push(TacInst::Label(l_top.clone()));
                let c = self.gen_expr(cond, sym);
                self.code.push(TacInst::IfFalse {
                    cond: c,
                    label: l_end.clone(),
                });
                for s in body {
                    self.gen_stmt(s, sym);
                }
                self.code.push(TacInst::Goto(l_top));
                self.code.push(TacInst::Label(l_end));
            }
            RawStmt::Return(opt) => {
                let v = opt.as_ref().map(|e| self.gen_expr(e, sym));
                self.code.push(TacInst::Return(v));
            }
            RawStmt::Emit(e) => {
                let v = self.gen_expr(e, sym);
                self.code.push(TacInst::Emit(v));
            }
            RawStmt::Expr(e) => {
                let _ = self.gen_expr(e, sym);
            }
        }
    }
}

#[derive(Debug)]
struct CompiledPython {
    symtables: BTreeMap<String, FnSymtable>,
    fn_params: BTreeMap<String, Vec<String>>,
    code: Vec<TacInst>,
}

fn compile_python_subset(src: &str) -> Result<CompiledPython, String> {
    let toks = lex(src)?;
    let (fns, top_stmts) = Parser::new(toks).parse_module()?;
    let mut emitter = Emitter::new();
    let mut symtables = BTreeMap::new();
    let mut fn_params = BTreeMap::new();

    for f in &fns {
        let st = analyze_fn_scope(f);
        let param_slots: Vec<String> = f
            .params
            .iter()
            .map(|p| format!("{}::{p}", f.name))
            .collect();
        if symtables.contains_key(&f.name) {
            return Err(format!("duplicate function '{}'", f.name));
        }
        fn_params.insert(f.name.clone(), param_slots);
        emitter.code.push(TacInst::Label(format!("fn_{}", f.name)));
        for s in &f.body {
            emitter.gen_stmt(s, Some(&st));
        }
        emitter.code.push(TacInst::Return(None));
        symtables.insert(f.name.clone(), st);
    }

    emitter.code.push(TacInst::Label("__module__".into()));
    for s in &top_stmts {
        emitter.gen_stmt(s, None);
    }
    emitter.code.push(TacInst::Return(None));

    Ok(CompiledPython {
        symtables,
        fn_params,
        code: emitter.code,
    })
}

// ---------------------------------------------------------------------------
// 5. Python-Semantics TAC Virtual Machine
// ---------------------------------------------------------------------------

fn py_truthy(v: &PyVal) -> bool {
    match v {
        PyVal::NoneVal => false,
        PyVal::Bool(b) => *b,
        PyVal::Int(n) => *n != 0,
        PyVal::Float(r) => *r != 0.0,
        PyVal::List(xs) => !xs.is_empty(),
    }
}

fn py_floordiv(a: i64, b: i64) -> Result<i64, String> {
    if b == 0 {
        return Err("ZeroDivisionError: integer division or modulo by zero".into());
    }
    let q = a / b;
    let r = a % b;
    if r != 0 && ((r < 0) != (b < 0)) {
        Ok(q - 1)
    } else {
        Ok(q)
    }
}

fn as_f64(v: &PyVal) -> Result<f64, String> {
    match v {
        PyVal::Int(n) => Ok(*n as f64),
        PyVal::Float(r) => Ok(*r),
        PyVal::Bool(b) => Ok(if *b { 1.0 } else { 0.0 }),
        other => Err(format!("TypeError: expected number, found {other}")),
    }
}

fn exec_tac(prog: &CompiledPython) -> Result<(BTreeMap<String, PyVal>, Vec<PyVal>), String> {
    let mut labels = BTreeMap::new();
    for (idx, inst) in prog.code.iter().enumerate() {
        if let TacInst::Label(name) = inst {
            labels.insert(name.clone(), idx);
        }
    }

    struct Frame {
        locals: BTreeMap<String, PyVal>,
        ret_pc: usize,
        ret_dst: Option<Slot>,
    }

    let read_slot = |slot: &Slot,
                     locals: &BTreeMap<String, PyVal>,
                     globals: &BTreeMap<String, PyVal>|
     -> Result<PyVal, String> {
        match slot {
            Slot::Local(key) => locals.get(key).cloned().ok_or_else(|| {
                let short = key.split("::").last().unwrap_or(key);
                format!("UnboundLocalError: local variable '{short}' referenced before assignment")
            }),
            Slot::Global(name) => globals
                .get(name)
                .cloned()
                .ok_or_else(|| format!("NameError: name '{name}' is not defined")),
        }
    };

    let write_slot = |slot: &Slot,
                      val: PyVal,
                      locals: &mut BTreeMap<String, PyVal>,
                      globals: &mut BTreeMap<String, PyVal>| {
        match slot {
            Slot::Local(key) => {
                locals.insert(key.clone(), val);
            }
            Slot::Global(name) => {
                globals.insert(name.clone(), val);
            }
        }
    };

    let eval_op = |op: &Operand,
                   locals: &BTreeMap<String, PyVal>,
                   globals: &BTreeMap<String, PyVal>|
     -> Result<PyVal, String> {
        match op {
            Operand::Lit(v) => Ok(v.clone()),
            Operand::Slot(s) => read_slot(s, locals, globals),
        }
    };

    let mut globals: BTreeMap<String, PyVal> = BTreeMap::new();
    let mut frames = vec![Frame {
        locals: BTreeMap::new(),
        ret_pc: prog.code.len(),
        ret_dst: None,
    }];
    let mut pc = labels["__module__"];
    let mut param_buf: Vec<PyVal> = Vec::new();
    let mut emitted = Vec::new();

    while pc < prog.code.len() {
        match &prog.code[pc] {
            TacInst::Label(_) => pc += 1,
            TacInst::Assign { dst, src } => {
                let v = eval_op(src, &frames.last().unwrap().locals, &globals)?;
                write_slot(dst, v, &mut frames.last_mut().unwrap().locals, &mut globals);
                pc += 1;
            }
            TacInst::MakeList { dst, elems } => {
                let mut vs = Vec::with_capacity(elems.len());
                for e in elems {
                    vs.push(eval_op(e, &frames.last().unwrap().locals, &globals)?);
                }
                write_slot(
                    dst,
                    PyVal::List(vs),
                    &mut frames.last_mut().unwrap().locals,
                    &mut globals,
                );
                pc += 1;
            }
            TacInst::ListLen { dst, base } => {
                let PyVal::List(xs) = eval_op(base, &frames.last().unwrap().locals, &globals)?
                else {
                    return Err("TypeError: len() requires a list".into());
                };
                write_slot(
                    dst,
                    PyVal::Int(xs.len() as i64),
                    &mut frames.last_mut().unwrap().locals,
                    &mut globals,
                );
                pc += 1;
            }
            TacInst::ListLoad { dst, base, idx } => {
                let PyVal::List(xs) = eval_op(base, &frames.last().unwrap().locals, &globals)?
                else {
                    return Err("TypeError: object is not subscriptable".into());
                };
                let PyVal::Int(raw_i) = eval_op(idx, &frames.last().unwrap().locals, &globals)?
                else {
                    return Err("TypeError: list indices must be integers".into());
                };
                let len = xs.len() as i64;
                // Python normalizes negative indices from the end (`-1` => `len - 1`).
                let norm = if raw_i < 0 { len + raw_i } else { raw_i };
                if norm < 0 || norm >= len {
                    return Err(format!("IndexError: list index {raw_i} out of range"));
                }
                let val = xs[norm as usize].clone();
                write_slot(
                    dst,
                    val,
                    &mut frames.last_mut().unwrap().locals,
                    &mut globals,
                );
                pc += 1;
            }
            TacInst::Neg { dst, src } => {
                let v = match eval_op(src, &frames.last().unwrap().locals, &globals)? {
                    PyVal::Int(n) => PyVal::Int(-n),
                    PyVal::Float(r) => PyVal::Float(-r),
                    _ => return Err("TypeError: bad unary '-'".into()),
                };
                write_slot(dst, v, &mut frames.last_mut().unwrap().locals, &mut globals);
                pc += 1;
            }
            TacInst::Not { dst, src } => {
                let v = eval_op(src, &frames.last().unwrap().locals, &globals)?;
                write_slot(
                    dst,
                    PyVal::Bool(!py_truthy(&v)),
                    &mut frames.last_mut().unwrap().locals,
                    &mut globals,
                );
                pc += 1;
            }
            TacInst::Bin {
                dst,
                op,
                left,
                right,
            } => {
                let l = eval_op(left, &frames.last().unwrap().locals, &globals)?;
                let r = eval_op(right, &frames.last().unwrap().locals, &globals)?;
                let out = match op {
                    BinOp::TrueDiv => {
                        let denom = as_f64(&r)?;
                        if denom == 0.0 {
                            return Err("ZeroDivisionError: float division by zero".into());
                        }
                        PyVal::Float(as_f64(&l)? / denom)
                    }
                    BinOp::FloorDiv => match (&l, &r) {
                        (PyVal::Int(a), PyVal::Int(b)) => PyVal::Int(py_floordiv(*a, *b)?),
                        _ => {
                            let denom = as_f64(&r)?;
                            if denom == 0.0 {
                                return Err("ZeroDivisionError: floor division by zero".into());
                            }
                            PyVal::Float((as_f64(&l)? / denom).floor())
                        }
                    },
                    BinOp::Add | BinOp::Sub | BinOp::Mul => match (&l, &r) {
                        (PyVal::Int(a), PyVal::Int(b)) => PyVal::Int(match op {
                            BinOp::Add => a + b,
                            BinOp::Sub => a - b,
                            BinOp::Mul => a * b,
                            _ => unreachable!(),
                        }),
                        _ => {
                            let a = as_f64(&l)?;
                            let b = as_f64(&r)?;
                            PyVal::Float(match op {
                                BinOp::Add => a + b,
                                BinOp::Sub => a - b,
                                BinOp::Mul => a * b,
                                _ => unreachable!(),
                            })
                        }
                    },
                };
                write_slot(
                    dst,
                    out,
                    &mut frames.last_mut().unwrap().locals,
                    &mut globals,
                );
                pc += 1;
            }
            TacInst::Rel {
                dst,
                op,
                left,
                right,
            } => {
                let l = eval_op(left, &frames.last().unwrap().locals, &globals)?;
                let r = eval_op(right, &frames.last().unwrap().locals, &globals)?;
                let b = match (&l, &r) {
                    (PyVal::Int(a), PyVal::Int(b)) => match op {
                        RelOp::Eq => a == b,
                        RelOp::Ne => a != b,
                        RelOp::Lt => a < b,
                        RelOp::Le => a <= b,
                        RelOp::Gt => a > b,
                        RelOp::Ge => a >= b,
                    },
                    _ => {
                        let a = as_f64(&l)?;
                        let b = as_f64(&r)?;
                        match op {
                            RelOp::Eq => a == b,
                            RelOp::Ne => a != b,
                            RelOp::Lt => a < b,
                            RelOp::Le => a <= b,
                            RelOp::Gt => a > b,
                            RelOp::Ge => a >= b,
                        }
                    }
                };
                write_slot(
                    dst,
                    PyVal::Bool(b),
                    &mut frames.last_mut().unwrap().locals,
                    &mut globals,
                );
                pc += 1;
            }
            TacInst::Param(op) => {
                param_buf.push(eval_op(op, &frames.last().unwrap().locals, &globals)?);
                pc += 1;
            }
            TacInst::Call { dst, func, argc } => {
                let params = prog
                    .fn_params
                    .get(func)
                    .ok_or_else(|| format!("NameError: function '{func}' is not defined"))?;
                if params.len() != *argc {
                    return Err(format!(
                        "TypeError: {func}() takes {} positional argument(s) but {argc} given",
                        params.len()
                    ));
                }
                let start = param_buf.len() - argc;
                let args: Vec<PyVal> = param_buf.drain(start..).collect();
                let mut callee_locals = BTreeMap::new();
                for (slot, val) in params.iter().zip(args) {
                    callee_locals.insert(slot.clone(), val);
                }
                frames.push(Frame {
                    locals: callee_locals,
                    ret_pc: pc + 1,
                    ret_dst: Some(dst.clone()),
                });
                pc = labels[&format!("fn_{func}")];
            }
            TacInst::IfFalse { cond, label } => {
                let c = eval_op(cond, &frames.last().unwrap().locals, &globals)?;
                if !py_truthy(&c) {
                    pc = labels[label];
                } else {
                    pc += 1;
                }
            }
            TacInst::Goto(l) => pc = labels[l],
            TacInst::Return(opt) => {
                let ret_val = match opt {
                    Some(o) => eval_op(o, &frames.last().unwrap().locals, &globals)?,
                    None => PyVal::NoneVal,
                };
                let popped = frames.pop().unwrap();
                if let Some(dst) = popped.ret_dst {
                    write_slot(
                        &dst,
                        ret_val,
                        &mut frames.last_mut().unwrap().locals,
                        &mut globals,
                    );
                    pc = popped.ret_pc;
                } else {
                    return Ok((globals, emitted));
                }
            }
            TacInst::Emit(op) => {
                emitted.push(eval_op(op, &frames.last().unwrap().locals, &globals)?);
                pc += 1;
            }
        }
    }
    Ok((globals, emitted))
}

const SAMPLE_PYTHON_SRC: &str = r#"
scale = 2
bonus = 2.5
calls = 0

def weighted_sum(arr, limit):
    global calls
    calls = calls + 1
    acc = 0
    i = 0
    while i < limit:
        if i == 0:
            w = 1
        elif i == 1:
            w = 4
        else:
            # Parenthesized expressions join multiple physical lines cleanly
            w = (
                i + 1
            ) * 2
        acc = acc + arr[i] * w
        i = i + 1
    # '/' always yields float (295.0); '//' yields integer floor division; arr[-1] is 40
    last_bonus = (arr[-1] // 16) * 0.5
    return (acc / scale) + bonus + last_bonus

scores = [10, 20, 30, 40]
total = weighted_sum(scores, len(scores))
emit(total)
emit(calls)
"#;

fn main() {
    let prog = compile_python_subset(SAMPLE_PYTHON_SRC).expect("valid Python subset");
    println!(
        "Compile-time function symtables ({}):",
        prog.symtables.len()
    );
    for (name, st) in &prog.symtables {
        let locals: Vec<&str> = st.locals.iter().map(String::as_str).collect();
        let globals: Vec<&str> = st.globals.iter().map(String::as_str).collect();
        println!(
            "  def {name} -> locals: [{}], global decls: [{}]",
            locals.join(", "),
            globals.join(", ")
        );
    }
    println!("\nEmitted TAC ({} instructions):", prog.code.len());
    for inst in &prog.code {
        println!("{inst}");
    }
    let (globals, emitted) = exec_tac(&prog).expect("TAC executes cleanly");
    println!("\nEmitted output : {emitted:?}");
    println!(
        "Module globals : total={}, calls={}",
        globals["total"], globals["calls"]
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compiles_indentation_scopes_div_modes_negative_index_and_runs_tac() {
        let prog = compile_python_subset(SAMPLE_PYTHON_SRC).expect("compiles");
        let (globals, emitted) = exec_tac(&prog).expect("runs");
        assert_eq!(emitted, vec![PyVal::Float(298.5), PyVal::Int(1)]);
        assert_eq!(globals["calls"], PyVal::Int(1));
        assert_eq!(globals["total"], PyVal::Float(298.5));
    }

    #[test]
    fn floor_division_matches_python_toward_negative_infinity() {
        let src = r#"
emit(-7 // 2)
emit(-7 / 2)
"#;
        let prog = compile_python_subset(src).expect("compiles");
        let (_, emitted) = exec_tac(&prog).expect("runs");
        assert_eq!(emitted, vec![PyVal::Int(-4), PyVal::Float(-3.5)]);
    }

    #[test]
    fn detects_unbound_local_error_from_symtable_prescan() {
        let src = r#"
x = 10
def broken():
    y = x + 1
    x = 20
    return y
broken()
"#;
        let prog = compile_python_subset(src).expect("lexes and parses cleanly");
        assert!(prog.symtables["broken"].locals.contains("x"));
        assert!(exec_tac(&prog).unwrap_err().contains("UnboundLocalError"));
    }

    #[test]
    fn rejects_bad_dedent_and_index_out_of_range() {
        let bad_indent = "def f():\n    x = 1\n  y = 2\n";
        assert!(compile_python_subset(bad_indent)
            .unwrap_err()
            .contains("IndentationError"));

        let oob = "xs = [1, 2]\nemit(xs[-3])\n";
        let oob_prog = compile_python_subset(oob).expect("compiles");
        assert!(exec_tac(&oob_prog).unwrap_err().contains("IndexError"));
    }
}
