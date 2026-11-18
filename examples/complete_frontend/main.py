"""Appendix A (2006 Purple) capstone: Complete structured block-scoped front end in Python.
Architecture parallels the modular front end (Lexer -> Env/Type -> AST -> TAC with short-circuit `jumping` & break labels), plus a TAC verifier VM.
"""

from dataclasses import dataclass
import unittest


# ---------------------------------------------------------------------------
# 1. Lexer
# ---------------------------------------------------------------------------

KEYWORDS = {
    "int": "INT_KW",
    "float": "FLOAT_KW",
    "bool": "BOOL_KW",
    "true": "TRUE_KW",
    "false": "FALSE_KW",
    "if": "IF",
    "else": "ELSE",
    "while": "WHILE",
    "do": "DO",
    "break": "BREAK",
}


@dataclass(frozen=True)
class Token:
    kind: str
    value: object
    line: int


def lex(src: str) -> list[Token]:
    """Scan C/Java-like structured input into tokens while skipping `//` and `/* ... */` comments."""
    n = len(src)
    i = 0
    line = 1
    out: list[Token] = []

    while i < n:
        ch = src[i]
        if ch.isspace():
            if ch == "\n":
                line += 1
            i += 1
            continue
        if ch == "/" and i + 1 < n and src[i + 1] == "/":
            i += 2
            while i < n and src[i] != "\n":
                i += 1
            continue
        if ch == "/" and i + 1 < n and src[i + 1] == "*":
            i += 2
            while i + 1 < n and not (src[i] == "*" and src[i + 1] == "/"):
                if src[i] == "\n":
                    line += 1
                i += 1
            if i + 1 >= n:
                raise ValueError(f"line {line}: unterminated block comment")
            i += 2
            continue
        if ch.isalpha() or ch == "_":
            start = i
            i += 1
            while i < n and (src[i].isalnum() or src[i] == "_"):
                i += 1
            word = src[start:i]
            out.append(Token(KEYWORDS.get(word, "IDENT"), word, line))
            continue
        if ch.isdigit():
            start = i
            while i < n and src[i].isdigit():
                i += 1
            if i + 1 < n and src[i] == "." and src[i + 1].isdigit():
                i += 1
                while i < n and src[i].isdigit():
                    i += 1
                out.append(Token("FLOAT_LIT", float(src[start:i]), line))
            else:
                out.append(Token("INT_LIT", int(src[start:i]), line))
            continue
        two = src[i : i + 2]
        two_map = {
            "&&": "AND_AND",
            "||": "OR_OR",
            "==": "EQ_EQ",
            "!=": "NE",
            "<=": "LE",
            ">=": "GE",
        }
        if two in two_map:
            out.append(Token(two_map[two], two, line))
            i += 2
            continue
        one_map = {
            "=": "ASSIGN",
            "<": "LT",
            ">": "GT",
            "+": "PLUS",
            "-": "MINUS",
            "*": "STAR",
            "/": "SLASH",
            "!": "BANG",
            ";": "SEMI",
            "{": "LBRACE",
            "}": "RBRACE",
            "(": "LPAREN",
            ")": "RPAREN",
            "[": "LBRACKET",
            "]": "RBRACKET",
        }
        if ch in one_map:
            out.append(Token(one_map[ch], ch, line))
            i += 1
            continue
        raise ValueError(f"line {line}: unexpected character '{ch}'")

    out.append(Token("EOF", "", line))
    return out


# ---------------------------------------------------------------------------
# 2. Types & Chained Block Environments (`Env`)
# ---------------------------------------------------------------------------

@dataclass(frozen=True)
class Type:
    kind: str  # "int", "float", "bool", "array"
    length: int = 0
    elem: "Type | None" = None

    def width(self) -> int:
        if self.kind == "int":
            return 4
        if self.kind == "float":
            return 8
        if self.kind == "bool":
            return 1
        if self.kind == "array" and self.elem is not None:
            return self.length * self.elem.width()
        return 0

    def is_numeric(self) -> bool:
        return self.kind in ("int", "float")

    @staticmethod
    def max_numeric(a: "Type", b: "Type") -> "Type | None":
        if not a.is_numeric() or not b.is_numeric():
            return None
        return T_FLOAT if (a.kind == "float" or b.kind == "float") else T_INT

    def __str__(self) -> str:
        dims: list[int] = []
        cur: Type | None = self
        while cur is not None and cur.kind == "array":
            dims.append(cur.length)
            cur = cur.elem
        base = cur.kind if cur is not None else "unknown"
        return base + "".join(f"[{d}]" for d in dims)


T_INT = Type("int")
T_FLOAT = Type("float")
T_BOOL = Type("bool")


@dataclass(frozen=True)
class VarBinding:
    name: str
    slot: str
    ty: Type
    offset: int
    block_id: int


class EnvStack:
    """Chained lexical environments: each `{ ... }` block pushes a new scope map."""

    def __init__(self) -> None:
        self.blocks: list[tuple[int, dict[str, VarBinding]]] = []
        self.next_block_id = 0
        self.used_bytes = 0
        self.all_decls: list[VarBinding] = []

    def push_block(self) -> None:
        bid = self.next_block_id
        self.next_block_id += 1
        self.blocks.append((bid, {}))

    def pop_block(self) -> None:
        self.blocks.pop()

    def declare(self, name: str, ty: Type, line: int) -> VarBinding:
        bid, mapping = self.blocks[-1]
        if name in mapping:
            raise ValueError(f"line {line}: duplicate declaration of '{name}' in same block")
        binding = VarBinding(name, f"{name}@b{bid}", ty, self.used_bytes, bid)
        self.used_bytes += ty.width()
        mapping[name] = binding
        self.all_decls.append(binding)
        return binding

    def lookup(self, name: str) -> VarBinding | None:
        for _, mapping in reversed(self.blocks):
            if name in mapping:
                return mapping[name]
        return None


# ---------------------------------------------------------------------------
# 3. Abstract Syntax Tree
# ---------------------------------------------------------------------------

@dataclass(frozen=True)
class Expr:
    kind: str
    ty: Type
    args: tuple[object, ...] = ()


@dataclass(frozen=True)
class Stmt:
    kind: str
    args: tuple[object, ...] = ()


def assignable(dst: Type, src: Type) -> bool:
    return dst.kind != "array" and (dst == src or (dst == T_FLOAT and src == T_INT))


# ---------------------------------------------------------------------------
# 4. Parser (Builds Scoped AST & Checks Types)
# ---------------------------------------------------------------------------

class Parser:
    def __init__(self, tokens: list[Token]) -> None:
        self.tokens = tokens
        self.pos = 0
        self.env = EnvStack()
        self.loop_stack: list[int] = []
        self.next_loop_id = 0

    def peek(self) -> Token:
        return self.tokens[self.pos]

    def line(self) -> int:
        return self.peek().line

    def advance(self) -> Token:
        t = self.tokens[self.pos]
        if self.pos + 1 < len(self.tokens):
            self.pos += 1
        return t

    def eat(self, kind: str) -> bool:
        if self.peek().kind == kind:
            self.advance()
            return True
        return False

    def expect(self, kind: str) -> None:
        if not self.eat(kind):
            raise ValueError(f"line {self.line()}: expected {kind}, found {self.peek().kind}")

    def parse_program(self) -> tuple[Stmt, list[VarBinding], int]:
        stmt = self.parse_block()
        self.expect("EOF")
        return stmt, self.env.all_decls, self.env.used_bytes

    def parse_block(self) -> Stmt:
        self.expect("LBRACE")
        self.env.push_block()
        self.parse_decls()
        stmts: list[Stmt] = []
        while self.peek().kind not in ("RBRACE", "EOF"):
            stmts.append(self.parse_stmt())
        self.expect("RBRACE")
        self.env.pop_block()
        if not stmts:
            return Stmt("empty")
        if len(stmts) == 1:
            return stmts[0]
        return Stmt("seq", (stmts,))

    def parse_decls(self) -> None:
        while self.peek().kind in ("INT_KW", "FLOAT_KW", "BOOL_KW"):
            ty = self.parse_type()
            tok = self.advance()
            if tok.kind != "IDENT":
                raise ValueError(f"line {tok.line}: expected identifier, found {tok.kind}")
            self.expect("SEMI")
            self.env.declare(str(tok.value), ty, tok.line)

    def parse_type(self) -> Type:
        tok = self.advance()
        base = {"INT_KW": T_INT, "FLOAT_KW": T_FLOAT, "BOOL_KW": T_BOOL}[tok.kind]
        dims: list[int] = []
        while self.eat("LBRACKET"):
            dtok = self.advance()
            if dtok.kind != "INT_LIT" or int(dtok.value) <= 0:  # type: ignore[arg-type]
                raise ValueError(f"line {dtok.line}: expected positive array size")
            self.expect("RBRACKET")
            dims.append(int(dtok.value))  # type: ignore[arg-type]
        ty = base
        for length in reversed(dims):
            ty = Type("array", length, ty)
        return ty

    def parse_stmt(self) -> Stmt:
        k = self.peek().kind
        ln = self.line()
        if k == "SEMI":
            self.advance()
            return Stmt("empty")
        if k == "LBRACE":
            return self.parse_block()
        if k == "IF":
            self.advance()
            self.expect("LPAREN")
            cond = self.parse_bool_expr()
            self.expect("RPAREN")
            then_s = self.parse_stmt()
            if self.eat("ELSE"):
                return Stmt("else", (cond, then_s, self.parse_stmt()))
            return Stmt("if", (cond, then_s))
        if k == "WHILE":
            self.advance()
            self.expect("LPAREN")
            cond = self.parse_bool_expr()
            self.expect("RPAREN")
            lid = self.next_loop_id
            self.next_loop_id += 1
            self.loop_stack.append(lid)
            body = self.parse_stmt()
            self.loop_stack.pop()
            return Stmt("while", (lid, cond, body))
        if k == "DO":
            self.advance()
            lid = self.next_loop_id
            self.next_loop_id += 1
            self.loop_stack.append(lid)
            body = self.parse_stmt()
            self.loop_stack.pop()
            self.expect("WHILE")
            self.expect("LPAREN")
            cond = self.parse_bool_expr()
            self.expect("RPAREN")
            self.expect("SEMI")
            return Stmt("dowhile", (lid, body, cond))
        if k == "BREAK":
            self.advance()
            self.expect("SEMI")
            if not self.loop_stack:
                raise ValueError(f"line {ln}: unenclosed 'break' statement")
            return Stmt("break", (self.loop_stack[-1],))
        if k == "IDENT":
            name = str(self.advance().value)
            binding = self.env.lookup(name)
            if binding is None:
                raise ValueError(f"line {ln}: undeclared identifier '{name}'")
            if self.peek().kind == "LBRACKET":
                base_slot, offset_expr, elem_ty = self.parse_array_offset(binding, ln)
                self.expect("ASSIGN")
                rhs = self.parse_expr()
                if not assignable(elem_ty, rhs.ty):
                    raise ValueError(f"line {ln}: type error assigning {rhs.ty} to {elem_ty}")
                self.expect("SEMI")
                return Stmt("setelem", (base_slot, offset_expr, rhs))
            self.expect("ASSIGN")
            rhs = self.parse_expr()
            if not assignable(binding.ty, rhs.ty):
                raise ValueError(f"line {ln}: type error assigning {rhs.ty} to {binding.ty}")
            self.expect("SEMI")
            return Stmt("set", (binding.slot, rhs))
        raise ValueError(f"line {ln}: unexpected token in statement: {k}")

    def parse_bool_expr(self) -> Expr:
        ln = self.line()
        e = self.parse_expr()
        if e.ty != T_BOOL:
            raise ValueError(f"line {ln}: boolean required in condition, got {e.ty}")
        return e

    def parse_array_offset(self, binding: VarBinding, line: int) -> tuple[str, Expr, Type]:
        cur_ty = binding.ty
        total_offset: Expr | None = None
        while self.eat("LBRACKET"):
            if cur_ty.kind != "array" or cur_ty.elem is None:
                raise ValueError(f"line {line}: cannot index non-array type {cur_ty}")
            elem_ty = cur_ty.elem
            idx_ln = self.line()
            idx = self.parse_expr()
            if idx.ty != T_INT:
                raise ValueError(f"line {idx_ln}: array subscript must be int, got {idx.ty}")
            self.expect("RBRACKET")
            scaled = Expr("arith", T_INT, ("*", idx, Expr("const", T_INT, (elem_ty.width(),))))
            total_offset = scaled if total_offset is None else Expr("arith", T_INT, ("+", total_offset, scaled))
            cur_ty = elem_ty
        assert total_offset is not None
        return binding.slot, total_offset, cur_ty

    def parse_expr(self) -> Expr:
        lhs = self.parse_join()
        while self.eat("OR_OR"):
            ln = self.line()
            rhs = self.parse_join()
            if lhs.ty != T_BOOL or rhs.ty != T_BOOL:
                raise ValueError(f"line {ln}: '||' requires bool operands")
            lhs = Expr("or", T_BOOL, (lhs, rhs))
        return lhs

    def parse_join(self) -> Expr:
        lhs = self.parse_equality()
        while self.eat("AND_AND"):
            ln = self.line()
            rhs = self.parse_equality()
            if lhs.ty != T_BOOL or rhs.ty != T_BOOL:
                raise ValueError(f"line {ln}: '&&' requires bool operands")
            lhs = Expr("and", T_BOOL, (lhs, rhs))
        return lhs

    def parse_equality(self) -> Expr:
        lhs = self.parse_rel()
        while self.peek().kind in ("EQ_EQ", "NE"):
            ln = self.line()
            op = "==" if self.advance().kind == "EQ_EQ" else "!="
            rhs = self.parse_rel()
            if not (lhs.ty == rhs.ty or (lhs.ty.is_numeric() and rhs.ty.is_numeric())):
                raise ValueError(f"line {ln}: incompatible types {lhs.ty} and {rhs.ty} for {op}")
            lhs = Expr("rel", T_BOOL, (op, lhs, rhs))
        return lhs

    def parse_rel(self) -> Expr:
        lhs = self.parse_arith()
        rmap = {"LT": "<", "LE": "<=", "GT": ">", "GE": ">="}
        if self.peek().kind in rmap:
            ln = self.line()
            op = rmap[self.advance().kind]
            rhs = self.parse_arith()
            if not lhs.ty.is_numeric() or not rhs.ty.is_numeric():
                raise ValueError(f"line {ln}: ordering '{op}' requires numeric operands")
            return Expr("rel", T_BOOL, (op, lhs, rhs))
        return lhs

    def parse_arith(self) -> Expr:
        lhs = self.parse_term()
        while self.peek().kind in ("PLUS", "MINUS"):
            ln = self.line()
            op = "+" if self.advance().kind == "PLUS" else "-"
            rhs = self.parse_term()
            res_ty = Type.max_numeric(lhs.ty, rhs.ty)
            if res_ty is None:
                raise ValueError(f"line {ln}: arithmetic '{op}' requires numeric types")
            lhs = Expr("arith", res_ty, (op, lhs, rhs))
        return lhs

    def parse_term(self) -> Expr:
        lhs = self.parse_unary()
        while self.peek().kind in ("STAR", "SLASH"):
            ln = self.line()
            op = "*" if self.advance().kind == "STAR" else "/"
            rhs = self.parse_unary()
            res_ty = Type.max_numeric(lhs.ty, rhs.ty)
            if res_ty is None:
                raise ValueError(f"line {ln}: arithmetic '{op}' requires numeric types")
            lhs = Expr("arith", res_ty, (op, lhs, rhs))
        return lhs

    def parse_unary(self) -> Expr:
        ln = self.line()
        if self.eat("BANG"):
            inner = self.parse_unary()
            if inner.ty != T_BOOL:
                raise ValueError(f"line {ln}: '!' requires bool operand")
            return Expr("not", T_BOOL, (inner,))
        if self.eat("MINUS"):
            inner = self.parse_unary()
            if not inner.ty.is_numeric():
                raise ValueError(f"line {ln}: unary '-' requires numeric operand")
            return Expr("neg", inner.ty, (inner,))
        return self.parse_factor()

    def parse_factor(self) -> Expr:
        tok = self.advance()
        ln = tok.line
        if tok.kind == "LPAREN":
            inner = self.parse_expr()
            self.expect("RPAREN")
            return inner
        if tok.kind == "INT_LIT":
            return Expr("const", T_INT, (tok.value,))
        if tok.kind == "FLOAT_LIT":
            return Expr("const", T_FLOAT, (tok.value,))
        if tok.kind == "TRUE_KW":
            return Expr("const", T_BOOL, (True,))
        if tok.kind == "FALSE_KW":
            return Expr("const", T_BOOL, (False,))
        if tok.kind == "IDENT":
            binding = self.env.lookup(str(tok.value))
            if binding is None:
                raise ValueError(f"line {ln}: undeclared identifier '{tok.value}'")
            if self.peek().kind == "LBRACKET":
                base_slot, offset_expr, elem_ty = self.parse_array_offset(binding, ln)
                return Expr("access", elem_ty, (base_slot, offset_expr))
            return Expr("id", binding.ty, (binding.slot,))
        raise ValueError(f"line {ln}: unexpected token {tok.kind}")


# ---------------------------------------------------------------------------
# 5. Label-Driven TAC Emitter (`gen` & Short-Circuit `jumping`)
# ---------------------------------------------------------------------------

@dataclass(frozen=True)
class Operand:
    is_const: bool
    val: object

    def __str__(self) -> str:
        if not self.is_const:
            return str(self.val)
        if isinstance(self.val, bool):
            return "true" if self.val else "false"
        if isinstance(self.val, float):
            return f"{self.val:.4f}"
        return str(self.val)


@dataclass(frozen=True)
class TacInst:
    op: str
    args: tuple[object, ...] = ()

    def __str__(self) -> str:
        if self.op == "label":
            return f"L{self.args[0]}:"
        if self.op == "assign":
            return f"  {self.args[0]} = {self.args[1]}"
        if self.op == "bin":
            return f"  {self.args[0]} = {self.args[2]} {self.args[1]} {self.args[3]}"
        if self.op == "neg":
            return f"  {self.args[0]} = -{self.args[1]}"
        if self.op == "aload":
            return f"  {self.args[0]} = {self.args[1]}[{self.args[2]}]"
        if self.op == "astore":
            return f"  {self.args[0]}[{self.args[1]}] = {self.args[2]}"
        if self.op == "goto":
            return f"  goto L{self.args[0]}"
        if self.op == "if_true":
            return f"  if {self.args[0]} goto L{self.args[1]}"
        if self.op == "if_false":
            return f"  iffalse {self.args[0]} goto L{self.args[1]}"
        if self.op == "if_rel":
            return f"  if {self.args[0]} {self.args[1]} {self.args[2]} goto L{self.args[3]}"
        return f"  iffalse {self.args[0]} {self.args[1]} {self.args[2]} goto L{self.args[3]}"


class Emitter:
    def __init__(self) -> None:
        self.next_label = 0
        self.next_temp = 0
        self.loop_after: dict[int, int] = {}
        self.code: list[TacInst] = []

    def new_label(self) -> int:
        self.next_label += 1
        return self.next_label

    def new_temp(self) -> str:
        self.next_temp += 1
        return f"t{self.next_temp}"

    def emit_label(self, l: int) -> None:
        self.code.append(TacInst("label", (l,)))

    def reduce(self, expr: Expr) -> Operand:
        if expr.kind == "const":
            return Operand(True, expr.args[0])
        if expr.kind == "id":
            return Operand(False, expr.args[0])
        if expr.kind == "access":
            base, off_expr = expr.args
            off = self.reduce(off_expr)  # type: ignore[arg-type]
            dst = self.new_temp()
            self.code.append(TacInst("aload", (dst, base, off)))
            return Operand(False, dst)
        if expr.kind == "arith":
            op, l_e, r_e = expr.args
            l, r = self.reduce(l_e), self.reduce(r_e)  # type: ignore[arg-type]
            dst = self.new_temp()
            self.code.append(TacInst("bin", (dst, op, l, r)))
            return Operand(False, dst)
        if expr.kind == "neg":
            v = self.reduce(expr.args[0])  # type: ignore[arg-type]
            dst = self.new_temp()
            self.code.append(TacInst("neg", (dst, v)))
            return Operand(False, dst)
        # Materialize boolean into a temp via jumping(t, f)
        t, f, end = self.new_label(), self.new_label(), self.new_label()
        dst = self.new_temp()
        self.jumping(expr, t, f)
        self.emit_label(t)
        self.code.append(TacInst("assign", (dst, Operand(True, True))))
        self.code.append(TacInst("goto", (end,)))
        self.emit_label(f)
        self.code.append(TacInst("assign", (dst, Operand(True, False))))
        self.emit_label(end)
        return Operand(False, dst)

    def jumping(self, expr: Expr, t: int, f: int) -> None:
        """Short-circuit boolean jumps; target 0 means fall through cleanly."""
        if expr.kind == "const":
            if expr.args[0] is True and t != 0:
                self.code.append(TacInst("goto", (t,)))
            elif expr.args[0] is False and f != 0:
                self.code.append(TacInst("goto", (f,)))
            return
        if expr.kind == "not":
            self.jumping(expr.args[0], f, t)  # type: ignore[arg-type]
            return
        if expr.kind == "or":
            left, right = expr.args
            label = t if t != 0 else self.new_label()
            self.jumping(left, label, 0)  # type: ignore[arg-type]
            self.jumping(right, t, f)  # type: ignore[arg-type]
            if t == 0:
                self.emit_label(label)
            return
        if expr.kind == "and":
            left, right = expr.args
            label = f if f != 0 else self.new_label()
            self.jumping(left, 0, label)  # type: ignore[arg-type]
            self.jumping(right, t, f)  # type: ignore[arg-type]
            if f == 0:
                self.emit_label(label)
            return
        if expr.kind == "rel":
            op, l_e, r_e = expr.args
            l, r = self.reduce(l_e), self.reduce(r_e)  # type: ignore[arg-type]
            if t != 0 and f != 0:
                self.code.append(TacInst("if_rel", (l, op, r, t)))
                self.code.append(TacInst("goto", (f,)))
            elif t != 0:
                self.code.append(TacInst("if_rel", (l, op, r, t)))
            elif f != 0:
                self.code.append(TacInst("if_not_rel", (l, op, r, f)))
            return
        v = self.reduce(expr)
        if t != 0 and f != 0:
            self.code.append(TacInst("if_true", (v, t)))
            self.code.append(TacInst("goto", (f,)))
        elif t != 0:
            self.code.append(TacInst("if_true", (v, t)))
        elif f != 0:
            self.code.append(TacInst("if_false", (v, f)))

    def gen_stmt(self, stmt: Stmt, begin_lbl: int, after_lbl: int) -> None:
        k = stmt.kind
        if k == "empty":
            return
        if k == "seq":
            stmts: list[Stmt] = stmt.args[0]  # type: ignore[assignment]
            cur_begin = begin_lbl
            for idx, s in enumerate(stmts):
                if idx + 1 == len(stmts):
                    self.gen_stmt(s, cur_begin, after_lbl)
                else:
                    mid = self.new_label()
                    self.gen_stmt(s, cur_begin, mid)
                    self.emit_label(mid)
                    cur_begin = mid
        elif k == "set":
            slot, rhs = stmt.args
            self.code.append(TacInst("assign", (slot, self.reduce(rhs))))  # type: ignore[arg-type]
        elif k == "setelem":
            base, off_e, rhs = stmt.args
            self.code.append(
                TacInst("astore", (base, self.reduce(off_e), self.reduce(rhs)))  # type: ignore[arg-type]
            )
        elif k == "if":
            cond, then_s = stmt.args
            l_body = self.new_label()
            self.jumping(cond, 0, after_lbl)  # type: ignore[arg-type]
            self.emit_label(l_body)
            self.gen_stmt(then_s, l_body, after_lbl)  # type: ignore[arg-type]
        elif k == "else":
            cond, then_s, else_s = stmt.args
            l_then, l_else = self.new_label(), self.new_label()
            self.jumping(cond, 0, l_else)  # type: ignore[arg-type]
            self.emit_label(l_then)
            self.gen_stmt(then_s, l_then, after_lbl)  # type: ignore[arg-type]
            self.code.append(TacInst("goto", (after_lbl,)))
            self.emit_label(l_else)
            self.gen_stmt(else_s, l_else, after_lbl)  # type: ignore[arg-type]
        elif k == "while":
            lid, cond, body = stmt.args
            self.loop_after[int(lid)] = after_lbl  # type: ignore[arg-type]
            self.jumping(cond, 0, after_lbl)  # type: ignore[arg-type]
            l_body = self.new_label()
            self.emit_label(l_body)
            self.gen_stmt(body, l_body, begin_lbl)  # type: ignore[arg-type]
            self.code.append(TacInst("goto", (begin_lbl,)))
        elif k == "dowhile":
            lid, body, cond = stmt.args
            self.loop_after[int(lid)] = after_lbl  # type: ignore[arg-type]
            l_cond = self.new_label()
            self.gen_stmt(body, begin_lbl, l_cond)  # type: ignore[arg-type]
            self.emit_label(l_cond)
            self.jumping(cond, begin_lbl, 0)  # type: ignore[arg-type]
        elif k == "break":
            self.code.append(TacInst("goto", (self.loop_after[int(stmt.args[0])],)))  # type: ignore[arg-type]


@dataclass
class CompiledProgram:
    decls: list[VarBinding]
    used_bytes: int
    code: list[TacInst]


def compile_frontend(src: str) -> CompiledProgram:
    ast, decls, used_bytes = Parser(lex(src)).parse_program()
    emitter = Emitter()
    begin, after = emitter.new_label(), emitter.new_label()
    emitter.emit_label(begin)
    emitter.gen_stmt(ast, begin, after)
    emitter.emit_label(after)
    return CompiledProgram(decls, used_bytes, emitter.code)


# ---------------------------------------------------------------------------
# 6. Byte-Offset TAC Interpreter
# ---------------------------------------------------------------------------

def exec_tac(prog: CompiledProgram) -> tuple[dict[str, object], dict[tuple[str, int], object]]:
    labels = {inst.args[0]: idx for idx, inst in enumerate(prog.code) if inst.op == "label"}
    vars_mem: dict[str, object] = {}
    arrays_mem: dict[tuple[str, int], object] = {}

    def eval_op(op: Operand) -> object:
        return op.val if op.is_const else vars_mem.get(str(op.val), 0)

    def eval_rel(l: object, op: str, r: object) -> bool:
        if op == "==":
            return l == r
        if op == "!=":
            return l != r
        lf, rf = float(l), float(r)  # type: ignore[arg-type]
        if op == "<":
            return lf < rf
        if op == "<=":
            return lf <= rf
        if op == ">":
            return lf > rf
        return lf >= rf

    pc = 0
    while pc < len(prog.code):
        inst = prog.code[pc]
        if inst.op == "label":
            pc += 1
        elif inst.op == "assign":
            vars_mem[str(inst.args[0])] = eval_op(inst.args[1])  # type: ignore[arg-type]
            pc += 1
        elif inst.op == "bin":
            dst, bop, l_op, r_op = inst.args
            lv, rv = eval_op(l_op), eval_op(r_op)  # type: ignore[arg-type]
            if isinstance(lv, int) and isinstance(rv, int) and not isinstance(lv, bool):
                out: object = (
                    lv + rv if bop == "+" else lv - rv if bop == "-" else lv * rv if bop == "*" else lv // rv
                )
            else:
                lf, rf = float(lv), float(rv)  # type: ignore[arg-type]
                out = lf + rf if bop == "+" else lf - rf if bop == "-" else lf * rf if bop == "*" else lf / rf
            vars_mem[str(dst)] = out
            pc += 1
        elif inst.op == "neg":
            vars_mem[str(inst.args[0])] = -eval_op(inst.args[1])  # type: ignore[arg-type,operator]
            pc += 1
        elif inst.op == "aload":
            dst, base, off_op = inst.args
            off = int(eval_op(off_op))  # type: ignore[arg-type]
            vars_mem[str(dst)] = arrays_mem.get((str(base), off), 0)
            pc += 1
        elif inst.op == "astore":
            base, off_op, src_op = inst.args
            off = int(eval_op(off_op))  # type: ignore[arg-type]
            arrays_mem[(str(base), off)] = eval_op(src_op)  # type: ignore[arg-type]
            pc += 1
        elif inst.op == "goto":
            pc = int(labels[inst.args[0]])
        elif inst.op == "if_true":
            pc = int(labels[inst.args[1]]) if bool(eval_op(inst.args[0])) else pc + 1  # type: ignore[arg-type]
        elif inst.op == "if_false":
            pc = int(labels[inst.args[1]]) if not bool(eval_op(inst.args[0])) else pc + 1  # type: ignore[arg-type]
        elif inst.op == "if_rel":
            l_op, rop, r_op, tgt = inst.args
            pc = int(labels[tgt]) if eval_rel(eval_op(l_op), str(rop), eval_op(r_op)) else pc + 1  # type: ignore[arg-type]
        else:
            l_op, rop, r_op, tgt = inst.args
            pc = int(labels[tgt]) if not eval_rel(eval_op(l_op), str(rop), eval_op(r_op)) else pc + 1  # type: ignore[arg-type]
    return vars_mem, arrays_mem


SAMPLE_SRC = """
{
    int i;
    int sum;
    float[2][3] grid;
    sum = 0;
    i = 0;
    while (i < 2) {
        int j;
        {
            // Inner `i` shadows outer `i` cleanly via chained Env blocks.
            int i;
            i = 100;
            sum = sum + i;
        }
        j = 0;
        do {
            if (i == 1 && j == 2) break;
            grid[i][j] = (i * 3 + j) + 0.5;
            j = j + 1;
        } while (j < 3);
        i = i + 1;
    }
}
"""


def main() -> None:
    prog = compile_frontend(SAMPLE_SRC)
    print(f"Lexical symbol bindings ({prog.used_bytes} bytes total storage):")
    for b in prog.decls:
        print(f"  {b.slot:<8} : {str(b.ty):<14} block={b.block_id} offset={b.offset}")
    print(f"\nEmitted TAC ({len(prog.code)} instructions):")
    for inst in prog.code:
        print(inst)
    vars_mem, arrays_mem = exec_tac(prog)
    print("\nFinal state checks:")
    print(f"  outer i@b0     = {vars_mem.get('i@b0')}")
    print(f"  inner i@b2     = {vars_mem.get('i@b2')}")
    print(f"  sum@b0         = {vars_mem.get('sum@b0')}")
    print(f"  grid[0][2]     = {arrays_mem.get(('grid@b0', 16))}")
    print(f"  grid[1][1]     = {arrays_mem.get(('grid@b0', 32))}")


class CompleteFrontendTests(unittest.TestCase):
    def test_lowers_nested_scopes_multidim_arrays_short_circuit_and_break(self) -> None:
        prog = compile_frontend(SAMPLE_SRC)
        vars_mem, arrays_mem = exec_tac(prog)
        self.assertEqual(vars_mem.get("i@b0"), 2)
        self.assertEqual(vars_mem.get("i@b2"), 100)
        self.assertEqual(vars_mem.get("sum@b0"), 200)
        self.assertEqual(arrays_mem.get(("grid@b0", 16)), 2.5)
        self.assertEqual(arrays_mem.get(("grid@b0", 32)), 4.5)
        self.assertNotIn(("grid@b0", 40), arrays_mem)

    def test_rejects_invalid_break_duplicates_and_type_errors(self) -> None:
        with self.assertRaisesRegex(ValueError, "unenclosed 'break'"):
            compile_frontend("{ break; }")
        with self.assertRaisesRegex(ValueError, "duplicate declaration"):
            compile_frontend("{ int x; float x; }")
        with self.assertRaisesRegex(ValueError, "boolean required"):
            compile_frontend("{ int x; if (x) x = 1; }")
        with self.assertRaisesRegex(ValueError, "cannot index non-array"):
            compile_frontend("{ int x; x = x[0]; }")


if __name__ == "__main__":
    main()
