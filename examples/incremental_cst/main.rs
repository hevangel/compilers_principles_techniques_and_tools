//! Modern Extension: Concrete Syntax Trees (Green/Red) & Incremental Reparsing.
//!
//! Explicitly labeled as a modern IDE/formatter parsing technique complementing
//! Dragon Book Ch. 4–5:
//! 1. Lossless Green Tree storing every token including whitespace and comments (`trivia`),
//!    with relative byte widths and zero parent pointers / zero absolute offsets,
//! 2. On-demand Red Tree (`SyntaxNode`) cursors computing absolute `[start, end)` byte
//!    ranges dynamically without mutating or invalidating shared Green nodes,
//! 3. Incremental reparse reusing unaffected `Rc<GreenNode>` statement subtrees across
//!    source edits, verified by pointer identity (`Rc::ptr_eq`) and exact structural
//!    equivalence to a fresh full parse.

use std::fmt;
use std::rc::Rc;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SyntaxKind {
    // Trivia & tokens
    Whitespace,
    Comment,
    KwLet,
    KwReturn,
    Ident,
    Int,
    Eq,
    Plus,
    Star,
    Semi,
    LParen,
    RParen,
    ErrorTok,
    // Composite nodes
    Program,
    LetStmt,
    ReturnStmt,
    ExprStmt,
    BinExpr,
    ParenExpr,
    ErrorStmt,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct GreenToken {
    kind: SyntaxKind,
    text: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum GreenElement {
    Node(Rc<GreenNode>),
    Token(GreenToken),
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct GreenNode {
    kind: SyntaxKind,
    width: usize,
    children: Vec<GreenElement>,
}

impl GreenElement {
    fn width(&self) -> usize {
        match self {
            GreenElement::Node(n) => n.width,
            GreenElement::Token(t) => t.text.len(),
        }
    }

    fn write_text(&self, out: &mut String) {
        match self {
            GreenElement::Node(n) => n.write_text(out),
            GreenElement::Token(t) => out.push_str(&t.text),
        }
    }
}

impl GreenNode {
    fn new(kind: SyntaxKind, children: Vec<GreenElement>) -> Rc<Self> {
        let width = children.iter().map(GreenElement::width).sum();
        Rc::new(Self {
            kind,
            width,
            children,
        })
    }

    fn write_text(&self, out: &mut String) {
        for child in &self.children {
            child.write_text(out);
        }
    }

    /// Losslessly reconstructs exact source text (tokens + whitespace + comments).
    fn text(&self) -> String {
        let mut out = String::with_capacity(self.width);
        self.write_text(&mut out);
        out
    }
}

// ---------------------------------------------------------------------------
// On-demand Red Tree cursor computing absolute byte spans dynamically
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq)]
struct TextRange {
    start: usize,
    end: usize,
}

impl fmt::Display for TextRange {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}..{}", self.start, self.end)
    }
}

#[derive(Clone, Debug)]
struct RedNode {
    green: Rc<GreenNode>,
    offset: usize,
}

impl RedNode {
    fn root(green: Rc<GreenNode>) -> Self {
        Self { green, offset: 0 }
    }

    fn range(&self) -> TextRange {
        TextRange {
            start: self.offset,
            end: self.offset + self.green.width,
        }
    }

    fn child_nodes(&self) -> Vec<RedNode> {
        let mut cursor = self.offset;
        let mut out = Vec::new();
        for child in &self.green.children {
            let w = child.width();
            if let GreenElement::Node(node) = child {
                out.push(RedNode {
                    green: Rc::clone(node),
                    offset: cursor,
                });
            }
            cursor += w;
        }
        out
    }
}

// ---------------------------------------------------------------------------
// Lossless Lexer (emits EVERY byte as a token, including trivia)
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
struct RawToken {
    kind: SyntaxKind,
    text: String,
    start: usize,
}

fn tokenize_lossless(src: &str) -> Vec<RawToken> {
    let bytes = src.as_bytes();
    let mut i = 0;
    let mut out = Vec::new();
    while i < bytes.len() {
        let start = i;
        if bytes[i].is_ascii_whitespace() {
            i += 1;
            while i < bytes.len() && bytes[i].is_ascii_whitespace() {
                i += 1;
            }
            out.push(RawToken {
                kind: SyntaxKind::Whitespace,
                text: src[start..i].into(),
                start,
            });
            continue;
        }
        if bytes[i] == b'/' && i + 1 < bytes.len() && bytes[i + 1] == b'/' {
            i += 2;
            while i < bytes.len() && bytes[i] != b'\n' {
                i += 1;
            }
            out.push(RawToken {
                kind: SyntaxKind::Comment,
                text: src[start..i].into(),
                start,
            });
            continue;
        }
        if bytes[i].is_ascii_digit() {
            i += 1;
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                i += 1;
            }
            out.push(RawToken {
                kind: SyntaxKind::Int,
                text: src[start..i].into(),
                start,
            });
            continue;
        }
        if bytes[i].is_ascii_alphabetic() || bytes[i] == b'_' {
            i += 1;
            while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                i += 1;
            }
            let lex = &src[start..i];
            let kind = match lex {
                "let" => SyntaxKind::KwLet,
                "return" => SyntaxKind::KwReturn,
                _ => SyntaxKind::Ident,
            };
            out.push(RawToken {
                kind,
                text: lex.into(),
                start,
            });
            continue;
        }
        let kind = match bytes[i] {
            b'=' => SyntaxKind::Eq,
            b'+' => SyntaxKind::Plus,
            b'*' => SyntaxKind::Star,
            b';' => SyntaxKind::Semi,
            b'(' => SyntaxKind::LParen,
            b')' => SyntaxKind::RParen,
            _ => SyntaxKind::ErrorTok,
        };
        i += 1;
        out.push(RawToken {
            kind,
            text: src[start..i].into(),
            start,
        });
    }
    out
}

// ---------------------------------------------------------------------------
// CST Parser emitting statement green nodes with preserved trivia
// ---------------------------------------------------------------------------

struct CstBuilder {
    tokens: Vec<RawToken>,
    pos: usize,
}

impl CstBuilder {
    fn new(src: &str, start_offset: usize) -> Self {
        Self {
            tokens: tokenize_lossless(&src[start_offset..])
                .into_iter()
                .map(|mut t| {
                    t.start += start_offset;
                    t
                })
                .collect(),
            pos: 0,
        }
    }

    fn is_trivia(k: SyntaxKind) -> bool {
        matches!(k, SyntaxKind::Whitespace | SyntaxKind::Comment)
    }

    fn at_eof(&self) -> bool {
        self.pos >= self.tokens.len()
    }

    fn current_byte_offset(&self, end_offset: usize) -> usize {
        self.tokens
            .get(self.pos)
            .map(|t| t.start)
            .unwrap_or(end_offset)
    }

    fn peek_significant(&self) -> Option<SyntaxKind> {
        let mut p = self.pos;
        while p < self.tokens.len() && Self::is_trivia(self.tokens[p].kind) {
            p += 1;
        }
        self.tokens.get(p).map(|t| t.kind)
    }

    fn eat_trivia(&mut self, sink: &mut Vec<GreenElement>) {
        while self.pos < self.tokens.len() && Self::is_trivia(self.tokens[self.pos].kind) {
            let t = self.tokens[self.pos].clone();
            sink.push(GreenElement::Token(GreenToken {
                kind: t.kind,
                text: t.text,
            }));
            self.pos += 1;
        }
    }

    fn bump_token(&mut self, sink: &mut Vec<GreenElement>) -> SyntaxKind {
        let t = self.tokens[self.pos].clone();
        self.pos += 1;
        let kind = t.kind;
        sink.push(GreenElement::Token(GreenToken { kind, text: t.text }));
        kind
    }

    fn parse_atom(&mut self) -> GreenElement {
        let mut pre_trivia = Vec::new();
        self.eat_trivia(&mut pre_trivia);
        match self.tokens.get(self.pos).map(|t| t.kind) {
            Some(SyntaxKind::LParen) => {
                let mut children = pre_trivia;
                self.bump_token(&mut children);
                children.push(self.parse_expr_min(0));
                self.eat_trivia(&mut children);
                if self.tokens.get(self.pos).map(|t| t.kind) == Some(SyntaxKind::RParen) {
                    self.bump_token(&mut children);
                }
                GreenElement::Node(GreenNode::new(SyntaxKind::ParenExpr, children))
            }
            Some(SyntaxKind::Int) | Some(SyntaxKind::Ident) => {
                if pre_trivia.is_empty() {
                    let t = self.tokens[self.pos].clone();
                    self.pos += 1;
                    GreenElement::Token(GreenToken {
                        kind: t.kind,
                        text: t.text,
                    })
                } else {
                    self.bump_token(&mut pre_trivia);
                    GreenElement::Node(GreenNode::new(SyntaxKind::ParenExpr, pre_trivia))
                }
            }
            _ => {
                if !self.at_eof() {
                    self.bump_token(&mut pre_trivia);
                }
                GreenElement::Node(GreenNode::new(SyntaxKind::ErrorStmt, pre_trivia))
            }
        }
    }

    fn parse_expr_min(&mut self, min_prec: u8) -> GreenElement {
        let mut lhs = self.parse_atom();
        while let Some(op_kind) = self.peek_significant() {
            let prec = match op_kind {
                SyntaxKind::Plus => 1,
                SyntaxKind::Star => 2,
                _ => break,
            };
            if prec < min_prec {
                break;
            }
            let mut children = vec![lhs];
            self.eat_trivia(&mut children);
            self.bump_token(&mut children);
            children.push(self.parse_expr_min(prec + 1));
            lhs = GreenElement::Node(GreenNode::new(SyntaxKind::BinExpr, children));
        }
        lhs
    }

    fn parse_one_stmt(&mut self) -> Option<Rc<GreenNode>> {
        if self.at_eof() {
            return None;
        }
        let mut children = Vec::new();
        self.eat_trivia(&mut children);
        if self.at_eof() {
            // Trailing trivia attached cleanly to a trailing ErrorStmt/Program? Keep as root trivia if no stmt!
            return None;
        }
        let kind = match self.tokens[self.pos].kind {
            SyntaxKind::KwLet => {
                self.bump_token(&mut children);
                self.eat_trivia(&mut children);
                if matches!(
                    self.tokens.get(self.pos).map(|t| t.kind),
                    Some(SyntaxKind::Ident)
                ) {
                    self.bump_token(&mut children);
                }
                self.eat_trivia(&mut children);
                if matches!(
                    self.tokens.get(self.pos).map(|t| t.kind),
                    Some(SyntaxKind::Eq)
                ) {
                    self.bump_token(&mut children);
                    children.push(self.parse_expr_min(0));
                }
                SyntaxKind::LetStmt
            }
            SyntaxKind::KwReturn => {
                self.bump_token(&mut children);
                if self.peek_significant() != Some(SyntaxKind::Semi) {
                    children.push(self.parse_expr_min(0));
                }
                SyntaxKind::ReturnStmt
            }
            _ => {
                children.push(self.parse_expr_min(0));
                SyntaxKind::ExprStmt
            }
        };
        // Consume up to terminating semicolon cleanly.
        self.eat_trivia(&mut children);
        if self.tokens.get(self.pos).map(|t| t.kind) == Some(SyntaxKind::Semi) {
            self.bump_token(&mut children);
        } else {
            while !self.at_eof() && self.tokens[self.pos].kind != SyntaxKind::Semi {
                self.bump_token(&mut children);
            }
            if self.tokens.get(self.pos).map(|t| t.kind) == Some(SyntaxKind::Semi) {
                self.bump_token(&mut children);
            }
        }
        // Attach horizontal trailing spaces/comments up to and including a single newline on the statement.
        while self.pos < self.tokens.len() {
            let tok = &self.tokens[self.pos];
            if tok.kind == SyntaxKind::Comment
                || (tok.kind == SyntaxKind::Whitespace && !tok.text.contains('\n'))
            {
                self.bump_token(&mut children);
            } else {
                break;
            }
        }
        if self.pos < self.tokens.len() && self.tokens[self.pos].kind == SyntaxKind::Whitespace {
            let ws = &self.tokens[self.pos];
            if let Some(nl) = ws.text.find('\n') {
                let split = nl + 1;
                let head = ws.text[..split].to_string();
                let tail = ws.text[split..].to_string();
                children.push(GreenElement::Token(GreenToken {
                    kind: SyntaxKind::Whitespace,
                    text: head,
                }));
                if tail.is_empty() {
                    self.pos += 1;
                } else {
                    self.tokens[self.pos].start += split;
                    self.tokens[self.pos].text = tail;
                }
            }
        }
        Some(GreenNode::new(kind, children))
    }
}

fn parse_cst_full(src: &str) -> Rc<GreenNode> {
    let mut builder = CstBuilder::new(src, 0);
    let mut children = Vec::new();
    while !builder.at_eof() {
        let start_pos = builder.pos;
        if let Some(stmt) = builder.parse_one_stmt() {
            children.push(GreenElement::Node(stmt));
        } else {
            // Trailing trivia only.
            for t in &builder.tokens[start_pos..] {
                children.push(GreenElement::Token(GreenToken {
                    kind: t.kind,
                    text: t.text.clone(),
                }));
            }
            break;
        }
    }
    GreenNode::new(SyntaxKind::Program, children)
}

#[derive(Debug)]
struct IncrementalStats {
    tree: Rc<GreenNode>,
    reused_stmts: usize,
    reparsed_stmts: usize,
}

/// Applies a text replacement `[edit_start .. edit_start + del_len]` -> `replacement`
/// and incrementally reparses while reusing unaffected Green statement subtrees.
fn incremental_reparse(
    old_root: &Rc<GreenNode>,
    old_src: &str,
    edit_start: usize,
    del_len: usize,
    replacement: &str,
) -> (String, IncrementalStats) {
    let mut new_src = String::with_capacity(old_src.len() - del_len + replacement.len());
    new_src.push_str(&old_src[..edit_start]);
    new_src.push_str(replacement);
    new_src.push_str(&old_src[edit_start + del_len..]);

    let edit_old_end = edit_start + del_len;
    let delta = replacement.len() as isize - del_len as isize;

    // Gather old top-level statements with their old byte ranges.
    let mut old_stmts: Vec<(usize, usize, Rc<GreenNode>)> = Vec::new();
    let mut old_cursor = 0;
    for child in &old_root.children {
        let w = child.width();
        if let GreenElement::Node(stmt) = child {
            old_stmts.push((old_cursor, old_cursor + w, Rc::clone(stmt)));
        }
        old_cursor += w;
    }

    let mut new_children = Vec::new();
    let mut reused = 0usize;
    let mut reparsed = 0usize;
    let mut new_pos = 0usize;
    let mut old_idx = 0usize;

    // 1. Reuse prefix statements ending strictly before `edit_start`.
    // Keep 1 byte margin so appending adjacent tokens at the end of a statement still triggers reparse.
    while old_idx < old_stmts.len() {
        let (os, oe, ref node) = old_stmts[old_idx];
        if oe < edit_start && new_pos == os && new_src[new_pos..].starts_with(&old_src[os..oe]) {
            new_children.push(GreenElement::Node(Rc::clone(node)));
            new_pos = oe;
            reused += 1;
            old_idx += 1;
        } else {
            break;
        }
    }

    // Determine suffix threshold in new_src where old statements cleanly resume.
    while old_idx < old_stmts.len() && old_stmts[old_idx].0 <= edit_old_end {
        old_idx += 1;
    }
    let suffix_resume_new = old_stmts
        .get(old_idx)
        .map(|(os, _, _)| (*os as isize + delta) as usize)
        .unwrap_or(new_src.len());

    // 2. Reparse only the affected window `[new_pos .. suffix_resume_new]` (stopping cleanly if
    //    statement boundaries extend past `suffix_resume_new`).
    while new_pos < suffix_resume_new {
        let mut builder = CstBuilder::new(&new_src, new_pos);
        if let Some(stmt) = builder.parse_one_stmt() {
            let consumed_end = builder.current_byte_offset(new_src.len());
            new_children.push(GreenElement::Node(stmt));
            new_pos = consumed_end;
            reparsed += 1;
        } else {
            for t in &builder.tokens {
                new_children.push(GreenElement::Token(GreenToken {
                    kind: t.kind,
                    text: t.text.clone(),
                }));
            }
            new_pos = new_src.len();
            break;
        }
    }

    // 3. Advance `old_idx` past any statement eclipsed by `new_pos`, then reuse suffix subtrees.
    while old_idx < old_stmts.len() {
        let (os, oe, ref node) = old_stmts[old_idx];
        let shifted_start = (os as isize + delta) as usize;
        if shifted_start < new_pos {
            old_idx += 1;
            continue;
        }
        if shifted_start == new_pos && new_src[new_pos..].starts_with(&old_src[os..oe]) && oe > os {
            new_children.push(GreenElement::Node(Rc::clone(node)));
            new_pos += oe - os;
            reused += 1;
            old_idx += 1;
        } else {
            break;
        }
    }

    // 4. Parse any remaining tail if alignment diverged.
    if new_pos < new_src.len() {
        let mut builder = CstBuilder::new(&new_src, new_pos);
        while !builder.at_eof() {
            let start_p = builder.pos;
            if let Some(stmt) = builder.parse_one_stmt() {
                new_children.push(GreenElement::Node(stmt));
                reparsed += 1;
            } else {
                for t in &builder.tokens[start_p..] {
                    new_children.push(GreenElement::Token(GreenToken {
                        kind: t.kind,
                        text: t.text.clone(),
                    }));
                }
                break;
            }
        }
    }

    (
        new_src,
        IncrementalStats {
            tree: GreenNode::new(SyntaxKind::Program, new_children),
            reused_stmts: reused,
            reparsed_stmts: reparsed,
        },
    )
}

fn main() {
    println!("=== Advanced Parsing (Modern): Concrete Syntax Trees & Incremental Reparsing ===");
    let src = "\
// Header comment
let x = 1 + 2 * 3;
let y = (4 + 5);
return x + y;
";
    let old_tree = parse_cst_full(src);
    assert_eq!(old_tree.text(), src);
    println!(
        "Original lossless round-trip matches: {}",
        old_tree.text() == src
    );

    // Edit middle statement: `let y = (4 + 5);` -> `let y = 100 + 200;`
    let needle = "(4 + 5)";
    let edit_start = src.find(needle).unwrap();
    let (new_src, stats) =
        incremental_reparse(&old_tree, src, edit_start, needle.len(), "100 + 200");
    let fresh = parse_cst_full(&new_src);
    assert_eq!(stats.tree, fresh);
    println!(
        "Incremental edit stats: reused_stmts={}, reparsed_stmts={}, lossless={}",
        stats.reused_stmts,
        stats.reparsed_stmts,
        stats.tree.text() == new_src
    );

    let old_red = RedNode::root(Rc::clone(&old_tree)).child_nodes();
    let new_red = RedNode::root(Rc::clone(&stats.tree)).child_nodes();
    for (idx, (o, n)) in old_red.iter().zip(new_red.iter()).enumerate() {
        println!(
            "  stmt[{idx}] {:?} old={} new={} ptr_reused={}",
            n.green.kind,
            o.range(),
            n.range(),
            Rc::ptr_eq(&o.green, &n.green)
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lossless_green_tree_preserves_comments_and_whitespace_verbatim() {
        let src = "  // leading trivia\nlet a = 10 + 20 * 30;  // inline\nreturn a;\n";
        let green = parse_cst_full(src);
        assert_eq!(green.text(), src);
        let red = RedNode::root(green);
        let stmts = red.child_nodes();
        assert_eq!(stmts.len(), 2);
        assert_eq!(stmts[0].green.kind, SyntaxKind::LetStmt);
        assert_eq!(stmts[1].green.kind, SyntaxKind::ReturnStmt);
        assert_eq!(stmts[1].range().end, src.len());
    }

    #[test]
    fn incremental_reparse_reuses_prefix_and_suffix_via_pointer_identity() {
        let src = "let a = 1;\nlet b = 2 + 3;\nreturn a + b;\n";
        let old_tree = parse_cst_full(src);
        let edit_pos = src.find("2 + 3").unwrap();
        let (new_src, stats) = incremental_reparse(&old_tree, src, edit_pos, 5, "200 * 300");

        let fresh = parse_cst_full(&new_src);
        assert_eq!(stats.tree, fresh);
        assert_eq!(stats.tree.text(), new_src);
        assert_eq!(stats.reused_stmts, 2);
        assert_eq!(stats.reparsed_stmts, 1);

        let old_stmts = RedNode::root(old_tree).child_nodes();
        let new_stmts = RedNode::root(stats.tree).child_nodes();
        // Statement 0 and Statement 2 physically share the exact same Rc<GreenNode>.
        assert!(Rc::ptr_eq(&old_stmts[0].green, &new_stmts[0].green));
        assert!(!Rc::ptr_eq(&old_stmts[1].green, &new_stmts[1].green));
        assert!(Rc::ptr_eq(&old_stmts[2].green, &new_stmts[2].green));
        // Red Tree range of statement 2 shifted cleanly by +4 bytes without modifying its GreenNode.
        assert_eq!(old_stmts[2].range().start + 4, new_stmts[2].range().start);
    }

    #[test]
    fn editing_first_statement_still_reuses_downstream_statements() {
        let src = "let first = 1;\nlet second = 2;\nreturn first + second;\n";
        let old_tree = parse_cst_full(src);
        let pos = src.find('1').unwrap();
        let (new_src, stats) = incremental_reparse(&old_tree, src, pos, 1, "9999");

        assert_eq!(stats.tree, parse_cst_full(&new_src));
        assert_eq!(stats.reused_stmts, 2);
        assert_eq!(stats.reparsed_stmts, 1);
    }
}
