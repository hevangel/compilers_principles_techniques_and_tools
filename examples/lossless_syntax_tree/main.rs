//! Modern Extension: Lossless Immutable Syntax Trees (Rowan-Style Green/Red Architecture).
//!
//! Complements Dragon Book §2.2.3 (Parse Trees), §2.5.1 (CST vs. AST), and §2.6.1
//! (where traditional lexers strip whitespace and comments):
//! 1. Preserves every byte of trivia (`Whitespace`, `Newline`, `LineComment`, `BlockComment`),
//!    exact token spellings (`0x2A` vs `42`), and `Error` recovery nodes,
//! 2. Interns immutable `GreenToken` / `GreenNode` subtrees in a `NodeCache` (Rowan pattern),
//! 3. Computes absolute byte `TextSpan`s and `(line, col)` coordinates lazily via parent-linked
//!    `SyntaxNode` (Red) cursors,
//! 4. Exposes typed semantic views (`LetDeclView`) ignoring trivia while supporting functional
//!    edits that preserve every comment and double-space byte-for-byte.

use std::collections::HashMap;
use std::fmt;
use std::rc::Rc;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum SyntaxKind {
    // Trivia
    Whitespace,
    Newline,
    LineComment,
    BlockComment,
    // Punctuation & keywords
    KwLet,
    Identifier,
    Equals,
    Number,
    Semicolon,
    Plus,
    Star,
    LParen,
    RParen,
    ErrorTok,
    // Composite nodes
    SourceFile,
    LetDeclaration,
    ExprStatement,
    BinaryExpr,
    ParenExpr,
    ErrorNode,
}

impl SyntaxKind {
    fn is_trivia(self) -> bool {
        matches!(
            self,
            SyntaxKind::Whitespace
                | SyntaxKind::Newline
                | SyntaxKind::LineComment
                | SyntaxKind::BlockComment
        )
    }

    fn label(self) -> &'static str {
        match self {
            SyntaxKind::Whitespace => "Whitespace",
            SyntaxKind::Newline => "Newline",
            SyntaxKind::LineComment => "Comment",
            SyntaxKind::BlockComment => "BlockComment",
            SyntaxKind::KwLet => "Keyword",
            SyntaxKind::Identifier => "Identifier",
            SyntaxKind::Equals => "Equals",
            SyntaxKind::Number => "Number",
            SyntaxKind::Semicolon => "Semicolon",
            SyntaxKind::Plus => "Plus",
            SyntaxKind::Star => "Star",
            SyntaxKind::LParen => "LParen",
            SyntaxKind::RParen => "RParen",
            SyntaxKind::ErrorTok => "ErrorToken",
            SyntaxKind::SourceFile => "SourceFile",
            SyntaxKind::LetDeclaration => "LetDeclaration",
            SyntaxKind::ExprStatement => "ExprStatement",
            SyntaxKind::BinaryExpr => "BinaryExpr",
            SyntaxKind::ParenExpr => "ParenExpr",
            SyntaxKind::ErrorNode => "ErrorNode",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct GreenToken {
    kind: SyntaxKind,
    text: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum GreenElement {
    Node(Rc<GreenNode>),
    Token(Rc<GreenToken>),
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

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct GreenNode {
    kind: SyntaxKind,
    width: usize,
    children: Vec<GreenElement>,
}

impl GreenNode {
    fn write_text(&self, out: &mut String) {
        for c in &self.children {
            c.write_text(out);
        }
    }

    fn text(&self) -> String {
        let mut out = String::with_capacity(self.width);
        self.write_text(&mut out);
        out
    }

    fn render_tree(&self) -> String {
        let mut lines = Vec::new();
        self.fmt_tree("", true, true, &mut lines);
        lines.join("\n")
    }

    fn fmt_tree(&self, prefix: &str, is_last: bool, is_root: bool, out: &mut Vec<String>) {
        if is_root {
            out.push(self.kind.label().to_string());
        } else {
            let branch = if is_last { "└── " } else { "├── " };
            out.push(format!("{prefix}{branch}{}", self.kind.label()));
        }
        let next = if is_root {
            String::new()
        } else if is_last {
            format!("{prefix}    ")
        } else {
            format!("{prefix}│   ")
        };
        for (idx, child) in self.children.iter().enumerate() {
            let last = idx + 1 == self.children.len();
            match child {
                GreenElement::Node(n) => n.fmt_tree(&next, last, false, out),
                GreenElement::Token(t) => {
                    let branch = if last { "└── " } else { "├── " };
                    out.push(format!("{next}{branch}{}({:?})", t.kind.label(), t.text));
                }
            }
        }
    }
}

/// Deduplicates identical immutable `GreenToken`s and `GreenNode`s (Rowan `NodeCache`).
#[derive(Default)]
struct NodeCache {
    tokens: HashMap<(SyntaxKind, String), Rc<GreenToken>>,
    nodes: HashMap<(SyntaxKind, Vec<GreenElement>), Rc<GreenNode>>,
    token_hits: usize,
    node_hits: usize,
}

impl NodeCache {
    fn token(&mut self, kind: SyntaxKind, text: &str) -> Rc<GreenToken> {
        let key = (kind, text.to_string());
        if let Some(existing) = self.tokens.get(&key) {
            self.token_hits += 1;
            return Rc::clone(existing);
        }
        let tok = Rc::new(GreenToken {
            kind,
            text: text.to_string(),
        });
        self.tokens.insert(key, Rc::clone(&tok));
        tok
    }

    fn node(&mut self, kind: SyntaxKind, children: Vec<GreenElement>) -> Rc<GreenNode> {
        let key = (kind, children);
        if let Some(existing) = self.nodes.get(&key) {
            self.node_hits += 1;
            return Rc::clone(existing);
        }
        let width = key.1.iter().map(GreenElement::width).sum();
        let node = Rc::new(GreenNode {
            kind: key.0,
            width,
            children: key.1.clone(),
        });
        self.nodes.insert(key, Rc::clone(&node));
        node
    }
}

// ---------------------------------------------------------------------------
// Lossless Lexer (emits EVERY byte including trivia & invalid error tokens)
// ---------------------------------------------------------------------------

fn lex_lossless(src: &str, cache: &mut NodeCache) -> Vec<Rc<GreenToken>> {
    let bytes = src.as_bytes();
    let mut i = 0;
    let mut out = Vec::new();

    while i < bytes.len() {
        let start = i;
        let b = bytes[i];
        if b == b'\r' || b == b'\n' {
            if b == b'\r' && i + 1 < bytes.len() && bytes[i + 1] == b'\n' {
                i += 2;
            } else {
                i += 1;
            }
            out.push(cache.token(SyntaxKind::Newline, &src[start..i]));
            continue;
        }
        if b == b' ' || b == b'\t' {
            while i < bytes.len() && (bytes[i] == b' ' || bytes[i] == b'\t') {
                i += 1;
            }
            out.push(cache.token(SyntaxKind::Whitespace, &src[start..i]));
            continue;
        }
        if i + 1 < bytes.len() && b == b'/' && bytes[i + 1] == b'/' {
            i += 2;
            while i < bytes.len() && bytes[i] != b'\r' && bytes[i] != b'\n' {
                i += 1;
            }
            out.push(cache.token(SyntaxKind::LineComment, &src[start..i]));
            continue;
        }
        if i + 1 < bytes.len() && b == b'/' && bytes[i + 1] == b'*' {
            i += 2;
            while i + 1 < bytes.len() && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
                i += 1;
            }
            if i + 1 < bytes.len() {
                i += 2;
            } else {
                i = bytes.len();
            }
            out.push(cache.token(SyntaxKind::BlockComment, &src[start..i]));
            continue;
        }
        if b.is_ascii_digit() {
            if b == b'0' && i + 1 < bytes.len() && (bytes[i + 1] == b'x' || bytes[i + 1] == b'X') {
                i += 2;
                while i < bytes.len() && (bytes[i].is_ascii_hexdigit() || bytes[i] == b'_') {
                    i += 1;
                }
            } else {
                while i < bytes.len() && (bytes[i].is_ascii_digit() || bytes[i] == b'_') {
                    i += 1;
                }
            }
            out.push(cache.token(SyntaxKind::Number, &src[start..i]));
            continue;
        }
        if b.is_ascii_alphabetic() || b == b'_' {
            while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                i += 1;
            }
            let lex = &src[start..i];
            let kind = if lex == "let" {
                SyntaxKind::KwLet
            } else {
                SyntaxKind::Identifier
            };
            out.push(cache.token(kind, lex));
            continue;
        }
        let kind = match b {
            b'=' => SyntaxKind::Equals,
            b'+' => SyntaxKind::Plus,
            b'*' => SyntaxKind::Star,
            b'(' => SyntaxKind::LParen,
            b')' => SyntaxKind::RParen,
            b';' => SyntaxKind::Semicolon,
            _ => SyntaxKind::ErrorTok,
        };
        let char_len = src[start..].chars().next().map(char::len_utf8).unwrap_or(1);
        i += char_len;
        out.push(cache.token(kind, &src[start..i]));
    }

    out
}

// ---------------------------------------------------------------------------
// Lossless Parser (keeps every token & trivia inside immutable GreenNodes)
// ---------------------------------------------------------------------------

/// Builds a lossless Green syntax tree preserving every byte of source text.
fn build_lossless_tree(src: &str, cache: &mut NodeCache) -> Rc<GreenNode> {
    let tokens = lex_lossless(src, cache);
    LosslessParser::new(tokens, cache).parse_source_file()
}

struct LosslessParser<'a> {
    tokens: Vec<Rc<GreenToken>>,
    pos: usize,
    cache: &'a mut NodeCache,
}

impl<'a> LosslessParser<'a> {
    fn new(tokens: Vec<Rc<GreenToken>>, cache: &'a mut NodeCache) -> Self {
        Self {
            tokens,
            pos: 0,
            cache,
        }
    }

    fn peek_significant(&self) -> Option<SyntaxKind> {
        let mut idx = self.pos;
        while idx < self.tokens.len() {
            let k = self.tokens[idx].kind;
            if !k.is_trivia() {
                return Some(k);
            }
            idx += 1;
        }
        None
    }

    fn eat_all_trivia(&mut self, into: &mut Vec<GreenElement>) {
        while self.pos < self.tokens.len() && self.tokens[self.pos].kind.is_trivia() {
            into.push(GreenElement::Token(Rc::clone(&self.tokens[self.pos])));
            self.pos += 1;
        }
    }

    fn eat_inline_trivia(&mut self, into: &mut Vec<GreenElement>) {
        while self.pos < self.tokens.len()
            && matches!(
                self.tokens[self.pos].kind,
                SyntaxKind::Whitespace | SyntaxKind::LineComment | SyntaxKind::BlockComment
            )
        {
            into.push(GreenElement::Token(Rc::clone(&self.tokens[self.pos])));
            self.pos += 1;
        }
    }

    fn bump_tok(&mut self, into: &mut Vec<GreenElement>) -> SyntaxKind {
        self.eat_all_trivia(into);
        let tok = Rc::clone(&self.tokens[self.pos]);
        let kind = tok.kind;
        self.pos += 1;
        into.push(GreenElement::Token(tok));
        kind
    }

    fn expect_tok(&mut self, kind: SyntaxKind, into: &mut Vec<GreenElement>) -> bool {
        if self.peek_significant() == Some(kind) {
            self.bump_tok(into);
            true
        } else {
            false
        }
    }

    fn parse_source_file(&mut self) -> Rc<GreenNode> {
        let mut children = Vec::new();
        while self.peek_significant().is_some() {
            self.eat_all_trivia(&mut children);
            children.push(GreenElement::Node(self.parse_stmt()));
        }
        self.eat_all_trivia(&mut children);
        self.cache.node(SyntaxKind::SourceFile, children)
    }

    fn parse_stmt(&mut self) -> Rc<GreenNode> {
        let mut children = Vec::new();
        if self.peek_significant() == Some(SyntaxKind::KwLet) {
            self.bump_tok(&mut children);
            self.expect_tok(SyntaxKind::Identifier, &mut children);
            self.expect_tok(SyntaxKind::Equals, &mut children);
            self.eat_all_trivia(&mut children);
            if let Some(expr) = self.parse_expr() {
                children.push(expr);
            } else {
                self.recover_until_semi(&mut children);
            }
            self.expect_tok(SyntaxKind::Semicolon, &mut children);
            self.eat_inline_trivia(&mut children);
            self.cache.node(SyntaxKind::LetDeclaration, children)
        } else if let Some(expr) = self.parse_expr() {
            children.push(expr);
            self.expect_tok(SyntaxKind::Semicolon, &mut children);
            self.eat_inline_trivia(&mut children);
            self.cache.node(SyntaxKind::ExprStatement, children)
        } else {
            self.recover_until_semi(&mut children);
            self.expect_tok(SyntaxKind::Semicolon, &mut children);
            self.eat_inline_trivia(&mut children);
            self.cache.node(SyntaxKind::ErrorNode, children)
        }
    }

    fn recover_until_semi(&mut self, into: &mut Vec<GreenElement>) {
        while self.pos < self.tokens.len() {
            let k = self.tokens[self.pos].kind;
            if matches!(k, SyntaxKind::Semicolon | SyntaxKind::KwLet) {
                break;
            }
            into.push(GreenElement::Token(Rc::clone(&self.tokens[self.pos])));
            self.pos += 1;
        }
    }

    fn parse_expr(&mut self) -> Option<GreenElement> {
        let mut lhs = self.parse_term()?;
        while self.peek_significant() == Some(SyntaxKind::Plus) {
            let mut ch = vec![lhs];
            self.bump_tok(&mut ch);
            self.eat_all_trivia(&mut ch);
            if let Some(rhs) = self.parse_term() {
                ch.push(rhs);
            }
            lhs = GreenElement::Node(self.cache.node(SyntaxKind::BinaryExpr, ch));
        }
        Some(lhs)
    }

    fn parse_term(&mut self) -> Option<GreenElement> {
        let mut lhs = self.parse_atom()?;
        while self.peek_significant() == Some(SyntaxKind::Star) {
            let mut ch = vec![lhs];
            self.bump_tok(&mut ch);
            self.eat_all_trivia(&mut ch);
            if let Some(rhs) = self.parse_atom() {
                ch.push(rhs);
            }
            lhs = GreenElement::Node(self.cache.node(SyntaxKind::BinaryExpr, ch));
        }
        Some(lhs)
    }

    fn parse_atom(&mut self) -> Option<GreenElement> {
        match self.peek_significant()? {
            SyntaxKind::Number | SyntaxKind::Identifier => {
                // Pre-check ensures trivia stays outside when caller called `eat_all_trivia`,
                // and `tok` follows immediately.
                let mut ch = Vec::new();
                self.eat_all_trivia(&mut ch);
                let tok = Rc::clone(&self.tokens[self.pos]);
                self.pos += 1;
                if ch.is_empty() {
                    Some(GreenElement::Token(tok))
                } else {
                    ch.push(GreenElement::Token(tok));
                    Some(GreenElement::Node(
                        self.cache.node(SyntaxKind::BinaryExpr, ch),
                    ))
                }
            }
            SyntaxKind::LParen => {
                let mut ch = Vec::new();
                self.bump_tok(&mut ch);
                self.eat_all_trivia(&mut ch);
                if let Some(inner) = self.parse_expr() {
                    ch.push(inner);
                }
                self.expect_tok(SyntaxKind::RParen, &mut ch);
                Some(GreenElement::Node(
                    self.cache.node(SyntaxKind::ParenExpr, ch),
                ))
            }
            _ => None,
        }
    }
}

// ---------------------------------------------------------------------------
// Red Tree (`SyntaxNode` cursor): parent links, byte spans, & line/col
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct TextSpan {
    start: usize,
    end: usize,
}

impl fmt::Display for TextSpan {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}..{}", self.start, self.end)
    }
}

#[derive(Clone, Debug)]
struct SyntaxNode {
    green: Rc<GreenNode>,
    offset: usize,
    parent: Option<Rc<SyntaxNode>>,
}

#[derive(Clone, Debug)]
enum SyntaxElement {
    Node(Rc<SyntaxNode>),
    Token {
        green: Rc<GreenToken>,
        span: TextSpan,
    },
}

impl SyntaxNode {
    fn root(green: Rc<GreenNode>) -> Rc<Self> {
        Rc::new(Self {
            green,
            offset: 0,
            parent: None,
        })
    }

    fn span(&self) -> TextSpan {
        TextSpan {
            start: self.offset,
            end: self.offset + self.green.width,
        }
    }

    fn children_with_tokens(self: &Rc<Self>) -> Vec<SyntaxElement> {
        let mut cursor = self.offset;
        let mut out = Vec::with_capacity(self.green.children.len());
        for child in &self.green.children {
            let w = child.width();
            match child {
                GreenElement::Node(n) => {
                    out.push(SyntaxElement::Node(Rc::new(SyntaxNode {
                        green: Rc::clone(n),
                        offset: cursor,
                        parent: Some(Rc::clone(self)),
                    })));
                }
                GreenElement::Token(t) => {
                    out.push(SyntaxElement::Token {
                        green: Rc::clone(t),
                        span: TextSpan {
                            start: cursor,
                            end: cursor + w,
                        },
                    });
                }
            }
            cursor += w;
        }
        out
    }

    fn child_nodes(self: &Rc<Self>) -> Vec<Rc<SyntaxNode>> {
        self.children_with_tokens()
            .into_iter()
            .filter_map(|e| match e {
                SyntaxElement::Node(n) => Some(n),
                SyntaxElement::Token { .. } => None,
            })
            .collect()
    }

    fn ancestors(self: &Rc<Self>) -> Vec<SyntaxKind> {
        let mut out = Vec::new();
        let mut cur = Some(Rc::clone(self));
        while let Some(node) = cur {
            out.push(node.green.kind);
            cur = node.parent.clone();
        }
        out
    }
}

fn line_col(src: &str, byte_offset: usize) -> (usize, usize) {
    let clamped = byte_offset.min(src.len());
    let prefix = &src[..clamped];
    let line = prefix.bytes().filter(|&b| b == b'\n').count() + 1;
    let col = match prefix.rfind('\n') {
        Some(nl) => prefix[nl + 1..].chars().count() + 1,
        None => prefix.chars().count() + 1,
    };
    (line, col)
}

// ---------------------------------------------------------------------------
// Typed AST Facade backed by the Lossless Syntax Tree (`rowan::ast` style)
// ---------------------------------------------------------------------------

#[derive(Debug, PartialEq, Eq)]
struct LetDeclSummary {
    name: String,
    name_span: TextSpan,
    raw_value_text: String,
    eval_value: Option<i64>,
    trailing_comment: Option<String>,
}

fn parse_int_literal(raw: &str) -> Option<i64> {
    let clean = raw.replace('_', "");
    if let Some(hex) = clean
        .strip_prefix("0x")
        .or_else(|| clean.strip_prefix("0X"))
    {
        i64::from_str_radix(hex, 16).ok()
    } else {
        clean.parse::<i64>().ok()
    }
}

fn eval_green(el: &GreenElement) -> Option<i64> {
    match el {
        GreenElement::Token(t) if t.kind == SyntaxKind::Number => parse_int_literal(&t.text),
        GreenElement::Node(n) => match n.kind {
            SyntaxKind::ParenExpr => n
                .children
                .iter()
                .find(|c| {
                    matches!(c, GreenElement::Node(_))
                        || matches!(c, GreenElement::Token(t) if t.kind == SyntaxKind::Number)
                })
                .and_then(eval_green),
            SyntaxKind::BinaryExpr => {
                let sig: Vec<&GreenElement> = n
                    .children
                    .iter()
                    .filter(|c| !matches!(c, GreenElement::Token(t) if t.kind.is_trivia()))
                    .collect();
                if sig.len() == 3 {
                    let l = eval_green(sig[0])?;
                    let r = eval_green(sig[2])?;
                    match sig[1] {
                        GreenElement::Token(op) if op.kind == SyntaxKind::Plus => Some(l + r),
                        GreenElement::Token(op) if op.kind == SyntaxKind::Star => Some(l * r),
                        _ => None,
                    }
                } else {
                    None
                }
            }
            _ => None,
        },
        _ => None,
    }
}

fn view_let_declaration(node: &Rc<SyntaxNode>) -> Option<LetDeclSummary> {
    if node.green.kind != SyntaxKind::LetDeclaration {
        return None;
    }
    let mut name = None;
    let mut name_span = None;
    let mut saw_eq = false;
    let mut raw_value_text = None;
    let mut eval_value = None;
    let mut trailing_comment = None;

    for el in node.children_with_tokens() {
        match el {
            SyntaxElement::Token { green, span } => {
                if green.kind == SyntaxKind::Identifier && !saw_eq {
                    name = Some(green.text.clone());
                    name_span = Some(span);
                } else if green.kind == SyntaxKind::Equals {
                    saw_eq = true;
                } else if saw_eq && green.kind == SyntaxKind::Number && raw_value_text.is_none() {
                    raw_value_text = Some(green.text.clone());
                    eval_value = parse_int_literal(&green.text);
                } else if matches!(
                    green.kind,
                    SyntaxKind::LineComment | SyntaxKind::BlockComment
                ) {
                    trailing_comment = Some(green.text.clone());
                }
            }
            SyntaxElement::Node(sub) if saw_eq && raw_value_text.is_none() => {
                raw_value_text = Some(sub.green.text());
                let green_el = GreenElement::Node(Rc::clone(&sub.green));
                eval_value = eval_green(&green_el);
            }
            SyntaxElement::Node(_) => {}
        }
    }

    Some(LetDeclSummary {
        name: name?,
        name_span: name_span?,
        raw_value_text: raw_value_text?,
        eval_value,
        trailing_comment,
    })
}

/// Functional immutable tree rewrite (Rowan `replace_with` idea):
/// rewrites non-trivia tokens matching `(target_kind, old_text)` while keeping every single
/// whitespace and comment token intact.
fn rewrite_tokens(
    node: &Rc<GreenNode>,
    target_kind: SyntaxKind,
    old_text: &str,
    new_text: &str,
    cache: &mut NodeCache,
) -> Rc<GreenNode> {
    let mut changed = false;
    let mut new_children = Vec::with_capacity(node.children.len());
    for child in &node.children {
        match child {
            GreenElement::Token(tok) => {
                if tok.kind == target_kind && tok.text == old_text {
                    changed = true;
                    new_children.push(GreenElement::Token(cache.token(target_kind, new_text)));
                } else {
                    new_children.push(GreenElement::Token(Rc::clone(tok)));
                }
            }
            GreenElement::Node(sub) => {
                let updated = rewrite_tokens(sub, target_kind, old_text, new_text, cache);
                if !Rc::ptr_eq(&updated, sub) {
                    changed = true;
                }
                new_children.push(GreenElement::Node(updated));
            }
        }
    }
    if changed {
        cache.node(node.kind, new_children)
    } else {
        Rc::clone(node)
    }
}

fn main() {
    let sample = "let x  =  42;  // important comment";
    let mut cache = NodeCache::default();
    let tree = build_lossless_tree(sample, &mut cache);
    assert_eq!(tree.text(), sample);

    println!("=== 1. Same Input as Classical CST/AST Lesson ===");
    println!("Original source ({} bytes): {:?}", sample.len(), sample);

    println!("\n=== 2. Traditional AST (drops formatting & comments) ===");
    let root_red = SyntaxNode::root(Rc::clone(&tree));
    let let_red = &root_red.child_nodes()[0];
    let summary = view_let_declaration(let_red).unwrap();
    println!("Let");
    println!("├── Name({:?})", summary.name);
    println!("└── Number({})", summary.eval_value.unwrap());

    println!("\n=== 3. Lossless Syntax Tree (preserves every byte as-is) ===");
    println!("{}", let_red.green.render_tree());
    println!(
        "\nByte-for-byte round-trip match: {} ({} bytes)",
        tree.text() == sample,
        tree.width
    );

    println!("\n=== 4. Typed AST Facade + Spans Over Lossless Tree ===");
    let (ln, col) = line_col(sample, summary.name_span.start);
    println!(
        "Binding {:?} at span {} (line {}, col {}), value={}, comment={:?}",
        summary.name, summary.name_span, ln, col, summary.raw_value_text, summary.trailing_comment
    );
    println!(
        "Decl span: {}, ancestors: {:?}, NodeCache token hits: {}",
        let_red.span(),
        let_red.ancestors(),
        cache.token_hits
    );

    println!("\n=== 5. Format-Preserving Immutable Refactoring ===");
    let renamed = rewrite_tokens(&tree, SyntaxKind::Identifier, "x", "answer", &mut cache);
    let updated = rewrite_tokens(&renamed, SyntaxKind::Number, "42", "0x2A", &mut cache);
    println!("Edited via Green tree rewrite: {:?}", updated.text());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lossless_roundtrip_single_line_matches_byte_for_byte() {
        let src = "let x  =  42;  // important comment";
        let mut cache = NodeCache::default();
        let tree = build_lossless_tree(src, &mut cache);
        assert_eq!(tree.text(), src);
        assert_eq!(tree.width, src.len());

        let root = SyntaxNode::root(Rc::clone(&tree));
        let decl = &root.child_nodes()[0];
        assert_eq!(decl.green.kind, SyntaxKind::LetDeclaration);
        let summary = view_let_declaration(decl).unwrap();
        assert_eq!(summary.name, "x");
        assert_eq!(summary.name_span, TextSpan { start: 4, end: 5 });
        assert_eq!(summary.eval_value, Some(42));
        assert_eq!(
            summary.trailing_comment.as_deref(),
            Some("// important comment")
        );
    }

    #[test]
    fn lossless_roundtrip_preserves_multiline_block_comments_crlf_and_errors() {
        let src =
            "  /* header */\r\nlet a  =  0x2A + (1 * 2); // tail\r\nlet bad = @@ 99; /* unclosed";
        let mut cache = NodeCache::default();
        let tree = build_lossless_tree(src, &mut cache);
        assert_eq!(tree.text(), src);

        let root = SyntaxNode::root(Rc::clone(&tree));
        let children = root.child_nodes();
        assert_eq!(children[0].green.kind, SyntaxKind::LetDeclaration);
        let s0 = view_let_declaration(&children[0]).unwrap();
        assert_eq!(s0.name, "a");
        assert_eq!(s0.raw_value_text, "0x2A + (1 * 2)");
        assert_eq!(s0.eval_value, Some(44));
        assert_eq!(line_col(src, children[0].span().start), (2, 1));
    }

    #[test]
    fn functional_rewrite_preserves_double_spaces_and_comments_and_shares_unchanged() {
        let src = "let x  =  42;  // important comment\nlet keep = 7;\n";
        let mut cache = NodeCache::default();
        let tree = build_lossless_tree(src, &mut cache);
        let tree2 = rewrite_tokens(&tree, SyntaxKind::Identifier, "x", "answer", &mut cache);
        let tree3 = rewrite_tokens(&tree2, SyntaxKind::Number, "42", "0x2A", &mut cache);

        assert_eq!(
            tree3.text(),
            "let answer  =  0x2A;  // important comment\nlet keep = 7;\n"
        );
        // children[0] = first LetDeclaration, children[1] = Newline, children[2] = second LetDeclaration.
        assert!(matches!(
            (&tree.children[2], &tree3.children[2]),
            (GreenElement::Node(a), GreenElement::Node(b)) if Rc::ptr_eq(a, b)
        ));
    }

    #[test]
    fn node_cache_interns_repeated_trivia_and_nodes() {
        let src = "let a = 1; let b = 1; let c = 1;";
        let mut cache = NodeCache::default();
        let tree = build_lossless_tree(src, &mut cache);
        assert_eq!(tree.text(), src);
        assert!(cache.token_hits > 0);
    }

    #[test]
    fn red_cursor_reports_parent_chain() {
        let src = "let x = 1 + 2;";
        let mut cache = NodeCache::default();
        let root = SyntaxNode::root(build_lossless_tree(src, &mut cache));
        let decl = &root.child_nodes()[0];
        let bin = &decl.child_nodes()[0];
        assert_eq!(
            bin.ancestors(),
            vec![
                SyntaxKind::BinaryExpr,
                SyntaxKind::LetDeclaration,
                SyntaxKind::SourceFile
            ]
        );
    }
}
