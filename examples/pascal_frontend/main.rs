//! Appendix A (1986 Red) capstone: Pascal-subset compiler front end in Rust.
//! Pipeline: Pascal Lexer -> Nested Scope Symbol Table -> Recursive-Descent Parser & Type Checker -> Three-Address Code (TAC) + TAC Simulator.

use std::collections::BTreeMap;
use std::fmt;

// ---------------------------------------------------------------------------
// 1. Lexical Analysis (Scanner)
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq)]
enum TokKind {
    // Keywords
    Program,
    Var,
    Integer,
    Real,
    Boolean,
    Array,
    Of,
    Procedure,
    Function,
    Begin,
    End,
    If,
    Then,
    Else,
    While,
    Do,
    Div,
    Mod,
    And,
    Or,
    Not,
    True,
    False,
    Write,
    // Literals & identifiers
    Ident(String),
    IntLit(i64),
    RealLit(f64),
    // Operators & delimiters
    Assign, // :=
    Dots,   // ..
    Le,     // <=
    Ge,     // >=
    Ne,     // <>
    Lt,
    Gt,
    Eq,
    Plus,
    Minus,
    Star,
    Slash,
    Colon,
    Semi,
    Comma,
    Dot,
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

/// Scans Pascal source text into tokens. Keywords and identifiers are normalized
/// to lowercase because standard Pascal is case-insensitive.
fn lex(src: &str) -> Result<Vec<Token>, String> {
    let bytes = src.as_bytes();
    let mut i = 0;
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
        // Brace comments `{ ... }`
        if ch == b'{' {
            i += 1;
            while i < bytes.len() && bytes[i] != b'}' {
                if bytes[i] == b'\n' {
                    line += 1;
                }
                i += 1;
            }
            if i >= bytes.len() {
                return Err(format!("line {line}: unterminated '{{' comment"));
            }
            i += 1;
            continue;
        }
        // Paren-star comments `(* ... *)`
        if ch == b'(' && bytes.get(i + 1) == Some(&b'*') {
            i += 2;
            while i + 1 < bytes.len() && !(bytes[i] == b'*' && bytes[i + 1] == b')') {
                if bytes[i] == b'\n' {
                    line += 1;
                }
                i += 1;
            }
            if i + 1 >= bytes.len() {
                return Err(format!("line {line}: unterminated '(*' comment"));
            }
            i += 2;
            continue;
        }
        // Identifiers and case-insensitive reserved words
        if ch.is_ascii_alphabetic() || ch == b'_' {
            let start = i;
            i += 1;
            while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                i += 1;
            }
            let word = src[start..i].to_ascii_lowercase();
            let kind = match word.as_str() {
                "program" => TokKind::Program,
                "var" => TokKind::Var,
                "integer" => TokKind::Integer,
                "real" => TokKind::Real,
                "boolean" => TokKind::Boolean,
                "array" => TokKind::Array,
                "of" => TokKind::Of,
                "procedure" => TokKind::Procedure,
                "function" => TokKind::Function,
                "begin" => TokKind::Begin,
                "end" => TokKind::End,
                "if" => TokKind::If,
                "then" => TokKind::Then,
                "else" => TokKind::Else,
                "while" => TokKind::While,
                "do" => TokKind::Do,
                "div" => TokKind::Div,
                "mod" => TokKind::Mod,
                "and" => TokKind::And,
                "or" => TokKind::Or,
                "not" => TokKind::Not,
                "true" => TokKind::True,
                "false" => TokKind::False,
                "write" => TokKind::Write,
                _ => TokKind::Ident(word),
            };
            out.push(Token { kind, line });
            continue;
        }
        // Integer or real literal. Note: `1..5` must NOT consume the first `.` of `..`
        // as a decimal point! Look ahead to require a digit after `.` for reals.
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
                    .map_err(|_| format!("line {line}: invalid real literal"))?;
                out.push(Token {
                    kind: TokKind::RealLit(val),
                    line,
                });
            } else {
                let val: i64 = src[start..i]
                    .parse()
                    .map_err(|_| format!("line {line}: invalid integer literal"))?;
                out.push(Token {
                    kind: TokKind::IntLit(val),
                    line,
                });
            }
            continue;
        }
        // Two-character and single-character tokens
        let kind = match (ch, bytes.get(i + 1).copied()) {
            (b':', Some(b'=')) => {
                i += 2;
                TokKind::Assign
            }
            (b'.', Some(b'.')) => {
                i += 2;
                TokKind::Dots
            }
            (b'<', Some(b'=')) => {
                i += 2;
                TokKind::Le
            }
            (b'>', Some(b'=')) => {
                i += 2;
                TokKind::Ge
            }
            (b'<', Some(b'>')) => {
                i += 2;
                TokKind::Ne
            }
            (b':', _) => {
                i += 1;
                TokKind::Colon
            }
            (b'.', _) => {
                i += 1;
                TokKind::Dot
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
                TokKind::Eq
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
// 2. Types and Nested Symbol Table
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq)]
enum PasType {
    Int,
    Real,
    Bool,
    Void,
    /// Pascal arrays allow arbitrary lower bounds such as `array[1..5] of integer`.
    Array {
        low: i64,
        high: i64,
        elem: Box<PasType>,
    },
}

impl PasType {
    fn byte_width(&self) -> usize {
        match self {
            PasType::Int => 4,
            PasType::Real => 8,
            PasType::Bool => 1,
            PasType::Void => 0,
            PasType::Array { low, high, elem } => {
                let count = (high - low + 1).max(0) as usize;
                count * elem.byte_width()
            }
        }
    }
}

impl fmt::Display for PasType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PasType::Int => write!(f, "integer"),
            PasType::Real => write!(f, "real"),
            PasType::Bool => write!(f, "boolean"),
            PasType::Void => write!(f, "void"),
            PasType::Array { low, high, elem } => write!(f, "array[{low}..{high}] of {elem}"),
        }
    }
}

#[derive(Clone, Debug)]
enum SymbolKind {
    Var {
        slot_name: String,
        ty: PasType,
        offset: usize,
    },
    /// Inside a function body, assigning to the function's own name sets its return slot.
    Subprogram { params: Vec<PasType>, ret: PasType },
}

#[derive(Clone, Debug)]
struct Symbol {
    name: String,
    kind: SymbolKind,
}

#[derive(Clone, Debug)]
struct Scope {
    label: String,
    symbols: BTreeMap<String, Symbol>,
    next_offset: usize,
}

/// Lexical scope stack: outermost scope holds program globals and subprograms;
/// inner scope holds the active procedure/function's parameters, locals, and return slot.
struct SymTab {
    scopes: Vec<Scope>,
}

impl SymTab {
    fn new() -> Self {
        Self {
            scopes: vec![Scope {
                label: "global".into(),
                symbols: BTreeMap::new(),
                next_offset: 0,
            }],
        }
    }

    fn push_scope(&mut self, label: &str) {
        self.scopes.push(Scope {
            label: label.to_string(),
            symbols: BTreeMap::new(),
            next_offset: 0,
        });
    }

    fn pop_scope(&mut self) -> Scope {
        self.scopes.pop().expect("cannot pop global scope")
    }

    fn declare_var(&mut self, name: &str, ty: PasType, line: usize) -> Result<String, String> {
        let is_global = self.scopes.len() == 1;
        let top = self.scopes.last_mut().expect("missing scope");
        if top.symbols.contains_key(name) {
            return Err(format!(
                "line {line}: duplicate identifier '{name}' in scope '{}'",
                top.label
            ));
        }
        let slot_name = if is_global {
            name.to_string()
        } else {
            format!("{}_{}", top.label, name)
        };
        let offset = top.next_offset;
        top.next_offset += ty.byte_width();
        top.symbols.insert(
            name.to_string(),
            Symbol {
                name: name.to_string(),
                kind: SymbolKind::Var {
                    slot_name: slot_name.clone(),
                    ty,
                    offset,
                },
            },
        );
        Ok(slot_name)
    }

    fn declare_subprogram(
        &mut self,
        name: &str,
        params: Vec<PasType>,
        ret: PasType,
        line: usize,
    ) -> Result<String, String> {
        let entry_label = format!("fn_{name}");
        let global = &mut self.scopes[0];
        if global.symbols.contains_key(name) {
            return Err(format!("line {line}: duplicate subprogram '{name}'"));
        }
        global.symbols.insert(
            name.to_string(),
            Symbol {
                name: name.to_string(),
                kind: SymbolKind::Subprogram { params, ret },
            },
        );
        Ok(entry_label)
    }

    /// Searches from the innermost active scope outward (lexical shadowing).
    fn lookup(&self, name: &str) -> Option<&Symbol> {
        self.scopes
            .iter()
            .rev()
            .find_map(|scope| scope.symbols.get(name))
    }
}

// ---------------------------------------------------------------------------
// 3. Three-Address Code (TAC) Intermediate Representation
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq)]
enum Val {
    Int(i64),
    Real(f64),
    Bool(bool),
}

impl fmt::Display for Val {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Val::Int(v) => write!(f, "{v}"),
            Val::Real(v) => write!(f, "{v:.4}"),
            Val::Bool(v) => write!(f, "{v}"),
        }
    }
}

#[derive(Clone, Debug)]
enum Operand {
    Var(String),
    Const(Val),
}

impl fmt::Display for Operand {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Operand::Var(s) => write!(f, "{s}"),
            Operand::Const(v) => write!(f, "{v}"),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TacOp {
    Add,
    Sub,
    Mul,
    RDiv, // real `/`
    IDiv, // integer `div`
    Mod,
    And,
    Or,
    Lt,
    Le,
    Gt,
    Ge,
    Eq,
    Ne,
}

impl fmt::Display for TacOp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            TacOp::Add => "+",
            TacOp::Sub => "-",
            TacOp::Mul => "*",
            TacOp::RDiv => "/",
            TacOp::IDiv => "div",
            TacOp::Mod => "mod",
            TacOp::And => "and",
            TacOp::Or => "or",
            TacOp::Lt => "<",
            TacOp::Le => "<=",
            TacOp::Gt => ">",
            TacOp::Ge => ">=",
            TacOp::Eq => "=",
            TacOp::Ne => "<>",
        };
        write!(f, "{s}")
    }
}

#[derive(Clone, Debug)]
enum TacInst {
    Label(String),
    Assign {
        dst: String,
        src: Operand,
    },
    Bin {
        dst: String,
        op: TacOp,
        left: Operand,
        right: Operand,
    },
    Neg {
        dst: String,
        src: Operand,
    },
    Not {
        dst: String,
        src: Operand,
    },
    IntToReal {
        dst: String,
        src: Operand,
    },
    ArrayLoad {
        dst: String,
        base: String,
        index: Operand,
    },
    ArrayStore {
        base: String,
        index: Operand,
        src: Operand,
    },
    Param(Operand),
    Call {
        dst: Option<String>,
        callee: String,
        param_slots: Vec<String>,
    },
    Return(Option<Operand>),
    Write(Operand),
    Jump(String),
    JumpIfFalse {
        cond: Operand,
        target: String,
    },
    Halt,
}

impl fmt::Display for TacInst {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TacInst::Label(l) => write!(f, "{l}:"),
            TacInst::Assign { dst, src } => write!(f, "  {dst} := {src}"),
            TacInst::Bin {
                dst,
                op,
                left,
                right,
            } => write!(f, "  {dst} := {left} {op} {right}"),
            TacInst::Neg { dst, src } => write!(f, "  {dst} := -{src}"),
            TacInst::Not { dst, src } => write!(f, "  {dst} := not {src}"),
            TacInst::IntToReal { dst, src } => write!(f, "  {dst} := int_to_real({src})"),
            TacInst::ArrayLoad { dst, base, index } => write!(f, "  {dst} := {base}[{index}]"),
            TacInst::ArrayStore { base, index, src } => write!(f, "  {base}[{index}] := {src}"),
            TacInst::Param(op) => write!(f, "  param {op}"),
            TacInst::Call {
                dst: Some(d),
                callee,
                param_slots,
            } => write!(f, "  {d} := call {callee}({})", param_slots.len()),
            TacInst::Call {
                dst: None,
                callee,
                param_slots,
            } => write!(f, "  call {callee}({})", param_slots.len()),
            TacInst::Return(Some(v)) => write!(f, "  return {v}"),
            TacInst::Return(None) => write!(f, "  return"),
            TacInst::Write(v) => write!(f, "  write {v}"),
            TacInst::Jump(l) => write!(f, "  goto {l}"),
            TacInst::JumpIfFalse { cond, target } => write!(f, "  ifFalse {cond} goto {target}"),
            TacInst::Halt => write!(f, "  halt"),
        }
    }
}

// ---------------------------------------------------------------------------
// 4. Recursive-Descent Parser, Type Checker, and TAC Generator
// ---------------------------------------------------------------------------

struct TypedExpr {
    op: Operand,
    ty: PasType,
}

#[derive(Debug)]
struct CompiledUnit {
    program_name: String,
    global_offsets: Vec<(String, PasType, usize)>,
    code: Vec<TacInst>,
}

struct Parser {
    tokens: Vec<Token>,
    pos: usize,
    sym: SymTab,
    code: Vec<TacInst>,
    subprogram_params: BTreeMap<String, Vec<String>>,
    next_temp: usize,
    next_label: usize,
    active_func_ret_slot: Option<(String, String, PasType)>,
}

impl Parser {
    fn new(tokens: Vec<Token>) -> Self {
        Self {
            tokens,
            pos: 0,
            sym: SymTab::new(),
            code: Vec::new(),
            subprogram_params: BTreeMap::new(),
            next_temp: 0,
            next_label: 0,
            active_func_ret_slot: None,
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

    fn expect_ident(&mut self) -> Result<(String, usize), String> {
        let tok = self.advance();
        match tok.kind {
            TokKind::Ident(s) => Ok((s, tok.line)),
            other => Err(format!(
                "line {}: expected identifier, found {:?}",
                tok.line, other
            )),
        }
    }

    fn new_temp(&mut self) -> String {
        self.next_temp += 1;
        format!("t{}", self.next_temp)
    }

    fn new_label(&mut self, prefix: &str) -> String {
        self.next_label += 1;
        format!("L{}_{}", self.next_label, prefix)
    }

    /// Promotes an `integer` operand to `real` when paired with a `real` operand.
    fn widen_to_real(&mut self, expr: TypedExpr) -> TypedExpr {
        if expr.ty == PasType::Real {
            return expr;
        }
        let dst = self.new_temp();
        self.code.push(TacInst::IntToReal {
            dst: dst.clone(),
            src: expr.op,
        });
        TypedExpr {
            op: Operand::Var(dst),
            ty: PasType::Real,
        }
    }

    /// `program id ; var_decls subprogram_decls begin ... end .`
    fn parse_program(&mut self) -> Result<CompiledUnit, String> {
        self.expect(&TokKind::Program)?;
        let (program_name, _) = self.expect_ident()?;
        self.expect(&TokKind::Semi)?;

        self.parse_var_section()?;

        // Jump over subprogram bodies so execution starts at the main program block.
        let main_label = self.new_label("main");
        self.code.push(TacInst::Jump(main_label.clone()));

        while matches!(self.peek().kind, TokKind::Procedure | TokKind::Function) {
            self.parse_subprogram()?;
        }

        self.code.push(TacInst::Label(main_label));
        self.parse_compound_stmt()?;
        self.expect(&TokKind::Dot)?;
        self.expect(&TokKind::Eof)?;
        self.code.push(TacInst::Halt);

        let global_offsets = self.sym.scopes[0]
            .symbols
            .values()
            .filter_map(|s| match &s.kind {
                SymbolKind::Var { ty, offset, .. } => Some((s.name.clone(), ty.clone(), *offset)),
                SymbolKind::Subprogram { .. } => None,
            })
            .collect();

        Ok(CompiledUnit {
            program_name,
            global_offsets,
            code: self.code.clone(),
        })
    }

    fn parse_var_section(&mut self) -> Result<(), String> {
        if !self.eat(&TokKind::Var) {
            return Ok(());
        }
        while matches!(self.peek().kind, TokKind::Ident(_)) {
            let mut names = Vec::new();
            let (first, first_line) = self.expect_ident()?;
            names.push((first, first_line));
            while self.eat(&TokKind::Comma) {
                names.push(self.expect_ident()?);
            }
            self.expect(&TokKind::Colon)?;
            let ty = self.parse_type()?;
            self.expect(&TokKind::Semi)?;
            for (name, ln) in names {
                self.sym.declare_var(&name, ty.clone(), ln)?;
            }
        }
        Ok(())
    }

    fn parse_type(&mut self) -> Result<PasType, String> {
        let ln = self.line();
        match self.advance().kind {
            TokKind::Integer => Ok(PasType::Int),
            TokKind::Real => Ok(PasType::Real),
            TokKind::Boolean => Ok(PasType::Bool),
            TokKind::Array => {
                self.expect(&TokKind::LBracket)?;
                let low = self.parse_signed_int()?;
                self.expect(&TokKind::Dots)?;
                let high = self.parse_signed_int()?;
                if low > high {
                    return Err(format!(
                        "line {ln}: invalid array bounds [{low}..{high}] (low > high)"
                    ));
                }
                self.expect(&TokKind::RBracket)?;
                self.expect(&TokKind::Of)?;
                let elem = self.parse_type()?;
                Ok(PasType::Array {
                    low,
                    high,
                    elem: Box::new(elem),
                })
            }
            other => Err(format!("line {ln}: expected type name, found {other:?}")),
        }
    }

    fn parse_signed_int(&mut self) -> Result<i64, String> {
        let neg = self.eat(&TokKind::Minus);
        let ln = self.line();
        match self.advance().kind {
            TokKind::IntLit(v) => Ok(if neg { -v } else { v }),
            other => Err(format!(
                "line {ln}: expected integer bound, found {other:?}"
            )),
        }
    }

    fn parse_subprogram(&mut self) -> Result<(), String> {
        let is_func = match self.advance().kind {
            TokKind::Function => true,
            TokKind::Procedure => false,
            other => {
                return Err(format!(
                    "line {}: expected procedure or function, found {other:?}",
                    self.line()
                ))
            }
        };
        let (name, name_line) = self.expect_ident()?;
        let mut params: Vec<(String, PasType, usize)> = Vec::new();
        if self.eat(&TokKind::LParen) {
            if !matches!(self.peek().kind, TokKind::RParen) {
                loop {
                    let mut group = Vec::new();
                    group.push(self.expect_ident()?);
                    while self.eat(&TokKind::Comma) {
                        group.push(self.expect_ident()?);
                    }
                    self.expect(&TokKind::Colon)?;
                    let ty = self.parse_type()?;
                    for (pname, pline) in group {
                        params.push((pname, ty.clone(), pline));
                    }
                    if !self.eat(&TokKind::Semi) {
                        break;
                    }
                }
            }
            self.expect(&TokKind::RParen)?;
        }

        let ret_ty = if is_func {
            self.expect(&TokKind::Colon)?;
            let ty = self.parse_type()?;
            if matches!(ty, PasType::Array { .. }) {
                return Err(format!(
                    "line {}: functions cannot return arrays in this subset",
                    self.line()
                ));
            }
            ty
        } else {
            PasType::Void
        };
        self.expect(&TokKind::Semi)?;

        let param_types: Vec<PasType> = params.iter().map(|(_, t, _)| t.clone()).collect();
        let entry_label =
            self.sym
                .declare_subprogram(&name, param_types, ret_ty.clone(), name_line)?;

        self.sym.push_scope(&name);
        let mut param_slots = Vec::new();
        for (pname, pty, pline) in params {
            param_slots.push(self.sym.declare_var(&pname, pty, pline)?);
        }
        self.subprogram_params.insert(name.clone(), param_slots);

        self.active_func_ret_slot = if is_func {
            let ret_slot = format!("{name}__ret");
            Some((name.clone(), ret_slot, ret_ty))
        } else {
            None
        };

        self.parse_var_section()?;
        self.code.push(TacInst::Label(entry_label));
        self.parse_compound_stmt()?;
        self.expect(&TokKind::Semi)?;

        if let Some((_, ret_slot, _)) = self.active_func_ret_slot.take() {
            self.code
                .push(TacInst::Return(Some(Operand::Var(ret_slot))));
        } else {
            self.code.push(TacInst::Return(None));
        }
        self.sym.pop_scope();
        Ok(())
    }

    fn parse_compound_stmt(&mut self) -> Result<(), String> {
        self.expect(&TokKind::Begin)?;
        if !matches!(self.peek().kind, TokKind::End) {
            self.parse_stmt()?;
            while self.eat(&TokKind::Semi) {
                // Allow a trailing semicolon cleanly before `end`.
                if matches!(self.peek().kind, TokKind::End) {
                    break;
                }
                self.parse_stmt()?;
            }
        }
        self.expect(&TokKind::End)?;
        Ok(())
    }

    fn parse_stmt(&mut self) -> Result<(), String> {
        match self.peek().kind.clone() {
            TokKind::Begin => self.parse_compound_stmt(),
            TokKind::If => {
                self.advance();
                let cond_line = self.line();
                let cond = self.parse_expr()?;
                if cond.ty != PasType::Bool {
                    return Err(format!(
                        "line {cond_line}: if condition must be boolean, got {}",
                        cond.ty
                    ));
                }
                self.expect(&TokKind::Then)?;
                let l_else = self.new_label("else");
                let l_end = self.new_label("endif");
                self.code.push(TacInst::JumpIfFalse {
                    cond: cond.op,
                    target: l_else.clone(),
                });
                self.parse_stmt()?;
                if self.eat(&TokKind::Else) {
                    self.code.push(TacInst::Jump(l_end.clone()));
                    self.code.push(TacInst::Label(l_else));
                    self.parse_stmt()?;
                    self.code.push(TacInst::Label(l_end));
                } else {
                    self.code.push(TacInst::Label(l_else));
                }
                Ok(())
            }
            TokKind::While => {
                self.advance();
                let l_top = self.new_label("while");
                let l_exit = self.new_label("endwhile");
                self.code.push(TacInst::Label(l_top.clone()));
                let cond_line = self.line();
                let cond = self.parse_expr()?;
                if cond.ty != PasType::Bool {
                    return Err(format!(
                        "line {cond_line}: while condition must be boolean, got {}",
                        cond.ty
                    ));
                }
                self.expect(&TokKind::Do)?;
                self.code.push(TacInst::JumpIfFalse {
                    cond: cond.op,
                    target: l_exit.clone(),
                });
                self.parse_stmt()?;
                self.code.push(TacInst::Jump(l_top));
                self.code.push(TacInst::Label(l_exit));
                Ok(())
            }
            TokKind::Write => {
                self.advance();
                self.expect(&TokKind::LParen)?;
                loop {
                    let arg = self.parse_expr()?;
                    if matches!(arg.ty, PasType::Array { .. } | PasType::Void) {
                        return Err(format!(
                            "line {}: cannot write expression of type {}",
                            self.line(),
                            arg.ty
                        ));
                    }
                    self.code.push(TacInst::Write(arg.op));
                    if !self.eat(&TokKind::Comma) {
                        break;
                    }
                }
                self.expect(&TokKind::RParen)?;
                Ok(())
            }
            TokKind::Ident(_) => {
                let (name, ln) = self.expect_ident()?;
                // Distinguish function return assignment, procedure call, array write, and variable write.
                if let Some((fn_name, ret_slot, ret_ty)) = self.active_func_ret_slot.clone() {
                    if name == fn_name && matches!(self.peek().kind, TokKind::Assign) {
                        self.advance();
                        let rhs = self.parse_expr()?;
                        let assigned = self.check_assignable(&ret_ty, rhs, ln)?;
                        self.code.push(TacInst::Assign {
                            dst: ret_slot,
                            src: assigned.op,
                        });
                        return Ok(());
                    }
                }

                if matches!(self.peek().kind, TokKind::LParen)
                    || !matches!(self.peek().kind, TokKind::Assign | TokKind::LBracket)
                {
                    // Procedure call
                    let (_, ret) = self.parse_call(&name, ln, false)?;
                    if ret != PasType::Void {
                        return Err(format!(
                            "line {ln}: function '{name}' return value cannot be discarded as a statement"
                        ));
                    }
                    return Ok(());
                }

                let sym = self
                    .sym
                    .lookup(&name)
                    .cloned()
                    .ok_or_else(|| format!("line {ln}: undeclared identifier '{name}'"))?;
                let (slot_name, var_ty) = match sym.kind {
                    SymbolKind::Var { slot_name, ty, .. } => (slot_name, ty),
                    SymbolKind::Subprogram { .. } => {
                        return Err(format!(
                            "line {ln}: cannot assign directly to subprogram '{name}'"
                        ))
                    }
                };

                if self.eat(&TokKind::LBracket) {
                    let idx_expr = self.parse_expr()?;
                    if idx_expr.ty != PasType::Int {
                        return Err(format!(
                            "line {ln}: array index must be integer, got {}",
                            idx_expr.ty
                        ));
                    }
                    self.expect(&TokKind::RBracket)?;
                    self.expect(&TokKind::Assign)?;
                    let (low, elem_ty) = match var_ty {
                        PasType::Array { low, elem, .. } => (low, *elem),
                        other => {
                            return Err(format!("line {ln}: cannot index non-array type {other}"))
                        }
                    };
                    let rhs = self.parse_expr()?;
                    let assigned = self.check_assignable(&elem_ty, rhs, ln)?;
                    let zero_idx = self.emit_zero_based_index(idx_expr.op, low);
                    self.code.push(TacInst::ArrayStore {
                        base: slot_name,
                        index: zero_idx,
                        src: assigned.op,
                    });
                } else {
                    self.expect(&TokKind::Assign)?;
                    let rhs = self.parse_expr()?;
                    let assigned = self.check_assignable(&var_ty, rhs, ln)?;
                    self.code.push(TacInst::Assign {
                        dst: slot_name,
                        src: assigned.op,
                    });
                }
                Ok(())
            }
            other => Err(format!(
                "line {}: expected statement, found {other:?}",
                self.line()
            )),
        }
    }

    fn emit_zero_based_index(&mut self, index_op: Operand, low: i64) -> Operand {
        if low == 0 {
            return index_op;
        }
        let tmp = self.new_temp();
        self.code.push(TacInst::Bin {
            dst: tmp.clone(),
            op: TacOp::Sub,
            left: index_op,
            right: Operand::Const(Val::Int(low)),
        });
        Operand::Var(tmp)
    }

    fn check_assignable(
        &mut self,
        target_ty: &PasType,
        rhs: TypedExpr,
        line: usize,
    ) -> Result<TypedExpr, String> {
        if target_ty == &rhs.ty {
            Ok(rhs)
        } else if *target_ty == PasType::Real && rhs.ty == PasType::Int {
            Ok(self.widen_to_real(rhs))
        } else {
            Err(format!(
                "line {line}: type mismatch: cannot assign {} to {target_ty}",
                rhs.ty
            ))
        }
    }

    fn parse_call(
        &mut self,
        name: &str,
        line: usize,
        need_value: bool,
    ) -> Result<(Option<String>, PasType), String> {
        let sym = self
            .sym
            .lookup(name)
            .cloned()
            .ok_or_else(|| format!("line {line}: undeclared subprogram '{name}'"))?;
        let (param_tys, ret_ty) = match sym.kind {
            SymbolKind::Subprogram { params, ret, .. } => (params, ret),
            SymbolKind::Var { .. } => {
                return Err(format!(
                    "line {line}: '{name}' is a variable, not a subprogram"
                ))
            }
        };
        if need_value && ret_ty == PasType::Void {
            return Err(format!(
                "line {line}: procedure '{name}' does not return a value"
            ));
        }

        let mut args = Vec::new();
        if self.eat(&TokKind::LParen) {
            if !matches!(self.peek().kind, TokKind::RParen) {
                loop {
                    args.push(self.parse_expr()?);
                    if !self.eat(&TokKind::Comma) {
                        break;
                    }
                }
            }
            self.expect(&TokKind::RParen)?;
        }

        if args.len() != param_tys.len() {
            return Err(format!(
                "line {line}: '{name}' expects {} arguments, got {}",
                param_tys.len(),
                args.len()
            ));
        }
        let mut coerced_ops = Vec::new();
        for (arg, expected_ty) in args.into_iter().zip(param_tys.iter()) {
            coerced_ops.push(self.check_assignable(expected_ty, arg, line)?.op);
        }
        for op in coerced_ops {
            self.code.push(TacInst::Param(op));
        }
        let param_slots = self
            .subprogram_params
            .get(name)
            .cloned()
            .unwrap_or_default();
        let dst = if ret_ty != PasType::Void {
            Some(self.new_temp())
        } else {
            None
        };
        self.code.push(TacInst::Call {
            dst: dst.clone(),
            callee: name.to_string(),
            param_slots,
        });
        Ok((dst, ret_ty))
    }

    fn parse_expr(&mut self) -> Result<TypedExpr, String> {
        let left = self.parse_simple_expr()?;
        let op = match self.peek().kind {
            TokKind::Lt => Some(TacOp::Lt),
            TokKind::Le => Some(TacOp::Le),
            TokKind::Gt => Some(TacOp::Gt),
            TokKind::Ge => Some(TacOp::Ge),
            TokKind::Eq => Some(TacOp::Eq),
            TokKind::Ne => Some(TacOp::Ne),
            _ => None,
        };
        if let Some(op) = op {
            let ln = self.line();
            self.advance();
            let right = self.parse_simple_expr()?;
            let (l, r) = self.align_comparable(left, right, op, ln)?;
            let dst = self.new_temp();
            self.code.push(TacInst::Bin {
                dst: dst.clone(),
                op,
                left: l.op,
                right: r.op,
            });
            Ok(TypedExpr {
                op: Operand::Var(dst),
                ty: PasType::Bool,
            })
        } else {
            Ok(left)
        }
    }

    fn align_comparable(
        &mut self,
        left: TypedExpr,
        right: TypedExpr,
        op: TacOp,
        line: usize,
    ) -> Result<(TypedExpr, TypedExpr), String> {
        match (&left.ty, &right.ty) {
            (PasType::Int, PasType::Int) | (PasType::Real, PasType::Real) => Ok((left, right)),
            (PasType::Int, PasType::Real) | (PasType::Real, PasType::Int) => {
                Ok((self.widen_to_real(left), self.widen_to_real(right)))
            }
            (PasType::Bool, PasType::Bool) if matches!(op, TacOp::Eq | TacOp::Ne) => {
                Ok((left, right))
            }
            _ => Err(format!(
                "line {line}: incompatible operand types {} and {} for {op}",
                left.ty, right.ty
            )),
        }
    }

    fn parse_simple_expr(&mut self) -> Result<TypedExpr, String> {
        let unary_neg = if self.eat(&TokKind::Minus) {
            true
        } else {
            self.eat(&TokKind::Plus);
            false
        };
        let mut lhs = self.parse_term()?;
        if unary_neg {
            if !matches!(lhs.ty, PasType::Int | PasType::Real) {
                return Err(format!(
                    "line {}: unary '-' requires numeric operand",
                    self.line()
                ));
            }
            let dst = self.new_temp();
            self.code.push(TacInst::Neg {
                dst: dst.clone(),
                src: lhs.op,
            });
            lhs = TypedExpr {
                op: Operand::Var(dst),
                ty: lhs.ty,
            };
        }

        while let Some(op) = match self.peek().kind {
            TokKind::Plus => Some(TacOp::Add),
            TokKind::Minus => Some(TacOp::Sub),
            TokKind::Or => Some(TacOp::Or),
            _ => None,
        } {
            let ln = self.line();
            self.advance();
            let rhs = self.parse_term()?;
            lhs = self.emit_binary(lhs, rhs, op, ln)?;
        }
        Ok(lhs)
    }

    fn parse_term(&mut self) -> Result<TypedExpr, String> {
        let mut lhs = self.parse_factor()?;
        while let Some(op) = match self.peek().kind {
            TokKind::Star => Some(TacOp::Mul),
            TokKind::Slash => Some(TacOp::RDiv),
            TokKind::Div => Some(TacOp::IDiv),
            TokKind::Mod => Some(TacOp::Mod),
            TokKind::And => Some(TacOp::And),
            _ => None,
        } {
            let ln = self.line();
            self.advance();
            let rhs = self.parse_factor()?;
            lhs = self.emit_binary(lhs, rhs, op, ln)?;
        }
        Ok(lhs)
    }

    fn emit_binary(
        &mut self,
        left: TypedExpr,
        right: TypedExpr,
        op: TacOp,
        line: usize,
    ) -> Result<TypedExpr, String> {
        match op {
            TacOp::And | TacOp::Or => {
                if left.ty != PasType::Bool || right.ty != PasType::Bool {
                    return Err(format!("line {line}: '{op}' requires boolean operands"));
                }
                let dst = self.new_temp();
                self.code.push(TacInst::Bin {
                    dst: dst.clone(),
                    op,
                    left: left.op,
                    right: right.op,
                });
                Ok(TypedExpr {
                    op: Operand::Var(dst),
                    ty: PasType::Bool,
                })
            }
            TacOp::IDiv | TacOp::Mod => {
                if left.ty != PasType::Int || right.ty != PasType::Int {
                    return Err(format!("line {line}: '{op}' requires integer operands"));
                }
                let dst = self.new_temp();
                self.code.push(TacInst::Bin {
                    dst: dst.clone(),
                    op,
                    left: left.op,
                    right: right.op,
                });
                Ok(TypedExpr {
                    op: Operand::Var(dst),
                    ty: PasType::Int,
                })
            }
            TacOp::RDiv => {
                // Pascal `/` always produces a `real` result.
                if !matches!(left.ty, PasType::Int | PasType::Real)
                    || !matches!(right.ty, PasType::Int | PasType::Real)
                {
                    return Err(format!("line {line}: '/' requires numeric operands"));
                }
                let l = self.widen_to_real(left);
                let r = self.widen_to_real(right);
                let dst = self.new_temp();
                self.code.push(TacInst::Bin {
                    dst: dst.clone(),
                    op,
                    left: l.op,
                    right: r.op,
                });
                Ok(TypedExpr {
                    op: Operand::Var(dst),
                    ty: PasType::Real,
                })
            }
            TacOp::Add | TacOp::Sub | TacOp::Mul => {
                let (l, r, res_ty) = match (&left.ty, &right.ty) {
                    (PasType::Int, PasType::Int) => (left, right, PasType::Int),
                    (PasType::Int | PasType::Real, PasType::Int | PasType::Real) => (
                        self.widen_to_real(left),
                        self.widen_to_real(right),
                        PasType::Real,
                    ),
                    _ => {
                        return Err(format!(
                            "line {line}: '{op}' requires numeric operands, got {} and {}",
                            left.ty, right.ty
                        ))
                    }
                };
                let dst = self.new_temp();
                self.code.push(TacInst::Bin {
                    dst: dst.clone(),
                    op,
                    left: l.op,
                    right: r.op,
                });
                Ok(TypedExpr {
                    op: Operand::Var(dst),
                    ty: res_ty,
                })
            }
            _ => unreachable!(),
        }
    }

    fn parse_factor(&mut self) -> Result<TypedExpr, String> {
        let ln = self.line();
        match self.advance().kind {
            TokKind::IntLit(v) => Ok(TypedExpr {
                op: Operand::Const(Val::Int(v)),
                ty: PasType::Int,
            }),
            TokKind::RealLit(v) => Ok(TypedExpr {
                op: Operand::Const(Val::Real(v)),
                ty: PasType::Real,
            }),
            TokKind::True => Ok(TypedExpr {
                op: Operand::Const(Val::Bool(true)),
                ty: PasType::Bool,
            }),
            TokKind::False => Ok(TypedExpr {
                op: Operand::Const(Val::Bool(false)),
                ty: PasType::Bool,
            }),
            TokKind::LParen => {
                let inner = self.parse_expr()?;
                self.expect(&TokKind::RParen)?;
                Ok(inner)
            }
            TokKind::Not => {
                let inner = self.parse_factor()?;
                if inner.ty != PasType::Bool {
                    return Err(format!("line {ln}: 'not' requires boolean operand"));
                }
                let dst = self.new_temp();
                self.code.push(TacInst::Not {
                    dst: dst.clone(),
                    src: inner.op,
                });
                Ok(TypedExpr {
                    op: Operand::Var(dst),
                    ty: PasType::Bool,
                })
            }
            TokKind::Ident(name) => {
                let sym = self
                    .sym
                    .lookup(&name)
                    .cloned()
                    .ok_or_else(|| format!("line {ln}: undeclared identifier '{name}'"))?;
                match sym.kind {
                    SymbolKind::Subprogram { .. } => {
                        let (dst, ret_ty) = self.parse_call(&name, ln, true)?;
                        Ok(TypedExpr {
                            op: Operand::Var(dst.expect("function returns a value")),
                            ty: ret_ty,
                        })
                    }
                    SymbolKind::Var { slot_name, ty, .. } => {
                        if self.eat(&TokKind::LBracket) {
                            let idx = self.parse_expr()?;
                            if idx.ty != PasType::Int {
                                return Err(format!(
                                    "line {ln}: array index must be integer, got {}",
                                    idx.ty
                                ));
                            }
                            self.expect(&TokKind::RBracket)?;
                            let (low, elem_ty) = match ty {
                                PasType::Array { low, elem, .. } => (low, *elem),
                                other => {
                                    return Err(format!(
                                        "line {ln}: cannot index non-array type {other}"
                                    ))
                                }
                            };
                            let zero_idx = self.emit_zero_based_index(idx.op, low);
                            let dst = self.new_temp();
                            self.code.push(TacInst::ArrayLoad {
                                dst: dst.clone(),
                                base: slot_name,
                                index: zero_idx,
                            });
                            Ok(TypedExpr {
                                op: Operand::Var(dst),
                                ty: elem_ty,
                            })
                        } else {
                            Ok(TypedExpr {
                                op: Operand::Var(slot_name),
                                ty,
                            })
                        }
                    }
                }
            }
            other => Err(format!("line {ln}: unexpected factor {other:?}")),
        }
    }
}

fn compile_pascal(src: &str) -> Result<CompiledUnit, String> {
    let tokens = lex(src)?;
    Parser::new(tokens).parse_program()
}

// ---------------------------------------------------------------------------
// 5. Minimal TAC Interpreter (Verifies Lowered Semantics)
// ---------------------------------------------------------------------------

fn exec_tac(unit: &CompiledUnit) -> Result<Vec<Val>, String> {
    let mut labels = BTreeMap::new();
    for (idx, inst) in unit.code.iter().enumerate() {
        if let TacInst::Label(name) = inst {
            labels.insert(name.clone(), idx);
        }
    }

    let mut pc = 0usize;
    let mut globals: BTreeMap<String, Val> = BTreeMap::new();
    let mut arrays: BTreeMap<String, BTreeMap<i64, Val>> = BTreeMap::new();
    let mut call_frames: Vec<(usize, Option<String>, BTreeMap<String, Val>)> = Vec::new();
    let mut param_stack: Vec<Val> = Vec::new();
    let mut writes = Vec::new();

    let read_var = |name: &str,
                    frames: &[(usize, Option<String>, BTreeMap<String, Val>)],
                    globals: &BTreeMap<String, Val>|
     -> Val {
        if let Some((_, _, locals)) = frames.last() {
            if let Some(v) = locals.get(name) {
                return v.clone();
            }
        }
        globals.get(name).cloned().unwrap_or(Val::Int(0))
    };

    let global_names: std::collections::BTreeSet<String> = unit
        .global_offsets
        .iter()
        .map(|(n, _, _)| n.clone())
        .collect();

    let write_var = |name: String,
                     val: Val,
                     frames: &mut [(usize, Option<String>, BTreeMap<String, Val>)],
                     globals: &mut BTreeMap<String, Val>| {
        if !global_names.contains(&name) {
            if let Some((_, _, locals)) = frames.last_mut() {
                locals.insert(name, val);
                return;
            }
        }
        globals.insert(name, val);
    };

    let eval_op = |op: &Operand,
                   frames: &[(usize, Option<String>, BTreeMap<String, Val>)],
                   globals: &BTreeMap<String, Val>|
     -> Val {
        match op {
            Operand::Const(v) => v.clone(),
            Operand::Var(s) => read_var(s, frames, globals),
        }
    };

    let as_f64 = |v: &Val| -> f64 {
        match v {
            Val::Int(i) => *i as f64,
            Val::Real(r) => *r,
            Val::Bool(_) => 0.0,
        }
    };

    while pc < unit.code.len() {
        match &unit.code[pc] {
            TacInst::Label(_) => pc += 1,
            TacInst::Assign { dst, src } => {
                let v = eval_op(src, &call_frames, &globals);
                write_var(dst.clone(), v, &mut call_frames, &mut globals);
                pc += 1;
            }
            TacInst::Bin {
                dst,
                op,
                left,
                right,
            } => {
                let l = eval_op(left, &call_frames, &globals);
                let r = eval_op(right, &call_frames, &globals);
                let out = match op {
                    TacOp::Add => match (&l, &r) {
                        (Val::Int(a), Val::Int(b)) => Val::Int(a + b),
                        _ => Val::Real(as_f64(&l) + as_f64(&r)),
                    },
                    TacOp::Sub => match (&l, &r) {
                        (Val::Int(a), Val::Int(b)) => Val::Int(a - b),
                        _ => Val::Real(as_f64(&l) - as_f64(&r)),
                    },
                    TacOp::Mul => match (&l, &r) {
                        (Val::Int(a), Val::Int(b)) => Val::Int(a * b),
                        _ => Val::Real(as_f64(&l) * as_f64(&r)),
                    },
                    TacOp::RDiv => Val::Real(as_f64(&l) / as_f64(&r)),
                    TacOp::IDiv => match (&l, &r) {
                        (Val::Int(a), Val::Int(b)) => Val::Int(a / b),
                        _ => return Err("div expected integers".into()),
                    },
                    TacOp::Mod => match (&l, &r) {
                        (Val::Int(a), Val::Int(b)) => Val::Int(a % b),
                        _ => return Err("mod expected integers".into()),
                    },
                    TacOp::And => match (&l, &r) {
                        (Val::Bool(a), Val::Bool(b)) => Val::Bool(*a && *b),
                        _ => return Err("and expected booleans".into()),
                    },
                    TacOp::Or => match (&l, &r) {
                        (Val::Bool(a), Val::Bool(b)) => Val::Bool(*a || *b),
                        _ => return Err("or expected booleans".into()),
                    },
                    TacOp::Lt => match (&l, &r) {
                        (Val::Int(a), Val::Int(b)) => Val::Bool(a < b),
                        _ => Val::Bool(as_f64(&l) < as_f64(&r)),
                    },
                    TacOp::Le => match (&l, &r) {
                        (Val::Int(a), Val::Int(b)) => Val::Bool(a <= b),
                        _ => Val::Bool(as_f64(&l) <= as_f64(&r)),
                    },
                    TacOp::Gt => match (&l, &r) {
                        (Val::Int(a), Val::Int(b)) => Val::Bool(a > b),
                        _ => Val::Bool(as_f64(&l) > as_f64(&r)),
                    },
                    TacOp::Ge => match (&l, &r) {
                        (Val::Int(a), Val::Int(b)) => Val::Bool(a >= b),
                        _ => Val::Bool(as_f64(&l) >= as_f64(&r)),
                    },
                    TacOp::Eq => Val::Bool(l == r),
                    TacOp::Ne => Val::Bool(l != r),
                };
                write_var(dst.clone(), out, &mut call_frames, &mut globals);
                pc += 1;
            }
            TacInst::Neg { dst, src } => {
                let v = match eval_op(src, &call_frames, &globals) {
                    Val::Int(i) => Val::Int(-i),
                    Val::Real(r) => Val::Real(-r),
                    _ => return Err("neg expected number".into()),
                };
                write_var(dst.clone(), v, &mut call_frames, &mut globals);
                pc += 1;
            }
            TacInst::Not { dst, src } => {
                let v = match eval_op(src, &call_frames, &globals) {
                    Val::Bool(b) => Val::Bool(!b),
                    _ => return Err("not expected boolean".into()),
                };
                write_var(dst.clone(), v, &mut call_frames, &mut globals);
                pc += 1;
            }
            TacInst::IntToReal { dst, src } => {
                let v = Val::Real(as_f64(&eval_op(src, &call_frames, &globals)));
                write_var(dst.clone(), v, &mut call_frames, &mut globals);
                pc += 1;
            }
            TacInst::ArrayLoad { dst, base, index } => {
                let idx = match eval_op(index, &call_frames, &globals) {
                    Val::Int(i) => i,
                    _ => return Err("array index must be integer".into()),
                };
                let val = arrays
                    .get(base)
                    .and_then(|m| m.get(&idx))
                    .cloned()
                    .unwrap_or(Val::Int(0));
                write_var(dst.clone(), val, &mut call_frames, &mut globals);
                pc += 1;
            }
            TacInst::ArrayStore { base, index, src } => {
                let idx = match eval_op(index, &call_frames, &globals) {
                    Val::Int(i) => i,
                    _ => return Err("array index must be integer".into()),
                };
                let val = eval_op(src, &call_frames, &globals);
                arrays.entry(base.clone()).or_default().insert(idx, val);
                pc += 1;
            }
            TacInst::Param(op) => {
                param_stack.push(eval_op(op, &call_frames, &globals));
                pc += 1;
            }
            TacInst::Call {
                dst,
                callee,
                param_slots,
            } => {
                let mut locals = BTreeMap::new();
                let start = param_stack.len().saturating_sub(param_slots.len());
                let passed: Vec<Val> = param_stack.drain(start..).collect();
                for (slot, val) in param_slots.iter().zip(passed) {
                    locals.insert(slot.clone(), val);
                }
                let target_pc = *labels
                    .get(&format!("fn_{callee}"))
                    .ok_or_else(|| format!("missing entry for {callee}"))?;
                call_frames.push((pc + 1, dst.clone(), locals));
                pc = target_pc;
            }
            TacInst::Return(opt_op) => {
                let ret_val = opt_op
                    .as_ref()
                    .map(|op| eval_op(op, &call_frames, &globals));
                let (ret_pc, dst_slot, _) = call_frames.pop().ok_or("return outside call")?;
                if let (Some(dst), Some(val)) = (dst_slot, ret_val) {
                    write_var(dst, val, &mut call_frames, &mut globals);
                }
                pc = ret_pc;
            }
            TacInst::Write(op) => {
                writes.push(eval_op(op, &call_frames, &globals));
                pc += 1;
            }
            TacInst::Jump(target) => {
                pc = *labels.get(target).ok_or("missing label")?;
            }
            TacInst::JumpIfFalse { cond, target } => {
                let b = match eval_op(cond, &call_frames, &globals) {
                    Val::Bool(b) => b,
                    _ => return Err("branch expected boolean".into()),
                };
                if !b {
                    pc = *labels.get(target).ok_or("missing label")?;
                } else {
                    pc += 1;
                }
            }
            TacInst::Halt => break,
        }
    }
    Ok(writes)
}

const SAMPLE_PASCAL: &str = "
program WeightedScores;
var
    scores: array[1..4] of integer;
    i: integer;
    avg: real;

function SumWeighted(limit: integer; bonus: real): real;
var
    k, total: integer;
begin
    total := 0;
    k := 1;
    while k <= limit do
    begin
        if (k mod 2) = 0 then
            total := total + scores[k] * 2
        else
            total := total + scores[k];
        k := k + 1
    end;
    SumWeighted := total + bonus
end;

begin
    i := 1;
    while i <= 4 do
    begin
        scores[i] := i * 10;
        i := i + 1
    end;
    avg := SumWeighted(4, 2.5) / 2;
    write(avg)
end.
";

fn main() {
    let unit = compile_pascal(SAMPLE_PASCAL).expect("sample Pascal compiles");
    println!("Program: {}", unit.program_name);
    println!("Global symbol layout:");
    for (name, ty, offset) in &unit.global_offsets {
        println!("  {name:<8} : {ty:<24} offset={offset}");
    }
    println!("\nEmitted TAC ({} instructions):", unit.code.len());
    for inst in &unit.code {
        println!("{inst}");
    }
    let out = exec_tac(&unit).expect("TAC executes cleanly");
    println!("\nExecution write output: {out:?}");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compiles_nested_scopes_array_bounds_and_real_coercion() {
        let unit = compile_pascal(SAMPLE_PASCAL).expect("sample program should compile");
        assert!(unit
            .code
            .iter()
            .any(|inst| matches!(inst, TacInst::IntToReal { .. })));
        let out = exec_tac(&unit).expect("TAC should execute");
        // scores = [10, 20, 30, 40]; weighted = 10 + 40 + 30 + 80 = 160; + 2.5 = 162.5; / 2 = 81.25
        assert_eq!(out, vec![Val::Real(81.25)]);
    }

    #[test]
    fn rejects_semantic_and_syntax_errors_cleanly() {
        let bad_cond = "program P; var x: integer; begin x := 1; if x then write(x) end.";
        assert!(compile_pascal(bad_cond).unwrap_err().contains("boolean"));

        let bad_bounds = "program P; var a: array[5..2] of integer; begin end.";
        assert!(compile_pascal(bad_bounds)
            .unwrap_err()
            .contains("low > high"));

        let dup_decl = "program P; var a, a: integer; begin end.";
        assert!(compile_pascal(dup_decl).unwrap_err().contains("duplicate"));

        let bad_assign = "program P; var n: integer; begin n := 3.5 end.";
        assert!(compile_pascal(bad_assign)
            .unwrap_err()
            .contains("type mismatch"));
    }
}
