"""Appendix A extension: Rust-subset compiler front end written in Python.

Parses nested `/* /* ... */ */` comments, `fn`, `let` / `let mut` immutability
and shadowing, strict typing with explicit `as` casts, expression-oriented blocks
and `if`/`else`, and fixed `[T; N]` arrays, lowers to labeled Three-Address Code
(TAC), and verifies execution on a TAC VM.
"""

from __future__ import annotations

from dataclasses import dataclass, field
import unittest


class CompileError(Exception):
    """Raised on lexical, syntactic, scope, mutability, type, or runtime errors."""


# ---------------------------------------------------------------------------
# 1. Lexer (Nested Block Comments + Rust Operators)
# ---------------------------------------------------------------------------


@dataclass(frozen=True)
class Token:
    kind: str
    value: object
    line: int


KEYWORDS = {
    "fn": "FN",
    "let": "LET",
    "mut": "MUT",
    "if": "IF",
    "else": "ELSE",
    "while": "WHILE",
    "return": "RETURN",
    "as": "AS",
    "emit": "EMIT",
    "true": "TRUE",
    "false": "FALSE",
    "i64": "I64",
    "f64": "F64",
    "bool": "BOOL",
}

TWO_CHAR_OPS = {
    "->": "ARROW",
    "&&": "ANDAND",
    "||": "OROR",
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
    "!": "BANG",
    ":": "COLON",
    ";": "SEMI",
    ",": "COMMA",
    "(": "LPAREN",
    ")": "RPAREN",
    "{": "LBRACE",
    "}": "RBRACE",
    "[": "LBRACKET",
    "]": "RBRACKET",
}


def lex(src: str) -> list[Token]:
    """Tokenizes Rust-subset source, supporting nested `/* /* ... */ */` comments."""
    i = 0
    n = len(src)
    line = 1
    tokens: list[Token] = []

    while i < n:
        ch = src[i]
        if ch.isspace():
            if ch == "\n":
                line += 1
            i += 1
            continue
        if src.startswith("//", i):
            i += 2
            while i < n and src[i] != "\n":
                i += 1
            continue
        if src.startswith("/*", i):
            # Rust block comments nest: track nesting depth cleanly.
            i += 2
            depth = 1
            while i < n and depth > 0:
                if src.startswith("/*", i):
                    depth += 1
                    i += 2
                elif src.startswith("*/", i):
                    depth -= 1
                    i += 2
                else:
                    if src[i] == "\n":
                        line += 1
                    i += 1
            if depth > 0:
                raise CompileError(f"line {line}: unterminated nested block comment")
            continue
        if ch.isalpha() or ch == "_":
            start = i
            i += 1
            while i < n and (src[i].isalnum() or src[i] == "_"):
                i += 1
            word = src[start:i]
            kind = KEYWORDS.get(word, "IDENT")
            val: object = True if kind == "TRUE" else False if kind == "FALSE" else word
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
        if ch in ONE_CHAR_OPS:
            tokens.append(Token(ONE_CHAR_OPS[ch], ch, line))
            i += 1
            continue
        raise CompileError(f"line {line}: unexpected character '{ch}'")

    tokens.append(Token("EOF", None, line))
    return tokens


# ---------------------------------------------------------------------------
# 2. Types, Lexical Scopes & Mutability / Shadowing Table
# ---------------------------------------------------------------------------


@dataclass(frozen=True)
class RsType:
    tag: str  # "()", "i64", "f64", "bool", "array"
    elem: RsType | None = None
    length: int = 0

    def width(self) -> int:
        if self.tag == "()":
            return 0
        if self.tag in ("i64", "f64"):
            return 8
        if self.tag == "bool":
            return 1
        assert self.elem is not None
        return self.length * self.elem.width()

    def is_numeric(self) -> bool:
        return self.tag in ("i64", "f64")

    def __str__(self) -> str:
        if self.tag == "array":
            return f"[{self.elem}; {self.length}]"
        return self.tag


UNIT = RsType("()")
I64 = RsType("i64")
F64 = RsType("f64")
BOOL = RsType("bool")


@dataclass(frozen=True)
class VarInfo:
    slot: str
    ty: RsType
    mutable: bool
    byte_off: int


@dataclass(frozen=True)
class FnSig:
    params: list[tuple[str, RsType]]
    ret_ty: RsType


class ScopeEnv:
    def __init__(self) -> None:
        self.scopes: list[dict[str, VarInfo]] = []
        self.history: list[VarInfo] = []
        self.next_slot_id = 0
        self.used_bytes = 0
        self.fn_sigs: dict[str, FnSig] = {}

    def push_scope(self) -> None:
        self.scopes.append({})

    def pop_scope(self) -> None:
        self.scopes.pop()

    def bind(self, name: str, ty: RsType, mutable: bool) -> VarInfo:
        """Binds or shadows `name` into a fresh `name#N` slot without overwriting older ones."""
        slot = f"{name}#{self.next_slot_id}"
        self.next_slot_id += 1
        off = self.used_bytes
        self.used_bytes += ty.width()
        info = VarInfo(slot, ty, mutable, off)
        self.history.append(info)
        self.scopes[-1][name] = info
        return info

    def lookup(self, name: str) -> VarInfo | None:
        for sc in reversed(self.scopes):
            if name in sc:
                return sc[name]
        return None


# ---------------------------------------------------------------------------
# 3. Typed Expression & Statement AST
# ---------------------------------------------------------------------------


@dataclass
class Expr:
    kind: str
    ty: RsType
    data: dict[str, object] = field(default_factory=dict)


@dataclass
class Stmt:
    kind: str
    data: dict[str, object] = field(default_factory=dict)


@dataclass
class FnDecl:
    name: str
    param_slots: list[str]
    body: Expr


# ---------------------------------------------------------------------------
# 4. Parser & Strict Type / Mutability Checker
# ---------------------------------------------------------------------------


class Parser:
    def __init__(self, toks: list[Token]) -> None:
        self.toks = toks
        self.pos = 0
        self.env = ScopeEnv()
        self.current_ret_ty = UNIT

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

    def parse_type(self) -> RsType:
        tok = self.peek()
        if tok.kind == "I64":
            self.pos += 1
            return I64
        if tok.kind == "F64":
            self.pos += 1
            return F64
        if tok.kind == "BOOL":
            self.pos += 1
            return BOOL
        if tok.kind == "LBRACKET":
            self.pos += 1
            elem = self.parse_type()
            self.expect("SEMI")
            ltok = self.peek()
            if ltok.kind != "INT" or int(ltok.value) <= 0:  # type: ignore[arg-type]
                raise self.err("expected positive integer length in [T; N]")
            self.pos += 1
            self.expect("RBRACKET")
            return RsType("array", elem=elem, length=int(ltok.value))  # type: ignore[arg-type]
        if tok.kind == "LPAREN":
            self.pos += 1
            self.expect("RPAREN")
            return UNIT
        raise self.err(f"expected type annotation, found {tok.kind}")

    def parse_program(self) -> tuple[list[FnDecl], dict[str, FnSig]]:
        saved_pos = self.pos
        while self.peek().kind != "EOF":
            self.expect("FN")
            name = str(self.expect("IDENT").value)
            self.expect("LPAREN")
            params: list[tuple[str, RsType]] = []
            while self.peek().kind != "RPAREN":
                pname = str(self.expect("IDENT").value)
                self.expect("COLON")
                pty = self.parse_type()
                params.append((pname, pty))
                if self.peek().kind == "COMMA":
                    self.pos += 1
                else:
                    break
            self.expect("RPAREN")
            ret_ty = UNIT
            if self.peek().kind == "ARROW":
                self.pos += 1
                ret_ty = self.parse_type()
            if name in self.env.fn_sigs:
                raise self.err(f"duplicate function definition '{name}'")
            self.env.fn_sigs[name] = FnSig(params, ret_ty)
            self.expect("LBRACE")
            depth = 1
            while self.peek().kind != "EOF" and depth > 0:
                if self.peek().kind == "LBRACE":
                    depth += 1
                elif self.peek().kind == "RBRACE":
                    depth -= 1
                self.pos += 1
            if depth != 0:
                raise CompileError("unclosed '{' in function body")

        self.pos = saved_pos
        fns: list[FnDecl] = []
        while self.peek().kind != "EOF":
            fns.append(self.parse_fn())
        if "main" not in self.env.fn_sigs:
            raise CompileError("program requires a `fn main()` entry point")
        return fns, dict(sorted(self.env.fn_sigs.items()))

    def parse_fn(self) -> FnDecl:
        self.expect("FN")
        name = str(self.expect("IDENT").value)
        sig = self.env.fn_sigs[name]
        self.expect("LPAREN")
        while self.peek().kind != "RPAREN":
            self.pos += 1
        self.expect("RPAREN")
        if self.peek().kind == "ARROW":
            self.pos += 1
            _ = self.parse_type()

        self.env.push_scope()
        param_slots: list[str] = []
        for pname, pty in sig.params:
            info = self.env.bind(pname, pty, False)
            param_slots.append(info.slot)
        self.current_ret_ty = sig.ret_ty
        body = self.parse_block()
        if body.ty != sig.ret_ty:
            raise self.err(
                f"function '{name}' expected return type {sig.ret_ty}, found {body.ty}"
            )
        self.env.pop_scope()
        return FnDecl(name, param_slots, body)

    def parse_block(self) -> Expr:
        self.expect("LBRACE")
        self.env.push_scope()
        stmts: list[Stmt] = []
        tail: Expr | None = None

        while self.peek().kind != "RBRACE":
            tok = self.peek()
            if tok.kind == "LET":
                self.pos += 1
                mutable = False
                if self.peek().kind == "MUT":
                    self.pos += 1
                    mutable = True
                name = str(self.expect("IDENT").value)
                ann_ty: RsType | None = None
                if self.peek().kind == "COLON":
                    self.pos += 1
                    ann_ty = self.parse_type()
                self.expect("ASSIGN")
                init = self.parse_expr()
                if ann_ty is not None and init.ty != ann_ty:
                    raise self.err(
                        f"let '{name}' declared {ann_ty}, initialized with {init.ty}"
                    )
                info = self.env.bind(name, init.ty, mutable)
                self.expect("SEMI")
                stmts.append(Stmt("let", {"slot": info.slot, "init": init}))
            elif tok.kind == "WHILE":
                self.pos += 1
                cond = self.parse_expr()
                if cond.ty != BOOL:
                    raise self.err(f"while condition must be bool, found {cond.ty}")
                body = self.parse_block()
                if body.ty != UNIT:
                    raise self.err("while body must evaluate to ()")
                stmts.append(Stmt("while", {"cond": cond, "body": body}))
            elif tok.kind == "RETURN":
                self.pos += 1
                expr_opt: Expr | None = None
                if self.peek().kind != "SEMI":
                    expr_opt = self.parse_expr()
                    if expr_opt.ty != self.current_ret_ty:
                        raise self.err(
                            f"return type mismatch: expected {self.current_ret_ty}, found {expr_opt.ty}"
                        )
                elif self.current_ret_ty != UNIT:
                    raise self.err("bare return in non-unit function")
                self.expect("SEMI")
                stmts.append(Stmt("return", {"expr": expr_opt}))
            elif tok.kind == "EMIT":
                self.pos += 1
                self.expect("LPAREN")
                arg = self.parse_expr()
                self.expect("RPAREN")
                self.expect("SEMI")
                stmts.append(Stmt("emit", {"expr": arg}))
            elif tok.kind == "IDENT" and self.peek_next().kind == "ASSIGN":
                name = str(tok.value)
                self.pos += 1
                var = self.env.lookup(name)
                if var is None:
                    raise self.err(f"undeclared variable '{name}'")
                if not var.mutable:
                    raise self.err(
                        f"cannot assign twice to immutable variable '{name}' (hint: declare with `let mut`)"
                    )
                self.expect("ASSIGN")
                rhs = self.parse_expr()
                if rhs.ty != var.ty:
                    raise self.err(
                        f"assignment to '{name}' expected {var.ty}, found {rhs.ty}"
                    )
                self.expect("SEMI")
                stmts.append(Stmt("assign", {"slot": var.slot, "rhs": rhs}))
            else:
                e = self.parse_expr()
                if self.peek().kind == "SEMI":
                    self.pos += 1
                    stmts.append(Stmt("expr", {"expr": e}))
                elif self.peek().kind == "RBRACE":
                    tail = e
                    break
                else:
                    raise self.err("expected ';' or '}' after expression")

        self.expect("RBRACE")
        self.env.pop_scope()
        blk_ty = tail.ty if tail is not None else UNIT
        return Expr("block", blk_ty, {"stmts": stmts, "tail": tail})

    def parse_expr(self) -> Expr:
        return self.parse_or()

    def parse_or(self) -> Expr:
        lhs = self.parse_and()
        while self.peek().kind == "OROR":
            self.pos += 1
            rhs = self.parse_and()
            if lhs.ty != BOOL or rhs.ty != BOOL:
                raise self.err("'||' requires bool operands")
            lhs = Expr("or", BOOL, {"left": lhs, "right": rhs})
        return lhs

    def parse_and(self) -> Expr:
        lhs = self.parse_rel()
        while self.peek().kind == "ANDAND":
            self.pos += 1
            rhs = self.parse_rel()
            if lhs.ty != BOOL or rhs.ty != BOOL:
                raise self.err("'&&' requires bool operands")
            lhs = Expr("and", BOOL, {"left": lhs, "right": rhs})
        return lhs

    def parse_rel(self) -> Expr:
        lhs = self.parse_add()
        rel_map = {"EQEQ": "==", "NE": "!=", "LT": "<", "LE": "<=", "Gt": ">", "GT": ">", "GE": ">="}
        if self.peek().kind not in rel_map:
            return lhs
        op = rel_map[self.peek().kind]
        self.pos += 1
        rhs = self.parse_add()
        if lhs.ty != rhs.ty:
            raise self.err(
                f"comparison operands must share exact type: {lhs.ty} vs {rhs.ty}"
            )
        return Expr("rel", BOOL, {"op": op, "left": lhs, "right": rhs})

    def parse_add(self) -> Expr:
        lhs = self.parse_mul()
        while self.peek().kind in ("PLUS", "MINUS"):
            op = "+" if self.peek().kind == "PLUS" else "-"
            self.pos += 1
            rhs = self.parse_mul()
            if not lhs.ty.is_numeric() or lhs.ty != rhs.ty:
                raise self.err(
                    f"mismatched types: cannot apply {op} to {lhs.ty} and {rhs.ty} (use 'as f64')"
                )
            lhs = Expr("bin", lhs.ty, {"op": op, "left": lhs, "right": rhs})
        return lhs

    def parse_mul(self) -> Expr:
        lhs = self.parse_cast()
        while self.peek().kind in ("STAR", "SLASH"):
            op = "*" if self.peek().kind == "STAR" else "/"
            self.pos += 1
            rhs = self.parse_cast()
            if not lhs.ty.is_numeric() or lhs.ty != rhs.ty:
                raise self.err(
                    f"mismatched types: cannot apply {op} to {lhs.ty} and {rhs.ty} (use 'as f64')"
                )
            lhs = Expr("bin", lhs.ty, {"op": op, "left": lhs, "right": rhs})
        return lhs

    def parse_cast(self) -> Expr:
        expr = self.parse_unary()
        while self.peek().kind == "AS":
            self.pos += 1
            tgt = self.parse_type()
            if not expr.ty.is_numeric() or not tgt.is_numeric():
                raise self.err(f"invalid cast from {expr.ty} as {tgt}")
            expr = Expr("cast", tgt, {"inner": expr, "target": tgt})
        return expr

    def parse_unary(self) -> Expr:
        if self.peek().kind == "MINUS":
            self.pos += 1
            inner = self.parse_unary()
            if not inner.ty.is_numeric():
                raise self.err("unary '-' requires numeric operand")
            return Expr("neg", inner.ty, {"inner": inner})
        if self.peek().kind == "BANG":
            self.pos += 1
            inner = self.parse_unary()
            if inner.ty != BOOL:
                raise self.err("unary '!' requires bool")
            return Expr("not", BOOL, {"inner": inner})
        return self.parse_postfix()

    def parse_postfix(self) -> Expr:
        base = self.parse_primary()
        while self.peek().kind == "LBRACKET":
            self.pos += 1
            idx = self.parse_expr()
            if idx.ty != I64:
                raise self.err(f"array index must be i64, found {idx.ty}")
            self.expect("RBRACKET")
            if base.ty.tag != "array" or base.ty.elem is None:
                raise self.err(f"cannot index non-array {base.ty}")
            base = Expr("index", base.ty.elem, {"base": base, "idx": idx})
        return base

    def parse_primary(self) -> Expr:
        tok = self.peek()
        if tok.kind == "INT":
            self.pos += 1
            return Expr("lit", I64, {"val": int(tok.value)})  # type: ignore[arg-type]
        if tok.kind == "FLOAT":
            self.pos += 1
            return Expr("lit", F64, {"val": float(tok.value)})  # type: ignore[arg-type]
        if tok.kind in ("TRUE", "FALSE"):
            self.pos += 1
            return Expr("lit", BOOL, {"val": bool(tok.value)})
        if tok.kind == "IF":
            self.pos += 1
            cond = self.parse_expr()
            if cond.ty != BOOL:
                raise self.err(f"if condition must be bool, found {cond.ty}")
            then_blk = self.parse_block()
            else_blk: Expr | None = None
            if self.peek().kind == "ELSE":
                self.pos += 1
                else_blk = self.parse_block()
                if else_blk.ty != then_blk.ty:
                    raise self.err(
                        f"if/else arm type mismatch: {then_blk.ty} vs {else_blk.ty}"
                    )
            elif then_blk.ty != UNIT:
                raise self.err("if without else must yield ()")
            return Expr(
                "if",
                then_blk.ty,
                {"cond": cond, "then": then_blk, "else": else_blk},
            )
        if tok.kind == "LBRACE":
            return self.parse_block()
        if tok.kind == "LBRACKET":
            self.pos += 1
            elems: list[Expr] = []
            while self.peek().kind != "RBRACKET":
                elems.append(self.parse_expr())
                if self.peek().kind == "COMMA":
                    self.pos += 1
                else:
                    break
            self.expect("RBRACKET")
            if not elems:
                raise self.err("empty array literal unsupported without type")
            first_ty = elems[0].ty
            for e in elems[1:]:
                if e.ty != first_ty:
                    raise self.err("array literal elements must have identical type")
            return Expr(
                "arr_lit",
                RsType("array", elem=first_ty, length=len(elems)),
                {"elems": elems},
            )
        if tok.kind == "LPAREN":
            self.pos += 1
            inner = self.parse_expr()
            self.expect("RPAREN")
            return inner
        if tok.kind == "IDENT":
            name = str(tok.value)
            self.pos += 1
            if self.peek().kind == "LPAREN":
                if name not in self.env.fn_sigs:
                    raise self.err(f"unknown function '{name}'")
                sig = self.env.fn_sigs[name]
                self.pos += 1
                args: list[Expr] = []
                while self.peek().kind != "RPAREN":
                    args.append(self.parse_expr())
                    if self.peek().kind == "COMMA":
                        self.pos += 1
                    else:
                        break
                self.expect("RPAREN")
                if len(args) != len(sig.params):
                    raise self.err(
                        f"call to '{name}' expected {len(sig.params)} args, found {len(args)}"
                    )
                for idx, (arg, (_, pty)) in enumerate(zip(args, sig.params), start=1):
                    if arg.ty != pty:
                        raise self.err(
                            f"arg #{idx} of '{name}' expected {pty}, found {arg.ty}"
                        )
                return Expr("call", sig.ret_ty, {"name": name, "args": args})
            info = self.env.lookup(name)
            if info is None:
                raise self.err(f"undeclared variable '{name}'")
            return Expr("var", info.ty, {"slot": info.slot})
        raise self.err(f"unexpected token in expression: {tok.kind}")


# ---------------------------------------------------------------------------
# 5. Three-Address Code (TAC) Lowering & VM Execution
# ---------------------------------------------------------------------------


@dataclass(frozen=True)
class Operand:
    is_slot: bool
    value: object

    def __str__(self) -> str:
        if self.is_slot:
            return str(self.value)
        if self.value is None:
            return "()"
        if isinstance(self.value, bool):
            return "true" if self.value else "false"
        if isinstance(self.value, float):
            return f"{self.value:.4f}"
        return str(self.value)


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
        if self.op == "make_arr":
            return f"  {a['dst']} = array({len(a['elems'])})"  # type: ignore[arg-type]
        if self.op == "arr_load":
            return f"  {a['dst']} = {a['base']}[{a['idx']}]"
        if self.op == "cast":
            return f"  {a['dst']} = {a['src']} as {a['target']}"
        if self.op == "neg":
            return f"  {a['dst']} = -{a['src']}"
        if self.op == "not":
            return f"  {a['dst']} = !{a['src']}"
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
            v = a["val"]
            return f"  return {v if v is not None else '()'}"
        return f"  emit {a['val']}"


class Emitter:
    def __init__(self) -> None:
        self.next_temp = 0
        self.next_lbl = 0
        self.code: list[TacInst] = []

    def temp(self) -> str:
        self.next_temp += 1
        return f"t{self.next_temp}"

    def label(self, prefix: str) -> str:
        self.next_lbl += 1
        return f"{prefix}_{self.next_lbl}"

    def gen_expr(self, expr: Expr) -> Operand:
        k = expr.kind
        d = expr.data
        if k == "lit":
            return Operand(False, d["val"])
        if k == "var":
            return Operand(True, d["slot"])
        if k == "arr_lit":
            ops = [self.gen_expr(e) for e in d["elems"]]  # type: ignore[attr-defined]
            dst = self.temp()
            self.code.append(TacInst("make_arr", {"dst": dst, "elems": ops}))
            return Operand(True, dst)
        if k == "index":
            b = self.gen_expr(d["base"])  # type: ignore[arg-type]
            i = self.gen_expr(d["idx"])  # type: ignore[arg-type]
            dst = self.temp()
            self.code.append(TacInst("arr_load", {"dst": dst, "base": b, "idx": i}))
            return Operand(True, dst)
        if k == "cast":
            src = self.gen_expr(d["inner"])  # type: ignore[arg-type]
            dst = self.temp()
            self.code.append(TacInst("cast", {"dst": dst, "src": src, "target": d["target"]}))
            return Operand(True, dst)
        if k == "neg":
            src = self.gen_expr(d["inner"])  # type: ignore[arg-type]
            dst = self.temp()
            self.code.append(TacInst("neg", {"dst": dst, "src": src}))
            return Operand(True, dst)
        if k == "not":
            src = self.gen_expr(d["inner"])  # type: ignore[arg-type]
            dst = self.temp()
            self.code.append(TacInst("not", {"dst": dst, "src": src}))
            return Operand(True, dst)
        if k in ("bin", "rel"):
            l = self.gen_expr(d["left"])  # type: ignore[arg-type]
            r = self.gen_expr(d["right"])  # type: ignore[arg-type]
            dst = self.temp()
            self.code.append(TacInst(k, {"dst": dst, "sym": d["op"], "left": l, "right": r}))
            return Operand(True, dst)
        if k == "and":
            dst = self.temp()
            l_false = self.label("and_false")
            l_end = self.label("and_end")
            l = self.gen_expr(d["left"])  # type: ignore[arg-type]
            self.code.append(TacInst("iffalse", {"cond": l, "label": l_false}))
            r = self.gen_expr(d["right"])  # type: ignore[arg-type]
            self.code.append(TacInst("assign", {"dst": dst, "src": r}))
            self.code.append(TacInst("goto", {"label": l_end}))
            self.code.append(TacInst("label", {"name": l_false}))
            self.code.append(TacInst("assign", {"dst": dst, "src": Operand(False, False)}))
            self.code.append(TacInst("label", {"name": l_end}))
            return Operand(True, dst)
        if k == "or":
            dst = self.temp()
            l_rhs = self.label("or_rhs")
            l_end = self.label("or_end")
            l = self.gen_expr(d["left"])  # type: ignore[arg-type]
            self.code.append(TacInst("iffalse", {"cond": l, "label": l_rhs}))
            self.code.append(TacInst("assign", {"dst": dst, "src": Operand(False, True)}))
            self.code.append(TacInst("goto", {"label": l_end}))
            self.code.append(TacInst("label", {"name": l_rhs}))
            r = self.gen_expr(d["right"])  # type: ignore[arg-type]
            self.code.append(TacInst("assign", {"dst": dst, "src": r}))
            self.code.append(TacInst("label", {"name": l_end}))
            return Operand(True, dst)
        if k == "call":
            ops = [self.gen_expr(a) for a in d["args"]]  # type: ignore[attr-defined]
            for op in ops:
                self.code.append(TacInst("param", {"op": op}))
            dst = self.temp()
            self.code.append(
                TacInst("call", {"dst": dst, "func": d["name"], "argc": len(ops)})
            )
            return Operand(True, dst)
        if k == "block":
            for s in d["stmts"]:  # type: ignore[attr-defined]
                self.gen_stmt(s)
            tail = d["tail"]
            if tail is not None:
                return self.gen_expr(tail)  # type: ignore[arg-type]
            return Operand(False, None)
        # k == "if"
        dst = self.temp()
        l_else = self.label("else")
        l_end = self.label("endif")
        c = self.gen_expr(d["cond"])  # type: ignore[arg-type]
        self.code.append(TacInst("iffalse", {"cond": c, "label": l_else}))
        tv = self.gen_expr(d["then"])  # type: ignore[arg-type]
        self.code.append(TacInst("assign", {"dst": dst, "src": tv}))
        self.code.append(TacInst("goto", {"label": l_end}))
        self.code.append(TacInst("label", {"name": l_else}))
        eb = d["else"]
        if eb is not None:
            ev = self.gen_expr(eb)  # type: ignore[arg-type]
            self.code.append(TacInst("assign", {"dst": dst, "src": ev}))
        else:
            self.code.append(TacInst("assign", {"dst": dst, "src": Operand(False, None)}))
        self.code.append(TacInst("label", {"name": l_end}))
        return Operand(True, dst)

    def gen_stmt(self, stmt: Stmt) -> None:
        k = stmt.kind
        d = stmt.data
        if k == "let":
            v = self.gen_expr(d["init"])  # type: ignore[arg-type]
            self.code.append(TacInst("assign", {"dst": d["slot"], "src": v}))
        elif k == "assign":
            v = self.gen_expr(d["rhs"])  # type: ignore[arg-type]
            self.code.append(TacInst("assign", {"dst": d["slot"], "src": v}))
        elif k == "while":
            l_top = self.label("while")
            l_end = self.label("endwhile")
            self.code.append(TacInst("label", {"name": l_top}))
            c = self.gen_expr(d["cond"])  # type: ignore[arg-type]
            self.code.append(TacInst("iffalse", {"cond": c, "label": l_end}))
            _ = self.gen_expr(d["body"])  # type: ignore[arg-type]
            self.code.append(TacInst("goto", {"label": l_top}))
            self.code.append(TacInst("label", {"name": l_end}))
        elif k == "return":
            opt = d["expr"]
            v = self.gen_expr(opt) if opt is not None else None  # type: ignore[arg-type]
            self.code.append(TacInst("return", {"val": v}))
        elif k == "emit":
            v = self.gen_expr(d["expr"])  # type: ignore[arg-type]
            self.code.append(TacInst("emit", {"val": v}))
        else:
            _ = self.gen_expr(d["expr"])  # type: ignore[arg-type]


@dataclass
class CompiledRust:
    fn_sigs: dict[str, FnSig]
    fn_params: dict[str, list[str]]
    bindings: list[VarInfo]
    used_bytes: int
    code: list[TacInst]


def compile_rust_subset(src: str) -> CompiledRust:
    toks = lex(src)
    parser = Parser(toks)
    fns, fn_sigs = parser.parse_program()
    emitter = Emitter()
    fn_params: dict[str, list[str]] = {}
    for f in fns:
        fn_params[f.name] = f.param_slots
        emitter.code.append(TacInst("label", {"name": f"fn_{f.name}"}))
        tail_val = emitter.gen_expr(f.body)
        emitter.code.append(TacInst("return", {"val": tail_val}))
    return CompiledRust(
        fn_sigs=fn_sigs,
        fn_params=fn_params,
        bindings=parser.env.history,
        used_bytes=parser.env.used_bytes,
        code=emitter.code,
    )


def exec_tac(prog: CompiledRust) -> tuple[object, list[object]]:
    labels = {
        str(inst.args["name"]): idx
        for idx, inst in enumerate(prog.code)
        if inst.op == "label"
    }

    def eval_op(op: Operand, env: dict[str, object]) -> object:
        return env.get(str(op.value), None) if op.is_slot else op.value

    frames: list[tuple[dict[str, object], int, str | None]] = [({}, len(prog.code), None)]
    pc = labels["fn_main"]
    param_buf: list[object] = []
    emitted: list[object] = []

    while pc < len(prog.code):
        inst = prog.code[pc]
        a = inst.args
        vars_top = frames[-1][0]
        if inst.op == "label":
            pc += 1
        elif inst.op == "assign":
            vars_top[str(a["dst"])] = eval_op(a["src"], vars_top)  # type: ignore[arg-type]
            pc += 1
        elif inst.op == "make_arr":
            vars_top[str(a["dst"])] = [
                eval_op(e, vars_top) for e in a["elems"]  # type: ignore[attr-defined]
            ]
            pc += 1
        elif inst.op == "arr_load":
            vec = eval_op(a["base"], vars_top)  # type: ignore[arg-type]
            idx = eval_op(a["idx"], vars_top)  # type: ignore[arg-type]
            if not isinstance(vec, list) or type(idx) is not int:
                raise CompileError("runtime type mismatch in array index")
            if idx < 0 or idx >= len(vec):
                raise CompileError(f"index out of bounds: {idx} for len {len(vec)}")
            vars_top[str(a["dst"])] = vec[idx]
            pc += 1
        elif inst.op == "cast":
            v = eval_op(a["src"], vars_top)  # type: ignore[arg-type]
            tgt: RsType = a["target"]  # type: ignore[assignment]
            vars_top[str(a["dst"])] = float(v) if tgt == F64 else int(v)  # type: ignore[arg-type]
            pc += 1
        elif inst.op == "neg":
            vars_top[str(a["dst"])] = -eval_op(a["src"], vars_top)  # type: ignore[operator,arg-type]
            pc += 1
        elif inst.op == "not":
            vars_top[str(a["dst"])] = not eval_op(a["src"], vars_top)  # type: ignore[arg-type]
            pc += 1
        elif inst.op == "bin":
            l = eval_op(a["left"], vars_top)  # type: ignore[arg-type]
            r = eval_op(a["right"], vars_top)  # type: ignore[arg-type]
            sym = a["sym"]
            if sym == "+":
                out = l + r  # type: ignore[operator]
            elif sym == "-":
                out = l - r  # type: ignore[operator]
            elif sym == "*":
                out = l * r  # type: ignore[operator]
            elif type(l) is int and type(r) is int:
                out = int(l / r)
            else:
                out = l / r  # type: ignore[operator]
            vars_top[str(a["dst"])] = out
            pc += 1
        elif inst.op == "rel":
            l = eval_op(a["left"], vars_top)  # type: ignore[arg-type]
            r = eval_op(a["right"], vars_top)  # type: ignore[arg-type]
            sym = a["sym"]
            if sym == "==":
                out_b = l == r
            elif sym == "!=":
                out_b = l != r
            elif sym == "<":
                out_b = l < r  # type: ignore[operator]
            elif sym == "<=":
                out_b = l <= r  # type: ignore[operator]
            elif sym == ">":
                out_b = l > r  # type: ignore[operator]
            else:
                out_b = l >= r  # type: ignore[operator]
            vars_top[str(a["dst"])] = out_b
            pc += 1
        elif inst.op == "param":
            param_buf.append(eval_op(a["op"], vars_top))  # type: ignore[arg-type]
            pc += 1
        elif inst.op == "call":
            argc = int(a["argc"])  # type: ignore[arg-type]
            args = param_buf[-argc:] if argc else []
            if argc:
                del param_buf[-argc:]
            func = str(a["func"])
            callee = dict(zip(prog.fn_params[func], args))
            frames.append((callee, pc + 1, str(a["dst"])))
            pc = labels[f"fn_{func}"]
        elif inst.op == "iffalse":
            if eval_op(a["cond"], vars_top) is False:  # type: ignore[arg-type]
                pc = labels[str(a["label"])]
            else:
                pc += 1
        elif inst.op == "goto":
            pc = labels[str(a["label"])]
        elif inst.op == "return":
            opt = a["val"]
            ret_val = eval_op(opt, vars_top) if opt is not None else None  # type: ignore[arg-type]
            _, ret_pc, ret_dst = frames.pop()
            if ret_dst is not None:
                frames[-1][0][ret_dst] = ret_val
                pc = ret_pc
            else:
                return ret_val, emitted
        elif inst.op == "emit":
            emitted.append(eval_op(a["val"], vars_top))  # type: ignore[arg-type]
            pc += 1
    return None, emitted


SAMPLE_RUST_SRC = """
fn weighted_sum(arr: [i64; 4], limit: i64, bonus: f64) -> f64 {
    let mut idx: i64 = 0;
    let mut acc: i64 = 0;
    while idx < limit {
        let weight: i64 = idx + 1;
        /* Nested /* block comment */: shadow `weight` immutably inside the loop body */
        let weight: i64 = if idx == 0 { weight } else { weight * 2 };
        acc = acc + arr[idx] * weight;
        idx = idx + 1;
    }
    let scale: f64 = 2.0;
    (acc as f64) / scale + bonus
}

fn main() -> i64 {
    let scores: [i64; 4] = [10, 20, 30, 40];
    let total: f64 = weighted_sum(scores, 4, 2.5);
    let rounded: i64 = total as i64;
    emit(total);
    rounded
}
"""


def main() -> None:
    prog = compile_rust_subset(SAMPLE_RUST_SRC)
    print(f"Function signatures ({len(prog.fn_sigs)}):")
    for name, sig in prog.fn_sigs.items():
        params = ", ".join(f"{n}: {t}" for n, t in sig.params)
        print(f"  fn {name}({params}) -> {sig.ret_ty}")
    print(f"\nBindings & stack byte offsets ({prog.used_bytes} bytes):")
    for b in prog.bindings:
        mut_tag = "mut " if b.mutable else ""
        print(f"  {b.slot:14} : {mut_tag}{str(b.ty):10} @ byte {b.byte_off}")
    print(f"\nEmitted TAC ({len(prog.code)} instructions):")
    for inst in prog.code:
        print(inst)
    ret_val, emitted = exec_tac(prog)
    print(f"\nEmitted output : {emitted}")
    print(f"main() returned: {ret_val}")


class RustFrontEndTest(unittest.TestCase):
    def test_compiles_shadowing_expression_if_explicit_casts_and_runs_tac(self) -> None:
        prog = compile_rust_subset(SAMPLE_RUST_SRC)
        self.assertEqual(prog.used_bytes, 136)
        ret_val, emitted = exec_tac(prog)
        self.assertEqual(emitted, [297.5])
        self.assertEqual(ret_val, 297)

    def test_short_circuit_and_or_guard_bounds_and_runtime_oob_errors(self) -> None:
        guarded = """
fn main() -> i64 {
    let a: [i64; 2] = [7, 9];
    let idx: i64 = 5;
    if idx < 2 && a[idx] > 0 { 1 } else { 0 }
}
"""
        prog = compile_rust_subset(guarded)
        self.assertEqual(exec_tac(prog)[0], 0)

        oob_prog = compile_rust_subset("fn main() -> i64 { let a: [i64; 2] = [7, 9]; a[2] }")
        with self.assertRaisesRegex(CompileError, "out of bounds"):
            exec_tac(oob_prog)

    def test_rejects_immutable_reassignment_implicit_widening_and_branch_mismatch(self) -> None:
        with self.assertRaisesRegex(CompileError, "immutable"):
            compile_rust_subset("fn main() -> i64 { let x: i64 = 1; x = 2; x }")
        with self.assertRaisesRegex(CompileError, "mismatched types"):
            compile_rust_subset("fn main() -> f64 { 1 + 2.5 }")
        with self.assertRaisesRegex(CompileError, "mismatch"):
            compile_rust_subset(
                "fn main() -> i64 { let x: i64 = if true { 1 } else { 2.0 }; x }"
            )


if __name__ == "__main__":
    main()
