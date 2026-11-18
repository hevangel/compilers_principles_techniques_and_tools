//! Chapter 4 (§4.9) -> Chapters 5 & 6 Bridge: AST Construction & Representations.
//!
//! Teaches:
//! 1. Concrete Parse Tree (CST) vs. Abstract Syntax Tree (AST) representation and size,
//! 2. Designing algebraic AST node types (`AstStmt`, `AstExpr`) with source `Span`s,
//! 3. Constructing both a CST and an AST via Recursive Descent,
//! 4. Constructing the *exact same* AST via Yacc-style LALR(1) semantic actions (`$N`),
//! 5. Preserving merged source locations (`line:col` + byte range) for pointed diagnostics,
//! 6. Walking the AST cleanly with the `AstVisitor` pattern (printer, stats, type/scope checker),
//! 7. Lowering the semantically checked AST to Three-Address Code (TAC) IR and executing it.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

// ---------------------------------------------------------------------------
// 1. Source Spans & Lexer
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Span {
    start: usize,
    end: usize,
    line: usize,
    col: usize,
}

impl Span {
    fn merge(self, other: Span) -> Span {
        if self.start <= other.start {
            Span {
                start: self.start,
                end: self.end.max(other.end),
                line: self.line,
                col: self.col,
            }
        } else {
            Span {
                start: other.start,
                end: self.end.max(other.end),
                line: other.line,
                col: other.col,
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum TokKind {
    Let,
    Emit,
    Ident,
    Int,
    Eq,     // =
    Plus,   // +
    Minus,  // -
    Star,   // *
    Slash,  // /
    LParen, // (
    RParen, // )
    Semi,   // ;
    Eof,
}

impl TokKind {
    fn term_name(&self) -> &'static str {
        match self {
            TokKind::Let => "let",
            TokKind::Emit => "emit",
            TokKind::Ident => "ident",
            TokKind::Int => "int",
            TokKind::Eq => "=",
            TokKind::Plus => "+",
            TokKind::Minus => "-",
            TokKind::Star => "*",
            TokKind::Slash => "/",
            TokKind::LParen => "(",
            TokKind::RParen => ")",
            TokKind::Semi => ";",
            TokKind::Eof => "$",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Token {
    kind: TokKind,
    lexeme: String,
    span: Span,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Diagnostic {
    span: Span,
    message: String,
}

fn format_diagnostic(src: &str, diag: &Diagnostic) -> String {
    let line_str = src
        .lines()
        .nth(diag.span.line.saturating_sub(1))
        .unwrap_or("");
    let width = diag.span.end.saturating_sub(diag.span.start).max(1);
    let pad = " ".repeat(diag.span.col.saturating_sub(1));
    let carets = "^".repeat(width.min(line_str.len().saturating_sub(diag.span.col - 1).max(1)));
    format!(
        "error at line {}, col {}: {}\n  | {}\n  | {}{}",
        diag.span.line, diag.span.col, diag.message, line_str, pad, carets
    )
}

fn lex(src: &str) -> Result<Vec<Token>, Diagnostic> {
    let bytes = src.as_bytes();
    let mut i = 0;
    let mut line = 1usize;
    let mut line_start = 0usize;
    let mut out = Vec::new();

    while i < bytes.len() {
        let b = bytes[i];
        if b == b'\n' {
            i += 1;
            line += 1;
            line_start = i;
            continue;
        }
        if b.is_ascii_whitespace() {
            i += 1;
            continue;
        }
        let start = i;
        let col = start - line_start + 1;
        if b.is_ascii_alphabetic() || b == b'_' {
            i += 1;
            while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                i += 1;
            }
            let lexeme = src[start..i].to_string();
            let kind = match lexeme.as_str() {
                "let" => TokKind::Let,
                "emit" => TokKind::Emit,
                _ => TokKind::Ident,
            };
            out.push(Token {
                kind,
                lexeme,
                span: Span {
                    start,
                    end: i,
                    line,
                    col,
                },
            });
            continue;
        }
        if b.is_ascii_digit() {
            i += 1;
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                i += 1;
            }
            let lexeme = src[start..i].to_string();
            if lexeme.parse::<i64>().is_err() {
                return Err(Diagnostic {
                    span: Span {
                        start,
                        end: i,
                        line,
                        col,
                    },
                    message: format!("integer literal overflow: '{lexeme}'"),
                });
            }
            out.push(Token {
                kind: TokKind::Int,
                lexeme,
                span: Span {
                    start,
                    end: i,
                    line,
                    col,
                },
            });
            continue;
        }
        let kind = match b {
            b'=' => TokKind::Eq,
            b'+' => TokKind::Plus,
            b'-' => TokKind::Minus,
            b'*' => TokKind::Star,
            b'/' => TokKind::Slash,
            b'(' => TokKind::LParen,
            b')' => TokKind::RParen,
            b';' => TokKind::Semi,
            _ => {
                return Err(Diagnostic {
                    span: Span {
                        start,
                        end: i + 1,
                        line,
                        col,
                    },
                    message: format!("unexpected character '{}'", b as char),
                });
            }
        };
        i += 1;
        out.push(Token {
            kind,
            lexeme: src[start..i].to_string(),
            span: Span {
                start,
                end: i,
                line,
                col,
            },
        });
    }
    out.push(Token {
        kind: TokKind::Eof,
        lexeme: String::new(),
        span: Span {
            start: src.len(),
            end: src.len(),
            line,
            col: src.len().saturating_sub(line_start) + 1,
        },
    });
    Ok(out)
}

// ---------------------------------------------------------------------------
// 2. Concrete Parse Tree (CST) vs. Abstract Syntax Tree (AST) Nodes
// ---------------------------------------------------------------------------

/// Concrete Parse Tree: keeps every grammar nonterminal, unit chain rule,
/// punctuation leaf (`=`, `(`, `)`, `;`), and empty epsilon marker.
#[derive(Clone, Debug, PartialEq, Eq)]
enum CstNode {
    Rule {
        nonterminal: String,
        children: Vec<CstNode>,
    },
    Leaf(Token),
}

fn cst_node_count(node: &CstNode) -> usize {
    match node {
        CstNode::Leaf(_) => 1,
        CstNode::Rule { children, .. } => 1 + children.iter().map(cst_node_count).sum::<usize>(),
    }
}

fn format_cst(node: &CstNode, indent: usize, out: &mut String) {
    let pad = "  ".repeat(indent);
    match node {
        CstNode::Leaf(tok) => {
            out.push_str(&format!(
                "{pad}token({}, {:?})\n",
                tok.kind.term_name(),
                tok.lexeme
            ));
        }
        CstNode::Rule {
            nonterminal,
            children,
        } => {
            out.push_str(&format!("{pad}{nonterminal}\n"));
            for ch in children {
                format_cst(ch, indent + 1, out);
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
}

impl fmt::Display for BinOp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BinOp::Add => write!(f, "+"),
            BinOp::Sub => write!(f, "-"),
            BinOp::Mul => write!(f, "*"),
            BinOp::Div => write!(f, "/"),
        }
    }
}

/// Abstract Syntax Tree (AST): strips punctuation and grammar chaining,
/// storing only semantic constructs plus source `Span`s for diagnostics.
#[derive(Clone, Debug, PartialEq, Eq)]
enum AstExpr {
    Int {
        val: i64,
        span: Span,
    },
    Var {
        name: String,
        span: Span,
    },
    Neg {
        inner: Box<AstExpr>,
        span: Span,
    },
    Bin {
        op: BinOp,
        left: Box<AstExpr>,
        right: Box<AstExpr>,
        span: Span,
    },
}

impl AstExpr {
    fn span(&self) -> Span {
        match self {
            AstExpr::Int { span, .. }
            | AstExpr::Var { span, .. }
            | AstExpr::Neg { span, .. }
            | AstExpr::Bin { span, .. } => *span,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum AstStmt {
    Let {
        name: String,
        expr: AstExpr,
        span: Span,
    },
    Emit {
        expr: AstExpr,
        span: Span,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct AstProgram {
    stmts: Vec<AstStmt>,
}

// ---------------------------------------------------------------------------
// 3. Path A: Recursive-Descent AST & Concrete Parse Tree Builder
// ---------------------------------------------------------------------------

struct RdParser {
    tokens: Vec<Token>,
    pos: usize,
}

impl RdParser {
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

    fn expect(&mut self, kind: &TokKind) -> Result<Token, Diagnostic> {
        let tok = self.peek().clone();
        if &tok.kind == kind {
            Ok(self.advance())
        } else {
            Err(Diagnostic {
                span: tok.span,
                message: format!(
                    "expected '{}', found '{}'",
                    kind.term_name(),
                    tok.kind.term_name()
                ),
            })
        }
    }

    fn parse_program(&mut self) -> Result<(CstNode, AstProgram), Diagnostic> {
        let mut cst_children = Vec::new();
        let mut stmts = Vec::new();
        while self.peek().kind != TokKind::Eof {
            let (cst_s, ast_s) = self.parse_stmt()?;
            cst_children.push(cst_s);
            stmts.push(ast_s);
        }
        Ok((
            CstNode::Rule {
                nonterminal: "Program".into(),
                children: cst_children,
            },
            AstProgram { stmts },
        ))
    }

    fn parse_stmt(&mut self) -> Result<(CstNode, AstStmt), Diagnostic> {
        let tok = self.peek().clone();
        match tok.kind {
            TokKind::Let => {
                let kw = self.advance();
                let id_tok = self.expect(&TokKind::Ident)?;
                let eq_tok = self.expect(&TokKind::Eq)?;
                let (expr_cst, expr_ast) = self.parse_expr()?;
                let semi = self.expect(&TokKind::Semi)?;
                let span = kw.span.merge(semi.span);
                let cst = CstNode::Rule {
                    nonterminal: "Stmt(Let)".into(),
                    children: vec![
                        CstNode::Leaf(kw),
                        CstNode::Leaf(id_tok.clone()),
                        CstNode::Leaf(eq_tok),
                        expr_cst,
                        CstNode::Leaf(semi),
                    ],
                };
                let ast = AstStmt::Let {
                    name: id_tok.lexeme,
                    expr: expr_ast,
                    span,
                };
                Ok((cst, ast))
            }
            TokKind::Emit => {
                let kw = self.advance();
                let (expr_cst, expr_ast) = self.parse_expr()?;
                let semi = self.expect(&TokKind::Semi)?;
                let span = kw.span.merge(semi.span);
                let cst = CstNode::Rule {
                    nonterminal: "Stmt(Emit)".into(),
                    children: vec![CstNode::Leaf(kw), expr_cst, CstNode::Leaf(semi)],
                };
                let ast = AstStmt::Emit {
                    expr: expr_ast,
                    span,
                };
                Ok((cst, ast))
            }
            _ => Err(Diagnostic {
                span: tok.span,
                message: format!("expected 'let' or 'emit', found '{}'", tok.kind.term_name()),
            }),
        }
    }

    fn parse_expr(&mut self) -> Result<(CstNode, AstExpr), Diagnostic> {
        let (first_cst, mut ast) = self.parse_term()?;
        let mut cst_children = vec![first_cst];
        while matches!(self.peek().kind, TokKind::Plus | TokKind::Minus) {
            let op_tok = self.advance();
            let op = if op_tok.kind == TokKind::Plus {
                BinOp::Add
            } else {
                BinOp::Sub
            };
            let (rhs_cst, rhs_ast) = self.parse_term()?;
            cst_children.push(CstNode::Leaf(op_tok));
            cst_children.push(rhs_cst);
            let span = ast.span().merge(rhs_ast.span());
            ast = AstExpr::Bin {
                op,
                left: Box::new(ast),
                right: Box::new(rhs_ast),
                span,
            };
        }
        Ok((
            CstNode::Rule {
                nonterminal: "Expr".into(),
                children: cst_children,
            },
            ast,
        ))
    }

    fn parse_term(&mut self) -> Result<(CstNode, AstExpr), Diagnostic> {
        let (first_cst, mut ast) = self.parse_unary()?;
        let mut cst_children = vec![first_cst];
        while matches!(self.peek().kind, TokKind::Star | TokKind::Slash) {
            let op_tok = self.advance();
            let op = if op_tok.kind == TokKind::Star {
                BinOp::Mul
            } else {
                BinOp::Div
            };
            let (rhs_cst, rhs_ast) = self.parse_unary()?;
            cst_children.push(CstNode::Leaf(op_tok));
            cst_children.push(rhs_cst);
            let span = ast.span().merge(rhs_ast.span());
            ast = AstExpr::Bin {
                op,
                left: Box::new(ast),
                right: Box::new(rhs_ast),
                span,
            };
        }
        Ok((
            CstNode::Rule {
                nonterminal: "Term".into(),
                children: cst_children,
            },
            ast,
        ))
    }

    fn parse_unary(&mut self) -> Result<(CstNode, AstExpr), Diagnostic> {
        if self.peek().kind == TokKind::Minus {
            let minus_tok = self.advance();
            let (inner_cst, inner_ast) = self.parse_unary()?;
            let span = minus_tok.span.merge(inner_ast.span());
            let cst = CstNode::Rule {
                nonterminal: "Unary".into(),
                children: vec![CstNode::Leaf(minus_tok), inner_cst],
            };
            let ast = AstExpr::Neg {
                inner: Box::new(inner_ast),
                span,
            };
            Ok((cst, ast))
        } else {
            let (atom_cst, ast) = self.parse_atom()?;
            Ok((
                CstNode::Rule {
                    nonterminal: "Unary".into(),
                    children: vec![atom_cst],
                },
                ast,
            ))
        }
    }

    fn parse_atom(&mut self) -> Result<(CstNode, AstExpr), Diagnostic> {
        let tok = self.peek().clone();
        match tok.kind {
            TokKind::Int => {
                let t = self.advance();
                let val = t.lexeme.parse::<i64>().unwrap();
                let ast = AstExpr::Int { val, span: t.span };
                Ok((
                    CstNode::Rule {
                        nonterminal: "Factor".into(),
                        children: vec![CstNode::Leaf(t)],
                    },
                    ast,
                ))
            }
            TokKind::Ident => {
                let t = self.advance();
                let ast = AstExpr::Var {
                    name: t.lexeme.clone(),
                    span: t.span,
                };
                Ok((
                    CstNode::Rule {
                        nonterminal: "Factor".into(),
                        children: vec![CstNode::Leaf(t)],
                    },
                    ast,
                ))
            }
            TokKind::LParen => {
                let lp = self.advance();
                let (inner_cst, inner_ast) = self.parse_expr()?;
                let rp = self.expect(&TokKind::RParen)?;
                Ok((
                    CstNode::Rule {
                        nonterminal: "Factor(Paren)".into(),
                        children: vec![CstNode::Leaf(lp), inner_cst, CstNode::Leaf(rp)],
                    },
                    inner_ast,
                ))
            }
            _ => Err(Diagnostic {
                span: tok.span,
                message: format!(
                    "expected expression operand, found '{}'",
                    tok.kind.term_name()
                ),
            }),
        }
    }
}

// ---------------------------------------------------------------------------
// 4. Path B: Yacc-Style LALR(1) Parser + Semantic Actions -> Identical AST
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Sym {
    Term(&'static str),
    Nt(&'static str),
}

#[derive(Clone, Debug)]
struct Prod {
    lhs: &'static str,
    rhs: Vec<Sym>,
    prec_level: Option<usize>,
    assoc_left: bool,
    action_id: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Lr1Item {
    prod: usize,
    dot: usize,
    lookahead: &'static str,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Lr0Item {
    prod: usize,
    dot: usize,
}

#[derive(Clone, Debug)]
enum Action {
    Shift(usize),
    Reduce(usize),
    Accept,
}

struct LalrTables {
    prods: Vec<Prod>,
    action: BTreeMap<(usize, String), Action>,
    goto: BTreeMap<(usize, String), usize>,
}

fn build_yacc_ast_tables() -> LalrTables {
    // Equivalent ambiguous operator grammar disambiguated by %left/%prec just like Yacc:
    //   0: S' -> Program
    //   1: Program -> StmtList
    //   2: StmtList -> StmtList Stmt
    //   3: StmtList -> Stmt
    //   4: Stmt -> 'let' 'ident' '=' Expr ';'
    //   5: Stmt -> 'emit' Expr ';'
    //   6: Expr -> Expr '+' Expr      (%left, level 1)
    //   7: Expr -> Expr '-' Expr      (%left, level 1)
    //   8: Expr -> Expr '*' Expr      (%left, level 2)
    //   9: Expr -> Expr '/' Expr      (%left, level 2)
    //  10: Expr -> '-' Expr           (%prec UMINUS, level 3)
    //  11: Expr -> '(' Expr ')'
    //  12: Expr -> 'ident'
    //  13: Expr -> 'int'
    use Sym::{Nt, Term};
    let prods = vec![
        Prod {
            lhs: "S'",
            rhs: vec![Nt("Program")],
            prec_level: None,
            assoc_left: true,
            action_id: 0,
        },
        Prod {
            lhs: "Program",
            rhs: vec![Nt("StmtList")],
            prec_level: None,
            assoc_left: true,
            action_id: 1,
        },
        Prod {
            lhs: "StmtList",
            rhs: vec![Nt("StmtList"), Nt("Stmt")],
            prec_level: None,
            assoc_left: true,
            action_id: 2,
        },
        Prod {
            lhs: "StmtList",
            rhs: vec![Nt("Stmt")],
            prec_level: None,
            assoc_left: true,
            action_id: 3,
        },
        Prod {
            lhs: "Stmt",
            rhs: vec![Term("let"), Term("ident"), Term("="), Nt("Expr"), Term(";")],
            prec_level: None,
            assoc_left: true,
            action_id: 4,
        },
        Prod {
            lhs: "Stmt",
            rhs: vec![Term("emit"), Nt("Expr"), Term(";")],
            prec_level: None,
            assoc_left: true,
            action_id: 5,
        },
        Prod {
            lhs: "Expr",
            rhs: vec![Nt("Expr"), Term("+"), Nt("Expr")],
            prec_level: Some(1),
            assoc_left: true,
            action_id: 6,
        },
        Prod {
            lhs: "Expr",
            rhs: vec![Nt("Expr"), Term("-"), Nt("Expr")],
            prec_level: Some(1),
            assoc_left: true,
            action_id: 7,
        },
        Prod {
            lhs: "Expr",
            rhs: vec![Nt("Expr"), Term("*"), Nt("Expr")],
            prec_level: Some(2),
            assoc_left: true,
            action_id: 8,
        },
        Prod {
            lhs: "Expr",
            rhs: vec![Nt("Expr"), Term("/"), Nt("Expr")],
            prec_level: Some(2),
            assoc_left: true,
            action_id: 9,
        },
        Prod {
            lhs: "Expr",
            rhs: vec![Term("-"), Nt("Expr")],
            prec_level: Some(3),
            assoc_left: false,
            action_id: 10,
        },
        Prod {
            lhs: "Expr",
            rhs: vec![Term("("), Nt("Expr"), Term(")")],
            prec_level: None,
            assoc_left: true,
            action_id: 11,
        },
        Prod {
            lhs: "Expr",
            rhs: vec![Term("ident")],
            prec_level: None,
            assoc_left: true,
            action_id: 12,
        },
        Prod {
            lhs: "Expr",
            rhs: vec![Term("int")],
            prec_level: None,
            assoc_left: true,
            action_id: 13,
        },
    ];

    let term_prec = |t: &str| -> Option<(usize, bool)> {
        match t {
            "+" | "-" => Some((1, true)),
            "*" | "/" => Some((2, true)),
            _ => None,
        }
    };

    // FIRST sets (none of these nonterminals are nullable).
    let mut first: BTreeMap<&'static str, BTreeSet<&'static str>> = BTreeMap::new();
    for p in &prods {
        first.entry(p.lhs).or_default();
    }
    loop {
        let mut changed = false;
        for p in &prods {
            if let Some(first_sym) = p.rhs.first() {
                match first_sym {
                    Term(t) => {
                        changed |= first.get_mut(p.lhs).unwrap().insert(t);
                    }
                    Nt(nt) => {
                        let copy = first[nt].clone();
                        for t in copy {
                            changed |= first.get_mut(p.lhs).unwrap().insert(t);
                        }
                    }
                }
            }
        }
        if !changed {
            break;
        }
    }

    let first_of_tail = |tail: &[Sym], la: &'static str| -> BTreeSet<&'static str> {
        match tail.first() {
            None => BTreeSet::from([la]),
            Some(Term(t)) => BTreeSet::from([*t]),
            Some(Nt(nt)) => first[nt].clone(),
        }
    };

    let closure = |mut set: BTreeSet<Lr1Item>| -> BTreeSet<Lr1Item> {
        loop {
            let mut added = Vec::new();
            for &item in &set {
                let p = &prods[item.prod];
                if let Some(Nt(b)) = p.rhs.get(item.dot) {
                    let las = first_of_tail(&p.rhs[item.dot + 1..], item.lookahead);
                    for (pi, cand) in prods.iter().enumerate() {
                        if cand.lhs == *b {
                            for &la in &las {
                                let ni = Lr1Item {
                                    prod: pi,
                                    dot: 0,
                                    lookahead: la,
                                };
                                if !set.contains(&ni) {
                                    added.push(ni);
                                }
                            }
                        }
                    }
                }
            }
            if added.is_empty() {
                break;
            }
            set.extend(added);
        }
        set
    };

    let goto_set = |items: &BTreeSet<Lr1Item>, sym: &Sym| -> BTreeSet<Lr1Item> {
        let mut seed = BTreeSet::new();
        for &it in items {
            if prods[it.prod].rhs.get(it.dot) == Some(sym) {
                seed.insert(Lr1Item {
                    prod: it.prod,
                    dot: it.dot + 1,
                    lookahead: it.lookahead,
                });
            }
        }
        if seed.is_empty() {
            seed
        } else {
            closure(seed)
        }
    };

    let mut all_syms: BTreeSet<Sym> = BTreeSet::new();
    for p in &prods {
        for s in &p.rhs {
            all_syms.insert(s.clone());
        }
    }

    let init = closure(BTreeSet::from([Lr1Item {
        prod: 0,
        dot: 0,
        lookahead: "$",
    }]));
    let mut lr1_states = vec![init.clone()];
    let mut lr1_index = BTreeMap::from([(init, 0usize)]);
    let mut lr1_trans: BTreeMap<(usize, Sym), usize> = BTreeMap::new();

    let mut q = 0;
    while q < lr1_states.len() {
        let cur = lr1_states[q].clone();
        for s in &all_syms {
            let nxt = goto_set(&cur, s);
            if !nxt.is_empty() {
                let dst = if let Some(&idx) = lr1_index.get(&nxt) {
                    idx
                } else {
                    let idx = lr1_states.len();
                    lr1_states.push(nxt.clone());
                    lr1_index.insert(nxt, idx);
                    idx
                };
                lr1_trans.insert((q, s.clone()), dst);
            }
        }
        q += 1;
    }

    // Merge LR(1) states sharing identical LR(0) cores -> LALR(1).
    let core_of = |st: &BTreeSet<Lr1Item>| -> BTreeSet<Lr0Item> {
        st.iter()
            .map(|it| Lr0Item {
                prod: it.prod,
                dot: it.dot,
            })
            .collect()
    };
    let mut lalr_cores: Vec<BTreeSet<Lr0Item>> = Vec::new();
    let mut core_map: BTreeMap<BTreeSet<Lr0Item>, usize> = BTreeMap::new();
    let mut lr1_to_lalr = vec![0usize; lr1_states.len()];
    let mut lalr_items: Vec<BTreeSet<Lr1Item>> = Vec::new();

    for (i, st) in lr1_states.iter().enumerate() {
        let c = core_of(st);
        let lid = if let Some(&id) = core_map.get(&c) {
            id
        } else {
            let id = lalr_cores.len();
            lalr_cores.push(c.clone());
            core_map.insert(c, id);
            lalr_items.push(BTreeSet::new());
            id
        };
        lr1_to_lalr[i] = lid;
        lalr_items[lid].extend(st.iter().copied());
    }

    let mut action: BTreeMap<(usize, String), Action> = BTreeMap::new();
    let mut goto: BTreeMap<(usize, String), usize> = BTreeMap::new();

    for (&(src, ref sym), &dst) in &lr1_trans {
        let lsrc = lr1_to_lalr[src];
        let ldst = lr1_to_lalr[dst];
        match sym {
            Term(t) => {
                action.insert((lsrc, (*t).to_string()), Action::Shift(ldst));
            }
            Nt(nt) => {
                goto.insert((lsrc, (*nt).to_string()), ldst);
            }
        }
    }

    for (lid, st) in lalr_items.iter().enumerate() {
        for &it in st {
            let p = &prods[it.prod];
            if it.dot == p.rhs.len() {
                if it.prod == 0 && it.lookahead == "$" {
                    action.insert((lid, "$".into()), Action::Accept);
                    continue;
                }
                let key = (lid, it.lookahead.to_string());
                match action.get(&key) {
                    None => {
                        action.insert(key, Action::Reduce(it.prod));
                    }
                    Some(Action::Shift(_)) => {
                        // Resolve Shift/Reduce via Yacc %left / %prec rules.
                        let r_prec = p
                            .prec_level
                            .expect("S/R conflict rule must have precedence");
                        let (t_prec, _) = term_prec(it.lookahead).expect("lookahead prec");
                        if r_prec > t_prec || (r_prec == t_prec && p.assoc_left) {
                            action.insert(key, Action::Reduce(it.prod));
                        }
                    }
                    Some(Action::Reduce(prev_prod)) => {
                        if it.prod < *prev_prod {
                            action.insert(key, Action::Reduce(it.prod));
                        }
                    }
                    Some(Action::Accept) => {}
                }
            }
        }
    }

    LalrTables {
        prods,
        action,
        goto,
    }
}

#[derive(Clone, Debug)]
enum SymVal {
    Tok(Token),
    Expr(AstExpr),
    Stmt(AstStmt),
    Stmts(Vec<AstStmt>),
}

fn parse_yacc_ast(tables: &LalrTables, tokens: &[Token]) -> Result<AstProgram, Diagnostic> {
    let mut states: Vec<usize> = vec![0];
    let mut vals: Vec<SymVal> = Vec::new();
    let mut idx = 0usize;

    loop {
        let tok = &tokens[idx];
        let st = *states.last().unwrap();
        let act = tables
            .action
            .get(&(st, tok.kind.term_name().to_string()))
            .ok_or_else(|| Diagnostic {
                span: tok.span,
                message: format!("Yacc syntax error near '{}'", tok.kind.term_name()),
            })?
            .clone();

        match act {
            Action::Shift(next_st) => {
                states.push(next_st);
                vals.push(SymVal::Tok(tok.clone()));
                idx += 1;
            }
            Action::Reduce(pi) => {
                let prod = &tables.prods[pi];
                let pop_n = prod.rhs.len();
                let popped: Vec<SymVal> = vals.drain(vals.len() - pop_n..).collect();
                states.truncate(states.len() - pop_n);

                // Run the Yacc `{ ... }` semantic action directly constructing AST nodes:
                let reduced = match prod.action_id {
                    1 => popped.into_iter().next().unwrap(), // Program -> StmtList
                    2 => {
                        let mut it = popped.into_iter();
                        let SymVal::Stmts(mut list) = it.next().unwrap() else {
                            unreachable!()
                        };
                        let SymVal::Stmt(s) = it.next().unwrap() else {
                            unreachable!()
                        };
                        list.push(s);
                        SymVal::Stmts(list)
                    }
                    3 => {
                        let SymVal::Stmt(s) = popped.into_iter().next().unwrap() else {
                            unreachable!()
                        };
                        SymVal::Stmts(vec![s])
                    }
                    4 => {
                        // 'let' $2(ident) '=' $4(Expr) ';'
                        let mut it = popped.into_iter();
                        let SymVal::Tok(kw) = it.next().unwrap() else {
                            unreachable!()
                        };
                        let SymVal::Tok(id) = it.next().unwrap() else {
                            unreachable!()
                        };
                        let _eq = it.next().unwrap();
                        let SymVal::Expr(expr) = it.next().unwrap() else {
                            unreachable!()
                        };
                        let SymVal::Tok(semi) = it.next().unwrap() else {
                            unreachable!()
                        };
                        SymVal::Stmt(AstStmt::Let {
                            name: id.lexeme,
                            expr,
                            span: kw.span.merge(semi.span),
                        })
                    }
                    5 => {
                        // 'emit' $2(Expr) ';'
                        let mut it = popped.into_iter();
                        let SymVal::Tok(kw) = it.next().unwrap() else {
                            unreachable!()
                        };
                        let SymVal::Expr(expr) = it.next().unwrap() else {
                            unreachable!()
                        };
                        let SymVal::Tok(semi) = it.next().unwrap() else {
                            unreachable!()
                        };
                        SymVal::Stmt(AstStmt::Emit {
                            expr,
                            span: kw.span.merge(semi.span),
                        })
                    }
                    6..=9 => {
                        // $1(Expr) op $3(Expr)
                        let mut it = popped.into_iter();
                        let SymVal::Expr(l) = it.next().unwrap() else {
                            unreachable!()
                        };
                        let _op = it.next().unwrap();
                        let SymVal::Expr(r) = it.next().unwrap() else {
                            unreachable!()
                        };
                        let op = match prod.action_id {
                            6 => BinOp::Add,
                            7 => BinOp::Sub,
                            8 => BinOp::Mul,
                            _ => BinOp::Div,
                        };
                        let span = l.span().merge(r.span());
                        SymVal::Expr(AstExpr::Bin {
                            op,
                            left: Box::new(l),
                            right: Box::new(r),
                            span,
                        })
                    }
                    10 => {
                        // '-' $2(Expr)
                        let mut it = popped.into_iter();
                        let SymVal::Tok(minus) = it.next().unwrap() else {
                            unreachable!()
                        };
                        let SymVal::Expr(inner) = it.next().unwrap() else {
                            unreachable!()
                        };
                        let span = minus.span.merge(inner.span());
                        SymVal::Expr(AstExpr::Neg {
                            inner: Box::new(inner),
                            span,
                        })
                    }
                    11 => {
                        // '(' $2(Expr) ')' -> parens drop cleanly in AST!
                        let mut it = popped.into_iter();
                        let _lp = it.next().unwrap();
                        let inner = it.next().unwrap();
                        let _rp = it.next().unwrap();
                        inner
                    }
                    12 => {
                        let SymVal::Tok(t) = popped.into_iter().next().unwrap() else {
                            unreachable!()
                        };
                        SymVal::Expr(AstExpr::Var {
                            name: t.lexeme,
                            span: t.span,
                        })
                    }
                    13 => {
                        let SymVal::Tok(t) = popped.into_iter().next().unwrap() else {
                            unreachable!()
                        };
                        let val = t.lexeme.parse::<i64>().unwrap();
                        SymVal::Expr(AstExpr::Int { val, span: t.span })
                    }
                    _ => unreachable!(),
                };

                let top_st = *states.last().unwrap();
                let next_st = tables.goto[&(top_st, prod.lhs.to_string())];
                states.push(next_st);
                vals.push(reduced);
            }
            Action::Accept => {
                let Some(SymVal::Stmts(stmts)) = vals.pop() else {
                    return Ok(AstProgram { stmts: Vec::new() });
                };
                return Ok(AstProgram { stmts });
            }
        }
    }
}

// ---------------------------------------------------------------------------
// 5. AST Visitor Pattern: Pretty Printer, Stats Counter, Semantic Checker
// ---------------------------------------------------------------------------

trait AstVisitor<T> {
    fn visit_stmt(&mut self, stmt: &AstStmt) -> T;
    fn visit_expr(&mut self, expr: &AstExpr) -> T;
}

struct PrettyVisitor;

impl AstVisitor<String> for PrettyVisitor {
    fn visit_stmt(&mut self, stmt: &AstStmt) -> String {
        match stmt {
            AstStmt::Let { name, expr, span } => format!(
                "(let {name} {} @L{}:C{})",
                self.visit_expr(expr),
                span.line,
                span.col
            ),
            AstStmt::Emit { expr, span } => {
                format!(
                    "(emit {} @L{}:C{})",
                    self.visit_expr(expr),
                    span.line,
                    span.col
                )
            }
        }
    }

    fn visit_expr(&mut self, expr: &AstExpr) -> String {
        match expr {
            AstExpr::Int { val, .. } => format!("{val}"),
            AstExpr::Var { name, .. } => name.clone(),
            AstExpr::Neg { inner, .. } => format!("(- {})", self.visit_expr(inner)),
            AstExpr::Bin {
                op, left, right, ..
            } => {
                format!(
                    "({op} {} {})",
                    self.visit_expr(left),
                    self.visit_expr(right)
                )
            }
        }
    }
}

struct AstStatsVisitor {
    stmt_count: usize,
    expr_count: usize,
}

impl AstStatsVisitor {
    fn new() -> Self {
        Self {
            stmt_count: 0,
            expr_count: 0,
        }
    }

    fn total_nodes(&self) -> usize {
        1 + self.stmt_count + self.expr_count
    }
}

impl AstVisitor<()> for AstStatsVisitor {
    fn visit_stmt(&mut self, stmt: &AstStmt) {
        self.stmt_count += 1;
        match stmt {
            AstStmt::Let { expr, .. } | AstStmt::Emit { expr, .. } => self.visit_expr(expr),
        }
    }

    fn visit_expr(&mut self, expr: &AstExpr) {
        self.expr_count += 1;
        match expr {
            AstExpr::Int { .. } | AstExpr::Var { .. } => {}
            AstExpr::Neg { inner, .. } => self.visit_expr(inner),
            AstExpr::Bin { left, right, .. } => {
                self.visit_expr(left);
                self.visit_expr(right);
            }
        }
    }
}

/// Chapter 5–6 Bridge: walks AST checking scopes and division-by-literal-zero,
/// reporting exact source-pinned `Diagnostic`s using each AST node's `Span`.
struct SemanticChecker {
    declared: BTreeSet<String>,
    errors: Vec<Diagnostic>,
}

impl SemanticChecker {
    fn new() -> Self {
        Self {
            declared: BTreeSet::new(),
            errors: Vec::new(),
        }
    }
}

impl AstVisitor<()> for SemanticChecker {
    fn visit_stmt(&mut self, stmt: &AstStmt) {
        match stmt {
            AstStmt::Let { name, expr, .. } => {
                self.visit_expr(expr);
                self.declared.insert(name.clone());
            }
            AstStmt::Emit { expr, .. } => self.visit_expr(expr),
        }
    }

    fn visit_expr(&mut self, expr: &AstExpr) {
        match expr {
            AstExpr::Int { .. } => {}
            AstExpr::Var { name, span } => {
                if !self.declared.contains(name) {
                    self.errors.push(Diagnostic {
                        span: *span,
                        message: format!("use of undeclared variable '{name}'"),
                    });
                }
            }
            AstExpr::Neg { inner, .. } => self.visit_expr(inner),
            AstExpr::Bin {
                op,
                left,
                right,
                span,
            } => {
                self.visit_expr(left);
                self.visit_expr(right);
                if *op == BinOp::Div && matches!(&**right, AstExpr::Int { val: 0, .. }) {
                    self.errors.push(Diagnostic {
                        span: *span,
                        message: "division by constant zero".into(),
                    });
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// 6. Chapter 6 Bridge: Lowering Checked AST -> Three-Address Code (TAC) + VM
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq)]
enum TacInst {
    Const {
        dst: String,
        val: i64,
    },
    Copy {
        dst: String,
        src: String,
    },
    Neg {
        dst: String,
        src: String,
    },
    Bin {
        dst: String,
        op: BinOp,
        left: String,
        right: String,
    },
    Emit {
        src: String,
    },
}

impl fmt::Display for TacInst {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TacInst::Const { dst, val } => write!(f, "{dst} := {val}"),
            TacInst::Copy { dst, src } => write!(f, "{dst} := {src}"),
            TacInst::Neg { dst, src } => write!(f, "{dst} := -{src}"),
            TacInst::Bin {
                dst,
                op,
                left,
                right,
            } => write!(f, "{dst} := {left} {op} {right}"),
            TacInst::Emit { src } => write!(f, "emit {src}"),
        }
    }
}

struct TacLowerer {
    next_temp: usize,
    code: Vec<TacInst>,
}

impl TacLowerer {
    fn new() -> Self {
        Self {
            next_temp: 0,
            code: Vec::new(),
        }
    }

    fn fresh(&mut self) -> String {
        let t = format!("t{}", self.next_temp);
        self.next_temp += 1;
        t
    }

    fn lower_program(&mut self, prog: &AstProgram) -> Vec<TacInst> {
        for s in &prog.stmts {
            self.visit_stmt(s);
        }
        self.code.clone()
    }
}

impl AstVisitor<String> for TacLowerer {
    fn visit_stmt(&mut self, stmt: &AstStmt) -> String {
        match stmt {
            AstStmt::Let { name, expr, .. } => {
                let rhs = self.visit_expr(expr);
                self.code.push(TacInst::Copy {
                    dst: name.clone(),
                    src: rhs,
                });
                name.clone()
            }
            AstStmt::Emit { expr, .. } => {
                let v = self.visit_expr(expr);
                self.code.push(TacInst::Emit { src: v.clone() });
                v
            }
        }
    }

    fn visit_expr(&mut self, expr: &AstExpr) -> String {
        match expr {
            AstExpr::Int { val, .. } => {
                let t = self.fresh();
                self.code.push(TacInst::Const {
                    dst: t.clone(),
                    val: *val,
                });
                t
            }
            AstExpr::Var { name, .. } => name.clone(),
            AstExpr::Neg { inner, .. } => {
                let v = self.visit_expr(inner);
                let t = self.fresh();
                self.code.push(TacInst::Neg {
                    dst: t.clone(),
                    src: v,
                });
                t
            }
            AstExpr::Bin {
                op, left, right, ..
            } => {
                let l = self.visit_expr(left);
                let r = self.visit_expr(right);
                let t = self.fresh();
                self.code.push(TacInst::Bin {
                    dst: t.clone(),
                    op: *op,
                    left: l,
                    right: r,
                });
                t
            }
        }
    }
}

fn eval_tac(code: &[TacInst]) -> Vec<i64> {
    let mut regs: BTreeMap<String, i64> = BTreeMap::new();
    let mut outputs = Vec::new();
    for inst in code {
        match inst {
            TacInst::Const { dst, val } => {
                regs.insert(dst.clone(), *val);
            }
            TacInst::Copy { dst, src } => {
                let v = regs[src];
                regs.insert(dst.clone(), v);
            }
            TacInst::Neg { dst, src } => {
                let v = -regs[src];
                regs.insert(dst.clone(), v);
            }
            TacInst::Bin {
                dst,
                op,
                left,
                right,
            } => {
                let l = regs[left];
                let r = regs[right];
                let v = match op {
                    BinOp::Add => l + r,
                    BinOp::Sub => l - r,
                    BinOp::Mul => l * r,
                    BinOp::Div => l / r,
                };
                regs.insert(dst.clone(), v);
            }
            TacInst::Emit { src } => outputs.push(regs[src]),
        }
    }
    outputs
}

fn main() {
    let src = "let base = (2 + 3) * 4;\nemit base + -1;\n";
    let tokens = lex(src).unwrap();

    // Path A: Recursive Descent produces BOTH Concrete Parse Tree & AST.
    let mut rd = RdParser::new(tokens.clone());
    let (cst, rd_ast) = rd.parse_program().unwrap();

    // Path B: Yacc LALR(1) semantic actions produce the IDENTICAL AST!
    let tables = build_yacc_ast_tables();
    let yacc_ast = parse_yacc_ast(&tables, &tokens).unwrap();
    assert_eq!(rd_ast, yacc_ast);

    let mut stats = AstStatsVisitor::new();
    let mut pretty = PrettyVisitor;
    let mut checker = SemanticChecker::new();
    for s in &rd_ast.stmts {
        stats.visit_stmt(s);
        checker.visit_stmt(s);
    }
    assert!(checker.errors.is_empty());

    let mut lowerer = TacLowerer::new();
    let tac = lowerer.lower_program(&rd_ast);
    let out = eval_tac(&tac);

    println!("=== AST Construction: Parse Tree vs. AST, Visitors & IR (§4.9 -> Ch. 5–6) ===");
    println!(
        "Node count: Parse tree (CST) = {} nodes  vs.  AST = {} nodes",
        cst_node_count(&cst),
        stats.total_nodes()
    );
    let mut cst_str = String::new();
    format_cst(&cst, 1, &mut cst_str);
    println!("\nConcrete Parse Tree (first 10 lines):");
    for line in cst_str.lines().take(10) {
        println!("{line}");
    }
    println!("\nAST via PrettyVisitor (RD == Yacc LALR(1) exact match, spans preserved):");
    for s in &yacc_ast.stmts {
        println!("  {}", pretty.visit_stmt(s));
    }
    println!("\nLowered Three-Address Code (TAC) -> output {out:?}:");
    for inst in &tac {
        println!("  {inst}");
    }

    let err_src = "let x = 10;\nemit x + missing;\n";
    let err_tokens = lex(err_src).unwrap();
    let err_ast = parse_yacc_ast(&tables, &err_tokens).unwrap();
    let mut err_check = SemanticChecker::new();
    for s in &err_ast.stmts {
        err_check.visit_stmt(s);
    }
    println!("\nSpan-pinned semantic error check:");
    for d in &err_check.errors {
        println!("{}", format_diagnostic(err_src, d));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rd_and_yacc_build_identical_ast_smaller_than_parse_tree() {
        let src = "let a = (1 + 2) * 3 - -4;\nemit a / 2 + 1;\n";
        let tokens = lex(src).unwrap();

        let mut rd = RdParser::new(tokens.clone());
        let (cst, rd_ast) = rd.parse_program().unwrap();

        let tables = build_yacc_ast_tables();
        let yacc_ast = parse_yacc_ast(&tables, &tokens).unwrap();
        assert_eq!(rd_ast, yacc_ast);

        let mut stats = AstStatsVisitor::new();
        let mut checker = SemanticChecker::new();
        for s in &rd_ast.stmts {
            stats.visit_stmt(s);
            checker.visit_stmt(s);
        }
        assert!(checker.errors.is_empty());
        assert!(cst_node_count(&cst) > stats.total_nodes() * 2);

        let tac = TacLowerer::new().lower_program(&rd_ast);
        // (1 + 2) * 3 - (-4) = 13; 13 / 2 + 1 = 7
        assert_eq!(eval_tac(&tac), vec![7]);
    }

    #[test]
    fn preserved_spans_pinpoint_semantic_errors_without_tokens() {
        let bad_src = "let x = 10;\nemit x + missing_var;\nemit 5 / 0;\n";
        let tokens = lex(bad_src).unwrap();
        let tables = build_yacc_ast_tables();
        let ast = parse_yacc_ast(&tables, &tokens).unwrap();

        let mut checker = SemanticChecker::new();
        for s in &ast.stmts {
            checker.visit_stmt(s);
        }
        assert_eq!(checker.errors.len(), 2);
        let msg0 = format_diagnostic(bad_src, &checker.errors[0]);
        let msg1 = format_diagnostic(bad_src, &checker.errors[1]);
        assert!(msg0.contains("line 2, col 10"));
        assert!(msg0.contains("missing_var"));
        assert!(msg1.contains("line 3, col 6"));
        assert!(msg1.contains("division by constant zero"));
    }

    #[test]
    fn visitor_pretty_print_reflects_associativity_and_precedence() {
        let src = "let z = 10 - 3 - 2 * 4;\n";
        let tokens = lex(src).unwrap();
        let tables = build_yacc_ast_tables();
        let ast = parse_yacc_ast(&tables, &tokens).unwrap();

        let mut pretty = PrettyVisitor;
        let rendered = pretty.visit_stmt(&ast.stmts[0]);
        assert_eq!(rendered, "(let z (- (- 10 3) (* 2 4)) @L1:C1)");

        let tac = TacLowerer::new().lower_program(&ast);
        assert!(tac.len() >= 4);
    }
}
