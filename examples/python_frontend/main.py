"""Appendix A extension: Python-subset compiler front end written in Python.

Demonstrates off-side rule indentation lexing (`INDENT`/`DEDENT`/`NEWLINE` plus
implicit line joining inside brackets), compile-time local-vs-global scope
pre-scan (`UnboundLocalError` semantics), `/` vs `//` division, negative list
indexing, lowering to labeled Three-Address Code (TAC), and a TAC VM.
"""

from __future__ import annotations

from dataclasses import dataclass, field
import math
import unittest


class CompileError(Exception):
    """Raised on indentation, syntax, scope, or runtime VM errors."""


# ---------------------------------------------------------------------------
# 1. Off-Side Rule Indentation Lexer (INDENT / DEDENT / NEWLINE)
# ---------------------------------------------------------------------------


@dataclass(frozen=True)
class Token:
    kind: str
    value: object
    line: int


KEYWORDS = {
    "def": "DEF",
    "global": "GLOBAL",
    "if": "IF",
    "elif": "ELIF",
    "else": "ELSE",
    "while": "WHILE",
    "return": "RETURN",
    "and": "AND",
    "or": "OR",
    "not": "NOT",
    "True": "TRUE",
    "False": "FALSE",
    "None": "NONE",
    "emit": "EMIT",
    "len": "LEN",
}

TWO_CHAR_OPS = {
    "//": "FLOORDIV",
    "==": "EQEQ",
    "!=": "NE",
    "<=": "LE",
    ">=": "GE",
}

ONE_CHAR_OPS = {
    "<": "LT",
    ">": "GT",
    "=": "ASSIGN",
    "+": "PLUS",
    "-": "MINUS",
    "*": "STAR",
    "/": "SLASH",
    ":": "COLON",
    ",": "COMMA",
}


def lex(src: str) -> list[Token]:
    """Tokenizes Python-subset source using an indent stack and bracket-depth tracking."""
    i = 0
    n = len(src)
    line = 1
    indent_stack = [0]
    bracket_depth = 0
    at_line_start = True
    tokens: list[Token] = []

    while i < n:
        if at_line_start:
            col = 0
            while i < n:
                if src[i] == " ":
                    col += 1
                    i += 1
                elif src[i] == "\r":
                    i += 1
                elif src[i] == "\t":
                    raise CompileError(
                        f"line {line}: TabError: inconsistent use of tabs and spaces"
                    )
                else:
                    break
            if i >= n:
                break
            if src[i] == "\n":
                i += 1
                line += 1
                continue
            if src[i] == "#":
                while i < n and src[i] != "\n":
                    i += 1
                continue
            at_line_start = False
            if bracket_depth == 0:
                top = indent_stack[-1]
                if col > top:
                    indent_stack.append(col)
                    tokens.append(Token("INDENT", col, line))
                elif col < top:
                    while col < indent_stack[-1]:
                        indent_stack.pop()
                        tokens.append(Token("DEDENT", col, line))
                    if col != indent_stack[-1]:
                        raise CompileError(
                            f"line {line}: IndentationError: unindent does not match any outer level"
                        )
            continue

        ch = src[i]
        if ch in (" ", "\r"):
            i += 1
            continue
        if ch == "#":
            while i < n and src[i] != "\n":
                i += 1
            continue
        if ch == "\n":
            if bracket_depth == 0 and tokens and tokens[-1].kind not in ("NEWLINE", "DEDENT"):
                tokens.append(Token("NEWLINE", "\n", line))
            i += 1
            line += 1
            at_line_start = True
            continue
        if ch.isalpha() or ch == "_":
            start = i
            i += 1
            while i < n and (src[i].isalnum() or src[i] == "_"):
                i += 1
            word = src[start:i]
            kind = KEYWORDS.get(word, "IDENT")
            val: object = (
                True
                if kind == "TRUE"
                else False
                if kind == "FALSE"
                else None
                if kind == "NONE"
                else word
            )
            tokens.append(Token(kind, val, line))
            continue
        if ch.isdigit():
            start = i
            while i < n and src[i].isdigit():
                i += 1
            if i + 1 < n and src[i] == "." and src[i + 1].isdigit():
                i += 1
                while i < n and src[i].isdigit():
                    i += 1
                tokens.append(Token("FLOAT", float(src[start:i]), line))
            else:
                tokens.append(Token("INT", int(src[start:i]), line))
            continue
        if i + 1 < n and src[i : i + 2] in TWO_CHAR_OPS:
            op = src[i : i + 2]
            tokens.append(Token(TWO_CHAR_OPS[op], op, line))
            i += 2
            continue
        if ch == "(":
            bracket_depth += 1
            tokens.append(Token("LPAREN", ch, line))
            i += 1
            continue
        if ch == ")":
            if bracket_depth == 0:
                raise CompileError(f"line {line}: unmatched ')'")
            bracket_depth -= 1
            tokens.append(Token("RPAREN", ch, line))
            i += 1
            continue
        if ch == "[":
            bracket_depth += 1
            tokens.append(Token("LBRACKET", ch, line))
            i += 1
            continue
        if ch == "]":
            if bracket_depth == 0:
                raise CompileError(f"line {line}: unmatched ']'")
            bracket_depth -= 1
            tokens.append(Token("RBRACKET", ch, line))
            i += 1
            continue
        if ch in ONE_CHAR_OPS:
            tokens.append(Token(ONE_CHAR_OPS[ch], ch, line))
            i += 1
            continue
        raise CompileError(f"line {line}: SyntaxError: unexpected character '{ch}'")

    if bracket_depth > 0:
        raise CompileError(f"line {line}: SyntaxError: unclosed '(' or '['")
    if tokens and tokens[-1].kind not in ("NEWLINE", "DEDENT"):
        tokens.append(Token("NEWLINE", "\n", line))
    while len(indent_stack) > 1:
        indent_stack.pop()
        tokens.append(Token("DEDENT", 0, line))
    tokens.append(Token("EOF", None, line))
    return tokens


# ---------------------------------------------------------------------------
# 2. AST & Indentation-Driven Recursive-Descent Parser
# ---------------------------------------------------------------------------


@dataclass
class RawExpr:
    kind: str
    data: dict[str, object] = field(default_factory=dict)


@dataclass
class RawStmt:
    kind: str
    data: dict[str, object] = field(default_factory=dict)


@dataclass
class RawFn:
    name: str
    params: list[str]
    body: list[RawStmt]


class Parser:
    def __init__(self, toks: list[Token]) -> None:
        self.toks = toks
        self.pos = 0

    def peek(self) -> Token:
        return self.toks[self.pos]

    def peek_next(self) -> Token:
        if self.pos + 1 < len(self.toks):
            return self.toks[self.pos + 1]
        return self.toks[-1]

    def err(self, msg: str) -> CompileError:
        return CompileError(f"line {self.peek().line}: {msg}")

    def expect(self, kind: str) -> Token:
        tok = self.peek()
        if tok.kind != kind:
            raise self.err(f"expected {kind}, found {tok.kind}")
        self.pos += 1
        return tok

    def parse_module(self) -> tuple[list[RawFn], list[RawStmt]]:
        fns: list[RawFn] = []
        top_stmts: list[RawStmt] = []
        while self.peek().kind != "EOF":
            while self.peek().kind == "NEWLINE":
                self.pos += 1
            if self.peek().kind == "EOF":
                break
            if self.peek().kind == "DEF":
                fns.append(self.parse_def())
            else:
                top_stmts.append(self.parse_stmt())
        return fns, top_stmts

    def parse_def(self) -> RawFn:
        self.expect("DEF")
        name = str(self.expect("IDENT").value)
        self.expect("LPAREN")
        params: list[str] = []
        while self.peek().kind != "RPAREN":
            params.append(str(self.expect("IDENT").value))
            if self.peek().kind == "COMMA":
                self.pos += 1
            else:
                break
        self.expect("RPAREN")
        self.expect("COLON")
        body = self.parse_suite()
        return RawFn(name, params, body)

    def parse_suite(self) -> list[RawStmt]:
        self.expect("NEWLINE")
        self.expect("INDENT")
        stmts: list[RawStmt] = []
        while self.peek().kind not in ("DEDENT", "EOF"):
            while self.peek().kind == "NEWLINE":
                self.pos += 1
            if self.peek().kind == "DEDENT":
                break
            stmts.append(self.parse_stmt())
        self.expect("DEDENT")
        if not stmts:
            raise self.err("expected non-empty indented block")
        return stmts

    def parse_stmt(self) -> RawStmt:
        tok = self.peek()
        if tok.kind == "GLOBAL":
            self.pos += 1
            names: list[str] = []
            while True:
                names.append(str(self.expect("IDENT").value))
                if self.peek().kind == "COMMA":
                    self.pos += 1
                else:
                    break
            self.expect("NEWLINE")
            return RawStmt("global", {"names": names})
        if tok.kind == "IF":
            self.pos += 1
            cond = self.parse_expr()
            self.expect("COLON")
            branches: list[tuple[RawExpr, list[RawStmt]]] = [(cond, self.parse_suite())]
            while self.peek().kind == "ELIF":
                self.pos += 1
                ec = self.parse_expr()
                self.expect("COLON")
                branches.append((ec, self.parse_suite()))
            else_body: list[RawStmt] = []
            if self.peek().kind == "ELSE":
                self.pos += 1
                self.expect("COLON")
                else_body = self.parse_suite()
            return RawStmt("if", {"branches": branches, "else": else_body})
        if tok.kind == "WHILE":
            self.pos += 1
            cond = self.parse_expr()
            self.expect("COLON")
            return RawStmt("while", {"cond": cond, "body": self.parse_suite()})
        if tok.kind == "RETURN":
            self.pos += 1
            val = self.parse_expr() if self.peek().kind != "NEWLINE" else None
            self.expect("NEWLINE")
            return RawStmt("return", {"val": val})
        if tok.kind == "EMIT":
            self.pos += 1
            self.expect("LPAREN")
            e = self.parse_expr()
            self.expect("RPAREN")
            self.expect("NEWLINE")
            return RawStmt("emit", {"expr": e})
        if tok.kind == "IDENT" and self.peek_next().kind == "ASSIGN":
            name = str(tok.value)
            self.pos += 2
            rhs = self.parse_expr()
            self.expect("NEWLINE")
            return RawStmt("assign", {"name": name, "rhs": rhs})
        e = self.parse_expr()
        self.expect("NEWLINE")
        return RawStmt("expr", {"expr": e})

    def parse_expr(self) -> RawExpr:
        return self.parse_or()

    def parse_or(self) -> RawExpr:
        lhs = self.parse_and()
        while self.peek().kind == "OR":
            self.pos += 1
            lhs = RawExpr("or", {"left": lhs, "right": self.parse_and()})
        return lhs

    def parse_and(self) -> RawExpr:
        lhs = self.parse_not()
        while self.peek().kind == "AND":
            self.pos += 1
            lhs = RawExpr("and", {"left": lhs, "right": self.parse_not()})
        return lhs

    def parse_not(self) -> RawExpr:
        if self.peek().kind == "NOT":
            self.pos += 1
            return RawExpr("not", {"inner": self.parse_not()})
        return self.parse_rel()

    def parse_rel(self) -> RawExpr:
        lhs = self.parse_add()
        rel_map = {"EQEQ": "==", "NE": "!=", "LT": "<", "LE": "<=", "GT": ">", "GE": ">="}
        if self.peek().kind not in rel_map:
            return lhs
        op = rel_map[self.peek().kind]
        self.pos += 1
        return RawExpr("rel", {"op": op, "left": lhs, "right": self.parse_add()})

    def parse_add(self) -> RawExpr:
        lhs = self.parse_mul()
        while self.peek().kind in ("PLUS", "MINUS"):
            op = "+" if self.peek().kind == "PLUS" else "-"
            self.pos += 1
            lhs = RawExpr("bin", {"op": op, "left": lhs, "right": self.parse_mul()})
        return lhs

    def parse_mul(self) -> RawExpr:
        lhs = self.parse_unary()
        while self.peek().kind in ("STAR", "SLASH", "FLOORDIV"):
            kind = self.peek().kind
            op = "*" if kind == "STAR" else "/" if kind == "SLASH" else "//"
            self.pos += 1
            lhs = RawExpr("bin", {"op": op, "left": lhs, "right": self.parse_unary()})
        return lhs

    def parse_unary(self) -> RawExpr:
        if self.peek().kind == "MINUS":
            self.pos += 1
            return RawExpr("neg", {"inner": self.parse_unary()})
        return self.parse_postfix()

    def parse_postfix(self) -> RawExpr:
        base = self.parse_primary()
        while self.peek().kind == "LBRACKET":
            self.pos += 1
            idx = self.parse_expr()
            self.expect("RBRACKET")
            base = RawExpr("index", {"base": base, "idx": idx})
        return base

    def parse_primary(self) -> RawExpr:
        tok = self.peek()
        if tok.kind in ("INT", "FLOAT", "TRUE", "FALSE", "NONE"):
            self.pos += 1
            return RawExpr("lit", {"val": tok.value})
        if tok.kind == "LEN":
            self.pos += 1
            self.expect("LPAREN")
            inner = self.parse_expr()
            self.expect("RPAREN")
            return RawExpr("len", {"inner": inner})
        if tok.kind == "LBRACKET":
            self.pos += 1
            elems: list[RawExpr] = []
            while self.peek().kind != "RBRACKET":
                elems.append(self.parse_expr())
                if self.peek().kind == "COMMA":
                    self.pos += 1
                else:
                    break
            self.expect("RBRACKET")
            return RawExpr("list", {"elems": elems})
        if tok.kind == "LPAREN":
            self.pos += 1
            inner = self.parse_expr()
            self.expect("RPAREN")
            return inner
        if tok.kind == "IDENT":
            name = str(tok.value)
            self.pos += 1
            if self.peek().kind == "LPAREN":
                self.pos += 1
                args: list[RawExpr] = []
                while self.peek().kind != "RPAREN":
                    args.append(self.parse_expr())
                    if self.peek().kind == "COMMA":
                        self.pos += 1
                    else:
                        break
                self.expect("RPAREN")
                return RawExpr("call", {"name": name, "args": args})
            return RawExpr("name", {"name": name})
        raise self.err(f"SyntaxError: unexpected token {tok.kind}")


# ---------------------------------------------------------------------------
# 3. Symtable Pre-Scan & Three-Address Code (TAC) Lowering
# ---------------------------------------------------------------------------


@dataclass(frozen=True)
class FnSymtable:
    name: str
    params: list[str]
    locals: set[str]
    globals: set[str]


def _collect_targets(stmts: list[RawStmt], assigned: set[str], globals_set: set[str]) -> None:
    for st in stmts:
        if st.kind == "global":
            globals_set.update(st.data["names"])  # type: ignore[arg-type]
        elif st.kind == "assign":
            assigned.add(str(st.data["name"]))
        elif st.kind == "if":
            for _, body in st.data["branches"]:  # type: ignore[attr-defined]
                _collect_targets(body, assigned, globals_set)
            _collect_targets(st.data["else"], assigned, globals_set)  # type: ignore[arg-type]
        elif st.kind == "while":
            _collect_targets(st.data["body"], assigned, globals_set)  # type: ignore[arg-type]


def analyze_fn_scope(f: RawFn) -> FnSymtable:
    """Pre-scans `def` bodies so assigned names not marked `global` become locals."""
    assigned: set[str] = set()
    globals_set: set[str] = set()
    _collect_targets(f.body, assigned, globals_set)
    locals_set = set(f.params) | (assigned - globals_set)
    return FnSymtable(f.name, list(f.params), locals_set, globals_set)


@dataclass(frozen=True)
class Slot:
    is_local: bool
    key: str

    def __str__(self) -> str:
        return self.key if self.is_local else f"global::{self.key}"


@dataclass(frozen=True)
class Operand:
    slot: Slot | None
    lit: object = None

    def __str__(self) -> str:
        if self.slot is not None:
            return str(self.slot)
        if isinstance(self.lit, float):
            return f"{self.lit:.4f}"
        return repr(self.lit)


@dataclass(frozen=True)
class TacInst:
    op: str
    args: dict[str, object]

    def __str__(self) -> str:
        a = self.args
        if self.op == "label":
            return f"{a['name']}:"
        if self.op == "assign":
            return f"  {a['dst']} = {a['src']}"
        if self.op == "make_list":
            return f"  {a['dst']} = list({len(a['elems'])})"  # type: ignore[arg-type]
        if self.op == "len":
            return f"  {a['dst']} = len({a['base']})"
        if self.op == "list_load":
            return f"  {a['dst']} = {a['base']}[{a['idx']}]"
        if self.op == "neg":
            return f"  {a['dst']} = -{a['src']}"
        if self.op == "not":
            return f"  {a['dst']} = not {a['src']}"
        if self.op in ("bin", "rel"):
            return f"  {a['dst']} = {a['left']} {a['sym']} {a['right']}"
        if self.op == "param":
            return f"  param {a['op']}"
        if self.op == "call":
            return f"  {a['dst']} = call {a['func']}/{a['argc']}"
        if self.op == "iffalse":
            return f"  iffalse {a['cond']} goto {a['label']}"
        if self.op == "goto":
            return f"  goto {a['label']}"
        if self.op == "return":
            return f"  return {a['val'] if a['val'] is not None else 'None'}"
        return f"  emit {a['val']}"


class Emitter:
    def __init__(self) -> None:
        self.next_temp = 0
        self.next_lbl = 0
        self.code: list[TacInst] = []

    def temp(self) -> Slot:
        self.next_temp += 1
        return Slot(True, f"t{self.next_temp}")

    def label(self, prefix: str) -> str:
        self.next_lbl += 1
        return f"{prefix}_{self.next_lbl}"

    @staticmethod
    def resolve(name: str, sym: FnSymtable | None) -> Slot:
        if sym is not None and name in sym.locals:
            return Slot(True, f"{sym.name}::{name}")
        return Slot(False, name)

    def gen_expr(self, expr: RawExpr, sym: FnSymtable | None) -> Operand:
        k = expr.kind
        d = expr.data
        if k == "lit":
            return Operand(None, d["val"])
        if k == "name":
            return Operand(self.resolve(str(d["name"]), sym))
        if k == "list":
            ops = [self.gen_expr(e, sym) for e in d["elems"]]  # type: ignore[attr-defined]
            dst = self.temp()
            self.code.append(TacInst("make_list", {"dst": dst, "elems": ops}))
            return Operand(dst)
        if k == "len":
            base = self.gen_expr(d["inner"], sym)  # type: ignore[arg-type]
            dst = self.temp()
            self.code.append(TacInst("len", {"dst": dst, "base": base}))
            return Operand(dst)
        if k == "index":
            b = self.gen_expr(d["base"], sym)  # type: ignore[arg-type]
            i = self.gen_expr(d["idx"], sym)  # type: ignore[arg-type]
            dst = self.temp()
            self.code.append(TacInst("list_load", {"dst": dst, "base": b, "idx": i}))
            return Operand(dst)
        if k == "neg":
            src = self.gen_expr(d["inner"], sym)  # type: ignore[arg-type]
            dst = self.temp()
            self.code.append(TacInst("neg", {"dst": dst, "src": src}))
            return Operand(dst)
        if k == "not":
            src = self.gen_expr(d["inner"], sym)  # type: ignore[arg-type]
            dst = self.temp()
            self.code.append(TacInst("not", {"dst": dst, "src": src}))
            return Operand(dst)
        if k in ("bin", "rel"):
            l = self.gen_expr(d["left"], sym)  # type: ignore[arg-type]
            r = self.gen_expr(d["right"], sym)  # type: ignore[arg-type]
            dst = self.temp()
            self.code.append(TacInst(k, {"dst": dst, "sym": d["op"], "left": l, "right": r}))
            return Operand(dst)
        if k == "and":
            dst = self.temp()
            l_false = self.label("and_false")
            l_end = self.label("and_end")
            l = self.gen_expr(d["left"], sym)  # type: ignore[arg-type]
            self.code.append(TacInst("iffalse", {"cond": l, "label": l_false}))
            r = self.gen_expr(d["right"], sym)  # type: ignore[arg-type]
            self.code.append(TacInst("assign", {"dst": dst, "src": r}))
            self.code.append(TacInst("goto", {"label": l_end}))
            self.code.append(TacInst("label", {"name": l_false}))
            self.code.append(TacInst("assign", {"dst": dst, "src": Operand(None, False)}))
            self.code.append(TacInst("label", {"name": l_end}))
            return Operand(dst)
        if k == "or":
            dst = self.temp()
            l_rhs = self.label("or_rhs")
            l_end = self.label("or_end")
            l = self.gen_expr(d["left"], sym)  # type: ignore[arg-type]
            self.code.append(TacInst("iffalse", {"cond": l, "label": l_rhs}))
            self.code.append(TacInst("assign", {"dst": dst, "src": Operand(None, True)}))
            self.code.append(TacInst("goto", {"label": l_end}))
            self.code.append(TacInst("label", {"name": l_rhs}))
            r = self.gen_expr(d["right"], sym)  # type: ignore[arg-type]
            self.code.append(TacInst("assign", {"dst": dst, "src": r}))
            self.code.append(TacInst("label", {"name": l_end}))
            return Operand(dst)
        ops = [self.gen_expr(a, sym) for a in d["args"]]  # type: ignore[attr-defined]
        for op in ops:
            self.code.append(TacInst("param", {"op": op}))
        dst = self.temp()
        self.code.append(TacInst("call", {"dst": dst, "func": d["name"], "argc": len(ops)}))
        return Operand(dst)

    def gen_stmt(self, stmt: RawStmt, sym: FnSymtable | None) -> None:
        k = stmt.kind
        d = stmt.data
        if k == "global":
            return
        if k == "assign":
            v = self.gen_expr(d["rhs"], sym)  # type: ignore[arg-type]
            self.code.append(TacInst("assign", {"dst": self.resolve(str(d["name"]), sym), "src": v}))
        elif k == "if":
            l_end = self.label("endif")
            for cond, body in d["branches"]:  # type: ignore[attr-defined]
                l_next = self.label("elif")
                c = self.gen_expr(cond, sym)
                self.code.append(TacInst("iffalse", {"cond": c, "label": l_next}))
                for s in body:
                    self.gen_stmt(s, sym)
                self.code.append(TacInst("goto", {"label": l_end}))
                self.code.append(TacInst("label", {"name": l_next}))
            for s in d["else"]:  # type: ignore[attr-defined]
                self.gen_stmt(s, sym)
            self.code.append(TacInst("label", {"name": l_end}))
        elif k == "while":
            l_top = self.label("while")
            l_end = self.label("endwhile")
            self.code.append(TacInst("label", {"name": l_top}))
            c = self.gen_expr(d["cond"], sym)  # type: ignore[arg-type]
            self.code.append(TacInst("iffalse", {"cond": c, "label": l_end}))
            for s in d["body"]:  # type: ignore[attr-defined]
                self.gen_stmt(s, sym)
            self.code.append(TacInst("goto", {"label": l_top}))
            self.code.append(TacInst("label", {"name": l_end}))
        elif k == "return":
            opt = d["val"]
            v = self.gen_expr(opt, sym) if opt is not None else None  # type: ignore[arg-type]
            self.code.append(TacInst("return", {"val": v}))
        elif k == "emit":
            self.code.append(TacInst("emit", {"val": self.gen_expr(d["expr"], sym)}))  # type: ignore[arg-type]
        else:
            _ = self.gen_expr(d["expr"], sym)  # type: ignore[arg-type]


@dataclass
class CompiledPython:
    symtables: dict[str, FnSymtable]
    fn_params: dict[str, list[str]]
    code: list[TacInst]


def compile_python_subset(src: str) -> CompiledPython:
    toks = lex(src)
    fns, top_stmts = Parser(toks).parse_module()
    emitter = Emitter()
    symtables: dict[str, FnSymtable] = {}
    fn_params: dict[str, list[str]] = {}

    for f in fns:
        if f.name in symtables:
            raise CompileError(f"duplicate function '{f.name}'")
        st = analyze_fn_scope(f)
        fn_params[f.name] = [f"{f.name}::{p}" for p in f.params]
        emitter.code.append(TacInst("label", {"name": f"fn_{f.name}"}))
        for s in f.body:
            emitter.gen_stmt(s, st)
        emitter.code.append(TacInst("return", {"val": None}))
        symtables[f.name] = st

    emitter.code.append(TacInst("label", {"name": "__module__"}))
    for s in top_stmts:
        emitter.gen_stmt(s, None)
    emitter.code.append(TacInst("return", {"val": None}))
    return CompiledPython(dict(sorted(symtables.items())), fn_params, emitter.code)


# ---------------------------------------------------------------------------
# 4. Python-Semantics TAC Virtual Machine
# ---------------------------------------------------------------------------


def exec_tac(prog: CompiledPython) -> tuple[dict[str, object], list[object]]:
    labels = {
        str(inst.args["name"]): idx
        for idx, inst in enumerate(prog.code)
        if inst.op == "label"
    }

    def read_slot(slot: Slot, locals_map: dict[str, object], globals_map: dict[str, object]) -> object:
        if slot.is_local:
            if slot.key not in locals_map:
                short = slot.key.split("::")[-1]
                raise CompileError(
                    f"UnboundLocalError: local variable '{short}' referenced before assignment"
                )
            return locals_map[slot.key]
        if slot.key not in globals_map:
            raise CompileError(f"NameError: name '{slot.key}' is not defined")
        return globals_map[slot.key]

    def write_slot(
        slot: Slot, val: object, locals_map: dict[str, object], globals_map: dict[str, object]
    ) -> None:
        if slot.is_local:
            locals_map[slot.key] = val
        else:
            globals_map[slot.key] = val

    def eval_op(op: Operand, locals_map: dict[str, object], globals_map: dict[str, object]) -> object:
        return read_slot(op.slot, locals_map, globals_map) if op.slot is not None else op.lit

    globals_map: dict[str, object] = {}
    frames: list[tuple[dict[str, object], int, Slot | None]] = [({}, len(prog.code), None)]
    pc = labels["__module__"]
    param_buf: list[object] = []
    emitted: list[object] = []

    while pc < len(prog.code):
        inst = prog.code[pc]
        a = inst.args
        locals_top = frames[-1][0]
        if inst.op == "label":
            pc += 1
        elif inst.op == "assign":
            v = eval_op(a["src"], locals_top, globals_map)  # type: ignore[arg-type]
            write_slot(a["dst"], v, locals_top, globals_map)  # type: ignore[arg-type]
            pc += 1
        elif inst.op == "make_list":
            vs = [eval_op(e, locals_top, globals_map) for e in a["elems"]]  # type: ignore[attr-defined]
            write_slot(a["dst"], vs, locals_top, globals_map)  # type: ignore[arg-type]
            pc += 1
        elif inst.op == "len":
            base = eval_op(a["base"], locals_top, globals_map)  # type: ignore[arg-type]
            if not isinstance(base, list):
                raise CompileError("TypeError: len() requires a list")
            write_slot(a["dst"], len(base), locals_top, globals_map)  # type: ignore[arg-type]
            pc += 1
        elif inst.op == "list_load":
            base = eval_op(a["base"], locals_top, globals_map)  # type: ignore[arg-type]
            raw_i = eval_op(a["idx"], locals_top, globals_map)  # type: ignore[arg-type]
            if not isinstance(base, list) or type(raw_i) is not int:
                raise CompileError("TypeError: invalid list subscript")
            norm = len(base) + raw_i if raw_i < 0 else raw_i
            if norm < 0 or norm >= len(base):
                raise CompileError(f"IndexError: list index {raw_i} out of range")
            write_slot(a["dst"], base[norm], locals_top, globals_map)  # type: ignore[arg-type]
            pc += 1
        elif inst.op == "neg":
            v = eval_op(a["src"], locals_top, globals_map)  # type: ignore[arg-type]
            write_slot(a["dst"], -v, locals_top, globals_map)  # type: ignore[operator,arg-type]
            pc += 1
        elif inst.op == "not":
            v = eval_op(a["src"], locals_top, globals_map)  # type: ignore[arg-type]
            write_slot(a["dst"], not bool(v), locals_top, globals_map)  # type: ignore[arg-type]
            pc += 1
        elif inst.op == "bin":
            l = eval_op(a["left"], locals_top, globals_map)  # type: ignore[arg-type]
            r = eval_op(a["right"], locals_top, globals_map)  # type: ignore[arg-type]
            sym = a["sym"]
            if sym == "/":
                if r == 0:
                    raise CompileError("ZeroDivisionError: float division by zero")
                out = l / r  # type: ignore[operator]
            elif sym == "//":
                if r == 0:
                    raise CompileError("ZeroDivisionError: integer division or modulo by zero")
                out = l // r if (type(l) is int and type(r) is int) else math.floor(l / r)  # type: ignore[operator]
            elif sym == "+":
                out = l + r  # type: ignore[operator]
            elif sym == "-":
                out = l - r  # type: ignore[operator]
            else:
                out = l * r  # type: ignore[operator]
            write_slot(a["dst"], out, locals_top, globals_map)  # type: ignore[arg-type]
            pc += 1
        elif inst.op == "rel":
            l = eval_op(a["left"], locals_top, globals_map)  # type: ignore[arg-type]
            r = eval_op(a["right"], locals_top, globals_map)  # type: ignore[arg-type]
            sym = a["sym"]
            if sym == "==":
                b = l == r
            elif sym == "!=":
                b = l != r
            elif sym == "<":
                b = l < r  # type: ignore[operator]
            elif sym == "<=":
                b = l <= r  # type: ignore[operator]
            elif sym == ">":
                b = l > r  # type: ignore[operator]
            else:
                b = l >= r  # type: ignore[operator]
            write_slot(a["dst"], b, locals_top, globals_map)  # type: ignore[arg-type]
            pc += 1
        elif inst.op == "param":
            param_buf.append(eval_op(a["op"], locals_top, globals_map))  # type: ignore[arg-type]
            pc += 1
        elif inst.op == "call":
            func = str(a["func"])
            if func not in prog.fn_params:
                raise CompileError(f"NameError: function '{func}' is not defined")
            argc = int(a["argc"])  # type: ignore[arg-type]
            params = prog.fn_params[func]
            if len(params) != argc:
                raise CompileError(
                    f"TypeError: {func}() takes {len(params)} positional argument(s) but {argc} given"
                )
            args = param_buf[-argc:] if argc else []
            if argc:
                del param_buf[-argc:]
            frames.append((dict(zip(params, args)), pc + 1, a["dst"]))  # type: ignore[arg-type]
            pc = labels[f"fn_{func}"]
        elif inst.op == "iffalse":
            c = eval_op(a["cond"], locals_top, globals_map)  # type: ignore[arg-type]
            if not bool(c):
                pc = labels[str(a["label"])]
            else:
                pc += 1
        elif inst.op == "goto":
            pc = labels[str(a["label"])]
        elif inst.op == "return":
            opt = a["val"]
            ret_val = eval_op(opt, locals_top, globals_map) if opt is not None else None  # type: ignore[arg-type]
            _, ret_pc, ret_dst = frames.pop()
            if ret_dst is not None:
                write_slot(ret_dst, ret_val, frames[-1][0], globals_map)
                pc = ret_pc
            else:
                return globals_map, emitted
        elif inst.op == "emit":
            emitted.append(eval_op(a["val"], locals_top, globals_map))  # type: ignore[arg-type]
            pc += 1
    return globals_map, emitted


SAMPLE_PYTHON_SRC = """
scale = 2
bonus = 2.5
calls = 0

def weighted_sum(arr, limit):
    global calls
    calls = calls + 1
    acc = 0
    i = 0
    while i < limit:
        if i == 0:
            w = 1
        elif i == 1:
            w = 4
        else:
            # Parenthesized expressions join multiple physical lines cleanly
            w = (
                i + 1
            ) * 2
        acc = acc + arr[i] * w
        i = i + 1
    # '/' always yields float (295.0); '//' yields integer floor division; arr[-1] is 40
    last_bonus = (arr[-1] // 16) * 0.5
    return (acc / scale) + bonus + last_bonus

scores = [10, 20, 30, 40]
total = weighted_sum(scores, len(scores))
emit(total)
emit(calls)
"""


def main() -> None:
    prog = compile_python_subset(SAMPLE_PYTHON_SRC)
    print(f"Compile-time function symtables ({len(prog.symtables)}):")
    for name, st in prog.symtables.items():
        locals_list = ", ".join(sorted(st.locals))
        globals_list = ", ".join(sorted(st.globals))
        print(f"  def {name} -> locals: [{locals_list}], global decls: [{globals_list}]")
    print(f"\nEmitted TAC ({len(prog.code)} instructions):")
    for inst in prog.code:
        print(inst)
    globals_map, emitted = exec_tac(prog)
    print(f"\nEmitted output : {emitted}")
    print(f"Module globals : total={globals_map['total']:.4f}, calls={globals_map['calls']}")


class PythonFrontEndTest(unittest.TestCase):
    def test_compiles_indentation_scopes_div_modes_negative_index_and_runs_tac(self) -> None:
        prog = compile_python_subset(SAMPLE_PYTHON_SRC)
        globals_map, emitted = exec_tac(prog)
        self.assertEqual(emitted, [298.5, 1])
        self.assertEqual(globals_map["calls"], 1)
        self.assertEqual(globals_map["total"], 298.5)

    def test_floor_division_matches_python_toward_negative_infinity(self) -> None:
        prog = compile_python_subset("emit(-7 // 2)\nemit(-7 / 2)\n")
        _, emitted = exec_tac(prog)
        self.assertEqual(emitted, [-4, -3.5])

    def test_detects_unbound_local_error_from_symtable_prescan(self) -> None:
        src = """
x = 10
def broken():
    y = x + 1
    x = 20
    return y
broken()
"""
        prog = compile_python_subset(src)
        self.assertIn("x", prog.symtables["broken"].locals)
        with self.assertRaisesRegex(CompileError, "UnboundLocalError"):
            exec_tac(prog)

    def test_rejects_bad_dedent_and_index_out_of_range(self) -> None:
        with self.assertRaisesRegex(CompileError, "IndentationError"):
            compile_python_subset("def f():\n    x = 1\n  y = 2\n")
        oob_prog = compile_python_subset("xs = [1, 2]\nemit(xs[-3])\n")
        with self.assertRaisesRegex(CompileError, "IndexError"):
            exec_tac(oob_prog)


if __name__ == "__main__":
    main()
