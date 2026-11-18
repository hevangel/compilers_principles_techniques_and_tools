//! Appendix A extension: Rust-subset compiler front end written in Rust.
//! Parses nested `/* /* ... */ */` comments, `fn`, `let` / `let mut` immutability & shadowing,
//! strict typing with explicit `as` casts, expression-oriented blocks & `if`/`else`, fixed `[T; N]`
//! arrays, lowers to labeled Three-Address Code (TAC), and verifies execution on a TAC VM.

use std::collections::BTreeMap;
use std::fmt;

// ---------------------------------------------------------------------------
// 1. Lexer (Nested Block Comments + Rust Operators)
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq)]
enum TokKind {
    Fn,
    Let,
    Mut,
    If,
    Else,
    While,
    Return,
    As,
    Emit,
    TrueKw,
    FalseKw,
    I64Kw,
    F64Kw,
    BoolKw,
    Ident(String),
    IntLit(i64),
    FloatLit(f64),
    Arrow,  // ->
    AndAnd, // &&
    OrOr,   // ||
    EqEq,   // ==
    Ne,     // !=
    Le,     // <=
    Ge,     // >=
    Lt,
    Gt,
    Assign,
    Plus,
    Minus,
    Star,
    Slash,
    Bang,
    Colon,
    Semi,
    Comma,
    LParen,
    RParen,
    LBrace,
    RBrace,
    LBracket,
    RBracket,
    Eof,
}

#[derive(Clone, Debug)]
struct Token {
    kind: TokKind,
    line: usize,
}

/// Tokenizes Rust-subset source, handling nested `/* ... /* ... */ ... */` comments.
fn lex(src: &str) -> Result<Vec<Token>, String> {
    let bytes = src.as_bytes();
    let mut i = 0usize;
    let mut line = 1usize;
    let mut out = Vec::new();

    while i < bytes.len() {
        let ch = bytes[i];
        if ch.is_ascii_whitespace() {
            if ch == b'\n' {
                line += 1;
            }
            i += 1;
            continue;
        }
        if ch == b'/' && bytes.get(i + 1) == Some(&b'/') {
            i += 2;
            while i < bytes.len() && bytes[i] != b'\n' {
                i += 1;
            }
            continue;
        }
        if ch == b'/' && bytes.get(i + 1) == Some(&b'*') {
            // Rust block comments nest: track nesting depth cleanly.
            i += 2;
            let mut depth = 1usize;
            while i < bytes.len() && depth > 0 {
                if bytes[i] == b'/' && bytes.get(i + 1) == Some(&b'*') {
                    depth += 1;
                    i += 2;
                } else if bytes[i] == b'*' && bytes.get(i + 1) == Some(&b'/') {
                    depth -= 1;
                    i += 2;
                } else {
                    if bytes[i] == b'\n' {
                        line += 1;
                    }
                    i += 1;
                }
            }
            if depth > 0 {
                return Err(format!("line {line}: unterminated nested block comment"));
            }
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
                "fn" => TokKind::Fn,
                "let" => TokKind::Let,
                "mut" => TokKind::Mut,
                "if" => TokKind::If,
                "else" => TokKind::Else,
                "while" => TokKind::While,
                "return" => TokKind::Return,
                "as" => TokKind::As,
                "emit" => TokKind::Emit,
                "true" => TokKind::TrueKw,
                "false" => TokKind::FalseKw,
                "i64" => TokKind::I64Kw,
                "f64" => TokKind::F64Kw,
                "bool" => TokKind::BoolKw,
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
                    .map_err(|_| format!("line {line}: bad float {s}"))?;
                out.push(Token {
                    kind: TokKind::FloatLit(v),
                    line,
                });
            } else {
                let s = std::str::from_utf8(&bytes[start..i]).unwrap();
                let v: i64 = s.parse().map_err(|_| format!("line {line}: bad int {s}"))?;
                out.push(Token {
                    kind: TokKind::IntLit(v),
                    line,
                });
            }
            continue;
        }

        let two = bytes.get(i + 1).copied();
        let kind = match (ch, two) {
            (b'-', Some(b'>')) => {
                i += 2;
                TokKind::Arrow
            }
            (b'&', Some(b'&')) => {
                i += 2;
                TokKind::AndAnd
            }
            (b'|', Some(b'|')) => {
                i += 2;
                TokKind::OrOr
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
            (b'!', _) => {
                i += 1;
                TokKind::Bang
            }
            (b':', _) => {
                i += 1;
                TokKind::Colon
            }
            (b';', _) => {
                i += 1;
                TokKind::Semi
            }
            (b',', _) => {
                i += 1;
                TokKind::Comma
            }
            (b'(', _) => {
                i += 1;
                TokKind::LParen
            }
            (b')', _) => {
                i += 1;
                TokKind::RParen
            }
            (b'{', _) => {
                i += 1;
                TokKind::LBrace
            }
            (b'}', _) => {
                i += 1;
                TokKind::RBrace
            }
            (b'[', _) => {
                i += 1;
                TokKind::LBracket
            }
            (b']', _) => {
                i += 1;
                TokKind::RBracket
            }
            _ => {
                return Err(format!(
                    "line {line}: unexpected character '{}'",
                    ch as char
                ));
            }
        };
        out.push(Token { kind, line });
    }
    out.push(Token {
        kind: TokKind::Eof,
        line,
    });
    Ok(out)
}

// ---------------------------------------------------------------------------
// 2. Types, Lexical Scopes & Mutability / Shadowing Table
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq)]
enum RsType {
    Unit,
    I64,
    F64,
    Bool,
    Array { elem: Box<RsType>, len: usize },
}

impl RsType {
    fn width(&self) -> usize {
        match self {
            RsType::Unit => 0,
            RsType::I64 | RsType::F64 => 8,
            RsType::Bool => 1,
            RsType::Array { elem, len } => len * elem.width(),
        }
    }

    fn is_numeric(&self) -> bool {
        matches!(self, RsType::I64 | RsType::F64)
    }
}

impl fmt::Display for RsType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RsType::Unit => write!(f, "()"),
            RsType::I64 => write!(f, "i64"),
            RsType::F64 => write!(f, "f64"),
            RsType::Bool => write!(f, "bool"),
            RsType::Array { elem, len } => write!(f, "[{elem}; {len}]"),
        }
    }
}

#[derive(Clone, Debug)]
struct VarInfo {
    slot: String,
    ty: RsType,
    mutable: bool,
    byte_off: usize,
}

#[derive(Clone, Debug)]
struct FnSig {
    params: Vec<(String, RsType)>,
    ret_ty: RsType,
}

struct ScopeEnv {
    scopes: Vec<BTreeMap<String, VarInfo>>,
    history: Vec<VarInfo>,
    next_slot_id: usize,
    used_bytes: usize,
    fn_sigs: BTreeMap<String, FnSig>,
}

impl ScopeEnv {
    fn new() -> Self {
        Self {
            scopes: Vec::new(),
            history: Vec::new(),
            next_slot_id: 0,
            used_bytes: 0,
            fn_sigs: BTreeMap::new(),
        }
    }

    fn push_scope(&mut self) {
        self.scopes.push(BTreeMap::new());
    }

    fn pop_scope(&mut self) {
        self.scopes.pop();
    }

    /// Declares or shadows a binding. In Rust, `let` in the same scope creates a fresh
    /// distinct slot (`name#N`) without overwriting previous slots cleanly!
    fn bind(&mut self, name: &str, ty: RsType, mutable: bool) -> VarInfo {
        let id = self.next_slot_id;
        self.next_slot_id += 1;
        let slot = format!("{name}#{id}");
        let off = self.used_bytes;
        self.used_bytes += ty.width();
        let info = VarInfo {
            slot,
            ty,
            mutable,
            byte_off: off,
        };
        self.history.push(info.clone());
        self.scopes
            .last_mut()
            .unwrap()
            .insert(name.to_string(), info.clone());
        info
    }

    fn lookup(&self, name: &str) -> Option<&VarInfo> {
        for sc in self.scopes.iter().rev() {
            if let Some(v) = sc.get(name) {
                return Some(v);
            }
        }
        None
    }
}

// ---------------------------------------------------------------------------
// 3. Typed Expression & Statement AST
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
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
enum RsVal {
    Unit,
    I64(i64),
    F64(f64),
    Bool(bool),
    Arr(Vec<RsVal>),
}

impl fmt::Display for RsVal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RsVal::Unit => write!(f, "()"),
            RsVal::I64(v) => write!(f, "{v}"),
            RsVal::F64(v) => write!(f, "{v:.4}"),
            RsVal::Bool(b) => write!(f, "{b}"),
            RsVal::Arr(xs) => write!(f, "arr(len={})", xs.len()),
        }
    }
}

#[derive(Clone, Debug)]
enum Expr {
    Lit(RsVal, RsType),
    Var {
        slot: String,
        ty: RsType,
    },
    ArrayLit {
        elems: Vec<Expr>,
        ty: RsType,
    },
    Index {
        base: Box<Expr>,
        idx: Box<Expr>,
        elem_ty: RsType,
    },
    Cast {
        inner: Box<Expr>,
        target_ty: RsType,
    },
    Neg {
        inner: Box<Expr>,
        ty: RsType,
    },
    Not {
        inner: Box<Expr>,
    },
    Bin {
        op: BinOp,
        left: Box<Expr>,
        right: Box<Expr>,
        ty: RsType,
    },
    Rel {
        op: RelOp,
        left: Box<Expr>,
        right: Box<Expr>,
    },
    And {
        left: Box<Expr>,
        right: Box<Expr>,
    },
    Or {
        left: Box<Expr>,
        right: Box<Expr>,
    },
    Call {
        name: String,
        args: Vec<Expr>,
        ty: RsType,
    },
    Block {
        stmts: Vec<Stmt>,
        tail: Option<Box<Expr>>,
        ty: RsType,
    },
    If {
        cond: Box<Expr>,
        then_blk: Box<Expr>,
        else_blk: Option<Box<Expr>>,
        ty: RsType,
    },
}

impl Expr {
    fn ty(&self) -> &RsType {
        match self {
            Expr::Lit(_, t)
            | Expr::Var { ty: t, .. }
            | Expr::ArrayLit { ty: t, .. }
            | Expr::Index { elem_ty: t, .. }
            | Expr::Cast { target_ty: t, .. }
            | Expr::Neg { ty: t, .. }
            | Expr::Bin { ty: t, .. }
            | Expr::Call { ty: t, .. }
            | Expr::Block { ty: t, .. }
            | Expr::If { ty: t, .. } => t,
            Expr::Not { .. } | Expr::Rel { .. } | Expr::And { .. } | Expr::Or { .. } => {
                &RsType::Bool
            }
        }
    }
}

#[derive(Clone, Debug)]
enum Stmt {
    Let { slot: String, init: Expr },
    Assign { slot: String, rhs: Expr },
    While { cond: Expr, body: Expr },
    Return(Option<Expr>),
    Emit(Expr),
    Expr(Expr),
}

#[derive(Clone, Debug)]
struct FnDecl {
    name: String,
    param_slots: Vec<String>,
    body: Expr,
}

// ---------------------------------------------------------------------------
// 4. Parser & Strict Type / Mutability Checker
// ---------------------------------------------------------------------------

struct Parser {
    toks: Vec<Token>,
    pos: usize,
    env: ScopeEnv,
    current_ret_ty: RsType,
}

impl Parser {
    fn new(toks: Vec<Token>) -> Self {
        Self {
            toks,
            pos: 0,
            env: ScopeEnv::new(),
            current_ret_ty: RsType::Unit,
        }
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

    fn expect(&mut self, expected: &TokKind) -> Result<(), String> {
        if self.peek() == expected {
            self.pos += 1;
            Ok(())
        } else {
            Err(self.err(&format!("expected {expected:?}, found {:?}", self.peek())))
        }
    }

    fn parse_type(&mut self) -> Result<RsType, String> {
        match self.peek().clone() {
            TokKind::I64Kw => {
                self.pos += 1;
                Ok(RsType::I64)
            }
            TokKind::F64Kw => {
                self.pos += 1;
                Ok(RsType::F64)
            }
            TokKind::BoolKw => {
                self.pos += 1;
                Ok(RsType::Bool)
            }
            TokKind::LBracket => {
                self.pos += 1;
                let elem = self.parse_type()?;
                self.expect(&TokKind::Semi)?;
                let len = match self.peek() {
                    TokKind::IntLit(n) if *n > 0 => {
                        let u = *n as usize;
                        self.pos += 1;
                        u
                    }
                    _ => return Err(self.err("expected positive integer length in [T; N]")),
                };
                self.expect(&TokKind::RBracket)?;
                Ok(RsType::Array {
                    elem: Box::new(elem),
                    len,
                })
            }
            TokKind::LParen => {
                self.pos += 1;
                self.expect(&TokKind::RParen)?;
                Ok(RsType::Unit)
            }
            other => Err(self.err(&format!("expected type annotation, found {other:?}"))),
        }
    }

    fn parse_program(&mut self) -> Result<(Vec<FnDecl>, BTreeMap<String, FnSig>), String> {
        // Pre-pass 1: collect all top-level function signatures so forward calls type-check.
        let saved_pos = self.pos;
        while *self.peek() != TokKind::Eof {
            self.expect(&TokKind::Fn)?;
            let name = match self.peek().clone() {
                TokKind::Ident(s) => {
                    self.pos += 1;
                    s
                }
                _ => return Err(self.err("expected function name after 'fn'")),
            };
            self.expect(&TokKind::LParen)?;
            let mut params = Vec::new();
            while *self.peek() != TokKind::RParen {
                let pname = match self.peek().clone() {
                    TokKind::Ident(s) => {
                        self.pos += 1;
                        s
                    }
                    _ => return Err(self.err("expected parameter name")),
                };
                self.expect(&TokKind::Colon)?;
                let pty = self.parse_type()?;
                params.push((pname, pty));
                if *self.peek() == TokKind::Comma {
                    self.pos += 1;
                } else {
                    break;
                }
            }
            self.expect(&TokKind::RParen)?;
            let ret_ty = if *self.peek() == TokKind::Arrow {
                self.pos += 1;
                self.parse_type()?
            } else {
                RsType::Unit
            };
            if self.env.fn_sigs.contains_key(&name) {
                return Err(self.err(&format!("duplicate function definition '{name}'")));
            }
            self.env.fn_sigs.insert(name, FnSig { params, ret_ty });
            // Skip `{ ... }` function body on pre-pass.
            self.expect(&TokKind::LBrace)?;
            let mut depth = 1usize;
            while *self.peek() != TokKind::Eof && depth > 0 {
                if *self.peek() == TokKind::LBrace {
                    depth += 1;
                } else if *self.peek() == TokKind::RBrace {
                    depth -= 1;
                }
                self.pos += 1;
            }
            if depth != 0 {
                return Err("unclosed '{' in function body".into());
            }
        }

        // Pass 2: parse every function body with full scope/type/mutability checks.
        self.pos = saved_pos;
        let mut fns = Vec::new();
        while *self.peek() != TokKind::Eof {
            fns.push(self.parse_fn()?);
        }
        if !self.env.fn_sigs.contains_key("main") {
            return Err("program requires a `fn main()` entry point".into());
        }
        Ok((fns, self.env.fn_sigs.clone()))
    }

    fn parse_fn(&mut self) -> Result<FnDecl, String> {
        self.expect(&TokKind::Fn)?;
        let name = match self.peek().clone() {
            TokKind::Ident(s) => {
                self.pos += 1;
                s
            }
            _ => return Err(self.err("expected function name")),
        };
        let sig = self.env.fn_sigs[&name].clone();
        self.expect(&TokKind::LParen)?;
        while *self.peek() != TokKind::RParen {
            self.pos += 1;
        }
        self.expect(&TokKind::RParen)?;
        if *self.peek() == TokKind::Arrow {
            self.pos += 1;
            let _ = self.parse_type()?;
        }

        self.env.push_scope();
        let mut param_slots = Vec::with_capacity(sig.params.len());
        for (pname, pty) in &sig.params {
            let info = self.env.bind(pname, pty.clone(), false);
            param_slots.push(info.slot);
        }
        self.current_ret_ty = sig.ret_ty.clone();
        let body = self.parse_block()?;
        if *body.ty() != sig.ret_ty {
            return Err(self.err(&format!(
                "function '{name}' expected return type {}, found {}",
                sig.ret_ty,
                body.ty()
            )));
        }
        self.env.pop_scope();
        Ok(FnDecl {
            name,
            param_slots,
            body,
        })
    }

    /// Parses a Rust block `{ stmt; ...; optional_tail_expr }` where a missing semicolon on
    /// the last expression yields that value as the block's type (`expression-oriented`).
    fn parse_block(&mut self) -> Result<Expr, String> {
        self.expect(&TokKind::LBrace)?;
        self.env.push_scope();
        let mut stmts = Vec::new();
        let mut tail: Option<Box<Expr>> = None;

        while *self.peek() != TokKind::RBrace {
            match self.peek().clone() {
                TokKind::Let => {
                    self.pos += 1;
                    let mutable = if *self.peek() == TokKind::Mut {
                        self.pos += 1;
                        true
                    } else {
                        false
                    };
                    let name = match self.peek().clone() {
                        TokKind::Ident(s) => {
                            self.pos += 1;
                            s
                        }
                        _ => return Err(self.err("expected binding identifier after 'let'")),
                    };
                    let ann_ty = if *self.peek() == TokKind::Colon {
                        self.pos += 1;
                        Some(self.parse_type()?)
                    } else {
                        None
                    };
                    self.expect(&TokKind::Assign)?;
                    let init = self.parse_expr()?;
                    if let Some(expected_ty) = ann_ty {
                        if init.ty() != &expected_ty {
                            return Err(self.err(&format!(
                                "let '{name}' declared {expected_ty}, initialized with {}",
                                init.ty()
                            )));
                        }
                    }
                    let info = self.env.bind(&name, init.ty().clone(), mutable);
                    self.expect(&TokKind::Semi)?;
                    stmts.push(Stmt::Let {
                        slot: info.slot,
                        init,
                    });
                }
                TokKind::While => {
                    self.pos += 1;
                    let cond = self.parse_expr()?;
                    if *cond.ty() != RsType::Bool {
                        return Err(self.err(&format!(
                            "while condition must be bool, found {}",
                            cond.ty()
                        )));
                    }
                    let body = self.parse_block()?;
                    if *body.ty() != RsType::Unit {
                        return Err(self.err("while body must evaluate to ()"));
                    }
                    stmts.push(Stmt::While { cond, body });
                }
                TokKind::Return => {
                    self.pos += 1;
                    let expr_opt = if *self.peek() != TokKind::Semi {
                        let e = self.parse_expr()?;
                        if *e.ty() != self.current_ret_ty {
                            return Err(self.err(&format!(
                                "return type mismatch: expected {}, found {}",
                                self.current_ret_ty,
                                e.ty()
                            )));
                        }
                        Some(e)
                    } else {
                        if self.current_ret_ty != RsType::Unit {
                            return Err(self.err("bare return in non-unit function"));
                        }
                        None
                    };
                    self.expect(&TokKind::Semi)?;
                    stmts.push(Stmt::Return(expr_opt));
                }
                TokKind::Emit => {
                    self.pos += 1;
                    self.expect(&TokKind::LParen)?;
                    let arg = self.parse_expr()?;
                    self.expect(&TokKind::RParen)?;
                    self.expect(&TokKind::Semi)?;
                    stmts.push(Stmt::Emit(arg));
                }
                TokKind::Ident(name) if *self.peek_next() == TokKind::Assign => {
                    self.pos += 1;
                    let var = self
                        .env
                        .lookup(&name)
                        .ok_or_else(|| self.err(&format!("undeclared variable '{name}'")))?
                        .clone();
                    if !var.mutable {
                        return Err(self.err(&format!(
                            "cannot assign twice to immutable variable '{name}' (hint: declare with `let mut`)"
                        )));
                    }
                    self.expect(&TokKind::Assign)?;
                    let rhs = self.parse_expr()?;
                    if *rhs.ty() != var.ty {
                        return Err(self.err(&format!(
                            "assignment to '{name}' expected {}, found {}",
                            var.ty,
                            rhs.ty()
                        )));
                    }
                    self.expect(&TokKind::Semi)?;
                    stmts.push(Stmt::Assign {
                        slot: var.slot,
                        rhs,
                    });
                }
                _ => {
                    let e = self.parse_expr()?;
                    if *self.peek() == TokKind::Semi {
                        self.pos += 1;
                        stmts.push(Stmt::Expr(e));
                    } else if *self.peek() == TokKind::RBrace {
                        tail = Some(Box::new(e));
                        break;
                    } else {
                        return Err(self.err("expected ';' or '}' after expression"));
                    }
                }
            }
        }
        self.expect(&TokKind::RBrace)?;
        self.env.pop_scope();
        let ty = tail.as_ref().map_or(RsType::Unit, |t| t.ty().clone());
        Ok(Expr::Block { stmts, tail, ty })
    }

    fn parse_expr(&mut self) -> Result<Expr, String> {
        self.parse_or()
    }

    fn parse_or(&mut self) -> Result<Expr, String> {
        let mut lhs = self.parse_and()?;
        while *self.peek() == TokKind::OrOr {
            self.pos += 1;
            let rhs = self.parse_and()?;
            if *lhs.ty() != RsType::Bool || *rhs.ty() != RsType::Bool {
                return Err(self.err("'||' requires bool operands"));
            }
            lhs = Expr::Or {
                left: Box::new(lhs),
                right: Box::new(rhs),
            };
        }
        Ok(lhs)
    }

    fn parse_and(&mut self) -> Result<Expr, String> {
        let mut lhs = self.parse_rel()?;
        while *self.peek() == TokKind::AndAnd {
            self.pos += 1;
            let rhs = self.parse_rel()?;
            if *lhs.ty() != RsType::Bool || *rhs.ty() != RsType::Bool {
                return Err(self.err("'&&' requires bool operands"));
            }
            lhs = Expr::And {
                left: Box::new(lhs),
                right: Box::new(rhs),
            };
        }
        Ok(lhs)
    }

    fn parse_rel(&mut self) -> Result<Expr, String> {
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
        if lhs.ty() != rhs.ty() {
            return Err(self.err(&format!(
                "comparison operands must share exact type: {} vs {}",
                lhs.ty(),
                rhs.ty()
            )));
        }
        Ok(Expr::Rel {
            op,
            left: Box::new(lhs),
            right: Box::new(rhs),
        })
    }

    fn parse_add(&mut self) -> Result<Expr, String> {
        let mut lhs = self.parse_mul()?;
        loop {
            let op = match self.peek() {
                TokKind::Plus => BinOp::Add,
                TokKind::Minus => BinOp::Sub,
                _ => break,
            };
            self.pos += 1;
            let rhs = self.parse_mul()?;
            if !lhs.ty().is_numeric() || lhs.ty() != rhs.ty() {
                return Err(self.err(&format!(
                    "mismatched types: cannot apply {:?} to {} and {} (use 'as f64')",
                    op,
                    lhs.ty(),
                    rhs.ty()
                )));
            }
            let ty = lhs.ty().clone();
            lhs = Expr::Bin {
                op,
                left: Box::new(lhs),
                right: Box::new(rhs),
                ty,
            };
        }
        Ok(lhs)
    }

    fn parse_mul(&mut self) -> Result<Expr, String> {
        let mut lhs = self.parse_cast()?;
        loop {
            let op = match self.peek() {
                TokKind::Star => BinOp::Mul,
                TokKind::Slash => BinOp::Div,
                _ => break,
            };
            self.pos += 1;
            let rhs = self.parse_cast()?;
            if !lhs.ty().is_numeric() || lhs.ty() != rhs.ty() {
                return Err(self.err(&format!(
                    "mismatched types: cannot apply {:?} to {} and {} (use 'as f64')",
                    op,
                    lhs.ty(),
                    rhs.ty()
                )));
            }
            let ty = lhs.ty().clone();
            lhs = Expr::Bin {
                op,
                left: Box::new(lhs),
                right: Box::new(rhs),
                ty,
            };
        }
        Ok(lhs)
    }

    fn parse_cast(&mut self) -> Result<Expr, String> {
        let mut expr = self.parse_unary()?;
        while *self.peek() == TokKind::As {
            self.pos += 1;
            let tgt = self.parse_type()?;
            if !expr.ty().is_numeric() || !tgt.is_numeric() {
                return Err(self.err(&format!("invalid cast from {} as {tgt}", expr.ty())));
            }
            expr = Expr::Cast {
                inner: Box::new(expr),
                target_ty: tgt,
            };
        }
        Ok(expr)
    }

    fn parse_unary(&mut self) -> Result<Expr, String> {
        match self.peek() {
            TokKind::Minus => {
                self.pos += 1;
                let inner = self.parse_unary()?;
                if !inner.ty().is_numeric() {
                    return Err(self.err("unary '-' requires numeric operand"));
                }
                let ty = inner.ty().clone();
                Ok(Expr::Neg {
                    inner: Box::new(inner),
                    ty,
                })
            }
            TokKind::Bang => {
                self.pos += 1;
                let inner = self.parse_unary()?;
                if *inner.ty() != RsType::Bool {
                    return Err(self.err("unary '!' requires bool"));
                }
                Ok(Expr::Not {
                    inner: Box::new(inner),
                })
            }
            _ => self.parse_postfix(),
        }
    }

    fn parse_postfix(&mut self) -> Result<Expr, String> {
        let mut base = self.parse_primary()?;
        while *self.peek() == TokKind::LBracket {
            self.pos += 1;
            let idx = self.parse_expr()?;
            if *idx.ty() != RsType::I64 {
                return Err(self.err(&format!("array index must be i64, found {}", idx.ty())));
            }
            self.expect(&TokKind::RBracket)?;
            let elem_ty = match base.ty() {
                RsType::Array { elem, .. } => (**elem).clone(),
                other => return Err(self.err(&format!("cannot index non-array {other}"))),
            };
            base = Expr::Index {
                base: Box::new(base),
                idx: Box::new(idx),
                elem_ty,
            };
        }
        Ok(base)
    }

    fn parse_primary(&mut self) -> Result<Expr, String> {
        match self.peek().clone() {
            TokKind::IntLit(n) => {
                self.pos += 1;
                Ok(Expr::Lit(RsVal::I64(n), RsType::I64))
            }
            TokKind::FloatLit(r) => {
                self.pos += 1;
                Ok(Expr::Lit(RsVal::F64(r), RsType::F64))
            }
            TokKind::TrueKw => {
                self.pos += 1;
                Ok(Expr::Lit(RsVal::Bool(true), RsType::Bool))
            }
            TokKind::FalseKw => {
                self.pos += 1;
                Ok(Expr::Lit(RsVal::Bool(false), RsType::Bool))
            }
            TokKind::If => {
                self.pos += 1;
                let cond = self.parse_expr()?;
                if *cond.ty() != RsType::Bool {
                    return Err(
                        self.err(&format!("if condition must be bool, found {}", cond.ty()))
                    );
                }
                let then_blk = self.parse_block()?;
                let else_blk = if *self.peek() == TokKind::Else {
                    self.pos += 1;
                    let eb = self.parse_block()?;
                    if eb.ty() != then_blk.ty() {
                        return Err(self.err(&format!(
                            "if/else arm type mismatch: {} vs {}",
                            then_blk.ty(),
                            eb.ty()
                        )));
                    }
                    Some(Box::new(eb))
                } else {
                    if *then_blk.ty() != RsType::Unit {
                        return Err(self.err("if without else must yield ()"));
                    }
                    None
                };
                let ty = then_blk.ty().clone();
                Ok(Expr::If {
                    cond: Box::new(cond),
                    then_blk: Box::new(then_blk),
                    else_blk,
                    ty,
                })
            }
            TokKind::LBrace => self.parse_block(),
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
                if elems.is_empty() {
                    return Err(self.err("empty array literal unsupported without type"));
                }
                let first_ty = elems[0].ty().clone();
                for e in &elems[1..] {
                    if *e.ty() != first_ty {
                        return Err(self.err("array literal elements must have identical type"));
                    }
                }
                let ty = RsType::Array {
                    elem: Box::new(first_ty),
                    len: elems.len(),
                };
                Ok(Expr::ArrayLit { elems, ty })
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
                    let sig = self
                        .env
                        .fn_sigs
                        .get(&name)
                        .ok_or_else(|| self.err(&format!("unknown function '{name}'")))?
                        .clone();
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
                    if args.len() != sig.params.len() {
                        return Err(self.err(&format!(
                            "call to '{name}' expected {} args, found {}",
                            sig.params.len(),
                            args.len()
                        )));
                    }
                    for (idx, (arg, (_, pty))) in args.iter().zip(sig.params.iter()).enumerate() {
                        if arg.ty() != pty {
                            return Err(self.err(&format!(
                                "arg #{} of '{name}' expected {pty}, found {}",
                                idx + 1,
                                arg.ty()
                            )));
                        }
                    }
                    Ok(Expr::Call {
                        name,
                        args,
                        ty: sig.ret_ty,
                    })
                } else {
                    let info = self
                        .env
                        .lookup(&name)
                        .ok_or_else(|| self.err(&format!("undeclared variable '{name}'")))?
                        .clone();
                    Ok(Expr::Var {
                        slot: info.slot,
                        ty: info.ty,
                    })
                }
            }
            other => Err(self.err(&format!("unexpected token in expression: {other:?}"))),
        }
    }
}

// ---------------------------------------------------------------------------
// 5. Three-Address Code (TAC) Lowering & VM Execution
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
enum Operand {
    Lit(RsVal),
    Slot(String),
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
        dst: String,
        src: Operand,
    },
    MakeArr {
        dst: String,
        elems: Vec<Operand>,
    },
    ArrLoad {
        dst: String,
        base: Operand,
        idx: Operand,
    },
    Cast {
        dst: String,
        src: Operand,
        target: RsType,
    },
    Neg {
        dst: String,
        src: Operand,
    },
    Not {
        dst: String,
        src: Operand,
    },
    Bin {
        dst: String,
        op: BinOp,
        left: Operand,
        right: Operand,
    },
    Rel {
        dst: String,
        op: RelOp,
        left: Operand,
        right: Operand,
    },
    Param(Operand),
    Call {
        dst: String,
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
            TacInst::MakeArr { dst, elems } => write!(f, "  {dst} = array({})", elems.len()),
            TacInst::ArrLoad { dst, base, idx } => write!(f, "  {dst} = {base}[{idx}]"),
            TacInst::Cast { dst, src, target } => write!(f, "  {dst} = {src} as {target}"),
            TacInst::Neg { dst, src } => write!(f, "  {dst} = -{src}"),
            TacInst::Not { dst, src } => write!(f, "  {dst} = !{src}"),
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
                    BinOp::Div => "/",
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
            TacInst::Return(None) => write!(f, "  return ()"),
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

    fn temp(&mut self) -> String {
        self.next_temp += 1;
        format!("t{}", self.next_temp)
    }

    fn label(&mut self, prefix: &str) -> String {
        self.next_lbl += 1;
        format!("{prefix}_{}", self.next_lbl)
    }

    fn gen_expr(&mut self, expr: &Expr) -> Operand {
        match expr {
            Expr::Lit(v, _) => Operand::Lit(v.clone()),
            Expr::Var { slot, .. } => Operand::Slot(slot.clone()),
            Expr::ArrayLit { elems, .. } => {
                let ops: Vec<Operand> = elems.iter().map(|e| self.gen_expr(e)).collect();
                let dst = self.temp();
                self.code.push(TacInst::MakeArr {
                    dst: dst.clone(),
                    elems: ops,
                });
                Operand::Slot(dst)
            }
            Expr::Index { base, idx, .. } => {
                let b = self.gen_expr(base);
                let i = self.gen_expr(idx);
                let dst = self.temp();
                self.code.push(TacInst::ArrLoad {
                    dst: dst.clone(),
                    base: b,
                    idx: i,
                });
                Operand::Slot(dst)
            }
            Expr::Cast { inner, target_ty } => {
                let src = self.gen_expr(inner);
                let dst = self.temp();
                self.code.push(TacInst::Cast {
                    dst: dst.clone(),
                    src,
                    target: target_ty.clone(),
                });
                Operand::Slot(dst)
            }
            Expr::Neg { inner, .. } => {
                let src = self.gen_expr(inner);
                let dst = self.temp();
                self.code.push(TacInst::Neg {
                    dst: dst.clone(),
                    src,
                });
                Operand::Slot(dst)
            }
            Expr::Not { inner } => {
                let src = self.gen_expr(inner);
                let dst = self.temp();
                self.code.push(TacInst::Not {
                    dst: dst.clone(),
                    src,
                });
                Operand::Slot(dst)
            }
            Expr::Bin {
                op, left, right, ..
            } => {
                let l = self.gen_expr(left);
                let r = self.gen_expr(right);
                let dst = self.temp();
                self.code.push(TacInst::Bin {
                    dst: dst.clone(),
                    op: *op,
                    left: l,
                    right: r,
                });
                Operand::Slot(dst)
            }
            Expr::Rel { op, left, right } => {
                let l = self.gen_expr(left);
                let r = self.gen_expr(right);
                let dst = self.temp();
                self.code.push(TacInst::Rel {
                    dst: dst.clone(),
                    op: *op,
                    left: l,
                    right: r,
                });
                Operand::Slot(dst)
            }
            Expr::And { left, right } => {
                let dst = self.temp();
                let l_false = self.label("and_false");
                let l_end = self.label("and_end");
                let l = self.gen_expr(left);
                self.code.push(TacInst::IfFalse {
                    cond: l,
                    label: l_false.clone(),
                });
                let r = self.gen_expr(right);
                self.code.push(TacInst::Assign {
                    dst: dst.clone(),
                    src: r,
                });
                self.code.push(TacInst::Goto(l_end.clone()));
                self.code.push(TacInst::Label(l_false));
                self.code.push(TacInst::Assign {
                    dst: dst.clone(),
                    src: Operand::Lit(RsVal::Bool(false)),
                });
                self.code.push(TacInst::Label(l_end));
                Operand::Slot(dst)
            }
            Expr::Or { left, right } => {
                let dst = self.temp();
                let l_rhs = self.label("or_rhs");
                let l_end = self.label("or_end");
                let l = self.gen_expr(left);
                self.code.push(TacInst::IfFalse {
                    cond: l,
                    label: l_rhs.clone(),
                });
                self.code.push(TacInst::Assign {
                    dst: dst.clone(),
                    src: Operand::Lit(RsVal::Bool(true)),
                });
                self.code.push(TacInst::Goto(l_end.clone()));
                self.code.push(TacInst::Label(l_rhs));
                let r = self.gen_expr(right);
                self.code.push(TacInst::Assign {
                    dst: dst.clone(),
                    src: r,
                });
                self.code.push(TacInst::Label(l_end));
                Operand::Slot(dst)
            }
            Expr::Call { name, args, .. } => {
                let ops: Vec<Operand> = args.iter().map(|a| self.gen_expr(a)).collect();
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
            Expr::Block { stmts, tail, .. } => {
                for s in stmts {
                    self.gen_stmt(s);
                }
                if let Some(t) = tail {
                    self.gen_expr(t)
                } else {
                    Operand::Lit(RsVal::Unit)
                }
            }
            Expr::If {
                cond,
                then_blk,
                else_blk,
                ..
            } => {
                let dst = self.temp();
                let l_else = self.label("else");
                let l_end = self.label("endif");
                let c = self.gen_expr(cond);
                self.code.push(TacInst::IfFalse {
                    cond: c,
                    label: l_else.clone(),
                });
                let tv = self.gen_expr(then_blk);
                self.code.push(TacInst::Assign {
                    dst: dst.clone(),
                    src: tv,
                });
                self.code.push(TacInst::Goto(l_end.clone()));
                self.code.push(TacInst::Label(l_else));
                if let Some(eb) = else_blk {
                    let ev = self.gen_expr(eb);
                    self.code.push(TacInst::Assign {
                        dst: dst.clone(),
                        src: ev,
                    });
                } else {
                    self.code.push(TacInst::Assign {
                        dst: dst.clone(),
                        src: Operand::Lit(RsVal::Unit),
                    });
                }
                self.code.push(TacInst::Label(l_end));
                Operand::Slot(dst)
            }
        }
    }

    fn gen_stmt(&mut self, stmt: &Stmt) {
        match stmt {
            Stmt::Let { slot, init } => {
                let v = self.gen_expr(init);
                self.code.push(TacInst::Assign {
                    dst: slot.clone(),
                    src: v,
                });
            }
            Stmt::Assign { slot, rhs } => {
                let v = self.gen_expr(rhs);
                self.code.push(TacInst::Assign {
                    dst: slot.clone(),
                    src: v,
                });
            }
            Stmt::While { cond, body } => {
                let l_top = self.label("while");
                let l_end = self.label("endwhile");
                self.code.push(TacInst::Label(l_top.clone()));
                let c = self.gen_expr(cond);
                self.code.push(TacInst::IfFalse {
                    cond: c,
                    label: l_end.clone(),
                });
                let _ = self.gen_expr(body);
                self.code.push(TacInst::Goto(l_top));
                self.code.push(TacInst::Label(l_end));
            }
            Stmt::Return(opt) => {
                let v = opt.as_ref().map(|e| self.gen_expr(e));
                self.code.push(TacInst::Return(v));
            }
            Stmt::Emit(e) => {
                let v = self.gen_expr(e);
                self.code.push(TacInst::Emit(v));
            }
            Stmt::Expr(e) => {
                let _ = self.gen_expr(e);
            }
        }
    }
}

#[derive(Debug)]
struct CompiledRust {
    fn_sigs: BTreeMap<String, FnSig>,
    fn_params: BTreeMap<String, Vec<String>>,
    bindings: Vec<VarInfo>,
    used_bytes: usize,
    code: Vec<TacInst>,
}

fn compile_rust_subset(src: &str) -> Result<CompiledRust, String> {
    let toks = lex(src)?;
    let mut parser = Parser::new(toks);
    let (fns, fn_sigs) = parser.parse_program()?;
    let mut emitter = Emitter::new();
    let mut fn_params = BTreeMap::new();

    for f in fns {
        fn_params.insert(f.name.clone(), f.param_slots);
        emitter.code.push(TacInst::Label(format!("fn_{}", f.name)));
        let tail_val = emitter.gen_expr(&f.body);
        emitter.code.push(TacInst::Return(Some(tail_val)));
    }
    Ok(CompiledRust {
        fn_sigs,
        fn_params,
        bindings: parser.env.history,
        used_bytes: parser.env.used_bytes,
        code: emitter.code,
    })
}

fn exec_tac(prog: &CompiledRust) -> Result<(RsVal, Vec<RsVal>), String> {
    let mut labels = BTreeMap::new();
    for (idx, inst) in prog.code.iter().enumerate() {
        if let TacInst::Label(name) = inst {
            labels.insert(name.clone(), idx);
        }
    }

    struct Frame {
        vars: BTreeMap<String, RsVal>,
        ret_pc: usize,
        ret_dst: Option<String>,
    }

    let eval_op = |op: &Operand, vars: &BTreeMap<String, RsVal>| -> RsVal {
        match op {
            Operand::Lit(v) => v.clone(),
            Operand::Slot(s) => vars.get(s).cloned().unwrap_or(RsVal::Unit),
        }
    };

    let mut frames = vec![Frame {
        vars: BTreeMap::new(),
        ret_pc: prog.code.len(),
        ret_dst: None,
    }];
    let mut pc = labels["fn_main"];
    let mut param_buf: Vec<RsVal> = Vec::new();
    let mut emitted = Vec::new();

    while pc < prog.code.len() {
        match &prog.code[pc] {
            TacInst::Label(_) => pc += 1,
            TacInst::Assign { dst, src } => {
                let v = eval_op(src, &frames.last().unwrap().vars);
                frames.last_mut().unwrap().vars.insert(dst.clone(), v);
                pc += 1;
            }
            TacInst::MakeArr { dst, elems } => {
                let vs: Vec<RsVal> = elems
                    .iter()
                    .map(|e| eval_op(e, &frames.last().unwrap().vars))
                    .collect();
                frames
                    .last_mut()
                    .unwrap()
                    .vars
                    .insert(dst.clone(), RsVal::Arr(vs));
                pc += 1;
            }
            TacInst::ArrLoad { dst, base, idx } => {
                let vars = &frames.last().unwrap().vars;
                let RsVal::Arr(vec) = eval_op(base, vars) else {
                    return Err("expected array at runtime".into());
                };
                let RsVal::I64(i) = eval_op(idx, vars) else {
                    return Err("expected integer index".into());
                };
                if i < 0 || (i as usize) >= vec.len() {
                    return Err(format!("index out of bounds: {i} for len {}", vec.len()));
                }
                let val = vec[i as usize].clone();
                frames.last_mut().unwrap().vars.insert(dst.clone(), val);
                pc += 1;
            }
            TacInst::Cast { dst, src, target } => {
                let v = eval_op(src, &frames.last().unwrap().vars);
                let out = match (v, target) {
                    (RsVal::I64(n), RsType::F64) => RsVal::F64(n as f64),
                    (RsVal::F64(r), RsType::I64) => RsVal::I64(r as i64),
                    (x, _) => x,
                };
                frames.last_mut().unwrap().vars.insert(dst.clone(), out);
                pc += 1;
            }
            TacInst::Neg { dst, src } => {
                let v = match eval_op(src, &frames.last().unwrap().vars) {
                    RsVal::I64(n) => RsVal::I64(-n),
                    RsVal::F64(r) => RsVal::F64(-r),
                    _ => return Err("bad unary '-'".into()),
                };
                frames.last_mut().unwrap().vars.insert(dst.clone(), v);
                pc += 1;
            }
            TacInst::Not { dst, src } => {
                let RsVal::Bool(b) = eval_op(src, &frames.last().unwrap().vars) else {
                    return Err("bad '!'".into());
                };
                frames
                    .last_mut()
                    .unwrap()
                    .vars
                    .insert(dst.clone(), RsVal::Bool(!b));
                pc += 1;
            }
            TacInst::Bin {
                dst,
                op,
                left,
                right,
            } => {
                let vars = &frames.last().unwrap().vars;
                let l = eval_op(left, vars);
                let r = eval_op(right, vars);
                let out = match (l, r) {
                    (RsVal::I64(a), RsVal::I64(b)) => RsVal::I64(match op {
                        BinOp::Add => a + b,
                        BinOp::Sub => a - b,
                        BinOp::Mul => a * b,
                        BinOp::Div => a / b,
                    }),
                    (RsVal::F64(a), RsVal::F64(b)) => RsVal::F64(match op {
                        BinOp::Add => a + b,
                        BinOp::Sub => a - b,
                        BinOp::Mul => a * b,
                        BinOp::Div => a / b,
                    }),
                    _ => return Err("runtime type mismatch in binary op".into()),
                };
                frames.last_mut().unwrap().vars.insert(dst.clone(), out);
                pc += 1;
            }
            TacInst::Rel {
                dst,
                op,
                left,
                right,
            } => {
                let vars = &frames.last().unwrap().vars;
                let l = eval_op(left, vars);
                let r = eval_op(right, vars);
                let b = match (&l, &r) {
                    (RsVal::I64(a), RsVal::I64(b)) => match op {
                        RelOp::Eq => a == b,
                        RelOp::Ne => a != b,
                        RelOp::Lt => a < b,
                        RelOp::Le => a <= b,
                        RelOp::Gt => a > b,
                        RelOp::Ge => a >= b,
                    },
                    (RsVal::F64(a), RsVal::F64(b)) => match op {
                        RelOp::Eq => a == b,
                        RelOp::Ne => a != b,
                        RelOp::Lt => a < b,
                        RelOp::Le => a <= b,
                        RelOp::Gt => a > b,
                        RelOp::Ge => a >= b,
                    },
                    (RsVal::Bool(a), RsVal::Bool(b)) => match op {
                        RelOp::Eq => a == b,
                        RelOp::Ne => a != b,
                        _ => false,
                    },
                    _ => false,
                };
                frames
                    .last_mut()
                    .unwrap()
                    .vars
                    .insert(dst.clone(), RsVal::Bool(b));
                pc += 1;
            }
            TacInst::Param(op) => {
                param_buf.push(eval_op(op, &frames.last().unwrap().vars));
                pc += 1;
            }
            TacInst::Call { dst, func, argc } => {
                let start = param_buf.len() - argc;
                let args: Vec<RsVal> = param_buf.drain(start..).collect();
                let mut callee_vars = BTreeMap::new();
                for (slot, val) in prog.fn_params[func].iter().zip(args) {
                    callee_vars.insert(slot.clone(), val);
                }
                frames.push(Frame {
                    vars: callee_vars,
                    ret_pc: pc + 1,
                    ret_dst: Some(dst.clone()),
                });
                pc = labels[&format!("fn_{func}")];
            }
            TacInst::IfFalse { cond, label } => {
                let c = eval_op(cond, &frames.last().unwrap().vars);
                if matches!(c, RsVal::Bool(false)) {
                    pc = labels[label];
                } else {
                    pc += 1;
                }
            }
            TacInst::Goto(l) => pc = labels[l],
            TacInst::Return(opt) => {
                let ret_val = opt
                    .as_ref()
                    .map_or(RsVal::Unit, |o| eval_op(o, &frames.last().unwrap().vars));
                let popped = frames.pop().unwrap();
                if let Some(dst) = popped.ret_dst {
                    frames.last_mut().unwrap().vars.insert(dst, ret_val);
                    pc = popped.ret_pc;
                } else {
                    return Ok((ret_val, emitted));
                }
            }
            TacInst::Emit(op) => {
                emitted.push(eval_op(op, &frames.last().unwrap().vars));
                pc += 1;
            }
        }
    }
    Ok((RsVal::Unit, emitted))
}

const SAMPLE_RUST_SRC: &str = r#"
fn weighted_sum(arr: [i64; 4], limit: i64, bonus: f64) -> f64 {
    let mut idx: i64 = 0;
    let mut acc: i64 = 0;
    while idx < limit {
        let weight: i64 = idx + 1;
        /* Nested /* block comment */: shadow `weight` immutably inside the loop body */
        let weight: i64 = if idx == 0 { weight } else { weight * 2 };
        acc = acc + arr[idx] * weight;
        idx = idx + 1;
    }
    let scale: f64 = 2.0;
    (acc as f64) / scale + bonus
}

fn main() -> i64 {
    let scores: [i64; 4] = [10, 20, 30, 40];
    let total: f64 = weighted_sum(scores, 4, 2.5);
    let rounded: i64 = total as i64;
    emit(total);
    rounded
}
"#;

fn main() {
    let prog = compile_rust_subset(SAMPLE_RUST_SRC).expect("valid Rust subset");
    println!("Function signatures ({}):", prog.fn_sigs.len());
    for (name, sig) in &prog.fn_sigs {
        let params: Vec<String> = sig
            .params
            .iter()
            .map(|(n, t)| format!("{n}: {t}"))
            .collect();
        println!("  fn {name}({}) -> {}", params.join(", "), sig.ret_ty);
    }
    println!(
        "\nBindings & stack byte offsets ({} bytes):",
        prog.used_bytes
    );
    for b in &prog.bindings {
        let mut_tag = if b.mutable { "mut " } else { "" };
        println!(
            "  {:14} : {mut_tag}{:10} @ byte {}",
            b.slot, b.ty, b.byte_off
        );
    }
    println!("\nEmitted TAC ({} instructions):", prog.code.len());
    for inst in &prog.code {
        println!("{inst}");
    }
    let (ret_val, emitted) = exec_tac(&prog).expect("TAC executes cleanly");
    println!("\nEmitted output : {emitted:?}");
    println!("main() returned: {ret_val}");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compiles_shadowing_expression_if_explicit_casts_and_runs_tac() {
        let prog = compile_rust_subset(SAMPLE_RUST_SRC).expect("compiles");
        assert_eq!(prog.used_bytes, 136);
        let (ret_val, emitted) = exec_tac(&prog).expect("runs");
        assert_eq!(emitted, vec![RsVal::F64(297.5)]);
        assert_eq!(ret_val, RsVal::I64(297));
    }

    #[test]
    fn short_circuit_and_or_guard_bounds_and_runtime_oob_errors() {
        let guarded = r#"
fn main() -> i64 {
    let a: [i64; 2] = [7, 9];
    let idx: i64 = 5;
    if idx < 2 && a[idx] > 0 { 1 } else { 0 }
}
"#;
        let prog = compile_rust_subset(guarded).expect("compiles");
        assert_eq!(exec_tac(&prog).expect("short-circuits").0, RsVal::I64(0));

        let oob = "fn main() -> i64 { let a: [i64; 2] = [7, 9]; a[2] }";
        let oob_prog = compile_rust_subset(oob).expect("compiles");
        assert!(exec_tac(&oob_prog).unwrap_err().contains("out of bounds"));
    }

    #[test]
    fn rejects_immutable_reassignment_implicit_widening_and_branch_mismatch() {
        assert!(
            compile_rust_subset("fn main() -> i64 { let x: i64 = 1; x = 2; x }")
                .unwrap_err()
                .contains("immutable")
        );
        assert!(compile_rust_subset("fn main() -> f64 { 1 + 2.5 }")
            .unwrap_err()
            .contains("mismatched types"));
        assert!(compile_rust_subset(
            "fn main() -> i64 { let x: i64 = if true { 1 } else { 2.0 }; x }"
        )
        .unwrap_err()
        .contains("mismatch"));
    }
}
