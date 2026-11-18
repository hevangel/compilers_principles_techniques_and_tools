"""Chapter 4 (§4.9) -> Chapters 5 & 6 Bridge: AST Construction & Representations.

Teaches:
1. Concrete Parse Tree (CST) vs. Abstract Syntax Tree (AST) representation and size,
2. Designing algebraic AST node types (`AstStmt`, `AstExpr`) with source `Span`s,
3. Constructing both a CST and an AST via Recursive Descent,
4. Constructing the *exact same* AST via Yacc-style LALR(1) semantic actions (`$N`),
5. Preserving merged source locations (`line:col` + byte range) for pointed diagnostics,
6. Walking the AST cleanly with the `AstVisitor` pattern (printer, stats, type/scope checker),
7. Lowering the semantically checked AST to Three-Address Code (TAC) IR and executing it.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import Generic, TypeVar
import unittest


# ---------------------------------------------------------------------------
# 1. Source Spans & Lexer
# ---------------------------------------------------------------------------


@dataclass(frozen=True)
class Span:
    start: int
    end: int
    line: int
    col: int

    def merge(self, other: Span) -> Span:
        if self.start <= other.start:
            return Span(
                start=self.start,
                end=max(self.end, other.end),
                line=self.line,
                col=self.col,
            )
        return Span(
            start=other.start,
            end=max(self.end, other.end),
            line=other.line,
            col=other.col,
        )


@dataclass(frozen=True)
class Token:
    kind: str  # let | emit | ident | int | = | + | - | * | / | ( | ) | ; | $
    lexeme: str
    span: Span


@dataclass(frozen=True)
class Diagnostic:
    span: Span
    message: str


class FrontEndError(Exception):
    def __init__(self, diag: Diagnostic) -> None:
        super().__init__(diag.message)
        self.diag = diag


def format_diagnostic(src: str, diag: Diagnostic) -> str:
    lines = src.splitlines()
    line_str = lines[diag.span.line - 1] if 0 <= diag.span.line - 1 < len(lines) else ""
    width = max(1, diag.span.end - diag.span.start)
    pad = " " * max(0, diag.span.col - 1)
    avail = max(1, len(line_str) - (diag.span.col - 1))
    carets = "^" * min(width, avail)
    return (
        f"error at line {diag.span.line}, col {diag.span.col}: {diag.message}\n"
        f"  | {line_str}\n"
        f"  | {pad}{carets}"
    )


def lex(src: str) -> list[Token]:
    single = {"=", "+", "-", "*", "/", "(", ")", ";"}
    i = 0
    n = len(src)
    line = 1
    line_start = 0
    out: list[Token] = []

    while i < n:
        ch = src[i]
        if ch == "\n":
            i += 1
            line += 1
            line_start = i
            continue
        if ch.isspace():
            i += 1
            continue
        start = i
        col = start - line_start + 1
        if ch.isalpha() or ch == "_":
            i += 1
            while i < n and (src[i].isalnum() or src[i] == "_"):
                i += 1
            lexeme = src[start:i]
            kind = lexeme if lexeme in ("let", "emit") else "ident"
            out.append(Token(kind=kind, lexeme=lexeme, span=Span(start, i, line, col)))
            continue
        if ch.isdigit():
            i += 1
            while i < n and src[i].isdigit():
                i += 1
            lexeme = src[start:i]
            out.append(Token(kind="int", lexeme=lexeme, span=Span(start, i, line, col)))
            continue
        if ch in single:
            i += 1
            out.append(Token(kind=ch, lexeme=ch, span=Span(start, i, line, col)))
            continue
        raise FrontEndError(
            Diagnostic(
                span=Span(start, i + 1, line, col),
                message=f"unexpected character '{ch}'",
            )
        )

    out.append(
        Token(
            kind="$",
            lexeme="",
            span=Span(n, n, line, max(1, n - line_start + 1)),
        )
    )
    return out


# ---------------------------------------------------------------------------
# 2. Concrete Parse Tree (CST) vs. Abstract Syntax Tree (AST) Nodes
# ---------------------------------------------------------------------------


@dataclass(frozen=True)
class CstRule:
    nonterminal: str
    children: tuple[CstNode, ...]


@dataclass(frozen=True)
class CstLeaf:
    token: Token


CstNode = CstRule | CstLeaf


def cst_node_count(node: CstNode) -> int:
    if isinstance(node, CstLeaf):
        return 1
    return 1 + sum(cst_node_count(ch) for ch in node.children)


def format_cst(node: CstNode, indent: int = 0) -> list[str]:
    pad = "  " * indent
    if isinstance(node, CstLeaf):
        return [f"{pad}token({node.token.kind}, {node.token.lexeme!r})"]
    lines = [f"{pad}{node.nonterminal}"]
    for ch in node.children:
        lines.extend(format_cst(ch, indent + 1))
    return lines


@dataclass(frozen=True)
class IntExpr:
    val: int
    span: Span


@dataclass(frozen=True)
class VarExpr:
    name: str
    span: Span


@dataclass(frozen=True)
class NegExpr:
    inner: AstExpr
    span: Span


@dataclass(frozen=True)
class BinExpr:
    op: str  # "+" | "-" | "*" | "/"
    left: AstExpr
    right: AstExpr
    span: Span


AstExpr = IntExpr | VarExpr | NegExpr | BinExpr


@dataclass(frozen=True)
class LetStmt:
    name: str
    expr: AstExpr
    span: Span


@dataclass(frozen=True)
class EmitStmt:
    expr: AstExpr
    span: Span


AstStmt = LetStmt | EmitStmt


@dataclass(frozen=True)
class AstProgram:
    stmts: tuple[AstStmt, ...]


# ---------------------------------------------------------------------------
# 3. Path A: Recursive-Descent AST & Concrete Parse Tree Builder
# ---------------------------------------------------------------------------


class RdParser:
    def __init__(self, tokens: list[Token]) -> None:
        self.tokens = tokens
        self.pos = 0

    def peek(self) -> Token:
        return self.tokens[self.pos]

    def advance(self) -> Token:
        tok = self.tokens[self.pos]
        if self.pos + 1 < len(self.tokens):
            self.pos += 1
        return tok

    def expect(self, kind: str) -> Token:
        tok = self.peek()
        if tok.kind == kind:
            return self.advance()
        raise FrontEndError(
            Diagnostic(
                span=tok.span,
                message=f"expected '{kind}', found '{tok.kind}'",
            )
        )

    def parse_program(self) -> tuple[CstNode, AstProgram]:
        cst_children: list[CstNode] = []
        stmts: list[AstStmt] = []
        while self.peek().kind != "$":
            cst_s, ast_s = self.parse_stmt()
            cst_children.append(cst_s)
            stmts.append(ast_s)
        return CstRule("Program", tuple(cst_children)), AstProgram(tuple(stmts))

    def parse_stmt(self) -> tuple[CstNode, AstStmt]:
        tok = self.peek()
        if tok.kind == "let":
            kw = self.advance()
            id_tok = self.expect("ident")
            eq_tok = self.expect("=")
            expr_cst, expr_ast = self.parse_expr()
            semi = self.expect(";")
            cst = CstRule(
                "Stmt(Let)",
                (CstLeaf(kw), CstLeaf(id_tok), CstLeaf(eq_tok), expr_cst, CstLeaf(semi)),
            )
            return cst, LetStmt(
                name=id_tok.lexeme, expr=expr_ast, span=kw.span.merge(semi.span)
            )
        if tok.kind == "emit":
            kw = self.advance()
            expr_cst, expr_ast = self.parse_expr()
            semi = self.expect(";")
            cst = CstRule("Stmt(Emit)", (CstLeaf(kw), expr_cst, CstLeaf(semi)))
            return cst, EmitStmt(expr=expr_ast, span=kw.span.merge(semi.span))
        raise FrontEndError(
            Diagnostic(
                span=tok.span, message=f"expected 'let' or 'emit', found '{tok.kind}'"
            )
        )

    def parse_expr(self) -> tuple[CstNode, AstExpr]:
        first_cst, ast = self.parse_term()
        cst_children: list[CstNode] = [first_cst]
        while self.peek().kind in ("+", "-"):
            op_tok = self.advance()
            rhs_cst, rhs_ast = self.parse_term()
            cst_children.extend([CstLeaf(op_tok), rhs_cst])
            ast = BinExpr(
                op=op_tok.kind,
                left=ast,
                right=rhs_ast,
                span=ast.span.merge(rhs_ast.span),
            )
        return CstRule("Expr", tuple(cst_children)), ast

    def parse_term(self) -> tuple[CstNode, AstExpr]:
        first_cst, ast = self.parse_unary()
        cst_children: list[CstNode] = [first_cst]
        while self.peek().kind in ("*", "/"):
            op_tok = self.advance()
            rhs_cst, rhs_ast = self.parse_unary()
            cst_children.extend([CstLeaf(op_tok), rhs_cst])
            ast = BinExpr(
                op=op_tok.kind,
                left=ast,
                right=rhs_ast,
                span=ast.span.merge(rhs_ast.span),
            )
        return CstRule("Term", tuple(cst_children)), ast

    def parse_unary(self) -> tuple[CstNode, AstExpr]:
        if self.peek().kind == "-":
            minus_tok = self.advance()
            inner_cst, inner_ast = self.parse_unary()
            return (
                CstRule("Unary", (CstLeaf(minus_tok), inner_cst)),
                NegExpr(inner=inner_ast, span=minus_tok.span.merge(inner_ast.span)),
            )
        atom_cst, ast = self.parse_atom()
        return CstRule("Unary", (atom_cst,)), ast

    def parse_atom(self) -> tuple[CstNode, AstExpr]:
        tok = self.peek()
        if tok.kind == "int":
            t = self.advance()
            return (
                CstRule("Factor", (CstLeaf(t),)),
                IntExpr(val=int(t.lexeme), span=t.span),
            )
        if tok.kind == "ident":
            t = self.advance()
            return (
                CstRule("Factor", (CstLeaf(t),)),
                VarExpr(name=t.lexeme, span=t.span),
            )
        if tok.kind == "(":
            lp = self.advance()
            inner_cst, inner_ast = self.parse_expr()
            rp = self.expect(")")
            return (
                CstRule("Factor(Paren)", (CstLeaf(lp), inner_cst, CstLeaf(rp))),
                inner_ast,
            )
        raise FrontEndError(
            Diagnostic(
                span=tok.span,
                message=f"expected expression operand, found '{tok.kind}'",
            )
        )


# ---------------------------------------------------------------------------
# 4. Path B: Yacc-Style LALR(1) Parser + Semantic Actions -> Identical AST
# ---------------------------------------------------------------------------


@dataclass(frozen=True)
class Prod:
    lhs: str
    rhs: tuple[tuple[bool, str], ...]  # (is_term, name)
    prec_level: int | None
    assoc_left: bool
    action_id: int


@dataclass
class LalrTables:
    prods: list[Prod]
    action: dict[tuple[int, str], tuple[str, int | None]]
    goto: dict[tuple[int, str], int]


def build_yacc_ast_tables() -> LalrTables:
    def T(s: str) -> tuple[bool, str]:
        return (True, s)

    def N(s: str) -> tuple[bool, str]:
        return (False, s)

    prods = [
        Prod("S'", (N("Program"),), None, True, 0),
        Prod("Program", (N("StmtList"),), None, True, 1),
        Prod("StmtList", (N("StmtList"), N("Stmt")), None, True, 2),
        Prod("StmtList", (N("Stmt"),), None, True, 3),
        Prod("Stmt", (T("let"), T("ident"), T("="), N("Expr"), T(";")), None, True, 4),
        Prod("Stmt", (T("emit"), N("Expr"), T(";")), None, True, 5),
        Prod("Expr", (N("Expr"), T("+"), N("Expr")), 1, True, 6),
        Prod("Expr", (N("Expr"), T("-"), N("Expr")), 1, True, 7),
        Prod("Expr", (N("Expr"), T("*"), N("Expr")), 2, True, 8),
        Prod("Expr", (N("Expr"), T("/"), N("Expr")), 2, True, 9),
        Prod("Expr", (T("-"), N("Expr")), 3, False, 10),
        Prod("Expr", (T("("), N("Expr"), T(")")), None, True, 11),
        Prod("Expr", (T("ident"),), None, True, 12),
        Prod("Expr", (T("int"),), None, True, 13),
    ]
    term_prec = {"+": 1, "-": 1, "*": 2, "/": 2}

    first: dict[str, set[str]] = {p.lhs: set() for p in prods}
    while True:
        changed = False
        for p in prods:
            is_t, name = p.rhs[0]
            if is_t:
                if name not in first[p.lhs]:
                    first[p.lhs].add(name)
                    changed = True
            else:
                for t in first[name]:
                    if t not in first[p.lhs]:
                        first[p.lhs].add(t)
                        changed = True
        if not changed:
            break

    def first_of_tail(tail: tuple[tuple[bool, str], ...], la: str) -> set[str]:
        if not tail:
            return {la}
        is_t, name = tail[0]
        return {name} if is_t else set(first[name])

    def closure(seed: frozenset[tuple[int, int, str]]) -> frozenset[tuple[int, int, str]]:
        cur = set(seed)
        while True:
            added: list[tuple[int, int, str]] = []
            for pi, dot, la in cur:
                p = prods[pi]
                if dot < len(p.rhs) and not p.rhs[dot][0]:
                    nt = p.rhs[dot][1]
                    las = first_of_tail(p.rhs[dot + 1 :], la)
                    for c_idx, cand in enumerate(prods):
                        if cand.lhs == nt:
                            for nla in las:
                                ni = (c_idx, 0, nla)
                                if ni not in cur:
                                    added.append(ni)
            if not added:
                return frozenset(cur)
            cur.update(added)

    def goto_set(
        items: frozenset[tuple[int, int, str]], sym: tuple[bool, str]
    ) -> frozenset[tuple[int, int, str]]:
        seed = frozenset(
            (pi, dot + 1, la)
            for pi, dot, la in items
            if dot < len(prods[pi].rhs) and prods[pi].rhs[dot] == sym
        )
        return closure(seed) if seed else frozenset()

    all_syms = sorted({s for p in prods for s in p.rhs})
    init = closure(frozenset({(0, 0, "$")}))
    lr1_states = [init]
    lr1_index = {init: 0}
    lr1_trans: dict[tuple[int, tuple[bool, str]], int] = {}

    q = 0
    while q < len(lr1_states):
        cur = lr1_states[q]
        for s in all_syms:
            nxt = goto_set(cur, s)
            if nxt:
                if nxt not in lr1_index:
                    lr1_index[nxt] = len(lr1_states)
                    lr1_states.append(nxt)
                lr1_trans[(q, s)] = lr1_index[nxt]
        q += 1

    core_map: dict[frozenset[tuple[int, int]], int] = {}
    lr1_to_lalr = [0] * len(lr1_states)
    lalr_items: list[set[tuple[int, int, str]]] = []
    for i, st in enumerate(lr1_states):
        core = frozenset((pi, dot) for pi, dot, _ in st)
        if core not in core_map:
            core_map[core] = len(lalr_items)
            lalr_items.append(set())
        lid = core_map[core]
        lr1_to_lalr[i] = lid
        lalr_items[lid].update(st)

    action: dict[tuple[int, str], tuple[str, int | None]] = {}
    goto: dict[tuple[int, str], int] = {}
    for (src, (is_t, sname)), dst in lr1_trans.items():
        lsrc, ldst = lr1_to_lalr[src], lr1_to_lalr[dst]
        if is_t:
            action[(lsrc, sname)] = ("S", ldst)
        else:
            goto[(lsrc, sname)] = ldst

    for lid, st in enumerate(lalr_items):
        for pi, dot, la in st:
            p = prods[pi]
            if dot == len(p.rhs):
                if pi == 0 and la == "$":
                    action[(lid, "$")] = ("A", None)
                    continue
                key = (lid, la)
                existing = action.get(key)
                if existing is None:
                    action[key] = ("R", pi)
                elif existing[0] == "S":
                    assert p.prec_level is not None
                    t_prec = term_prec[la]
                    if p.prec_level > t_prec or (
                        p.prec_level == t_prec and p.assoc_left
                    ):
                        action[key] = ("R", pi)
                elif existing[0] == "R":
                    assert existing[1] is not None
                    if pi < existing[1]:
                        action[key] = ("R", pi)

    return LalrTables(prods=prods, action=action, goto=goto)


def parse_yacc_ast(tables: LalrTables, tokens: list[Token]) -> AstProgram:
    states: list[int] = [0]
    vals: list[object] = []
    idx = 0

    while True:
        tok = tokens[idx]
        st = states[-1]
        act = tables.action.get((st, tok.kind))
        if act is None:
            raise FrontEndError(
                Diagnostic(
                    span=tok.span, message=f"Yacc syntax error near '{tok.kind}'"
                )
            )
        kind, arg = act
        if kind == "S":
            assert arg is not None
            states.append(arg)
            vals.append(tok)
            idx += 1
        elif kind == "R":
            assert arg is not None
            prod = tables.prods[arg]
            pop_n = len(prod.rhs)
            popped = vals[-pop_n:]
            del vals[-pop_n:]
            del states[-pop_n:]

            aid = prod.action_id
            if aid == 1:
                reduced: object = popped[0]
            elif aid == 2:
                assert isinstance(popped[0], list) and isinstance(
                    popped[1], (LetStmt, EmitStmt)
                )
                reduced = popped[0] + [popped[1]]
            elif aid == 3:
                reduced = [popped[0]]
            elif aid == 4:
                kw, id_t, _, expr, semi = popped
                assert isinstance(kw, Token) and isinstance(id_t, Token) and isinstance(semi, Token)
                reduced = LetStmt(
                    name=id_t.lexeme,
                    expr=expr,  # type: ignore[arg-type]
                    span=kw.span.merge(semi.span),
                )
            elif aid == 5:
                kw, expr, semi = popped
                assert isinstance(kw, Token) and isinstance(semi, Token)
                reduced = EmitStmt(
                    expr=expr,  # type: ignore[arg-type]
                    span=kw.span.merge(semi.span),
                )
            elif aid in (6, 7, 8, 9):
                l, _, r = popped
                op = {6: "+", 7: "-", 8: "*", 9: "/"}[aid]
                reduced = BinExpr(
                    op=op,
                    left=l,  # type: ignore[arg-type]
                    right=r,  # type: ignore[arg-type]
                    span=l.span.merge(r.span),  # type: ignore[attr-defined]
                )
            elif aid == 10:
                minus, inner = popped
                assert isinstance(minus, Token)
                reduced = NegExpr(
                    inner=inner,  # type: ignore[arg-type]
                    span=minus.span.merge(inner.span),  # type: ignore[attr-defined]
                )
            elif aid == 11:
                reduced = popped[1]
            elif aid == 12:
                t = popped[0]
                assert isinstance(t, Token)
                reduced = VarExpr(name=t.lexeme, span=t.span)
            else:
                t = popped[0]
                assert isinstance(t, Token)
                reduced = IntExpr(val=int(t.lexeme), span=t.span)

            top_st = states[-1]
            states.append(tables.goto[(top_st, prod.lhs)])
            vals.append(reduced)
        else:
            stmts = vals[-1] if vals else []
            return AstProgram(stmts=tuple(stmts))  # type: ignore[arg-type]


# ---------------------------------------------------------------------------
# 5. AST Visitor Pattern: Pretty Printer, Stats Counter, Semantic Checker
# ---------------------------------------------------------------------------

T = TypeVar("T")


class AstVisitor(Generic[T]):
    def visit_stmt(self, stmt: AstStmt) -> T:
        raise NotImplementedError

    def visit_expr(self, expr: AstExpr) -> T:
        raise NotImplementedError


class PrettyVisitor(AstVisitor[str]):
    def visit_stmt(self, stmt: AstStmt) -> str:
        if isinstance(stmt, LetStmt):
            return f"(let {stmt.name} {self.visit_expr(stmt.expr)} @L{stmt.span.line}:C{stmt.span.col})"
        return f"(emit {self.visit_expr(stmt.expr)} @L{stmt.span.line}:C{stmt.span.col})"

    def visit_expr(self, expr: AstExpr) -> str:
        if isinstance(expr, IntExpr):
            return str(expr.val)
        if isinstance(expr, VarExpr):
            return expr.name
        if isinstance(expr, NegExpr):
            return f"(- {self.visit_expr(expr.inner)})"
        return f"({expr.op} {self.visit_expr(expr.left)} {self.visit_expr(expr.right)})"


class AstStatsVisitor(AstVisitor[None]):
    def __init__(self) -> None:
        self.stmt_count = 0
        self.expr_count = 0

    def total_nodes(self) -> int:
        return 1 + self.stmt_count + self.expr_count

    def visit_stmt(self, stmt: AstStmt) -> None:
        self.stmt_count += 1
        self.visit_expr(stmt.expr)

    def visit_expr(self, expr: AstExpr) -> None:
        self.expr_count += 1
        if isinstance(expr, NegExpr):
            self.visit_expr(expr.inner)
        elif isinstance(expr, BinExpr):
            self.visit_expr(expr.left)
            self.visit_expr(expr.right)


class SemanticChecker(AstVisitor[None]):
    def __init__(self) -> None:
        self.declared: set[str] = set()
        self.errors: list[Diagnostic] = []

    def visit_stmt(self, stmt: AstStmt) -> None:
        if isinstance(stmt, LetStmt):
            self.visit_expr(stmt.expr)
            self.declared.add(stmt.name)
        else:
            self.visit_expr(stmt.expr)

    def visit_expr(self, expr: AstExpr) -> None:
        if isinstance(expr, IntExpr):
            return
        if isinstance(expr, VarExpr):
            if expr.name not in self.declared:
                self.errors.append(
                    Diagnostic(
                        span=expr.span,
                        message=f"use of undeclared variable '{expr.name}'",
                    )
                )
        elif isinstance(expr, NegExpr):
            self.visit_expr(expr.inner)
        elif isinstance(expr, BinExpr):
            self.visit_expr(expr.left)
            self.visit_expr(expr.right)
            if expr.op == "/" and isinstance(expr.right, IntExpr) and expr.right.val == 0:
                self.errors.append(
                    Diagnostic(span=expr.span, message="division by constant zero")
                )


# ---------------------------------------------------------------------------
# 6. Chapter 6 Bridge: Lowering Checked AST -> Three-Address Code (TAC) + VM
# ---------------------------------------------------------------------------


@dataclass(frozen=True)
class TacInst:
    kind: str  # Const | Copy | Neg | Bin | Emit
    dst: str = ""
    src: str = ""
    op: str = ""
    left: str = ""
    right: str = ""
    val: int = 0

    def __str__(self) -> str:
        if self.kind == "Const":
            return f"{self.dst} := {self.val}"
        if self.kind == "Copy":
            return f"{self.dst} := {self.src}"
        if self.kind == "Neg":
            return f"{self.dst} := -{self.src}"
        if self.kind == "Bin":
            return f"{self.dst} := {self.left} {self.op} {self.right}"
        return f"emit {self.src}"


class TacLowerer(AstVisitor[str]):
    def __init__(self) -> None:
        self.next_temp = 0
        self.code: list[TacInst] = []

    def fresh(self) -> str:
        t = f"t{self.next_temp}"
        self.next_temp += 1
        return t

    def lower_program(self, prog: AstProgram) -> list[TacInst]:
        for s in prog.stmts:
            self.visit_stmt(s)
        return list(self.code)

    def visit_stmt(self, stmt: AstStmt) -> str:
        if isinstance(stmt, LetStmt):
            rhs = self.visit_expr(stmt.expr)
            self.code.append(TacInst(kind="Copy", dst=stmt.name, src=rhs))
            return stmt.name
        v = self.visit_expr(stmt.expr)
        self.code.append(TacInst(kind="Emit", src=v))
        return v

    def visit_expr(self, expr: AstExpr) -> str:
        if isinstance(expr, IntExpr):
            t = self.fresh()
            self.code.append(TacInst(kind="Const", dst=t, val=expr.val))
            return t
        if isinstance(expr, VarExpr):
            return expr.name
        if isinstance(expr, NegExpr):
            v = self.visit_expr(expr.inner)
            t = self.fresh()
            self.code.append(TacInst(kind="Neg", dst=t, src=v))
            return t
        l = self.visit_expr(expr.left)
        r = self.visit_expr(expr.right)
        t = self.fresh()
        self.code.append(TacInst(kind="Bin", dst=t, op=expr.op, left=l, right=r))
        return t


def eval_tac(code: list[TacInst]) -> list[int]:
    regs: dict[str, int] = {}
    outputs: list[int] = []
    for inst in code:
        if inst.kind == "Const":
            regs[inst.dst] = inst.val
        elif inst.kind == "Copy":
            regs[inst.dst] = regs[inst.src]
        elif inst.kind == "Neg":
            regs[inst.dst] = -regs[inst.src]
        elif inst.kind == "Bin":
            l, r = regs[inst.left], regs[inst.right]
            if inst.op == "+":
                regs[inst.dst] = l + r
            elif inst.op == "-":
                regs[inst.dst] = l - r
            elif inst.op == "*":
                regs[inst.dst] = l * r
            else:
                regs[inst.dst] = int(l / r)
        else:
            outputs.append(regs[inst.src])
    return outputs


def main() -> None:
    src = "let base = (2 + 3) * 4;\nemit base + -1;\n"
    tokens = lex(src)

    cst, rd_ast = RdParser(tokens).parse_program()
    tables = build_yacc_ast_tables()
    yacc_ast = parse_yacc_ast(tables, tokens)
    assert rd_ast == yacc_ast

    stats = AstStatsVisitor()
    pretty = PrettyVisitor()
    checker = SemanticChecker()
    for s in rd_ast.stmts:
        stats.visit_stmt(s)
        checker.visit_stmt(s)
    assert not checker.errors

    tac = TacLowerer().lower_program(rd_ast)
    out = eval_tac(tac)

    print("=== AST Construction: Parse Tree vs. AST, Visitors & IR (§4.9 -> Ch. 5–6) ===")
    print(
        f"Node count: Parse tree (CST) = {cst_node_count(cst)} nodes  vs.  "
        f"AST = {stats.total_nodes()} nodes"
    )
    print("\nConcrete Parse Tree (first 10 lines):")
    for line in format_cst(cst, 1)[:10]:
        print(line)
    print("\nAST via PrettyVisitor (RD == Yacc LALR(1) exact match, spans preserved):")
    for s in yacc_ast.stmts:
        print(f"  {pretty.visit_stmt(s)}")
    print(f"\nLowered Three-Address Code (TAC) -> output {out}:")
    for inst in tac:
        print(f"  {inst}")

    err_src = "let x = 10;\nemit x + missing;\n"
    err_tokens = lex(err_src)
    err_ast = parse_yacc_ast(tables, err_tokens)
    err_check = SemanticChecker()
    for s in err_ast.stmts:
        err_check.visit_stmt(s)
    print("\nSpan-pinned semantic error check:")
    for d in err_check.errors:
        print(format_diagnostic(err_src, d))


class AstConstructionTests(unittest.TestCase):
    def test_rd_and_yacc_build_identical_ast_smaller_than_parse_tree(self) -> None:
        src = "let a = (1 + 2) * 3 - -4;\nemit a / 2 + 1;\n"
        tokens = lex(src)
        cst, rd_ast = RdParser(tokens).parse_program()
        tables = build_yacc_ast_tables()
        yacc_ast = parse_yacc_ast(tables, tokens)
        self.assertEqual(rd_ast, yacc_ast)

        stats = AstStatsVisitor()
        checker = SemanticChecker()
        for s in rd_ast.stmts:
            stats.visit_stmt(s)
            checker.visit_stmt(s)
        self.assertEqual(checker.errors, [])
        self.assertGreater(cst_node_count(cst), stats.total_nodes() * 2)

        tac = TacLowerer().lower_program(rd_ast)
        self.assertEqual(eval_tac(tac), [7])

    def test_preserved_spans_pinpoint_semantic_errors_without_tokens(self) -> None:
        bad_src = "let x = 10;\nemit x + missing_var;\nemit 5 / 0;\n"
        tokens = lex(bad_src)
        tables = build_yacc_ast_tables()
        ast = parse_yacc_ast(tables, tokens)

        checker = SemanticChecker()
        for s in ast.stmts:
            checker.visit_stmt(s)
        self.assertEqual(len(checker.errors), 2)
        msg0 = format_diagnostic(bad_src, checker.errors[0])
        msg1 = format_diagnostic(bad_src, checker.errors[1])
        self.assertIn("line 2, col 10", msg0)
        self.assertIn("missing_var", msg0)
        self.assertIn("line 3, col 6", msg1)
        self.assertIn("division by constant zero", msg1)

    def test_visitor_pretty_print_reflects_associativity_and_precedence(self) -> None:
        src = "let z = 10 - 3 - 2 * 4;\n"
        tokens = lex(src)
        tables = build_yacc_ast_tables()
        ast = parse_yacc_ast(tables, tokens)

        rendered = PrettyVisitor().visit_stmt(ast.stmts[0])
        self.assertEqual(rendered, "(let z (- (- 10 3) (* 2 4)) @L1:C1)")
        tac = TacLowerer().lower_program(ast)
        self.assertGreaterEqual(len(tac), 4)


if __name__ == "__main__":
    main()
