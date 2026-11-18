"""Modern Extension: Lossless Immutable Syntax Trees (Rowan-Style Green/Red Architecture).

Complements Dragon Book §2.2.3 (Parse Trees), §2.5.1 (CST vs. AST), and §2.6.1
(where traditional lexers strip whitespace and comments):
1. Preserves every byte of trivia (`Whitespace`, `Newline`, `Comment`, `BlockComment`),
   exact token spellings (`0x2A` vs `42`), and `Error` recovery nodes,
2. Interns immutable `GreenToken` / `GreenNode` subtrees in a `NodeCache` (Rowan pattern),
3. Computes absolute byte `TextSpan`s and `(line, col)` coordinates lazily via parent-linked
   `SyntaxNode` (Red) cursors,
4. Exposes typed semantic views (`view_let_declaration`) ignoring trivia while supporting
   functional edits that preserve every comment and double-space byte-for-byte.
"""

from __future__ import annotations

from dataclasses import dataclass
import unittest

TRIVIA_KINDS = frozenset({"Whitespace", "Newline", "Comment", "BlockComment"})


@dataclass(frozen=True)
class GreenToken:
    kind: str
    text: str

    @property
    def width(self) -> int:
        return len(self.text)

    def write_text(self, chunks: list[str]) -> None:
        chunks.append(self.text)


@dataclass(frozen=True)
class GreenNode:
    kind: str
    width: int
    children: tuple[GreenNode | GreenToken, ...]

    def write_text(self, chunks: list[str]) -> None:
        for c in self.children:
            c.write_text(chunks)

    def text(self) -> str:
        chunks: list[str] = []
        self.write_text(chunks)
        return "".join(chunks)

    def render_tree(self) -> str:
        lines: list[str] = []
        _fmt_green(self, "", True, True, lines)
        return "\n".join(lines)


def _fmt_green(node: GreenNode, prefix: str, is_last: bool, is_root: bool, out: list[str]) -> None:
    if is_root:
        out.append(node.kind)
    else:
        branch = "└── " if is_last else "├── "
        out.append(f"{prefix}{branch}{node.kind}")
    nxt = "" if is_root else prefix + ("    " if is_last else "│   ")
    for idx, ch in enumerate(node.children):
        last = idx + 1 == len(node.children)
        if isinstance(ch, GreenNode):
            _fmt_green(ch, nxt, last, False, out)
        else:
            branch = "└── " if last else "├── "
            out.append(f"{nxt}{branch}{ch.kind}({ch.text!r})")


class NodeCache:
    """Deduplicates identical immutable `GreenToken`s and `GreenNode`s (Rowan `NodeCache`)."""

    def __init__(self) -> None:
        self.tokens: dict[tuple[str, str], GreenToken] = {}
        self.nodes: dict[tuple[str, tuple[GreenNode | GreenToken, ...]], GreenNode] = {}
        self.token_hits = 0
        self.node_hits = 0

    def token(self, kind: str, text: str) -> GreenToken:
        key = (kind, text)
        existing = self.tokens.get(key)
        if existing is not None:
            self.token_hits += 1
            return existing
        tok = GreenToken(kind, text)
        self.tokens[key] = tok
        return tok

    def node(self, kind: str, children: list[GreenNode | GreenToken]) -> GreenNode:
        child_tup = tuple(children)
        key = (kind, child_tup)
        existing = self.nodes.get(key)
        if existing is not None:
            self.node_hits += 1
            return existing
        n = GreenNode(kind, sum(c.width for c in child_tup), child_tup)
        self.nodes[key] = n
        return n


def lex_lossless(src: str, cache: NodeCache) -> list[GreenToken]:
    n = len(src)
    i = 0
    out: list[GreenToken] = []
    single = {
        "=": "Equals",
        "+": "Plus",
        "*": "Star",
        "(": "LParen",
        ")": "RParen",
        ";": "Semicolon",
    }

    while i < n:
        start = i
        ch = src[i]
        if ch in ("\r", "\n"):
            if ch == "\r" and i + 1 < n and src[i + 1] == "\n":
                i += 2
            else:
                i += 1
            out.append(cache.token("Newline", src[start:i]))
            continue
        if ch in (" ", "\t"):
            while i < n and src[i] in (" ", "\t"):
                i += 1
            out.append(cache.token("Whitespace", src[start:i]))
            continue
        if src.startswith("//", i):
            i += 2
            while i < n and src[i] not in ("\r", "\n"):
                i += 1
            out.append(cache.token("Comment", src[start:i]))
            continue
        if src.startswith("/*", i):
            end_idx = src.find("*/", i + 2)
            i = n if end_idx == -1 else end_idx + 2
            out.append(cache.token("BlockComment", src[start:i]))
            continue
        if ch.isdigit():
            if src.startswith(("0x", "0X"), i):
                i += 2
                while i < n and (src[i] in "0123456789abcdefABCDEF_"):
                    i += 1
            else:
                while i < n and (src[i].isdigit() or src[i] == "_"):
                    i += 1
            out.append(cache.token("Number", src[start:i]))
            continue
        if ch.isalpha() or ch == "_":
            while i < n and (src[i].isalnum() or src[i] == "_"):
                i += 1
            lex = src[start:i]
            kind = "Keyword" if lex == "let" else "Identifier"
            out.append(cache.token(kind, lex))
            continue
        kind = single.get(ch, "ErrorToken")
        i += 1
        out.append(cache.token(kind, src[start:i]))

    return out


class LosslessParser:
    def __init__(self, tokens: list[GreenToken], cache: NodeCache) -> None:
        self.tokens = tokens
        self.pos = 0
        self.cache = cache

    def peek_significant(self) -> str | None:
        idx = self.pos
        while idx < len(self.tokens):
            k = self.tokens[idx].kind
            if k not in TRIVIA_KINDS:
                return k
            idx += 1
        return None

    def eat_all_trivia(self, into: list[GreenNode | GreenToken]) -> None:
        while self.pos < len(self.tokens) and self.tokens[self.pos].kind in TRIVIA_KINDS:
            into.append(self.tokens[self.pos])
            self.pos += 1

    def eat_inline_trivia(self, into: list[GreenNode | GreenToken]) -> None:
        while self.pos < len(self.tokens) and self.tokens[self.pos].kind in (
            "Whitespace",
            "Comment",
            "BlockComment",
        ):
            into.append(self.tokens[self.pos])
            self.pos += 1

    def bump_tok(self, into: list[GreenNode | GreenToken]) -> str:
        self.eat_all_trivia(into)
        tok = self.tokens[self.pos]
        self.pos += 1
        into.append(tok)
        return tok.kind

    def expect_tok(self, kind: str, into: list[GreenNode | GreenToken]) -> bool:
        if self.peek_significant() == kind:
            self.bump_tok(into)
            return True
        return False

    def parse_source_file(self) -> GreenNode:
        children: list[GreenNode | GreenToken] = []
        while self.peek_significant() is not None:
            self.eat_all_trivia(children)
            children.append(self.parse_stmt())
        self.eat_all_trivia(children)
        return self.cache.node("SourceFile", children)

    def parse_stmt(self) -> GreenNode:
        children: list[GreenNode | GreenToken] = []
        if self.peek_significant() == "Keyword":
            self.bump_tok(children)
            self.expect_tok("Identifier", children)
            self.expect_tok("Equals", children)
            self.eat_all_trivia(children)
            expr = self.parse_expr()
            if expr is not None:
                children.append(expr)
            else:
                self.recover_until_semi(children)
            self.expect_tok("Semicolon", children)
            self.eat_inline_trivia(children)
            return self.cache.node("LetDeclaration", children)
        expr = self.parse_expr()
        if expr is not None:
            children.append(expr)
            self.expect_tok("Semicolon", children)
            self.eat_inline_trivia(children)
            return self.cache.node("ExprStatement", children)
        self.recover_until_semi(children)
        self.expect_tok("Semicolon", children)
        self.eat_inline_trivia(children)
        return self.cache.node("ErrorNode", children)

    def recover_until_semi(self, into: list[GreenNode | GreenToken]) -> None:
        while self.pos < len(self.tokens):
            if self.tokens[self.pos].kind in ("Semicolon", "Keyword"):
                break
            into.append(self.tokens[self.pos])
            self.pos += 1

    def parse_expr(self) -> GreenNode | GreenToken | None:
        lhs = self.parse_term()
        if lhs is None:
            return None
        while self.peek_significant() == "Plus":
            ch: list[GreenNode | GreenToken] = [lhs]
            self.bump_tok(ch)
            self.eat_all_trivia(ch)
            rhs = self.parse_term()
            if rhs is not None:
                ch.append(rhs)
            lhs = self.cache.node("BinaryExpr", ch)
        return lhs

    def parse_term(self) -> GreenNode | GreenToken | None:
        lhs = self.parse_atom()
        if lhs is None:
            return None
        while self.peek_significant() == "Star":
            ch: list[GreenNode | GreenToken] = [lhs]
            self.bump_tok(ch)
            self.eat_all_trivia(ch)
            rhs = self.parse_atom()
            if rhs is not None:
                ch.append(rhs)
            lhs = self.cache.node("BinaryExpr", ch)
        return lhs

    def parse_atom(self) -> GreenNode | GreenToken | None:
        sig = self.peek_significant()
        if sig in ("Number", "Identifier"):
            ch: list[GreenNode | GreenToken] = []
            self.eat_all_trivia(ch)
            tok = self.tokens[self.pos]
            self.pos += 1
            if not ch:
                return tok
            ch.append(tok)
            return self.cache.node("BinaryExpr", ch)
        if sig == "LParen":
            ch = []
            self.bump_tok(ch)
            self.eat_all_trivia(ch)
            inner = self.parse_expr()
            if inner is not None:
                ch.append(inner)
            self.expect_tok("RParen", ch)
            return self.cache.node("ParenExpr", ch)
        return None


def build_lossless_tree(src: str, cache: NodeCache) -> GreenNode:
    return LosslessParser(lex_lossless(src, cache), cache).parse_source_file()


# ---------------------------------------------------------------------------
# Red Tree (`SyntaxNode` cursor) + Typed AST Facade + Functional Edit
# ---------------------------------------------------------------------------

@dataclass(frozen=True)
class SyntaxNode:
    green: GreenNode
    offset: int
    parent: SyntaxNode | None

    @classmethod
    def root(cls, green: GreenNode) -> SyntaxNode:
        return cls(green=green, offset=0, parent=None)

    @property
    def span(self) -> tuple[int, int]:
        return (self.offset, self.offset + self.green.width)

    def children_with_tokens(self) -> list[SyntaxNode | tuple[GreenToken, tuple[int, int]]]:
        cursor = self.offset
        out: list[SyntaxNode | tuple[GreenToken, tuple[int, int]]] = []
        for ch in self.green.children:
            w = ch.width
            if isinstance(ch, GreenNode):
                out.append(SyntaxNode(ch, cursor, self))
            else:
                out.append((ch, (cursor, cursor + w)))
            cursor += w
        return out

    def child_nodes(self) -> list[SyntaxNode]:
        return [c for c in self.children_with_tokens() if isinstance(c, SyntaxNode)]

    def ancestors(self) -> list[str]:
        out: list[str] = []
        cur: SyntaxNode | None = self
        while cur is not None:
            out.append(cur.green.kind)
            cur = cur.parent
        return out


def line_col(src: str, byte_offset: int) -> tuple[int, int]:
    clamped = min(byte_offset, len(src))
    prefix = src[:clamped]
    line = prefix.count("\n") + 1
    nl = prefix.rfind("\n")
    col = clamped + 1 if nl == -1 else clamped - nl
    return line, col


def parse_int_literal(raw: str) -> int | None:
    clean = raw.replace("_", "")
    try:
        if clean.lower().startswith("0x"):
            return int(clean[2:], 16)
        return int(clean)
    except ValueError:
        return None


def eval_green(el: GreenNode | GreenToken) -> int | None:
    if isinstance(el, GreenToken):
        return parse_int_literal(el.text) if el.kind == "Number" else None
    if el.kind == "ParenExpr":
        for c in el.children:
            if isinstance(c, GreenNode) or c.kind == "Number":
                v = eval_green(c)
                if v is not None:
                    return v
    if el.kind == "BinaryExpr":
        sig = [c for c in el.children if isinstance(c, GreenNode) or c.kind not in TRIVIA_KINDS]
        if len(sig) == 3 and isinstance(sig[1], GreenToken):
            l = eval_green(sig[0])
            r = eval_green(sig[2])
            if l is not None and r is not None:
                if sig[1].kind == "Plus":
                    return l + r
                if sig[1].kind == "Star":
                    return l * r
    return None


@dataclass(frozen=True)
class LetDeclSummary:
    name: str
    name_span: tuple[int, int]
    raw_value_text: str
    eval_value: int | None
    trailing_comment: str | None


def view_let_declaration(node: SyntaxNode) -> LetDeclSummary | None:
    if node.green.kind != "LetDeclaration":
        return None
    name: str | None = None
    name_span: tuple[int, int] | None = None
    saw_eq = False
    raw_val: str | None = None
    eval_val: int | None = None
    comment: str | None = None
    for item in node.children_with_tokens():
        if isinstance(item, SyntaxNode):
            if saw_eq and raw_val is None:
                raw_val = item.green.text()
                eval_val = eval_green(item.green)
        else:
            tok, sp = item
            if tok.kind == "Identifier" and not saw_eq:
                name = tok.text
                name_span = sp
            elif tok.kind == "Equals":
                saw_eq = True
            elif saw_eq and tok.kind == "Number" and raw_val is None:
                raw_val = tok.text
                eval_val = parse_int_literal(tok.text)
            elif tok.kind in ("Comment", "BlockComment"):
                comment = tok.text
    if name is None or name_span is None or raw_val is None:
        return None
    return LetDeclSummary(name, name_span, raw_val, eval_val, comment)


def rewrite_tokens(
    node: GreenNode, target_kind: str, old_text: str, new_text: str, cache: NodeCache
) -> GreenNode:
    changed = False
    new_children: list[GreenNode | GreenToken] = []
    for ch in node.children:
        if isinstance(ch, GreenToken):
            if ch.kind == target_kind and ch.text == old_text:
                changed = True
                new_children.append(cache.token(target_kind, new_text))
            else:
                new_children.append(ch)
        else:
            upd = rewrite_tokens(ch, target_kind, old_text, new_text, cache)
            if upd is not ch:
                changed = True
            new_children.append(upd)
    return cache.node(node.kind, new_children) if changed else node


def main() -> None:
    sample = "let x  =  42;  // important comment"
    cache = NodeCache()
    tree = build_lossless_tree(sample, cache)
    assert tree.text() == sample

    root_red = SyntaxNode.root(tree)
    let_red = root_red.child_nodes()[0]
    summary = view_let_declaration(let_red)
    assert summary is not None

    print("=== 1. Same Input as Classical CST/AST Lesson ===")
    print(f"Original source ({len(sample)} bytes): {sample!r}")

    print("\n=== 2. Traditional AST (drops formatting & comments) ===")
    print(f"Let\n├── Name({summary.name!r})\n└── Number({summary.eval_value})")

    print("\n=== 3. Lossless Syntax Tree (preserves every byte as-is) ===")
    print(let_red.green.render_tree())
    print(f"\nByte-for-byte round-trip match: {tree.text() == sample} ({tree.width} bytes)")

    print("\n=== 4. Typed AST Facade + Spans Over Lossless Tree ===")
    ln, col = line_col(sample, summary.name_span[0])
    print(
        f"Binding {summary.name!r} at span {summary.name_span[0]}..{summary.name_span[1]} "
        f"(line {ln}, col {col}), value={summary.raw_value_text}, comment={summary.trailing_comment!r}"
    )

    print("\n=== 5. Format-Preserving Immutable Refactoring ===")
    renamed = rewrite_tokens(tree, "Identifier", "x", "answer", cache)
    updated = rewrite_tokens(renamed, "Number", "42", "0x2A", cache)
    print(f"Edited via Green tree rewrite: {updated.text()!r}")


class LosslessSyntaxTreeTests(unittest.TestCase):
    def test_single_line_byte_for_byte_roundtrip(self) -> None:
        src = "let x  =  42;  // important comment"
        cache = NodeCache()
        tree = build_lossless_tree(src, cache)
        self.assertEqual(tree.text(), src)
        root = SyntaxNode.root(tree)
        decl = root.child_nodes()[0]
        summary = view_let_declaration(decl)
        assert summary is not None
        self.assertEqual(summary.name, "x")
        self.assertEqual(summary.name_span, (4, 5))
        self.assertEqual(summary.eval_value, 42)
        self.assertEqual(summary.trailing_comment, "// important comment")

    def test_multiline_block_comments_crlf_and_errors_roundtrip(self) -> None:
        src = "  /* header */\r\nlet a  =  0x2A + (1 * 2); // tail\r\nlet bad = @@ 99; /* unclosed"
        cache = NodeCache()
        tree = build_lossless_tree(src, cache)
        self.assertEqual(tree.text(), src)
        root = SyntaxNode.root(tree)
        children = root.child_nodes()
        s0 = view_let_declaration(children[0])
        assert s0 is not None
        self.assertEqual(s0.name, "a")
        self.assertEqual(s0.eval_value, 44)
        self.assertEqual(line_col(src, children[0].span[0]), (2, 1))

    def test_functional_rewrite_preserves_trivia_and_shares_unchanged(self) -> None:
        src = "let x  =  42;  // important comment\nlet keep = 7;\n"
        cache = NodeCache()
        tree = build_lossless_tree(src, cache)
        tree2 = rewrite_tokens(tree, "Identifier", "x", "answer", cache)
        tree3 = rewrite_tokens(tree2, "Number", "42", "0x2A", cache)
        self.assertEqual(tree3.text(), "let answer  =  0x2A;  // important comment\nlet keep = 7;\n")
        self.assertIs(tree.children[2], tree3.children[2])

    def test_node_cache_interns_trivia(self) -> None:
        src = "let a = 1; let b = 1; let c = 1;"
        cache = NodeCache()
        tree = build_lossless_tree(src, cache)
        self.assertEqual(tree.text(), src)
        self.assertGreater(cache.token_hits, 0)

    def test_red_ancestors(self) -> None:
        src = "let x = 1 + 2;"
        root = SyntaxNode.root(build_lossless_tree(src, NodeCache()))
        decl = root.child_nodes()[0]
        bin_node = decl.child_nodes()[0]
        self.assertEqual(bin_node.ancestors(), ["BinaryExpr", "LetDeclaration", "SourceFile"])


if __name__ == "__main__":
    main()
