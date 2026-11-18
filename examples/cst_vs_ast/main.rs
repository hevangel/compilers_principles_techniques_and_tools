//! Dragon Book Classical CST (Parse Tree) vs. AST & Syntax-Tree Variants
//! (§2.2.3, §2.5.1, §2.6.1, §2.8.2, §5.3.1, §6.1).
//!
//! Demonstrates:
//! 1. Classical lexical analysis (§2.6.1) stripping whitespace and comments before parsing,
//!    explaining why even a classical CST/parse tree is NOT byte-for-byte lossless,
//! 2. Grammar derivations (`=>_lm`) and concrete parse trees (§2.2.3) preserving every
//!    intermediate chain (`Expr -> Term -> Factor`) and punctuation token (`let`, `=`, `(`, `)`, `;`),
//! 3. Transforming CST -> AST (§2.5.1, §2.8.2, §5.3.1) and AST -> canonical CST + source,
//! 4. Purple §6.1 syntax-tree variants: folding a tree AST into a value-numbered DAG.

use std::collections::HashMap;
use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TokenKind {
    KwLet,
    Ident,
    Int,
    Eq,
    Plus,
    Minus,
    Star,
    Slash,
    LParen,
    RParen,
    Semi,
    Eof,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Token {
    kind: TokenKind,
    lexeme: String,
    start: usize,
    end: usize,
}

#[derive(Debug, PartialEq, Eq)]
struct LexResult {
    tokens: Vec<Token>,
    stripped_whitespace_bytes: usize,
    stripped_comments: usize,
}

/// Classical §2.6.1 lexical analyzer: removes whitespace and `// ...` comments
/// before returning the token stream to the parser.
fn lex_strip_trivia(src: &str) -> Result<LexResult, String> {
    let bytes = src.as_bytes();
    let mut i = 0;
    let mut tokens = Vec::new();
    let mut stripped_whitespace_bytes = 0;
    let mut stripped_comments = 0;

    while i < bytes.len() {
        let b = bytes[i];
        if b.is_ascii_whitespace() {
            let start = i;
            while i < bytes.len() && bytes[i].is_ascii_whitespace() {
                i += 1;
            }
            stripped_whitespace_bytes += i - start;
            continue;
        }
        if i + 1 < bytes.len() && b == b'/' && bytes[i + 1] == b'/' {
            while i < bytes.len() && bytes[i] != b'\n' {
                i += 1;
            }
            stripped_comments += 1;
            continue;
        }
        if b.is_ascii_digit() {
            let start = i;
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                i += 1;
            }
            tokens.push(Token {
                kind: TokenKind::Int,
                lexeme: src[start..i].to_string(),
                start,
                end: i,
            });
            continue;
        }
        if b.is_ascii_alphabetic() || b == b'_' {
            let start = i;
            while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                i += 1;
            }
            let lex = &src[start..i];
            let kind = if lex == "let" {
                TokenKind::KwLet
            } else {
                TokenKind::Ident
            };
            tokens.push(Token {
                kind,
                lexeme: lex.to_string(),
                start,
                end: i,
            });
            continue;
        }
        let kind = match b {
            b'=' => TokenKind::Eq,
            b'+' => TokenKind::Plus,
            b'-' => TokenKind::Minus,
            b'*' => TokenKind::Star,
            b'/' => TokenKind::Slash,
            b'(' => TokenKind::LParen,
            b')' => TokenKind::RParen,
            b';' => TokenKind::Semi,
            _ => return Err(format!("unexpected byte '{}' at {}", b as char, i)),
        };
        tokens.push(Token {
            kind,
            lexeme: src[i..i + 1].to_string(),
            start: i,
            end: i + 1,
        });
        i += 1;
    }

    tokens.push(Token {
        kind: TokenKind::Eof,
        lexeme: String::new(),
        start: src.len(),
        end: src.len(),
    });

    Ok(LexResult {
        tokens,
        stripped_whitespace_bytes,
        stripped_comments,
    })
}

/// Classical Concrete Syntax Tree (Parse Tree, Purple §2.2.3).
/// Notice: NO Whitespace or Comment children exist because §2.6.1 discarded them.
#[derive(Clone, Debug, PartialEq, Eq)]
enum CstNode {
    Rule {
        name: &'static str,
        children: Vec<CstNode>,
    },
    Terminal(Token),
}

impl CstNode {
    fn rule(name: &'static str, children: Vec<CstNode>) -> Self {
        Self::Rule { name, children }
    }

    fn node_count(&self) -> usize {
        match self {
            CstNode::Terminal(_) => 1,
            CstNode::Rule { children, .. } => {
                1 + children.iter().map(CstNode::node_count).sum::<usize>()
            }
        }
    }

    fn terminal_yield(&self) -> Vec<String> {
        let mut out = Vec::new();
        self.collect_terminals(&mut out);
        out
    }

    fn collect_terminals(&self, out: &mut Vec<String>) {
        match self {
            CstNode::Terminal(t) => out.push(t.lexeme.clone()),
            CstNode::Rule { children, .. } => {
                for c in children {
                    c.collect_terminals(out);
                }
            }
        }
    }

    fn render_tree(&self) -> String {
        let mut lines = Vec::new();
        self.fmt_tree("", true, true, &mut lines);
        lines.join("\n")
    }

    fn fmt_tree(&self, prefix: &str, is_last: bool, is_root: bool, out: &mut Vec<String>) {
        let label = match self {
            CstNode::Rule { name, .. } => (*name).to_string(),
            CstNode::Terminal(tok) => format!("'{:?}'({:?})", tok.kind, tok.lexeme),
        };
        if is_root {
            out.push(label);
        } else {
            let branch = if is_last { "└── " } else { "├── " };
            out.push(format!("{prefix}{branch}{label}"));
        }
        if let CstNode::Rule { children, .. } = self {
            let child_prefix = if is_root {
                String::new()
            } else if is_last {
                format!("{prefix}    ")
            } else {
                format!("{prefix}│   ")
            };
            for (idx, child) in children.iter().enumerate() {
                child.fmt_tree(&child_prefix, idx + 1 == children.len(), false, out);
            }
        }
    }
}

/// Generates the leftmost derivation (`=>_lm`) steps corresponding to a CST (§2.2.3).
fn leftmost_derivation(cst: &CstNode) -> Vec<String> {
    #[derive(Clone)]
    enum Sym<'a> {
        Node(&'a CstNode),
        Done(String),
    }

    fn fmt_form(syms: &[Sym<'_>]) -> String {
        syms.iter()
            .map(|s| match s {
                Sym::Node(CstNode::Rule { name, .. }) => (*name).to_string(),
                Sym::Node(CstNode::Terminal(t)) => t.lexeme.clone(),
                Sym::Done(text) => text.clone(),
            })
            .collect::<Vec<_>>()
            .join(" ")
    }

    let mut form = vec![Sym::Node(cst)];
    let mut steps = vec![fmt_form(&form)];

    loop {
        let next_idx = form
            .iter()
            .position(|s| matches!(s, Sym::Node(CstNode::Rule { .. })));
        let Some(idx) = next_idx else {
            break;
        };
        let Sym::Node(CstNode::Rule { children, .. }) = form[idx] else {
            unreachable!();
        };
        let replacement: Vec<Sym<'_>> = children
            .iter()
            .map(|c| match c {
                CstNode::Terminal(t) => Sym::Done(t.lexeme.clone()),
                rule @ CstNode::Rule { .. } => Sym::Node(rule),
            })
            .collect();
        form.splice(idx..=idx, replacement);
        steps.push(fmt_form(&form));
    }

    steps
}

struct CstParser {
    tokens: Vec<Token>,
    pos: usize,
}

impl CstParser {
    fn new(tokens: Vec<Token>) -> Self {
        Self { tokens, pos: 0 }
    }

    fn peek(&self) -> &Token {
        &self.tokens[self.pos]
    }

    fn bump(&mut self) -> Token {
        let tok = self.tokens[self.pos].clone();
        if tok.kind != TokenKind::Eof {
            self.pos += 1;
        }
        tok
    }

    fn expect(&mut self, kind: TokenKind) -> Result<CstNode, String> {
        if self.peek().kind == kind {
            Ok(CstNode::Terminal(self.bump()))
        } else {
            Err(format!(
                "expected {:?}, found {:?} ({:?})",
                kind,
                self.peek().kind,
                self.peek().lexeme
            ))
        }
    }

    /// Program -> StmtList
    fn parse_program(&mut self) -> Result<CstNode, String> {
        let mut stmts = Vec::new();
        while self.peek().kind != TokenKind::Eof {
            stmts.push(self.parse_stmt()?);
        }
        let prog = CstNode::rule("Program", vec![CstNode::rule("StmtList", stmts)]);
        Ok(prog)
    }

    /// Stmt -> 'let' Ident '=' Expr ';' | Expr ';'
    fn parse_stmt(&mut self) -> Result<CstNode, String> {
        if self.peek().kind == TokenKind::KwLet {
            let kw = self.expect(TokenKind::KwLet)?;
            let id = self.expect(TokenKind::Ident)?;
            let eq = self.expect(TokenKind::Eq)?;
            let expr = self.parse_expr()?;
            let semi = self.expect(TokenKind::Semi)?;
            Ok(CstNode::rule("Stmt", vec![kw, id, eq, expr, semi]))
        } else {
            let expr = self.parse_expr()?;
            let semi = self.expect(TokenKind::Semi)?;
            Ok(CstNode::rule("Stmt", vec![expr, semi]))
        }
    }

    /// Expr -> Expr ('+' | '-') Term | Term   (left-associative concrete tree)
    fn parse_expr(&mut self) -> Result<CstNode, String> {
        let mut node = CstNode::rule("Expr", vec![self.parse_term()?]);
        while matches!(self.peek().kind, TokenKind::Plus | TokenKind::Minus) {
            let op = CstNode::Terminal(self.bump());
            let rhs = self.parse_term()?;
            node = CstNode::rule("Expr", vec![node, op, rhs]);
        }
        Ok(node)
    }

    /// Term -> Term ('*' | '/') Factor | Factor
    fn parse_term(&mut self) -> Result<CstNode, String> {
        let mut node = CstNode::rule("Term", vec![self.parse_factor()?]);
        while matches!(self.peek().kind, TokenKind::Star | TokenKind::Slash) {
            let op = CstNode::Terminal(self.bump());
            let rhs = self.parse_factor()?;
            node = CstNode::rule("Term", vec![node, op, rhs]);
        }
        Ok(node)
    }

    /// Factor -> '(' Expr ')' | Ident | Int
    fn parse_factor(&mut self) -> Result<CstNode, String> {
        match self.peek().kind {
            TokenKind::LParen => {
                let lp = self.expect(TokenKind::LParen)?;
                let inner = self.parse_expr()?;
                let rp = self.expect(TokenKind::RParen)?;
                Ok(CstNode::rule("Factor", vec![lp, inner, rp]))
            }
            TokenKind::Ident => Ok(CstNode::rule(
                "Factor",
                vec![self.expect(TokenKind::Ident)?],
            )),
            TokenKind::Int => Ok(CstNode::rule("Factor", vec![self.expect(TokenKind::Int)?])),
            other => Err(format!("unexpected token {:?} in Factor", other)),
        }
    }
}

// ---------------------------------------------------------------------------
// Abstract Syntax Tree (§2.5.1, §2.8.2, §5.3.1)
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
}

impl BinOp {
    fn sym(self) -> &'static str {
        match self {
            BinOp::Add => "+",
            BinOp::Sub => "-",
            BinOp::Mul => "*",
            BinOp::Div => "/",
        }
    }

    fn prec(self) -> u8 {
        match self {
            BinOp::Add | BinOp::Sub => 1,
            BinOp::Mul | BinOp::Div => 2,
        }
    }

    fn from_kind(kind: TokenKind) -> Option<Self> {
        match kind {
            TokenKind::Plus => Some(BinOp::Add),
            TokenKind::Minus => Some(BinOp::Sub),
            TokenKind::Star => Some(BinOp::Mul),
            TokenKind::Slash => Some(BinOp::Div),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum AstExpr {
    Ident(String),
    Int(i64),
    Bin {
        op: BinOp,
        left: Box<AstExpr>,
        right: Box<AstExpr>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum AstStmt {
    Let { name: String, value: AstExpr },
    Expr(AstExpr),
}

impl AstExpr {
    fn node_count(&self) -> usize {
        match self {
            AstExpr::Ident(_) | AstExpr::Int(_) => 1,
            AstExpr::Bin { left, right, .. } => 1 + left.node_count() + right.node_count(),
        }
    }

    fn fmt_tree(&self, prefix: &str, is_last: bool, out: &mut Vec<String>) {
        let branch = if is_last { "└── " } else { "├── " };
        let next = if is_last {
            format!("{prefix}    ")
        } else {
            format!("{prefix}│   ")
        };
        match self {
            AstExpr::Ident(name) => out.push(format!("{prefix}{branch}Name({name:?})")),
            AstExpr::Int(v) => out.push(format!("{prefix}{branch}Number({v})")),
            AstExpr::Bin { op, left, right } => {
                out.push(format!("{prefix}{branch}BinOp({:?})", op.sym()));
                left.fmt_tree(&next, false, out);
                right.fmt_tree(&next, true, out);
            }
        }
    }
}

impl AstStmt {
    fn node_count(&self) -> usize {
        match self {
            AstStmt::Let { value, .. } => 2 + value.node_count(), // Let + Name + value
            AstStmt::Expr(e) => 1 + e.node_count(),
        }
    }

    fn render_tree(&self) -> String {
        let mut lines = Vec::new();
        match self {
            AstStmt::Let { name, value } => {
                lines.push("Let".to_string());
                lines.push(format!("├── Name({name:?})"));
                value.fmt_tree("", true, &mut lines);
            }
            AstStmt::Expr(e) => {
                lines.push("ExprStmt".to_string());
                e.fmt_tree("", true, &mut lines);
            }
        }
        lines.join("\n")
    }
}

fn program_ast_node_count(stmts: &[AstStmt]) -> usize {
    1 + stmts.iter().map(AstStmt::node_count).sum::<usize>()
}

/// Transforms a Dragon Book concrete parse tree (CST) into a compact AST.
fn cst_to_ast(cst: &CstNode) -> Result<Vec<AstStmt>, String> {
    let CstNode::Rule {
        name: "Program",
        children,
    } = cst
    else {
        return Err("expected Program root".to_string());
    };
    let Some(CstNode::Rule {
        name: "StmtList",
        children: stmt_nodes,
    }) = children.first()
    else {
        return Err("expected StmtList".to_string());
    };
    stmt_nodes.iter().map(lower_stmt).collect()
}

fn lower_stmt(node: &CstNode) -> Result<AstStmt, String> {
    let CstNode::Rule {
        name: "Stmt",
        children,
    } = node
    else {
        return Err("expected Stmt rule".to_string());
    };
    match children.as_slice() {
        [CstNode::Terminal(kw), CstNode::Terminal(id), CstNode::Terminal(_eq), expr, CstNode::Terminal(_semi)]
            if kw.kind == TokenKind::KwLet && id.kind == TokenKind::Ident =>
        {
            Ok(AstStmt::Let {
                name: id.lexeme.clone(),
                value: lower_expr(expr)?,
            })
        }
        [expr, CstNode::Terminal(_semi)] => Ok(AstStmt::Expr(lower_expr(expr)?)),
        _ => Err("malformed Stmt CST node".to_string()),
    }
}

fn lower_expr(node: &CstNode) -> Result<AstExpr, String> {
    match node {
        CstNode::Rule {
            name: "Expr" | "Term",
            children,
        } => match children.as_slice() {
            [single] => lower_expr(single),
            [left, CstNode::Terminal(op_tok), right] => {
                let op = BinOp::from_kind(op_tok.kind)
                    .ok_or_else(|| format!("unknown operator {:?}", op_tok.lexeme))?;
                Ok(AstExpr::Bin {
                    op,
                    left: Box::new(lower_expr(left)?),
                    right: Box::new(lower_expr(right)?),
                })
            }
            _ => Err("malformed Expr/Term CST node".to_string()),
        },
        CstNode::Rule {
            name: "Factor",
            children,
        } => match children.as_slice() {
            [CstNode::Terminal(tok)] => match tok.kind {
                TokenKind::Ident => Ok(AstExpr::Ident(tok.lexeme.clone())),
                TokenKind::Int => {
                    let val = tok
                        .lexeme
                        .parse::<i64>()
                        .map_err(|e| format!("bad int '{}': {e}", tok.lexeme))?;
                    Ok(AstExpr::Int(val))
                }
                _ => Err("unexpected leaf in Factor".to_string()),
            },
            [CstNode::Terminal(lp), inner, CstNode::Terminal(rp)]
                if lp.kind == TokenKind::LParen && rp.kind == TokenKind::RParen =>
            {
                lower_expr(inner)
            }
            _ => Err("malformed Factor CST node".to_string()),
        },
        _ => Err("unexpected node during CST -> AST".to_string()),
    }
}

// ---------------------------------------------------------------------------
// AST -> Canonical CST & Formatted Source
// ---------------------------------------------------------------------------

fn synth_tok(kind: TokenKind, lexeme: &str) -> CstNode {
    CstNode::Terminal(Token {
        kind,
        lexeme: lexeme.to_string(),
        start: 0,
        end: 0,
    })
}

fn kind_for_op(op: BinOp) -> TokenKind {
    match op {
        BinOp::Add => TokenKind::Plus,
        BinOp::Sub => TokenKind::Minus,
        BinOp::Mul => TokenKind::Star,
        BinOp::Div => TokenKind::Slash,
    }
}

fn needs_parens(child: &AstExpr, parent_op: BinOp, is_right_child: bool) -> bool {
    let AstExpr::Bin { op: child_op, .. } = child else {
        return false;
    };
    if child_op.prec() < parent_op.prec() {
        return true;
    }
    is_right_child
        && child_op.prec() == parent_op.prec()
        && matches!(parent_op, BinOp::Sub | BinOp::Div)
}

/// Rebuilds a canonical grammar CST from an AST (inverse up to stripped trivia).
fn ast_to_canonical_cst(stmts: &[AstStmt]) -> CstNode {
    let stmt_nodes = stmts
        .iter()
        .map(|stmt| match stmt {
            AstStmt::Let { name, value } => CstNode::rule(
                "Stmt",
                vec![
                    synth_tok(TokenKind::KwLet, "let"),
                    synth_tok(TokenKind::Ident, name),
                    synth_tok(TokenKind::Eq, "="),
                    expr_to_cst(value, 1),
                    synth_tok(TokenKind::Semi, ";"),
                ],
            ),
            AstStmt::Expr(e) => CstNode::rule(
                "Stmt",
                vec![expr_to_cst(e, 1), synth_tok(TokenKind::Semi, ";")],
            ),
        })
        .collect();
    CstNode::rule("Program", vec![CstNode::rule("StmtList", stmt_nodes)])
}

fn expr_to_cst(expr: &AstExpr, target_level: u8) -> CstNode {
    match (target_level, expr) {
        // Level 1: Expr -> Expr (+|-) Term | Term
        (1, AstExpr::Bin { op, left, right }) if op.prec() == 1 => CstNode::rule(
            "Expr",
            vec![
                expr_to_cst(left, 1),
                synth_tok(kind_for_op(*op), op.sym()),
                if needs_parens(right, *op, true) {
                    CstNode::rule("Term", vec![wrap_parens(right)])
                } else {
                    expr_to_cst(right, 2)
                },
            ],
        ),
        (1, other) => CstNode::rule("Expr", vec![expr_to_cst(other, 2)]),
        // Level 2: Term -> Term (*|/) Factor | Factor
        (2, AstExpr::Bin { op, left, right }) if op.prec() == 2 => {
            let lhs = if needs_parens(left, *op, false) {
                CstNode::rule("Term", vec![wrap_parens(left)])
            } else {
                expr_to_cst(left, 2)
            };
            let rhs = if needs_parens(right, *op, true) {
                wrap_parens(right)
            } else {
                expr_to_cst(right, 3)
            };
            CstNode::rule(
                "Term",
                vec![lhs, synth_tok(kind_for_op(*op), op.sym()), rhs],
            )
        }
        (2, other) => CstNode::rule("Term", vec![expr_to_cst(other, 3)]),
        // Level 3: Factor -> '(' Expr ')' | Ident | Int
        (_, AstExpr::Ident(name)) => {
            CstNode::rule("Factor", vec![synth_tok(TokenKind::Ident, name)])
        }
        (_, AstExpr::Int(v)) => {
            CstNode::rule("Factor", vec![synth_tok(TokenKind::Int, &v.to_string())])
        }
        (_, bin @ AstExpr::Bin { .. }) => wrap_parens(bin),
    }
}

fn wrap_parens(expr: &AstExpr) -> CstNode {
    CstNode::rule(
        "Factor",
        vec![
            synth_tok(TokenKind::LParen, "("),
            expr_to_cst(expr, 1),
            synth_tok(TokenKind::RParen, ")"),
        ],
    )
}

fn pretty_print_cst(cst: &CstNode) -> String {
    let CstNode::Rule {
        name: "Program",
        children,
    } = cst
    else {
        return cst.terminal_yield().join(" ");
    };
    let Some(CstNode::Rule {
        name: "StmtList",
        children: stmts,
    }) = children.first()
    else {
        return String::new();
    };
    stmts
        .iter()
        .map(pretty_print_stmt)
        .collect::<Vec<_>>()
        .join("\n")
}

fn pretty_print_stmt(stmt: &CstNode) -> String {
    let toks = stmt.terminal_yield();
    let mut out = String::new();
    for (idx, tok) in toks.iter().enumerate() {
        if idx > 0 {
            let prev = toks[idx - 1].as_str();
            let curr = tok.as_str();
            if curr != ";" && curr != ")" && prev != "(" {
                out.push(' ');
            }
        }
        out.push_str(tok);
    }
    out
}

fn eval_program(stmts: &[AstStmt]) -> Result<(HashMap<String, i64>, Option<i64>), String> {
    let mut env = HashMap::new();
    let mut last = None;
    for stmt in stmts {
        match stmt {
            AstStmt::Let { name, value } => {
                let v = eval_expr(value, &env)?;
                env.insert(name.clone(), v);
                last = Some(v);
            }
            AstStmt::Expr(e) => {
                last = Some(eval_expr(e, &env)?);
            }
        }
    }
    Ok((env, last))
}

fn eval_expr(expr: &AstExpr, env: &HashMap<String, i64>) -> Result<i64, String> {
    match expr {
        AstExpr::Ident(name) => env
            .get(name)
            .copied()
            .ok_or_else(|| format!("undefined variable '{name}'")),
        AstExpr::Int(v) => Ok(*v),
        AstExpr::Bin { op, left, right } => {
            let l = eval_expr(left, env)?;
            let r = eval_expr(right, env)?;
            match op {
                BinOp::Add => Ok(l + r),
                BinOp::Sub => Ok(l - r),
                BinOp::Mul => Ok(l * r),
                BinOp::Div => {
                    if r == 0 {
                        Err("division by zero".to_string())
                    } else {
                        Ok(l / r)
                    }
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Purple §6.1 Variant: Value-Numbered Syntax DAG
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum DagOp {
    Ident(String),
    Int(i64),
    Bin(BinOp, usize, usize),
}

impl fmt::Display for DagOp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DagOp::Ident(name) => write!(f, "Name({name:?})"),
            DagOp::Int(v) => write!(f, "Number({v})"),
            DagOp::Bin(op, l, r) => write!(f, "Bin({:?}, #{l}, #{r})", op.sym()),
        }
    }
}

#[derive(Default, Debug)]
struct ExprDag {
    nodes: Vec<DagOp>,
    intern: HashMap<DagOp, usize>,
}

impl ExprDag {
    fn insert(&mut self, expr: &AstExpr) -> usize {
        let key = match expr {
            AstExpr::Ident(s) => DagOp::Ident(s.clone()),
            AstExpr::Int(v) => DagOp::Int(*v),
            AstExpr::Bin { op, left, right } => {
                let l = self.insert(left);
                let r = self.insert(right);
                DagOp::Bin(*op, l, r)
            }
        };
        if let Some(&id) = self.intern.get(&key) {
            return id;
        }
        let id = self.nodes.len();
        self.nodes.push(key.clone());
        self.intern.insert(key, id);
        id
    }
}

fn main() -> Result<(), String> {
    let sample_let = "let x  =  42;  // important comment";
    let lexed = lex_strip_trivia(sample_let)?;
    let cst = CstParser::new(lexed.tokens.clone()).parse_program()?;
    let ast = cst_to_ast(&cst)?;
    let canonical_cst = ast_to_canonical_cst(&ast);
    let rebuilt_src = pretty_print_cst(&canonical_cst);

    println!("=== 1. Classical §2.6.1 Lexing Drops Trivia Before Parsing ===");
    println!(
        "Original source ({} bytes): {:?}",
        sample_let.len(),
        sample_let
    );
    println!(
        "Stripped: {} whitespace bytes, {} comments",
        lexed.stripped_whitespace_bytes, lexed.stripped_comments
    );
    println!(
        "\n=== 2. Classical CST / Parse Tree (§2.2.3) ({} nodes) ===",
        cst.node_count()
    );
    println!("{}", cst.render_tree());
    println!("\nLeftmost derivation (=>_lm):");
    for (idx, step) in leftmost_derivation(&cst).iter().enumerate() {
        println!("  {idx:>2}. {step}");
    }

    println!(
        "\n=== 3. Abstract Syntax Tree (§2.5.1, §2.8.2, §5.3.1) ({} nodes) ===",
        program_ast_node_count(&ast)
    );
    println!("{}", ast[0].render_tree());

    println!("\n=== 4. Classical CST Is NOT Lossless ===");
    println!("Rebuilt canonical CST yield: {:?}", rebuilt_src);
    println!(
        "Byte-for-byte match? {} (double spaces & '// important comment' were lost in §2.6.1)",
        rebuilt_src == sample_let
    );

    let dag_src = "let a = 2; let b = 3; (a + b) * 4 + (a + b) * 4;";
    let dag_lex = lex_strip_trivia(dag_src)?;
    let dag_cst = CstParser::new(dag_lex.tokens).parse_program()?;
    let dag_ast = cst_to_ast(&dag_cst)?;
    let AstStmt::Expr(ref shared_expr) = dag_ast[2] else {
        unreachable!();
    };
    let mut dag = ExprDag::default();
    let root_id = dag.insert(shared_expr);
    let (_, val) = eval_program(&dag_ast)?;
    println!("\n=== 5. §6.1 Syntax-Tree Variant: Tree AST vs. Value-Numbered DAG ===");
    println!(
        "Expression: '(a + b) * 4 + (a + b) * 4' -> AST nodes = {}, DAG nodes = {}, root = #{}, eval = {:?}",
        shared_expr.node_count(),
        dag.nodes.len(),
        root_id,
        val
    );
    for (id, node) in dag.nodes.iter().enumerate() {
        println!("  #{id}: {node}");
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classical_cst_strips_trivia_and_cannot_roundtrip_verbatim() {
        let src = "let x  =  42;  // important comment";
        let lexed = lex_strip_trivia(src).unwrap();
        assert_eq!(lexed.stripped_whitespace_bytes, 7);
        assert_eq!(lexed.stripped_comments, 1);

        let cst = CstParser::new(lexed.tokens).parse_program().unwrap();
        let ast = cst_to_ast(&cst).unwrap();
        assert_eq!(
            ast,
            vec![AstStmt::Let {
                name: "x".to_string(),
                value: AstExpr::Int(42),
            }]
        );
        assert!(cst.node_count() > program_ast_node_count(&ast));

        let canonical_cst = ast_to_canonical_cst(&ast);
        let formatted = pretty_print_cst(&canonical_cst);
        assert_eq!(formatted, "let x = 42;");
        assert_ne!(formatted, src);
    }

    #[test]
    fn roundtrip_preserves_ast_semantics_across_precedence_and_parens() {
        let src = "let x = (1 + 2) * 3; x - (4 - 1);";
        let lexed = lex_strip_trivia(src).unwrap();
        let cst = CstParser::new(lexed.tokens).parse_program().unwrap();
        let ast = cst_to_ast(&cst).unwrap();

        let canonical = ast_to_canonical_cst(&ast);
        let formatted = pretty_print_cst(&canonical);
        assert_eq!(formatted, "let x = (1 + 2) * 3;\nx - (4 - 1);");

        let relexed = lex_strip_trivia(&formatted).unwrap();
        let recst = CstParser::new(relexed.tokens).parse_program().unwrap();
        let reast = cst_to_ast(&recst).unwrap();
        assert_eq!(reast, ast);

        let (env, last) = eval_program(&reast).unwrap();
        assert_eq!(env.get("x"), Some(&9));
        assert_eq!(last, Some(6));
    }

    #[test]
    fn leftmost_derivation_terminates_at_cst_yield() {
        let src = "let x = 42;";
        let cst = CstParser::new(lex_strip_trivia(src).unwrap().tokens)
            .parse_program()
            .unwrap();
        let steps = leftmost_derivation(&cst);
        assert_eq!(steps.first().unwrap(), "Program");
        assert_eq!(steps.last().unwrap(), "let x = 42 ;");
        assert!(steps.len() >= 6);
    }

    #[test]
    fn syntax_dag_shares_identical_subexpressions() {
        let src = "let a = 2; let b = 3; (a + b) * 4 + (a + b) * 4;";
        let cst = CstParser::new(lex_strip_trivia(src).unwrap().tokens)
            .parse_program()
            .unwrap();
        let ast = cst_to_ast(&cst).unwrap();
        let AstStmt::Expr(ref expr) = ast[2] else {
            panic!("expected expr stmt");
        };
        assert_eq!(expr.node_count(), 11);
        let mut dag = ExprDag::default();
        let root = dag.insert(expr);
        assert_eq!(dag.nodes.len(), 6);
        assert_eq!(root, 5);
    }
}
