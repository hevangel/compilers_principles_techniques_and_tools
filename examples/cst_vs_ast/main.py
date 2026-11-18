"""Dragon Book Classical CST (Parse Tree) vs. AST & Syntax-Tree Variants
(§2.2.3, §2.5.1, §2.6.1, §2.8.2, §5.3.1, §6.1).

Demonstrates:
1. Classical lexical analysis (§2.6.1) stripping whitespace and comments before parsing,
   explaining why even a classical CST/parse tree is NOT byte-for-byte lossless,
2. Grammar derivations (`=>_lm`) and concrete parse trees (§2.2.3) preserving every
   intermediate chain (`Expr -> Term -> Factor`) and punctuation token (`let`, `=`, `(`, `)`, `;`),
3. Transforming CST -> AST (§2.5.1, §2.8.2, §5.3.1) and AST -> canonical CST + source,
4. Purple §6.1 syntax-tree variants: folding a tree AST into a value-numbered DAG.
"""

from __future__ import annotations

from dataclasses import dataclass
import unittest


@dataclass(frozen=True)
class Token:
    kind: str
    lexeme: str
    start: int
    end: int


@dataclass(frozen=True)
class LexResult:
    tokens: list[Token]
    stripped_whitespace_bytes: int
    stripped_comments: int


def lex_strip_trivia(src: str) -> LexResult:
    """Classical §2.6.1 lexer: removes whitespace and `//` comments before parsing."""
    n = len(src)
    i = 0
    tokens: list[Token] = []
    stripped_ws = 0
    stripped_comments = 0
    single = {
        "=": "EQ",
        "+": "PLUS",
        "-": "MINUS",
        "*": "STAR",
        "/": "SLASH",
        "(": "LPAREN",
        ")": "RPAREN",
        ";": "SEMI",
    }

    while i < n:
        ch = src[i]
        if ch.isspace():
            start = i
            while i < n and src[i].isspace():
                i += 1
            stripped_ws += i - start
            continue
        if src.startswith("//", i):
            while i < n and src[i] != "\n":
                i += 1
            stripped_comments += 1
            continue
        if ch.isdigit():
            start = i
            while i < n and src[i].isdigit():
                i += 1
            tokens.append(Token("INT", src[start:i], start, i))
            continue
        if ch.isalpha() or ch == "_":
            start = i
            while i < n and (src[i].isalnum() or src[i] == "_"):
                i += 1
            lex = src[start:i]
            kind = "KW_LET" if lex == "let" else "IDENT"
            tokens.append(Token(kind, lex, start, i))
            continue
        if ch in single:
            tokens.append(Token(single[ch], ch, i, i + 1))
            i += 1
            continue
        raise ValueError(f"unexpected character {ch!r} at {i}")

    tokens.append(Token("EOF", "", n, n))
    return LexResult(tokens, stripped_ws, stripped_comments)


@dataclass(frozen=True)
class CstRule:
    name: str
    children: tuple[CstRule | CstLeaf, ...]

    def node_count(self) -> int:
        return 1 + sum(c.node_count() for c in self.children)

    def terminal_yield(self) -> list[str]:
        out: list[str] = []
        for c in self.children:
            out.extend(c.terminal_yield())
        return out

    def render_tree(self) -> str:
        lines: list[str] = []
        _fmt_cst(self, "", True, True, lines)
        return "\n".join(lines)


@dataclass(frozen=True)
class CstLeaf:
    token: Token

    def node_count(self) -> int:
        return 1

    def terminal_yield(self) -> list[str]:
        return [self.token.lexeme]


CstNode = CstRule | CstLeaf


def _fmt_cst(node: CstNode, prefix: str, is_last: bool, is_root: bool, out: list[str]) -> None:
    label = node.name if isinstance(node, CstRule) else f"{node.token.kind!r}({node.token.lexeme!r})"
    if is_root:
        out.append(label)
    else:
        branch = "└── " if is_last else "├── "
        out.append(f"{prefix}{branch}{label}")
    if isinstance(node, CstRule):
        child_prefix = "" if is_root else prefix + ("    " if is_last else "│   ")
        for idx, child in enumerate(node.children):
            _fmt_cst(child, child_prefix, idx + 1 == len(node.children), False, out)


def leftmost_derivation(cst: CstNode) -> list[str]:
    """Generate the leftmost derivation (`=>_lm`) steps represented by `cst` (§2.2.3)."""
    form: list[tuple[str, CstNode | str]] = [("node", cst)]

    def fmt_form() -> str:
        parts = []
        for tag, val in form:
            if tag == "done":
                parts.append(str(val))
            elif isinstance(val, CstRule):
                parts.append(val.name)
            elif isinstance(val, CstLeaf):
                parts.append(val.token.lexeme)
        return " ".join(parts)

    steps = [fmt_form()]
    while True:
        idx = next(
            (i for i, (tag, val) in enumerate(form) if tag == "node" and isinstance(val, CstRule)),
            None,
        )
        if idx is None:
            break
        rule_node = form[idx][1]
        assert isinstance(rule_node, CstRule)
        repl: list[tuple[str, CstNode | str]] = []
        for child in rule_node.children:
            if isinstance(child, CstLeaf):
                repl.append(("done", child.token.lexeme))
            else:
                repl.append(("node", child))
        form[idx : idx + 1] = repl
        steps.append(fmt_form())
    return steps


class CstParser:
    def __init__(self, tokens: list[Token]) -> None:
        self.tokens = tokens
        self.pos = 0

    def peek(self) -> Token:
        return self.tokens[self.pos]

    def bump(self) -> Token:
        tok = self.tokens[self.pos]
        if tok.kind != "EOF":
            self.pos += 1
        return tok

    def expect(self, kind: str) -> CstLeaf:
        tok = self.peek()
        if tok.kind != kind:
            raise ValueError(f"expected {kind}, found {tok.kind} ({tok.lexeme!r})")
        return CstLeaf(self.bump())

    def parse_program(self) -> CstRule:
        stmts: list[CstNode] = []
        while self.peek().kind != "EOF":
            stmts.append(self.parse_stmt())
        return CstRule("Program", (CstRule("StmtList", tuple(stmts)),))

    def parse_stmt(self) -> CstRule:
        if self.peek().kind == "KW_LET":
            return CstRule(
                "Stmt",
                (
                    self.expect("KW_LET"),
                    self.expect("IDENT"),
                    self.expect("EQ"),
                    self.parse_expr(),
                    self.expect("SEMI"),
                ),
            )
        return CstRule("Stmt", (self.parse_expr(), self.expect("SEMI")))

    def parse_expr(self) -> CstRule:
        node = CstRule("Expr", (self.parse_term(),))
        while self.peek().kind in ("PLUS", "MINUS"):
            op = CstLeaf(self.bump())
            rhs = self.parse_term()
            node = CstRule("Expr", (node, op, rhs))
        return node

    def parse_term(self) -> CstRule:
        node = CstRule("Term", (self.parse_factor(),))
        while self.peek().kind in ("STAR", "SLASH"):
            op = CstLeaf(self.bump())
            rhs = self.parse_factor()
            node = CstRule("Term", (node, op, rhs))
        return node

    def parse_factor(self) -> CstRule:
        kind = self.peek().kind
        if kind == "LPAREN":
            return CstRule("Factor", (self.expect("LPAREN"), self.parse_expr(), self.expect("RPAREN")))
        if kind == "IDENT":
            return CstRule("Factor", (self.expect("IDENT"),))
        if kind == "INT":
            return CstRule("Factor", (self.expect("INT"),))
        raise ValueError(f"unexpected token {kind} in Factor")


# ---------------------------------------------------------------------------
# Abstract Syntax Tree (§2.5.1, §2.8.2, §5.3.1)
# ---------------------------------------------------------------------------

OP_PREC = {"+": 1, "-": 1, "*": 2, "/": 2}
OP_KIND = {"+": "PLUS", "-": "MINUS", "*": "STAR", "/": "SLASH"}


@dataclass(frozen=True)
class AstIdent:
    name: str

    def node_count(self) -> int:
        return 1

    def fmt_tree(self, prefix: str, is_last: bool, out: list[str]) -> None:
        branch = "└── " if is_last else "├── "
        out.append(f"{prefix}{branch}Name({self.name!r})")


@dataclass(frozen=True)
class AstInt:
    value: int

    def node_count(self) -> int:
        return 1

    def fmt_tree(self, prefix: str, is_last: bool, out: list[str]) -> None:
        branch = "└── " if is_last else "├── "
        out.append(f"{prefix}{branch}Number({self.value})")


@dataclass(frozen=True)
class AstBin:
    op: str
    left: AstExpr
    right: AstExpr

    def node_count(self) -> int:
        return 1 + self.left.node_count() + self.right.node_count()

    def fmt_tree(self, prefix: str, is_last: bool, out: list[str]) -> None:
        branch = "└── " if is_last else "├── "
        nxt = prefix + ("    " if is_last else "│   ")
        out.append(f"{prefix}{branch}BinOp({self.op!r})")
        self.left.fmt_tree(nxt, False, out)
        self.right.fmt_tree(nxt, True, out)


AstExpr = AstIdent | AstInt | AstBin


@dataclass(frozen=True)
class AstLet:
    name: str
    value: AstExpr

    def node_count(self) -> int:
        return 2 + self.value.node_count()

    def render_tree(self) -> str:
        lines = ["Let", f"├── Name({self.name!r})"]
        self.value.fmt_tree("", True, lines)
        return "\n".join(lines)


@dataclass(frozen=True)
class AstExprStmt:
    expr: AstExpr

    def node_count(self) -> int:
        return 1 + self.expr.node_count()

    def render_tree(self) -> str:
        lines = ["ExprStmt"]
        self.expr.fmt_tree("", True, lines)
        return "\n".join(lines)


AstStmt = AstLet | AstExprStmt


def program_ast_node_count(stmts: list[AstStmt]) -> int:
    return 1 + sum(s.node_count() for s in stmts)


def cst_to_ast(cst: CstRule) -> list[AstStmt]:
    if cst.name != "Program" or not cst.children:
        raise ValueError("expected Program root")
    stmt_list = cst.children[0]
    if not isinstance(stmt_list, CstRule) or stmt_list.name != "StmtList":
        raise ValueError("expected StmtList")
    return [_lower_stmt(s) for s in stmt_list.children]


def _lower_stmt(node: CstNode) -> AstStmt:
    if not isinstance(node, CstRule) or node.name != "Stmt":
        raise ValueError("expected Stmt")
    ch = node.children
    if (
        len(ch) == 5
        and isinstance(ch[0], CstLeaf)
        and ch[0].token.kind == "KW_LET"
        and isinstance(ch[1], CstLeaf)
        and ch[1].token.kind == "IDENT"
    ):
        return AstLet(ch[1].token.lexeme, _lower_expr(ch[3]))
    if len(ch) == 2:
        return AstExprStmt(_lower_expr(ch[0]))
    raise ValueError("malformed Stmt CST node")


def _lower_expr(node: CstNode) -> AstExpr:
    if not isinstance(node, CstRule):
        raise ValueError("expected Rule node")
    ch = node.children
    if node.name in ("Expr", "Term"):
        if len(ch) == 1:
            return _lower_expr(ch[0])
        if len(ch) == 3 and isinstance(ch[1], CstLeaf):
            return AstBin(ch[1].token.lexeme, _lower_expr(ch[0]), _lower_expr(ch[2]))
    if node.name == "Factor":
        if len(ch) == 1 and isinstance(ch[0], CstLeaf):
            tok = ch[0].token
            if tok.kind == "IDENT":
                return AstIdent(tok.lexeme)
            if tok.kind == "INT":
                return AstInt(int(tok.lexeme))
        if (
            len(ch) == 3
            and isinstance(ch[0], CstLeaf)
            and ch[0].token.kind == "LPAREN"
            and isinstance(ch[2], CstLeaf)
            and ch[2].token.kind == "RPAREN"
        ):
            return _lower_expr(ch[1])
    raise ValueError(f"malformed CST node {node.name}")


# ---------------------------------------------------------------------------
# AST -> Canonical CST & Formatted Source
# ---------------------------------------------------------------------------

def _synth(kind: str, lexeme: str) -> CstLeaf:
    return CstLeaf(Token(kind, lexeme, 0, 0))


def _needs_parens(child: AstExpr, parent_op: str, is_right: bool) -> bool:
    if not isinstance(child, AstBin):
        return False
    if OP_PREC[child.op] < OP_PREC[parent_op]:
        return True
    return is_right and OP_PREC[child.op] == OP_PREC[parent_op] and parent_op in ("-", "/")


def _wrap_parens(expr: AstExpr) -> CstRule:
    return CstRule("Factor", (_synth("LPAREN", "("), _expr_to_cst(expr, 1), _synth("RPAREN", ")")))


def _expr_to_cst(expr: AstExpr, level: int) -> CstRule:
    if level == 1:
        if isinstance(expr, AstBin) and OP_PREC[expr.op] == 1:
            rhs = (
                CstRule("Term", (_wrap_parens(expr.right),))
                if _needs_parens(expr.right, expr.op, True)
                else _expr_to_cst(expr.right, 2)
            )
            return CstRule("Expr", (_expr_to_cst(expr.left, 1), _synth(OP_KIND[expr.op], expr.op), rhs))
        return CstRule("Expr", (_expr_to_cst(expr, 2),))
    if level == 2:
        if isinstance(expr, AstBin) and OP_PREC[expr.op] == 2:
            lhs = (
                CstRule("Term", (_wrap_parens(expr.left),))
                if _needs_parens(expr.left, expr.op, False)
                else _expr_to_cst(expr.left, 2)
            )
            rhs = (
                _wrap_parens(expr.right)
                if _needs_parens(expr.right, expr.op, True)
                else _expr_to_cst(expr.right, 3)
            )
            return CstRule("Term", (lhs, _synth(OP_KIND[expr.op], expr.op), rhs))
        return CstRule("Term", (_expr_to_cst(expr, 3),))
    if isinstance(expr, AstIdent):
        return CstRule("Factor", (_synth("IDENT", expr.name),))
    if isinstance(expr, AstInt):
        return CstRule("Factor", (_synth("INT", str(expr.value)),))
    return _wrap_parens(expr)


def ast_to_canonical_cst(stmts: list[AstStmt]) -> CstRule:
    nodes: list[CstNode] = []
    for stmt in stmts:
        if isinstance(stmt, AstLet):
            nodes.append(
                CstRule(
                    "Stmt",
                    (
                        _synth("KW_LET", "let"),
                        _synth("IDENT", stmt.name),
                        _synth("EQ", "="),
                        _expr_to_cst(stmt.value, 1),
                        _synth("SEMI", ";"),
                    ),
                )
            )
        else:
            nodes.append(CstRule("Stmt", (_expr_to_cst(stmt.expr, 1), _synth("SEMI", ";"))))
    return CstRule("Program", (CstRule("StmtList", tuple(nodes)),))


def pretty_print_cst(cst: CstRule) -> str:
    stmt_list = cst.children[0]
    assert isinstance(stmt_list, CstRule)
    lines = []
    for stmt in stmt_list.children:
        toks = stmt.terminal_yield()
        out: list[str] = []
        for idx, tok in enumerate(toks):
            if idx > 0:
                prev = toks[idx - 1]
                if tok not in (";", ")") and prev != "(":
                    out.append(" ")
            out.append(tok)
        lines.append("".join(out))
    return "\n".join(lines)


def eval_expr(expr: AstExpr, env: dict[str, int]) -> int:
    if isinstance(expr, AstIdent):
        return env[expr.name]
    if isinstance(expr, AstInt):
        return expr.value
    l = eval_expr(expr.left, env)
    r = eval_expr(expr.right, env)
    if expr.op == "+":
        return l + r
    if expr.op == "-":
        return l - r
    if expr.op == "*":
        return l * r
    return l // r


def eval_program(stmts: list[AstStmt]) -> tuple[dict[str, int], int | None]:
    env: dict[str, int] = {}
    last = None
    for s in stmts:
        if isinstance(s, AstLet):
            v = eval_expr(s.value, env)
            env[s.name] = v
            last = v
        else:
            last = eval_expr(s.expr, env)
    return env, last


# ---------------------------------------------------------------------------
# Purple §6.1 Variant: Value-Numbered Syntax DAG
# ---------------------------------------------------------------------------

class ExprDag:
    def __init__(self) -> None:
        self.nodes: list[tuple] = []
        self.intern: dict[tuple, int] = {}

    def insert(self, expr: AstExpr) -> int:
        if isinstance(expr, AstIdent):
            key: tuple = ("Name", expr.name)
        elif isinstance(expr, AstInt):
            key = ("Number", expr.value)
        else:
            l = self.insert(expr.left)
            r = self.insert(expr.right)
            key = ("Bin", expr.op, l, r)
        if key in self.intern:
            return self.intern[key]
        idx = len(self.nodes)
        self.nodes.append(key)
        self.intern[key] = idx
        return idx


def main() -> None:
    sample = "let x  =  42;  // important comment"
    lexed = lex_strip_trivia(sample)
    cst = CstParser(lexed.tokens).parse_program()
    ast = cst_to_ast(cst)
    canonical = ast_to_canonical_cst(ast)
    rebuilt = pretty_print_cst(canonical)

    print("=== 1. Classical §2.6.1 Lexing Drops Trivia Before Parsing ===")
    print(f"Original source ({len(sample)} bytes): {sample!r}")
    print(f"Stripped: {lexed.stripped_whitespace_bytes} whitespace bytes, {lexed.stripped_comments} comments")
    print(f"\n=== 2. Classical CST / Parse Tree (§2.2.3) ({cst.node_count()} nodes) ===")
    print(cst.render_tree())
    print("\nLeftmost derivation (=>_lm):")
    for idx, step in enumerate(leftmost_derivation(cst)):
        print(f"  {idx:2d}. {step}")

    print(f"\n=== 3. Abstract Syntax Tree (§2.5.1, §2.8.2, §5.3.1) ({program_ast_node_count(ast)} nodes) ===")
    print(ast[0].render_tree())

    print("\n=== 4. Classical CST Is NOT Lossless ===")
    print(f"Rebuilt canonical CST yield: {rebuilt!r}")
    print(f"Byte-for-byte match? {rebuilt == sample}")

    dag_src = "let a = 2; let b = 3; (a + b) * 4 + (a + b) * 4;"
    dag_cst = CstParser(lex_strip_trivia(dag_src).tokens).parse_program()
    dag_ast = cst_to_ast(dag_cst)
    shared = dag_ast[2]
    assert isinstance(shared, AstExprStmt)
    dag = ExprDag()
    root_id = dag.insert(shared.expr)
    _, val = eval_program(dag_ast)
    print("\n=== 5. §6.1 Syntax-Tree Variant: Tree AST vs. Value-Numbered DAG ===")
    print(
        f"Expression: '(a + b) * 4 + (a + b) * 4' -> AST nodes = {shared.expr.node_count()}, "
        f"DAG nodes = {len(dag.nodes)}, root = #{root_id}, eval = {val}"
    )


class CstVsAstTests(unittest.TestCase):
    def test_classical_cst_drops_trivia_and_cannot_roundtrip_verbatim(self) -> None:
        src = "let x  =  42;  // important comment"
        lexed = lex_strip_trivia(src)
        self.assertEqual(lexed.stripped_whitespace_bytes, 7)
        self.assertEqual(lexed.stripped_comments, 1)
        cst = CstParser(lexed.tokens).parse_program()
        ast = cst_to_ast(cst)
        self.assertEqual(ast, [AstLet("x", AstInt(42))])
        self.assertGreater(cst.node_count(), program_ast_node_count(ast))
        rebuilt = pretty_print_cst(ast_to_canonical_cst(ast))
        self.assertEqual(rebuilt, "let x = 42;")
        self.assertNotEqual(rebuilt, src)

    def test_canonical_roundtrip_preserves_ast_and_eval(self) -> None:
        src = "let x = (1 + 2) * 3; x - (4 - 1);"
        ast = cst_to_ast(CstParser(lex_strip_trivia(src).tokens).parse_program())
        formatted = pretty_print_cst(ast_to_canonical_cst(ast))
        self.assertEqual(formatted, "let x = (1 + 2) * 3;\nx - (4 - 1);")
        reast = cst_to_ast(CstParser(lex_strip_trivia(formatted).tokens).parse_program())
        self.assertEqual(reast, ast)
        env, last = eval_program(reast)
        self.assertEqual(env["x"], 9)
        self.assertEqual(last, 6)

    def test_leftmost_derivation(self) -> None:
        cst = CstParser(lex_strip_trivia("let x = 42;").tokens).parse_program()
        steps = leftmost_derivation(cst)
        self.assertEqual(steps[0], "Program")
        self.assertEqual(steps[-1], "let x = 42 ;")

    def test_dag_sharing(self) -> None:
        src = "let a = 2; let b = 3; (a + b) * 4 + (a + b) * 4;"
        ast = cst_to_ast(CstParser(lex_strip_trivia(src).tokens).parse_program())
        stmt = ast[2]
        assert isinstance(stmt, AstExprStmt)
        self.assertEqual(stmt.expr.node_count(), 11)
        dag = ExprDag()
        root = dag.insert(stmt.expr)
        self.assertEqual(len(dag.nodes), 6)
        self.assertEqual(root, 5)


if __name__ == "__main__":
    main()
