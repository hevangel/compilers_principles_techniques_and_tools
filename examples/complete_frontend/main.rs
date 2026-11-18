//! Appendix A (2006 Purple) capstone: Complete structured block-scoped front end in Rust.
//! Architecture parallels the classic multi-package front end (Lexer -> Env/Type -> AST -> TAC with short-circuit `jumping` & break labels), plus a TAC verifier VM.

use std::collections::BTreeMap;
use std::fmt;

// ---------------------------------------------------------------------------
// 1. Lexer
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq)]
enum TokKind {
    // Type & statement keywords
    IntKw,
    FloatKw,
    BoolKw,
    TrueKw,
    FalseKw,
    If,
    Else,
    While,
    Do,
    Break,
    // Literals & identifiers
    Ident(String),
    IntLit(i64),
    FloatLit(f64),
    // Two-character & single-character operators
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
    Semi,
    LBrace,
    RBrace,
    LParen,
    RParen,
    LBracket,
    RBracket,
    Eof,
}

#[derive(Clone, Debug)]
struct Token {
    kind: TokKind,
    line: usize,
}

/// Scans C/Java-like structured source text into tokens, skipping whitespace
/// and `//` or `/* ... */` comments while tracking source line numbers.
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
        // Line comment `// ...`
        if ch == b'/' && bytes.get(i + 1) == Some(&b'/') {
            i += 2;
            while i < bytes.len() && bytes[i] != b'\n' {
                i += 1;
            }
            continue;
        }
        // Block comment `/* ... */`
        if ch == b'/' && bytes.get(i + 1) == Some(&b'*') {
            i += 2;
            while i + 1 < bytes.len() && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
                if bytes[i] == b'\n' {
                    line += 1;
                }
                i += 1;
            }
            if i + 1 >= bytes.len() {
                return Err(format!("line {line}: unterminated block comment"));
            }
            i += 2;
            continue;
        }
        // Identifiers & reserved words (lookup happens only after longest match)
        if ch.is_ascii_alphabetic() || ch == b'_' {
            let start = i;
            i += 1;
            while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                i += 1;
            }
            let word = &src[start..i];
            let kind = match word {
                "int" => TokKind::IntKw,
                "float" => TokKind::FloatKw,
                "bool" => TokKind::BoolKw,
                "true" => TokKind::TrueKw,
                "false" => TokKind::FalseKw,
                "if" => TokKind::If,
                "else" => TokKind::Else,
                "while" => TokKind::While,
                "do" => TokKind::Do,
                "break" => TokKind::Break,
                _ => TokKind::Ident(word.to_string()),
            };
            out.push(Token { kind, line });
            continue;
        }
        // Integer or floating-point literal
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
                let val: f64 = src[start..i]
                    .parse()
                    .map_err(|_| format!("line {line}: invalid float literal"))?;
                out.push(Token {
                    kind: TokKind::FloatLit(val),
                    line,
                });
            } else {
                let val: i64 = src[start..i]
                    .parse()
                    .map_err(|_| format!("line {line}: invalid int literal"))?;
                out.push(Token {
                    kind: TokKind::IntLit(val),
                    line,
                });
            }
            continue;
        }
        let kind = match (ch, bytes.get(i + 1).copied()) {
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
            (b'=', _) => {
                i += 1;
                TokKind::Assign
            }
            (b'<', _) => {
                i += 1;
                TokKind::Lt
            }
            (b'>', _) => {
                i += 1;
                TokKind::Gt
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
            (b';', _) => {
                i += 1;
                TokKind::Semi
            }
            (b'{', _) => {
                i += 1;
                TokKind::LBrace
            }
            (b'}', _) => {
                i += 1;
                TokKind::RBrace
            }
            (b'(', _) => {
                i += 1;
                TokKind::LParen
            }
            (b')', _) => {
                i += 1;
                TokKind::RParen
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
// 2. Types & Chained Block Environments (`Env`)
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq)]
enum Type {
    Int,
    Float,
    Bool,
    Array { len: usize, elem: Box<Type> },
}

impl Type {
    fn width(&self) -> usize {
        match self {
            Type::Int => 4,
            Type::Float => 8,
            Type::Bool => 1,
            Type::Array { len, elem } => len * elem.width(),
        }
    }

    fn is_numeric(&self) -> bool {
        matches!(self, Type::Int | Type::Float)
    }

    /// Coercion lattice: `float` dominates `int`.
    fn max_numeric(a: &Type, b: &Type) -> Option<Type> {
        if !a.is_numeric() || !b.is_numeric() {
            None
        } else if *a == Type::Float || *b == Type::Float {
            Some(Type::Float)
        } else {
            Some(Type::Int)
        }
    }
}

impl fmt::Display for Type {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut dims = Vec::new();
        let mut cur = self;
        while let Type::Array { len, elem } = cur {
            dims.push(*len);
            cur = elem;
        }
        match cur {
            Type::Int => write!(f, "int")?,
            Type::Float => write!(f, "float")?,
            Type::Bool => write!(f, "bool")?,
            Type::Array { .. } => unreachable!(),
        }
        for d in dims {
            write!(f, "[{d}]")?;
        }
        Ok(())
    }
}

#[derive(Clone, Debug)]
struct VarBinding {
    slot: String,
    ty: Type,
    offset: usize,
    block_id: usize,
}

/// Lexical environment chain (`Env` in Appendix A): each `{ ... }` block pushes a new
/// map onto the scope chain so inner variables shadow outer ones cleanly.
struct EnvStack {
    blocks: Vec<(usize, BTreeMap<String, VarBinding>)>,
    next_block_id: usize,
    used_bytes: usize,
    all_decls: Vec<VarBinding>,
}

impl EnvStack {
    fn new() -> Self {
        Self {
            blocks: Vec::new(),
            next_block_id: 0,
            used_bytes: 0,
            all_decls: Vec::new(),
        }
    }

    fn push_block(&mut self) {
        let id = self.next_block_id;
        self.next_block_id += 1;
        self.blocks.push((id, BTreeMap::new()));
    }

    fn pop_block(&mut self) {
        self.blocks.pop();
    }

    fn declare(&mut self, name: &str, ty: Type, line: usize) -> Result<VarBinding, String> {
        let (block_id, map) = self.blocks.last_mut().expect("missing block scope");
        if map.contains_key(name) {
            return Err(format!(
                "line {line}: duplicate declaration of '{name}' in same block"
            ));
        }
        let slot = format!("{name}@b{block_id}");
        let offset = self.used_bytes;
        self.used_bytes += ty.width();
        let binding = VarBinding {
            slot,
            ty,
            offset,
            block_id: *block_id,
        };
        map.insert(name.to_string(), binding.clone());
        self.all_decls.push(binding.clone());
        Ok(binding)
    }

    fn lookup(&self, name: &str) -> Option<&VarBinding> {
        self.blocks.iter().rev().find_map(|(_, m)| m.get(name))
    }
}

// ---------------------------------------------------------------------------
// 3. Abstract Syntax Tree (`Expr` & `Stmt`)
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
}

impl fmt::Display for BinOp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            BinOp::Add => "+",
            BinOp::Sub => "-",
            BinOp::Mul => "*",
            BinOp::Div => "/",
        };
        write!(f, "{s}")
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RelOp {
    Lt,
    Le,
    Eq,
    Ne,
    Gt,
    Ge,
}

impl fmt::Display for RelOp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            RelOp::Lt => "<",
            RelOp::Le => "<=",
            RelOp::Eq => "==",
            RelOp::Ne => "!=",
            RelOp::Gt => ">",
            RelOp::Ge => ">=",
        };
        write!(f, "{s}")
    }
}

#[derive(Clone, Debug, PartialEq)]
enum ConstVal {
    Int(i64),
    Float(f64),
    Bool(bool),
}

impl fmt::Display for ConstVal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConstVal::Int(i) => write!(f, "{i}"),
            ConstVal::Float(r) => write!(f, "{r:.4}"),
            ConstVal::Bool(b) => write!(f, "{b}"),
        }
    }
}

#[derive(Clone, Debug)]
enum Expr {
    Const(ConstVal, Type),
    Id {
        slot: String,
        ty: Type,
    },
    /// Array element `base[byte_offset_expr]` with element type `ty`.
    Access {
        base_slot: String,
        offset_expr: Box<Expr>,
        ty: Type,
    },
    Arith {
        op: BinOp,
        left: Box<Expr>,
        right: Box<Expr>,
        ty: Type,
    },
    Neg {
        inner: Box<Expr>,
        ty: Type,
    },
    Not {
        inner: Box<Expr>,
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
}

impl Expr {
    fn ty(&self) -> Type {
        match self {
            Expr::Const(_, t)
            | Expr::Id { ty: t, .. }
            | Expr::Access { ty: t, .. }
            | Expr::Arith { ty: t, .. }
            | Expr::Neg { ty: t, .. } => t.clone(),
            Expr::Not { .. } | Expr::Rel { .. } | Expr::And { .. } | Expr::Or { .. } => Type::Bool,
        }
    }
}

#[derive(Clone, Debug)]
enum Stmt {
    Empty,
    Seq(Vec<Stmt>),
    Set {
        slot: String,
        rhs: Expr,
    },
    SetElem {
        base_slot: String,
        offset_expr: Expr,
        rhs: Expr,
    },
    If {
        cond: Expr,
        then_stmt: Box<Stmt>,
    },
    Else {
        cond: Expr,
        then_stmt: Box<Stmt>,
        else_stmt: Box<Stmt>,
    },
    While {
        loop_id: usize,
        cond: Expr,
        body: Box<Stmt>,
    },
    DoWhile {
        loop_id: usize,
        body: Box<Stmt>,
        cond: Expr,
    },
    Break {
        loop_id: usize,
    },
}

// ---------------------------------------------------------------------------
// 4. Parser (Builds Scoped AST & Checks Types)
// ---------------------------------------------------------------------------

struct Parser {
    tokens: Vec<Token>,
    pos: usize,
    env: EnvStack,
    loop_stack: Vec<usize>,
    next_loop_id: usize,
}

impl Parser {
    fn new(tokens: Vec<Token>) -> Self {
        Self {
            tokens,
            pos: 0,
            env: EnvStack::new(),
            loop_stack: Vec::new(),
            next_loop_id: 0,
        }
    }

    fn peek(&self) -> &Token {
        &self.tokens[self.pos]
    }

    fn line(&self) -> usize {
        self.peek().line
    }

    fn advance(&mut self) -> Token {
        let t = self.tokens[self.pos].clone();
        if self.pos + 1 < self.tokens.len() {
            self.pos += 1;
        }
        t
    }

    fn eat(&mut self, expected: &TokKind) -> bool {
        if &self.peek().kind == expected {
            self.advance();
            true
        } else {
            false
        }
    }

    fn expect(&mut self, expected: &TokKind) -> Result<(), String> {
        if self.eat(expected) {
            Ok(())
        } else {
            Err(format!(
                "line {}: expected {:?}, found {:?}",
                self.line(),
                expected,
                self.peek().kind
            ))
        }
    }

    fn parse_program(&mut self) -> Result<(Stmt, Vec<VarBinding>, usize), String> {
        let stmt = self.parse_block()?;
        self.expect(&TokKind::Eof)?;
        Ok((stmt, self.env.all_decls.clone(), self.env.used_bytes))
    }

    fn parse_block(&mut self) -> Result<Stmt, String> {
        self.expect(&TokKind::LBrace)?;
        self.env.push_block();
        self.parse_decls()?;
        let mut stmts = Vec::new();
        while !matches!(self.peek().kind, TokKind::RBrace | TokKind::Eof) {
            stmts.push(self.parse_stmt()?);
        }
        self.expect(&TokKind::RBrace)?;
        self.env.pop_block();
        Ok(match stmts.len() {
            0 => Stmt::Empty,
            1 => stmts.remove(0),
            _ => Stmt::Seq(stmts),
        })
    }

    fn parse_decls(&mut self) -> Result<(), String> {
        while matches!(
            self.peek().kind,
            TokKind::IntKw | TokKind::FloatKw | TokKind::BoolKw
        ) {
            let ty = self.parse_type()?;
            let ln = self.line();
            let name = match self.advance().kind {
                TokKind::Ident(s) => s,
                other => return Err(format!("line {ln}: expected identifier, found {other:?}")),
            };
            self.expect(&TokKind::Semi)?;
            self.env.declare(&name, ty, ln)?;
        }
        Ok(())
    }

    fn parse_type(&mut self) -> Result<Type, String> {
        let ln = self.line();
        let mut ty = match self.advance().kind {
            TokKind::IntKw => Type::Int,
            TokKind::FloatKw => Type::Float,
            TokKind::BoolKw => Type::Bool,
            other => return Err(format!("line {ln}: expected type, found {other:?}")),
        };
        // Parse `[N][M]` right-recursively so `float[2][3]` means array(2) of array(3) of float.
        let mut dims = Vec::new();
        while self.eat(&TokKind::LBracket) {
            let d_ln = self.line();
            let len = match self.advance().kind {
                TokKind::IntLit(v) if v > 0 => v as usize,
                other => {
                    return Err(format!(
                        "line {d_ln}: expected positive array size, found {other:?}"
                    ))
                }
            };
            self.expect(&TokKind::RBracket)?;
            dims.push(len);
        }
        for len in dims.into_iter().rev() {
            ty = Type::Array {
                len,
                elem: Box::new(ty),
            };
        }
        Ok(ty)
    }

    fn parse_stmt(&mut self) -> Result<Stmt, String> {
        let ln = self.line();
        match self.peek().kind.clone() {
            TokKind::Semi => {
                self.advance();
                Ok(Stmt::Empty)
            }
            TokKind::LBrace => self.parse_block(),
            TokKind::If => {
                self.advance();
                self.expect(&TokKind::LParen)?;
                let cond = self.parse_bool_expr()?;
                self.expect(&TokKind::RParen)?;
                let then_stmt = Box::new(self.parse_stmt()?);
                if self.eat(&TokKind::Else) {
                    let else_stmt = Box::new(self.parse_stmt()?);
                    Ok(Stmt::Else {
                        cond,
                        then_stmt,
                        else_stmt,
                    })
                } else {
                    Ok(Stmt::If { cond, then_stmt })
                }
            }
            TokKind::While => {
                self.advance();
                self.expect(&TokKind::LParen)?;
                let cond = self.parse_bool_expr()?;
                self.expect(&TokKind::RParen)?;
                let loop_id = self.next_loop_id;
                self.next_loop_id += 1;
                self.loop_stack.push(loop_id);
                let body = Box::new(self.parse_stmt()?);
                self.loop_stack.pop();
                Ok(Stmt::While {
                    loop_id,
                    cond,
                    body,
                })
            }
            TokKind::Do => {
                self.advance();
                let loop_id = self.next_loop_id;
                self.next_loop_id += 1;
                self.loop_stack.push(loop_id);
                let body = Box::new(self.parse_stmt()?);
                self.loop_stack.pop();
                self.expect(&TokKind::While)?;
                self.expect(&TokKind::LParen)?;
                let cond = self.parse_bool_expr()?;
                self.expect(&TokKind::RParen)?;
                self.expect(&TokKind::Semi)?;
                Ok(Stmt::DoWhile {
                    loop_id,
                    body,
                    cond,
                })
            }
            TokKind::Break => {
                self.advance();
                self.expect(&TokKind::Semi)?;
                let loop_id = *self
                    .loop_stack
                    .last()
                    .ok_or_else(|| format!("line {ln}: unenclosed 'break' statement"))?;
                Ok(Stmt::Break { loop_id })
            }
            TokKind::Ident(name) => {
                self.advance();
                let binding = self
                    .env
                    .lookup(&name)
                    .cloned()
                    .ok_or_else(|| format!("line {ln}: undeclared identifier '{name}'"))?;
                if matches!(self.peek().kind, TokKind::LBracket) {
                    let (base_slot, offset_expr, elem_ty) = self.parse_array_offset(binding, ln)?;
                    self.expect(&TokKind::Assign)?;
                    let rhs = self.parse_expr()?;
                    if !assignable(&elem_ty, &rhs.ty()) {
                        return Err(format!(
                            "line {ln}: type error assigning {} to {elem_ty}",
                            rhs.ty()
                        ));
                    }
                    self.expect(&TokKind::Semi)?;
                    Ok(Stmt::SetElem {
                        base_slot,
                        offset_expr,
                        rhs,
                    })
                } else {
                    self.expect(&TokKind::Assign)?;
                    let rhs = self.parse_expr()?;
                    if !assignable(&binding.ty, &rhs.ty()) {
                        return Err(format!(
                            "line {ln}: type error assigning {} to {}",
                            rhs.ty(),
                            binding.ty
                        ));
                    }
                    self.expect(&TokKind::Semi)?;
                    Ok(Stmt::Set {
                        slot: binding.slot,
                        rhs,
                    })
                }
            }
            other => Err(format!(
                "line {ln}: unexpected token in statement: {other:?}"
            )),
        }
    }

    fn parse_bool_expr(&mut self) -> Result<Expr, String> {
        let ln = self.line();
        let e = self.parse_expr()?;
        if e.ty() != Type::Bool {
            return Err(format!(
                "line {ln}: boolean required in condition, got {}",
                e.ty()
            ));
        }
        Ok(e)
    }

    /// Lowers nested indexing `a[i][j]` into a linear byte-offset expression `i * w1 + j * w2`.
    fn parse_array_offset(
        &mut self,
        binding: VarBinding,
        line: usize,
    ) -> Result<(String, Expr, Type), String> {
        let mut cur_ty = binding.ty;
        let mut total_offset: Option<Expr> = None;

        while self.eat(&TokKind::LBracket) {
            let elem_ty = match cur_ty {
                Type::Array { elem, .. } => *elem,
                other => return Err(format!("line {line}: cannot index non-array type {other}")),
            };
            let idx_ln = self.line();
            let idx = self.parse_expr()?;
            if idx.ty() != Type::Int {
                return Err(format!(
                    "line {idx_ln}: array subscript must be int, got {}",
                    idx.ty()
                ));
            }
            self.expect(&TokKind::RBracket)?;

            let stride = elem_ty.width() as i64;
            let scaled = Expr::Arith {
                op: BinOp::Mul,
                left: Box::new(idx),
                right: Box::new(Expr::Const(ConstVal::Int(stride), Type::Int)),
                ty: Type::Int,
            };
            total_offset = Some(match total_offset {
                None => scaled,
                Some(prev) => Expr::Arith {
                    op: BinOp::Add,
                    left: Box::new(prev),
                    right: Box::new(scaled),
                    ty: Type::Int,
                },
            });
            cur_ty = elem_ty;
        }
        Ok((
            binding.slot,
            total_offset.expect("at least one bracket"),
            cur_ty,
        ))
    }

    fn parse_expr(&mut self) -> Result<Expr, String> {
        let mut lhs = self.parse_join()?;
        while self.eat(&TokKind::OrOr) {
            let ln = self.line();
            let rhs = self.parse_join()?;
            if lhs.ty() != Type::Bool || rhs.ty() != Type::Bool {
                return Err(format!("line {ln}: '||' requires bool operands"));
            }
            lhs = Expr::Or {
                left: Box::new(lhs),
                right: Box::new(rhs),
            };
        }
        Ok(lhs)
    }

    fn parse_join(&mut self) -> Result<Expr, String> {
        let mut lhs = self.parse_equality()?;
        while self.eat(&TokKind::AndAnd) {
            let ln = self.line();
            let rhs = self.parse_equality()?;
            if lhs.ty() != Type::Bool || rhs.ty() != Type::Bool {
                return Err(format!("line {ln}: '&&' requires bool operands"));
            }
            lhs = Expr::And {
                left: Box::new(lhs),
                right: Box::new(rhs),
            };
        }
        Ok(lhs)
    }

    fn parse_equality(&mut self) -> Result<Expr, String> {
        let mut lhs = self.parse_rel()?;
        while let Some(op) = match self.peek().kind {
            TokKind::EqEq => Some(RelOp::Eq),
            TokKind::Ne => Some(RelOp::Ne),
            _ => None,
        } {
            let ln = self.line();
            self.advance();
            let rhs = self.parse_rel()?;
            if !(lhs.ty() == rhs.ty() || (lhs.ty().is_numeric() && rhs.ty().is_numeric())) {
                return Err(format!(
                    "line {ln}: incompatible types {} and {} for {op}",
                    lhs.ty(),
                    rhs.ty()
                ));
            }
            lhs = Expr::Rel {
                op,
                left: Box::new(lhs),
                right: Box::new(rhs),
            };
        }
        Ok(lhs)
    }

    fn parse_rel(&mut self) -> Result<Expr, String> {
        let lhs = self.parse_arith()?;
        let op = match self.peek().kind {
            TokKind::Lt => Some(RelOp::Lt),
            TokKind::Le => Some(RelOp::Le),
            TokKind::Gt => Some(RelOp::Gt),
            TokKind::Ge => Some(RelOp::Ge),
            _ => None,
        };
        if let Some(op) = op {
            let ln = self.line();
            self.advance();
            let rhs = self.parse_arith()?;
            if !lhs.ty().is_numeric() || !rhs.ty().is_numeric() {
                return Err(format!(
                    "line {ln}: ordering '{op}' requires numeric operands, got {} and {}",
                    lhs.ty(),
                    rhs.ty()
                ));
            }
            Ok(Expr::Rel {
                op,
                left: Box::new(lhs),
                right: Box::new(rhs),
            })
        } else {
            Ok(lhs)
        }
    }

    fn parse_arith(&mut self) -> Result<Expr, String> {
        let mut lhs = self.parse_term()?;
        while let Some(op) = match self.peek().kind {
            TokKind::Plus => Some(BinOp::Add),
            TokKind::Minus => Some(BinOp::Sub),
            _ => None,
        } {
            let ln = self.line();
            self.advance();
            let rhs = self.parse_term()?;
            let res_ty = Type::max_numeric(&lhs.ty(), &rhs.ty()).ok_or_else(|| {
                format!(
                    "line {ln}: arithmetic '{op}' requires numeric types, got {} and {}",
                    lhs.ty(),
                    rhs.ty()
                )
            })?;
            lhs = Expr::Arith {
                op,
                left: Box::new(lhs),
                right: Box::new(rhs),
                ty: res_ty,
            };
        }
        Ok(lhs)
    }

    fn parse_term(&mut self) -> Result<Expr, String> {
        let mut lhs = self.parse_unary()?;
        while let Some(op) = match self.peek().kind {
            TokKind::Star => Some(BinOp::Mul),
            TokKind::Slash => Some(BinOp::Div),
            _ => None,
        } {
            let ln = self.line();
            self.advance();
            let rhs = self.parse_unary()?;
            let res_ty = Type::max_numeric(&lhs.ty(), &rhs.ty()).ok_or_else(|| {
                format!(
                    "line {ln}: arithmetic '{op}' requires numeric types, got {} and {}",
                    lhs.ty(),
                    rhs.ty()
                )
            })?;
            lhs = Expr::Arith {
                op,
                left: Box::new(lhs),
                right: Box::new(rhs),
                ty: res_ty,
            };
        }
        Ok(lhs)
    }

    fn parse_unary(&mut self) -> Result<Expr, String> {
        let ln = self.line();
        if self.eat(&TokKind::Bang) {
            let inner = self.parse_unary()?;
            if inner.ty() != Type::Bool {
                return Err(format!("line {ln}: '!' requires bool operand"));
            }
            return Ok(Expr::Not {
                inner: Box::new(inner),
            });
        }
        if self.eat(&TokKind::Minus) {
            let inner = self.parse_unary()?;
            let ty = inner.ty();
            if !ty.is_numeric() {
                return Err(format!("line {ln}: unary '-' requires numeric operand"));
            }
            return Ok(Expr::Neg {
                inner: Box::new(inner),
                ty,
            });
        }
        self.parse_factor()
    }

    fn parse_factor(&mut self) -> Result<Expr, String> {
        let tok = self.advance();
        let ln = tok.line;
        match tok.kind {
            TokKind::LParen => {
                let inner = self.parse_expr()?;
                self.expect(&TokKind::RParen)?;
                Ok(inner)
            }
            TokKind::IntLit(v) => Ok(Expr::Const(ConstVal::Int(v), Type::Int)),
            TokKind::FloatLit(v) => Ok(Expr::Const(ConstVal::Float(v), Type::Float)),
            TokKind::TrueKw => Ok(Expr::Const(ConstVal::Bool(true), Type::Bool)),
            TokKind::FalseKw => Ok(Expr::Const(ConstVal::Bool(false), Type::Bool)),
            TokKind::Ident(name) => {
                let binding = self
                    .env
                    .lookup(&name)
                    .cloned()
                    .ok_or_else(|| format!("line {ln}: undeclared identifier '{name}'"))?;
                if matches!(self.peek().kind, TokKind::LBracket) {
                    let (base_slot, offset_expr, elem_ty) = self.parse_array_offset(binding, ln)?;
                    Ok(Expr::Access {
                        base_slot,
                        offset_expr: Box::new(offset_expr),
                        ty: elem_ty,
                    })
                } else {
                    Ok(Expr::Id {
                        slot: binding.slot,
                        ty: binding.ty,
                    })
                }
            }
            other => Err(format!("line {ln}: unexpected token {other:?}")),
        }
    }
}

fn assignable(dst: &Type, src: &Type) -> bool {
    !matches!(dst, Type::Array { .. }) && (dst == src || (*dst == Type::Float && *src == Type::Int))
}

// ---------------------------------------------------------------------------
// 5. Label-Driven TAC Emitter (`gen` & Short-Circuit `jumping`)
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
enum Operand {
    Var(String),
    Const(ConstVal),
}

impl fmt::Display for Operand {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Operand::Var(s) => write!(f, "{s}"),
            Operand::Const(c) => write!(f, "{c}"),
        }
    }
}

#[derive(Clone, Debug)]
enum TacInst {
    Label(usize),
    Assign {
        dst: String,
        src: Operand,
    },
    Bin {
        dst: String,
        op: BinOp,
        left: Operand,
        right: Operand,
    },
    Neg {
        dst: String,
        src: Operand,
    },
    ArrayLoad {
        dst: String,
        base: String,
        byte_off: Operand,
    },
    ArrayStore {
        base: String,
        byte_off: Operand,
        src: Operand,
    },
    Goto(usize),
    IfTrue {
        cond: Operand,
        target: usize,
    },
    IfFalse {
        cond: Operand,
        target: usize,
    },
    IfRel {
        left: Operand,
        op: RelOp,
        right: Operand,
        target: usize,
    },
    IfNotRel {
        left: Operand,
        op: RelOp,
        right: Operand,
        target: usize,
    },
}

impl fmt::Display for TacInst {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TacInst::Label(l) => write!(f, "L{l}:"),
            TacInst::Assign { dst, src } => write!(f, "  {dst} = {src}"),
            TacInst::Bin {
                dst,
                op,
                left,
                right,
            } => write!(f, "  {dst} = {left} {op} {right}"),
            TacInst::Neg { dst, src } => write!(f, "  {dst} = -{src}"),
            TacInst::ArrayLoad {
                dst,
                base,
                byte_off,
            } => write!(f, "  {dst} = {base}[{byte_off}]"),
            TacInst::ArrayStore {
                base,
                byte_off,
                src,
            } => write!(f, "  {base}[{byte_off}] = {src}"),
            TacInst::Goto(l) => write!(f, "  goto L{l}"),
            TacInst::IfTrue { cond, target } => write!(f, "  if {cond} goto L{target}"),
            TacInst::IfFalse { cond, target } => write!(f, "  iffalse {cond} goto L{target}"),
            TacInst::IfRel {
                left,
                op,
                right,
                target,
            } => write!(f, "  if {left} {op} {right} goto L{target}"),
            TacInst::IfNotRel {
                left,
                op,
                right,
                target,
            } => write!(f, "  iffalse {left} {op} {right} goto L{target}"),
        }
    }
}

struct Emitter {
    next_label: usize,
    next_temp: usize,
    loop_after: BTreeMap<usize, usize>,
    code: Vec<TacInst>,
}

impl Emitter {
    fn new() -> Self {
        Self {
            next_label: 0,
            next_temp: 0,
            loop_after: BTreeMap::new(),
            code: Vec::new(),
        }
    }

    /// Labels start at `1`; label `0` means "fall through to the next instruction".
    fn new_label(&mut self) -> usize {
        self.next_label += 1;
        self.next_label
    }

    fn new_temp(&mut self) -> String {
        self.next_temp += 1;
        format!("t{}", self.next_temp)
    }

    fn emit_label(&mut self, l: usize) {
        self.code.push(TacInst::Label(l));
    }

    /// Reduces an arbitrary expression down to a flat TAC operand (variable/temp/const).
    fn reduce(&mut self, expr: &Expr) -> Operand {
        match expr {
            Expr::Const(v, _) => Operand::Const(v.clone()),
            Expr::Id { slot, .. } => Operand::Var(slot.clone()),
            Expr::Access {
                base_slot,
                offset_expr,
                ..
            } => {
                let off = self.reduce(offset_expr);
                let dst = self.new_temp();
                self.code.push(TacInst::ArrayLoad {
                    dst: dst.clone(),
                    base: base_slot.clone(),
                    byte_off: off,
                });
                Operand::Var(dst)
            }
            Expr::Arith {
                op, left, right, ..
            } => {
                let l = self.reduce(left);
                let r = self.reduce(right);
                let dst = self.new_temp();
                self.code.push(TacInst::Bin {
                    dst: dst.clone(),
                    op: *op,
                    left: l,
                    right: r,
                });
                Operand::Var(dst)
            }
            Expr::Neg { inner, .. } => {
                let v = self.reduce(inner);
                let dst = self.new_temp();
                self.code.push(TacInst::Neg {
                    dst: dst.clone(),
                    src: v,
                });
                Operand::Var(dst)
            }
            Expr::Not { .. } | Expr::Rel { .. } | Expr::And { .. } | Expr::Or { .. } => {
                // Materialize a boolean temporary only when a boolean value is stored in a variable.
                let t = self.new_label();
                let f = self.new_label();
                let end = self.new_label();
                let dst = self.new_temp();
                self.jumping(expr, t, f);
                self.emit_label(t);
                self.code.push(TacInst::Assign {
                    dst: dst.clone(),
                    src: Operand::Const(ConstVal::Bool(true)),
                });
                self.code.push(TacInst::Goto(end));
                self.emit_label(f);
                self.code.push(TacInst::Assign {
                    dst: dst.clone(),
                    src: Operand::Const(ConstVal::Bool(false)),
                });
                self.emit_label(end);
                Operand::Var(dst)
            }
        }
    }

    /// Short-circuit boolean code generator (`jumping(t, f)` in 2006 Appendix A):
    /// `0` as `t` or `f` means fall through cleanly without emitting a redundant jump.
    fn jumping(&mut self, expr: &Expr, t: usize, f: usize) {
        match expr {
            Expr::Const(ConstVal::Bool(true), _) => {
                if t != 0 {
                    self.code.push(TacInst::Goto(t));
                }
            }
            Expr::Const(ConstVal::Bool(false), _) => {
                if f != 0 {
                    self.code.push(TacInst::Goto(f));
                }
            }
            Expr::Not { inner } => self.jumping(inner, f, t),
            Expr::Or { left, right } => {
                let label = if t != 0 { t } else { self.new_label() };
                self.jumping(left, label, 0);
                self.jumping(right, t, f);
                if t == 0 {
                    self.emit_label(label);
                }
            }
            Expr::And { left, right } => {
                let label = if f != 0 { f } else { self.new_label() };
                self.jumping(left, 0, label);
                self.jumping(right, t, f);
                if f == 0 {
                    self.emit_label(label);
                }
            }
            Expr::Rel { op, left, right } => {
                let l = self.reduce(left);
                let r = self.reduce(right);
                match (t != 0, f != 0) {
                    (true, true) => {
                        self.code.push(TacInst::IfRel {
                            left: l,
                            op: *op,
                            right: r,
                            target: t,
                        });
                        self.code.push(TacInst::Goto(f));
                    }
                    (true, false) => self.code.push(TacInst::IfRel {
                        left: l,
                        op: *op,
                        right: r,
                        target: t,
                    }),
                    (false, true) => self.code.push(TacInst::IfNotRel {
                        left: l,
                        op: *op,
                        right: r,
                        target: f,
                    }),
                    (false, false) => {}
                }
            }
            other => {
                let v = self.reduce(other);
                match (t != 0, f != 0) {
                    (true, true) => {
                        self.code.push(TacInst::IfTrue { cond: v, target: t });
                        self.code.push(TacInst::Goto(f));
                    }
                    (true, false) => self.code.push(TacInst::IfTrue { cond: v, target: t }),
                    (false, true) => self.code.push(TacInst::IfFalse { cond: v, target: f }),
                    (false, false) => {}
                }
            }
        }
    }

    fn gen_stmt(&mut self, stmt: &Stmt, begin_lbl: usize, after_lbl: usize) {
        match stmt {
            Stmt::Empty => {}
            Stmt::Seq(list) => {
                let mut cur_begin = begin_lbl;
                for (idx, s) in list.iter().enumerate() {
                    if idx + 1 == list.len() {
                        self.gen_stmt(s, cur_begin, after_lbl);
                    } else {
                        let mid = self.new_label();
                        self.gen_stmt(s, cur_begin, mid);
                        self.emit_label(mid);
                        cur_begin = mid;
                    }
                }
            }
            Stmt::Set { slot, rhs } => {
                let val = self.reduce(rhs);
                self.code.push(TacInst::Assign {
                    dst: slot.clone(),
                    src: val,
                });
            }
            Stmt::SetElem {
                base_slot,
                offset_expr,
                rhs,
            } => {
                let off = self.reduce(offset_expr);
                let val = self.reduce(rhs);
                self.code.push(TacInst::ArrayStore {
                    base: base_slot.clone(),
                    byte_off: off,
                    src: val,
                });
            }
            Stmt::If { cond, then_stmt } => {
                let l_body = self.new_label();
                self.jumping(cond, 0, after_lbl);
                self.emit_label(l_body);
                self.gen_stmt(then_stmt, l_body, after_lbl);
            }
            Stmt::Else {
                cond,
                then_stmt,
                else_stmt,
            } => {
                let l_then = self.new_label();
                let l_else = self.new_label();
                self.jumping(cond, 0, l_else);
                self.emit_label(l_then);
                self.gen_stmt(then_stmt, l_then, after_lbl);
                self.code.push(TacInst::Goto(after_lbl));
                self.emit_label(l_else);
                self.gen_stmt(else_stmt, l_else, after_lbl);
            }
            Stmt::While {
                loop_id,
                cond,
                body,
            } => {
                self.loop_after.insert(*loop_id, after_lbl);
                self.jumping(cond, 0, after_lbl);
                let l_body = self.new_label();
                self.emit_label(l_body);
                self.gen_stmt(body, l_body, begin_lbl);
                self.code.push(TacInst::Goto(begin_lbl));
            }
            Stmt::DoWhile {
                loop_id,
                body,
                cond,
            } => {
                self.loop_after.insert(*loop_id, after_lbl);
                let l_cond = self.new_label();
                self.gen_stmt(body, begin_lbl, l_cond);
                self.emit_label(l_cond);
                self.jumping(cond, begin_lbl, 0);
            }
            Stmt::Break { loop_id } => {
                let exit = self.loop_after[loop_id];
                self.code.push(TacInst::Goto(exit));
            }
        }
    }
}

#[derive(Debug)]
struct CompiledProgram {
    decls: Vec<VarBinding>,
    used_bytes: usize,
    code: Vec<TacInst>,
}

fn compile_frontend(src: &str) -> Result<CompiledProgram, String> {
    let tokens = lex(src)?;
    let (ast, decls, used_bytes) = Parser::new(tokens).parse_program()?;
    let mut emitter = Emitter::new();
    let begin = emitter.new_label();
    let after = emitter.new_label();
    emitter.emit_label(begin);
    emitter.gen_stmt(&ast, begin, after);
    emitter.emit_label(after);
    Ok(CompiledProgram {
        decls,
        used_bytes,
        code: emitter.code,
    })
}

// ---------------------------------------------------------------------------
// 6. Byte-Offset TAC Interpreter
// ---------------------------------------------------------------------------

#[derive(Default)]
struct VmState {
    vars: BTreeMap<String, ConstVal>,
    arrays: BTreeMap<(String, i64), ConstVal>,
}

fn exec_tac(prog: &CompiledProgram) -> Result<VmState, String> {
    let mut labels = BTreeMap::new();
    for (idx, inst) in prog.code.iter().enumerate() {
        if let TacInst::Label(id) = inst {
            labels.insert(*id, idx);
        }
    }
    let mut state = VmState::default();
    let eval = |op: &Operand, vars: &BTreeMap<String, ConstVal>| -> ConstVal {
        match op {
            Operand::Const(v) => v.clone(),
            Operand::Var(s) => vars.get(s).cloned().unwrap_or(ConstVal::Int(0)),
        }
    };
    let as_f64 = |v: &ConstVal| -> f64 {
        match v {
            ConstVal::Int(i) => *i as f64,
            ConstVal::Float(r) => *r,
            ConstVal::Bool(b) => {
                if *b {
                    1.0
                } else {
                    0.0
                }
            }
        }
    };
    let eval_rel = |l: &ConstVal, op: RelOp, r: &ConstVal| -> bool {
        match op {
            RelOp::Eq => l == r,
            RelOp::Ne => l != r,
            RelOp::Lt => as_f64(l) < as_f64(r),
            RelOp::Le => as_f64(l) <= as_f64(r),
            RelOp::Gt => as_f64(l) > as_f64(r),
            RelOp::Ge => as_f64(l) >= as_f64(r),
        }
    };

    let mut pc = 0usize;
    while pc < prog.code.len() {
        match &prog.code[pc] {
            TacInst::Label(_) => pc += 1,
            TacInst::Assign { dst, src } => {
                let v = eval(src, &state.vars);
                state.vars.insert(dst.clone(), v);
                pc += 1;
            }
            TacInst::Bin {
                dst,
                op,
                left,
                right,
            } => {
                let l = eval(left, &state.vars);
                let r = eval(right, &state.vars);
                let out = match (&l, &r) {
                    (ConstVal::Int(a), ConstVal::Int(b)) => ConstVal::Int(match op {
                        BinOp::Add => a + b,
                        BinOp::Sub => a - b,
                        BinOp::Mul => a * b,
                        BinOp::Div => a / b,
                    }),
                    _ => {
                        let a = as_f64(&l);
                        let b = as_f64(&r);
                        ConstVal::Float(match op {
                            BinOp::Add => a + b,
                            BinOp::Sub => a - b,
                            BinOp::Mul => a * b,
                            BinOp::Div => a / b,
                        })
                    }
                };
                state.vars.insert(dst.clone(), out);
                pc += 1;
            }
            TacInst::Neg { dst, src } => {
                let v = match eval(src, &state.vars) {
                    ConstVal::Int(i) => ConstVal::Int(-i),
                    ConstVal::Float(r) => ConstVal::Float(-r),
                    _ => return Err("unary neg expected numeric".into()),
                };
                state.vars.insert(dst.clone(), v);
                pc += 1;
            }
            TacInst::ArrayLoad {
                dst,
                base,
                byte_off,
            } => {
                let off = match eval(byte_off, &state.vars) {
                    ConstVal::Int(i) => i,
                    _ => return Err("array byte offset must be int".into()),
                };
                let val = state
                    .arrays
                    .get(&(base.clone(), off))
                    .cloned()
                    .unwrap_or(ConstVal::Int(0));
                state.vars.insert(dst.clone(), val);
                pc += 1;
            }
            TacInst::ArrayStore {
                base,
                byte_off,
                src,
            } => {
                let off = match eval(byte_off, &state.vars) {
                    ConstVal::Int(i) => i,
                    _ => return Err("array byte offset must be int".into()),
                };
                let val = eval(src, &state.vars);
                state.arrays.insert((base.clone(), off), val);
                pc += 1;
            }
            TacInst::Goto(target) => pc = labels[target],
            TacInst::IfTrue { cond, target } => {
                if matches!(eval(cond, &state.vars), ConstVal::Bool(true)) {
                    pc = labels[target];
                } else {
                    pc += 1;
                }
            }
            TacInst::IfFalse { cond, target } => {
                if !matches!(eval(cond, &state.vars), ConstVal::Bool(true)) {
                    pc = labels[target];
                } else {
                    pc += 1;
                }
            }
            TacInst::IfRel {
                left,
                op,
                right,
                target,
            } => {
                let l = eval(left, &state.vars);
                let r = eval(right, &state.vars);
                pc = if eval_rel(&l, *op, &r) {
                    labels[target]
                } else {
                    pc + 1
                };
            }
            TacInst::IfNotRel {
                left,
                op,
                right,
                target,
            } => {
                let l = eval(left, &state.vars);
                let r = eval(right, &state.vars);
                pc = if !eval_rel(&l, *op, &r) {
                    labels[target]
                } else {
                    pc + 1
                };
            }
        }
    }
    Ok(state)
}

const SAMPLE_SRC: &str = "
{
    int i;
    int sum;
    float[2][3] grid;
    sum = 0;
    i = 0;
    while (i < 2) {
        int j;
        {
            // Inner `i` shadows outer `i` cleanly via chained Env blocks.
            int i;
            i = 100;
            sum = sum + i;
        }
        j = 0;
        do {
            if (i == 1 && j == 2) break;
            grid[i][j] = (i * 3 + j) + 0.5;
            j = j + 1;
        } while (j < 3);
        i = i + 1;
    }
}
";

fn main() {
    let prog = compile_frontend(SAMPLE_SRC).expect("sample compiles");
    println!(
        "Lexical symbol bindings ({} bytes total storage):",
        prog.used_bytes
    );
    for b in &prog.decls {
        println!(
            "  {:<8} : {:<14} block={} offset={}",
            b.slot,
            b.ty.to_string(),
            b.block_id,
            b.offset
        );
    }
    println!("\nEmitted TAC ({} instructions):", prog.code.len());
    for inst in &prog.code {
        println!("{inst}");
    }
    let state = exec_tac(&prog).expect("TAC executes");
    println!("\nFinal state checks:");
    println!("  outer i@b0     = {:?}", state.vars.get("i@b0"));
    println!("  inner i@b2     = {:?}", state.vars.get("i@b2"));
    println!("  sum@b0         = {:?}", state.vars.get("sum@b0"));
    println!(
        "  grid[0][2]     = {:?}",
        state.arrays.get(&("grid@b0".into(), 16))
    );
    println!(
        "  grid[1][1]     = {:?}",
        state.arrays.get(&("grid@b0".into(), 32))
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lowers_nested_scopes_multidim_arrays_short_circuit_and_break() {
        let prog = compile_frontend(SAMPLE_SRC).expect("valid program");
        let state = exec_tac(&prog).expect("TAC executes cleanly");
        assert_eq!(state.vars.get("i@b0"), Some(&ConstVal::Int(2)));
        assert_eq!(state.vars.get("i@b2"), Some(&ConstVal::Int(100)));
        assert_eq!(state.vars.get("sum@b0"), Some(&ConstVal::Int(200)));
        // grid[0][2] -> byte offset 0*24 + 2*8 = 16 -> value 2.5
        assert_eq!(
            state.arrays.get(&("grid@b0".into(), 16)),
            Some(&ConstVal::Float(2.5))
        );
        // grid[1][1] -> byte offset 1*24 + 1*8 = 32 -> value 4.5
        assert_eq!(
            state.arrays.get(&("grid@b0".into(), 32)),
            Some(&ConstVal::Float(4.5))
        );
        // grid[1][2] -> skipped by `if (i == 1 && j == 2) break;`
        assert!(!state.arrays.contains_key(&("grid@b0".into(), 40)));
    }

    #[test]
    fn rejects_invalid_break_duplicates_and_type_errors() {
        assert!(compile_frontend("{ break; }")
            .unwrap_err()
            .contains("unenclosed 'break'"));
        assert!(compile_frontend("{ int x; float x; }")
            .unwrap_err()
            .contains("duplicate declaration"));
        assert!(compile_frontend("{ int x; if (x) x = 1; }")
            .unwrap_err()
            .contains("boolean required"));
        assert!(compile_frontend("{ int x; x = x[0]; }")
            .unwrap_err()
            .contains("cannot index non-array"));
    }
}
