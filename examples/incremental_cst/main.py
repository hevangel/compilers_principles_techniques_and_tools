"""Modern Extension: Concrete Syntax Trees (Green/Red) & Incremental Reparsing.

Explicitly labeled as a modern IDE/formatter parsing technique complementing
Dragon Book Ch. 4–5:
1. Lossless Green Tree storing every token including whitespace and comments (`trivia`),
   with relative byte widths and zero parent pointers / zero absolute offsets,
2. On-demand Red Tree (`RedNode`) cursors computing absolute `[start, end)` byte
   ranges dynamically without mutating or invalidating shared Green nodes,
3. Incremental reparse reusing unaffected `GreenNode` statement subtrees across
   source edits, verified by object identity (`is`) and exact structural equivalence
   to a fresh full parse.
"""

from __future__ import annotations

from dataclasses import dataclass
import unittest


@dataclass(frozen=True)
class GreenToken:
    kind: str
    text: str

    @property
    def width(self) -> int:
        return len(self.text)

    def write_text(self, chunks: list[str]) -> None:
        chunks.append(self.text)


@dataclass(frozen=True, eq=True)
class GreenNode:
    kind: str
    width: int
    children: tuple[GreenNode | GreenToken, ...]

    @classmethod
    def make(cls, kind: str, children: list[GreenNode | GreenToken]) -> GreenNode:
        return cls(kind=kind, width=sum(c.width for c in children), children=tuple(children))

    def write_text(self, chunks: list[str]) -> None:
        for child in self.children:
            child.write_text(chunks)

    def text(self) -> str:
        chunks: list[str] = []
        self.write_text(chunks)
        return "".join(chunks)


@dataclass(frozen=True)
class RedNode:
    green: GreenNode
    offset: int

    @classmethod
    def root(cls, green: GreenNode) -> RedNode:
        return cls(green=green, offset=0)

    @property
    def range(self) -> tuple[int, int]:
        return (self.offset, self.offset + self.green.width)

    def child_nodes(self) -> list[RedNode]:
        cursor = self.offset
        out: list[RedNode] = []
        for child in self.green.children:
            if isinstance(child, GreenNode):
                out.append(RedNode(green=child, offset=cursor))
            cursor += child.width
        return out


@dataclass
class RawToken:
    kind: str
    text: str
    start: int


def tokenize_lossless(src: str) -> list[RawToken]:
    n = len(src)
    i = 0
    out: list[RawToken] = []
    single = {
        "=": "EQ", "+": "PLUS", "*": "STAR", ";": "SEMI", "(": "LPAREN", ")": "RPAREN",
    }
    while i < n:
        start = i
        if src[i].isspace():
            i += 1
            while i < n and src[i].isspace():
                i += 1
            out.append(RawToken("WHITESPACE", src[start:i], start))
            continue
        if src.startswith("//", i):
            i += 2
            while i < n and src[i] != "\n":
                i += 1
            out.append(RawToken("COMMENT", src[start:i], start))
            continue
        if src[i].isdigit():
            i += 1
            while i < n and src[i].isdigit():
                i += 1
            out.append(RawToken("INT", src[start:i], start))
            continue
        if src[i].isalpha() or src[i] == "_":
            i += 1
            while i < n and (src[i].isalnum() or src[i] == "_"):
                i += 1
            lex = src[start:i]
            kind = "KW_LET" if lex == "let" else "KW_RETURN" if lex == "return" else "IDENT"
            out.append(RawToken(kind, lex, start))
            continue
        kind = single.get(src[i], "ERROR_TOK")
        i += 1
        out.append(RawToken(kind, src[start:i], start))
    return out


class CstBuilder:
    TRIVIA = {"WHITESPACE", "COMMENT"}

    def __init__(self, src: str, start_offset: int = 0) -> None:
        self.tokens = tokenize_lossless(src[start_offset:])
        for tok in self.tokens:
            tok.start += start_offset
        self.pos = 0

    def at_eof(self) -> bool:
        return self.pos >= len(self.tokens)

    def current_offset(self, end_offset: int) -> int:
        return self.tokens[self.pos].start if self.pos < len(self.tokens) else end_offset

    def peek_significant(self) -> str | None:
        p = self.pos
        while p < len(self.tokens) and self.tokens[p].kind in self.TRIVIA:
            p += 1
        return self.tokens[p].kind if p < len(self.tokens) else None

    def eat_trivia(self, sink: list[GreenNode | GreenToken]) -> None:
        while self.pos < len(self.tokens) and self.tokens[self.pos].kind in self.TRIVIA:
            t = self.tokens[self.pos]
            sink.append(GreenToken(t.kind, t.text))
            self.pos += 1

    def bump_token(self, sink: list[GreenNode | GreenToken]) -> str:
        t = self.tokens[self.pos]
        self.pos += 1
        sink.append(GreenToken(t.kind, t.text))
        return t.kind

    def parse_atom(self) -> GreenNode | GreenToken:
        pre: list[GreenNode | GreenToken] = []
        self.eat_trivia(pre)
        kind = self.tokens[self.pos].kind if self.pos < len(self.tokens) else None
        if kind == "LPAREN":
            children = pre
            self.bump_token(children)
            children.append(self.parse_expr_min(0))
            self.eat_trivia(children)
            if self.pos < len(self.tokens) and self.tokens[self.pos].kind == "RPAREN":
                self.bump_token(children)
            return GreenNode.make("ParenExpr", children)
        if kind in ("INT", "IDENT"):
            if not pre:
                t = self.tokens[self.pos]
                self.pos += 1
                return GreenToken(t.kind, t.text)
            self.bump_token(pre)
            return GreenNode.make("ParenExpr", pre)
        if not self.at_eof():
            self.bump_token(pre)
        return GreenNode.make("ErrorStmt", pre)

    def parse_expr_min(self, min_prec: int) -> GreenNode | GreenToken:
        lhs = self.parse_atom()
        while True:
            op = self.peek_significant()
            prec = 1 if op == "PLUS" else 2 if op == "STAR" else 0
            if prec == 0 or prec < min_prec:
                break
            children: list[GreenNode | GreenToken] = [lhs]
            self.eat_trivia(children)
            self.bump_token(children)
            children.append(self.parse_expr_min(prec + 1))
            lhs = GreenNode.make("BinExpr", children)
        return lhs

    def parse_one_stmt(self) -> GreenNode | None:
        if self.at_eof():
            return None
        children: list[GreenNode | GreenToken] = []
        self.eat_trivia(children)
        if self.at_eof():
            return None
        tok_kind = self.tokens[self.pos].kind
        if tok_kind == "KW_LET":
            self.bump_token(children)
            self.eat_trivia(children)
            if self.pos < len(self.tokens) and self.tokens[self.pos].kind == "IDENT":
                self.bump_token(children)
            self.eat_trivia(children)
            if self.pos < len(self.tokens) and self.tokens[self.pos].kind == "EQ":
                self.bump_token(children)
                children.append(self.parse_expr_min(0))
            kind = "LetStmt"
        elif tok_kind == "KW_RETURN":
            self.bump_token(children)
            if self.peek_significant() != "SEMI":
                children.append(self.parse_expr_min(0))
            kind = "ReturnStmt"
        else:
            children.append(self.parse_expr_min(0))
            kind = "ExprStmt"

        self.eat_trivia(children)
        if self.pos < len(self.tokens) and self.tokens[self.pos].kind == "SEMI":
            self.bump_token(children)
        else:
            while not self.at_eof() and self.tokens[self.pos].kind != "SEMI":
                self.bump_token(children)
            if self.pos < len(self.tokens) and self.tokens[self.pos].kind == "SEMI":
                self.bump_token(children)

        while self.pos < len(self.tokens):
            tok = self.tokens[self.pos]
            if tok.kind == "COMMENT" or (tok.kind == "WHITESPACE" and "\n" not in tok.text):
                self.bump_token(children)
            else:
                break
        if self.pos < len(self.tokens) and self.tokens[self.pos].kind == "WHITESPACE":
            ws = self.tokens[self.pos]
            nl = ws.text.find("\n")
            if nl != -1:
                split = nl + 1
                children.append(GreenToken("WHITESPACE", ws.text[:split]))
                tail = ws.text[split:]
                if not tail:
                    self.pos += 1
                else:
                    ws.start += split
                    ws.text = tail
        return GreenNode.make(kind, children)


def parse_cst_full(src: str) -> GreenNode:
    builder = CstBuilder(src, 0)
    children: list[GreenNode | GreenToken] = []
    while not builder.at_eof():
        start_pos = builder.pos
        stmt = builder.parse_one_stmt()
        if stmt is not None:
            children.append(stmt)
        else:
            for t in builder.tokens[start_pos:]:
                children.append(GreenToken(t.kind, t.text))
            break
    return GreenNode.make("Program", children)


@dataclass
class IncrementalStats:
    tree: GreenNode
    reused_stmts: int
    reparsed_stmts: int


def incremental_reparse(
    old_root: GreenNode,
    old_src: str,
    edit_start: int,
    del_len: int,
    replacement: str,
) -> tuple[str, IncrementalStats]:
    new_src = old_src[:edit_start] + replacement + old_src[edit_start + del_len:]
    edit_old_end = edit_start + del_len
    delta = len(replacement) - del_len

    old_stmts: list[tuple[int, int, GreenNode]] = []
    old_cursor = 0
    for child in old_root.children:
        w = child.width
        if isinstance(child, GreenNode):
            old_stmts.append((old_cursor, old_cursor + w, child))
        old_cursor += w

    new_children: list[GreenNode | GreenToken] = []
    reused = 0
    reparsed = 0
    new_pos = 0
    old_idx = 0

    while old_idx < len(old_stmts):
        os, oe, node = old_stmts[old_idx]
        if oe < edit_start and new_pos == os and new_src.startswith(old_src[os:oe], new_pos):
            new_children.append(node)
            new_pos = oe
            reused += 1
            old_idx += 1
        else:
            break

    while old_idx < len(old_stmts) and old_stmts[old_idx][0] <= edit_old_end:
        old_idx += 1
    suffix_resume_new = (
        old_stmts[old_idx][0] + delta if old_idx < len(old_stmts) else len(new_src)
    )

    while new_pos < suffix_resume_new:
        builder = CstBuilder(new_src, new_pos)
        stmt = builder.parse_one_stmt()
        if stmt is not None:
            new_children.append(stmt)
            new_pos = builder.current_offset(len(new_src))
            reparsed += 1
        else:
            for t in builder.tokens:
                new_children.append(GreenToken(t.kind, t.text))
            new_pos = len(new_src)
            break

    while old_idx < len(old_stmts):
        os, oe, node = old_stmts[old_idx]
        shifted_start = os + delta
        if shifted_start < new_pos:
            old_idx += 1
            continue
        if shifted_start == new_pos and oe > os and new_src.startswith(old_src[os:oe], new_pos):
            new_children.append(node)
            new_pos += oe - os
            reused += 1
            old_idx += 1
        else:
            break

    if new_pos < len(new_src):
        builder = CstBuilder(new_src, new_pos)
        while not builder.at_eof():
            start_p = builder.pos
            stmt = builder.parse_one_stmt()
            if stmt is not None:
                new_children.append(stmt)
                reparsed += 1
            else:
                for t in builder.tokens[start_p:]:
                    new_children.append(GreenToken(t.kind, t.text))
                break

    return new_src, IncrementalStats(
        tree=GreenNode.make("Program", new_children),
        reused_stmts=reused,
        reparsed_stmts=reparsed,
    )


def main() -> None:
    print("=== Advanced Parsing (Modern): Concrete Syntax Trees & Incremental Reparsing ===")
    src = "// Header comment\nlet x = 1 + 2 * 3;\nlet y = (4 + 5);\nreturn x + y;\n"
    old_tree = parse_cst_full(src)
    print(f"Original lossless round-trip matches: {old_tree.text() == src}")

    needle = "(4 + 5)"
    pos = src.index(needle)
    new_src, stats = incremental_reparse(old_tree, src, pos, len(needle), "100 + 200")
    fresh = parse_cst_full(new_src)
    assert stats.tree == fresh
    print(
        f"Incremental edit stats: reused_stmts={stats.reused_stmts}, "
        f"reparsed_stmts={stats.reparsed_stmts}, lossless={stats.tree.text() == new_src}"
    )
    old_red = RedNode.root(old_tree).child_nodes()
    new_red = RedNode.root(stats.tree).child_nodes()
    for idx, (o, n) in enumerate(zip(old_red, new_red)):
        print(
            f"  stmt[{idx}] {n.green.kind} old={o.range[0]}..{o.range[1]} "
            f"new={n.range[0]}..{n.range[1]} ptr_reused={o.green is n.green}"
        )


class IncrementalCstTests(unittest.TestCase):
    def test_lossless_green_tree_preserves_comments_and_whitespace(self) -> None:
        src = "  // leading trivia\nlet a = 10 + 20 * 30;  // inline\nreturn a;\n"
        green = parse_cst_full(src)
        self.assertEqual(green.text(), src)
        stmts = RedNode.root(green).child_nodes()
        self.assertEqual(len(stmts), 2)
        self.assertEqual(stmts[0].green.kind, "LetStmt")
        self.assertEqual(stmts[1].green.kind, "ReturnStmt")
        self.assertEqual(stmts[1].range[1], len(src))

    def test_incremental_reparse_reuses_prefix_and_suffix_identity(self) -> None:
        src = "let a = 1;\nlet b = 2 + 3;\nreturn a + b;\n"
        old_tree = parse_cst_full(src)
        pos = src.index("2 + 3")
        new_src, stats = incremental_reparse(old_tree, src, pos, 5, "200 * 300")
        self.assertEqual(stats.tree, parse_cst_full(new_src))
        self.assertEqual(stats.tree.text(), new_src)
        self.assertEqual(stats.reused_stmts, 2)
        self.assertEqual(stats.reparsed_stmts, 1)

        old_stmts = RedNode.root(old_tree).child_nodes()
        new_stmts = RedNode.root(stats.tree).child_nodes()
        self.assertIs(old_stmts[0].green, new_stmts[0].green)
        self.assertIsNot(old_stmts[1].green, new_stmts[1].green)
        self.assertIs(old_stmts[2].green, new_stmts[2].green)
        self.assertEqual(old_stmts[2].range[0] + 4, new_stmts[2].range[0])

    def test_editing_first_statement_still_reuses_downstream_statements(self) -> None:
        src = "let first = 1;\nlet second = 2;\nreturn first + second;\n"
        old_tree = parse_cst_full(src)
        pos = src.index("1")
        new_src, stats = incremental_reparse(old_tree, src, pos, 1, "9999")
        self.assertEqual(stats.tree, parse_cst_full(new_src))
        self.assertEqual(stats.reused_stmts, 2)
        self.assertEqual(stats.reparsed_stmts, 1)


if __name__ == "__main__":
    main()
