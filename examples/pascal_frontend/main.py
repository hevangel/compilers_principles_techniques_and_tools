"""Appendix A (1986 Red) capstone: Pascal-subset compiler front end in Python.
Pipeline: Pascal Lexer -> Nested Scope Symbol Table -> Recursive-Descent Parser & Type Checker -> Three-Address Code (TAC) + TAC Simulator.
"""

from dataclasses import dataclass, field
import unittest


# ---------------------------------------------------------------------------
# 1. Lexical Analysis (Scanner)
# ---------------------------------------------------------------------------

KEYWORDS = {
    "program", "var", "integer", "real", "boolean", "array", "of",
    "procedure", "function", "begin", "end", "if", "then", "else",
    "while", "do", "div", "mod", "and", "or", "not", "true", "false", "write",
}


@dataclass(frozen=True)
class Token:
    kind: str
    value: object
    line: int


def lex(src: str) -> list[Token]:
    """Scan Pascal source text with case-insensitive keywords and `{}`/`(* *)` comments."""
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
        # Brace comment `{ ... }`
        if ch == "{":
            i += 1
            while i < n and src[i] != "}":
                if src[i] == "\n":
                    line += 1
                i += 1
            if i >= n:
                raise ValueError(f"line {line}: unterminated '{{' comment")
            i += 1
            continue
        # Paren-star comment `(* ... *)`
        if ch == "(" and i + 1 < n and src[i + 1] == "*":
            i += 2
            while i + 1 < n and not (src[i] == "*" and src[i + 1] == ")"):
                if src[i] == "\n":
                    line += 1
                i += 1
            if i + 1 >= n:
                raise ValueError(f"line {line}: unterminated '(*' comment")
            i += 2
            continue
        # Identifiers and reserved words (normalized to lowercase per Pascal rules)
        if ch.isalpha() or ch == "_":
            start = i
            i += 1
            while i < n and (src[i].isalnum() or src[i] == "_"):
                i += 1
            word = src[start:i].lower()
            if word in KEYWORDS:
                out.append(Token(word.upper(), word, line))
            else:
                out.append(Token("IDENT", word, line))
            continue
        # Numbers: require a digit after '.' for reals so `1..4` keeps `..` as DOTS
        if ch.isdigit():
            start = i
            while i < n and src[i].isdigit():
                i += 1
            if i + 1 < n and src[i] == "." and src[i + 1].isdigit():
                i += 1
                while i < n and src[i].isdigit():
                    i += 1
                out.append(Token("REAL_LIT", float(src[start:i]), line))
            else:
                out.append(Token("INT_LIT", int(src[start:i]), line))
            continue
        # Multi-character and single-character tokens
        two = src[i : i + 2]
        two_map = {
            ":=": "ASSIGN",
            "..": "DOTS",
            "<=": "LE",
            ">=": "GE",
            "<>": "NE",
        }
        if two in two_map:
            out.append(Token(two_map[two], two, line))
            i += 2
            continue
        one_map = {
            ":": "COLON",
            ".": "DOT",
            "<": "LT",
            ">": "GT",
            "=": "EQ",
            "+": "PLUS",
            "-": "MINUS",
            "*": "STAR",
            "/": "SLASH",
            ";": "SEMI",
            ",": "COMMA",
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
# 2. Types and Nested Symbol Table
# ---------------------------------------------------------------------------

@dataclass(frozen=True)
class PasType:
    kind: str  # "integer", "real", "boolean", "void", "array"
    low: int = 0
    high: int = 0
    elem: "PasType | None" = None

    def byte_width(self) -> int:
        if self.kind == "integer":
            return 4
        if self.kind == "real":
            return 8
        if self.kind == "boolean":
            return 1
        if self.kind == "void":
            return 0
        if self.kind == "array" and self.elem is not None:
            return max(0, self.high - self.low + 1) * self.elem.byte_width()
        return 0

    def __str__(self) -> str:
        if self.kind == "array":
            return f"array[{self.low}..{self.high}] of {self.elem}"
        return self.kind


T_INT = PasType("integer")
T_REAL = PasType("real")
T_BOOL = PasType("boolean")
T_VOID = PasType("void")


@dataclass
class Symbol:
    name: str
    is_subprogram: bool
    slot_name: str = ""
    ty: PasType = T_VOID
    offset: int = 0
    params: list[PasType] = field(default_factory=list)
    ret: PasType = T_VOID
    entry_label: str = ""


@dataclass
class Scope:
    label: str
    symbols: dict[str, Symbol] = field(default_factory=dict)
    next_offset: int = 0


class SymTab:
    """Stack of lexical scopes: scope[0] is program-global; scope[1] is active subprogram."""

    def __init__(self) -> None:
        self.scopes = [Scope("global")]

    def push_scope(self, label: str) -> None:
        self.scopes.append(Scope(label))

    def pop_scope(self) -> Scope:
        return self.scopes.pop()

    def declare_var(self, name: str, ty: PasType, line: int) -> str:
        top = self.scopes[-1]
        if name in top.symbols:
            raise ValueError(f"line {line}: duplicate identifier '{name}' in scope '{top.label}'")
        slot_name = name if len(self.scopes) == 1 else f"{top.label}_{name}"
        offset = top.next_offset
        top.next_offset += ty.byte_width()
        top.symbols[name] = Symbol(name=name, is_subprogram=False, slot_name=slot_name, ty=ty, offset=offset)
        return slot_name

    def declare_subprogram(self, name: str, params: list[PasType], ret: PasType, line: int) -> str:
        g = self.scopes[0]
        if name in g.symbols:
            raise ValueError(f"line {line}: duplicate subprogram '{name}'")
        entry_label = f"fn_{name}"
        g.symbols[name] = Symbol(
            name=name, is_subprogram=True, params=params, ret=ret, entry_label=entry_label
        )
        return entry_label

    def lookup(self, name: str) -> Symbol | None:
        for sc in reversed(self.scopes):
            if name in sc.symbols:
                return sc.symbols[name]
        return None


# ---------------------------------------------------------------------------
# 3. Three-Address Code (TAC) Intermediate Representation
# ---------------------------------------------------------------------------

@dataclass(frozen=True)
class Operand:
    is_const: bool
    val: object  # var name (str) when is_const=False, literal value otherwise

    def __str__(self) -> str:
        if not self.is_const:
            return str(self.val)
        if isinstance(self.val, float):
            return f"{self.val:.4f}"
        if isinstance(self.val, bool):
            return "true" if self.val else "false"
        return str(self.val)


@dataclass(frozen=True)
class TacInst:
    op: str
    args: tuple[object, ...] = ()

    def __str__(self) -> str:
        if self.op == "label":
            return f"{self.args[0]}:"
        if self.op == "assign":
            return f"  {self.args[0]} := {self.args[1]}"
        if self.op == "bin":
            dst, bop, l, r = self.args
            return f"  {dst} := {l} {bop} {r}"
        if self.op == "neg":
            return f"  {self.args[0]} := -{self.args[1]}"
        if self.op == "not":
            return f"  {self.args[0]} := not {self.args[1]}"
        if self.op == "int_to_real":
            return f"  {self.args[0]} := int_to_real({self.args[1]})"
        if self.op == "aload":
            return f"  {self.args[0]} := {self.args[1]}[{self.args[2]}]"
        if self.op == "astore":
            return f"  {self.args[0]}[{self.args[1]}] := {self.args[2]}"
        if self.op == "param":
            return f"  param {self.args[0]}"
        if self.op == "call":
            dst, callee, param_slots = self.args
            prefix = f"{dst} := " if dst is not None else ""
            return f"  {prefix}call {callee}({len(param_slots)})"
        if self.op == "return":
            return f"  return {self.args[0]}" if self.args[0] is not None else "  return"
        if self.op == "write":
            return f"  write {self.args[0]}"
        if self.op == "goto":
            return f"  goto {self.args[0]}"
        if self.op == "ifFalse":
            return f"  ifFalse {self.args[0]} goto {self.args[1]}"
        return "  halt"


@dataclass
class TypedExpr:
    op: Operand
    ty: PasType


@dataclass
class CompiledUnit:
    program_name: str
    global_offsets: list[tuple[str, PasType, int]]
    code: list[TacInst]
    subprogram_params: dict[str, list[str]]


# ---------------------------------------------------------------------------
# 4. Recursive-Descent Parser, Type Checker, and TAC Generator
# ---------------------------------------------------------------------------

class Parser:
    def __init__(self, tokens: list[Token]) -> None:
        self.tokens = tokens
        self.pos = 0
        self.sym = SymTab()
        self.code: list[TacInst] = []
        self.subprogram_params: dict[str, list[str]] = {}
        self.next_temp = 0
        self.next_label = 0
        self.active_func_ret: tuple[str, str, PasType] | None = None

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

    def expect_ident(self) -> tuple[str, int]:
        tok = self.advance()
        if tok.kind != "IDENT":
            raise ValueError(f"line {tok.line}: expected identifier, found {tok.kind}")
        return str(tok.value), tok.line

    def new_temp(self) -> str:
        self.next_temp += 1
        return f"t{self.next_temp}"

    def new_label(self, prefix: str) -> str:
        self.next_label += 1
        return f"L{self.next_label}_{prefix}"

    def widen_to_real(self, expr: TypedExpr) -> TypedExpr:
        """Promote integer operands to real when paired with real values."""
        if expr.ty == T_REAL:
            return expr
        dst = self.new_temp()
        self.code.append(TacInst("int_to_real", (dst, expr.op)))
        return TypedExpr(Operand(False, dst), T_REAL)

    def parse_program(self) -> CompiledUnit:
        self.expect("PROGRAM")
        prog_name, _ = self.expect_ident()
        self.expect("SEMI")
        self.parse_var_section()

        # Skip past procedure/function bodies so execution begins at main block.
        main_label = self.new_label("main")
        self.code.append(TacInst("goto", (main_label,)))

        while self.peek().kind in ("PROCEDURE", "FUNCTION"):
            self.parse_subprogram()

        self.code.append(TacInst("label", (main_label,)))
        self.parse_compound_stmt()
        self.expect("DOT")
        self.expect("EOF")
        self.code.append(TacInst("halt"))

        global_offsets = [
            (s.name, s.ty, s.offset)
            for s in self.sym.scopes[0].symbols.values()
            if not s.is_subprogram
        ]
        return CompiledUnit(prog_name, global_offsets, self.code, self.subprogram_params)

    def parse_var_section(self) -> None:
        if not self.eat("VAR"):
            return
        while self.peek().kind == "IDENT":
            names = [self.expect_ident()]
            while self.eat("COMMA"):
                names.append(self.expect_ident())
            self.expect("COLON")
            ty = self.parse_type()
            self.expect("SEMI")
            for name, ln in names:
                self.sym.declare_var(name, ty, ln)

    def parse_signed_int(self) -> int:
        neg = self.eat("MINUS")
        tok = self.advance()
        if tok.kind != "INT_LIT":
            raise ValueError(f"line {tok.line}: expected integer bound, found {tok.kind}")
        v = int(tok.value)  # type: ignore[arg-type]
        return -v if neg else v

    def parse_type(self) -> PasType:
        tok = self.advance()
        if tok.kind == "INTEGER":
            return T_INT
        if tok.kind == "REAL":
            return T_REAL
        if tok.kind == "BOOLEAN":
            return T_BOOL
        if tok.kind == "ARRAY":
            self.expect("LBRACKET")
            low = self.parse_signed_int()
            self.expect("DOTS")
            high = self.parse_signed_int()
            if low > high:
                raise ValueError(f"line {tok.line}: invalid array bounds [{low}..{high}] (low > high)")
            self.expect("RBRACKET")
            self.expect("OF")
            elem = self.parse_type()
            return PasType("array", low, high, elem)
        raise ValueError(f"line {tok.line}: expected type name, found {tok.kind}")

    def parse_subprogram(self) -> None:
        is_func = self.advance().kind == "FUNCTION"
        name, name_ln = self.expect_ident()
        params: list[tuple[str, PasType, int]] = []
        if self.eat("LPAREN"):
            if self.peek().kind != "RPAREN":
                while True:
                    group = [self.expect_ident()]
                    while self.eat("COMMA"):
                        group.append(self.expect_ident())
                    self.expect("COLON")
                    ty = self.parse_type()
                    for pname, pline in group:
                        params.append((pname, ty, pline))
                    if not self.eat("SEMI"):
                        break
            self.expect("RPAREN")

        if is_func:
            self.expect("COLON")
            ret_ty = self.parse_type()
            if ret_ty.kind == "array":
                raise ValueError(f"line {self.line()}: functions cannot return arrays")
        else:
            ret_ty = T_VOID
        self.expect("SEMI")

        entry_label = self.sym.declare_subprogram(name, [t for _, t, _ in params], ret_ty, name_ln)
        self.sym.push_scope(name)
        param_slots = [self.sym.declare_var(pn, pt, pl) for pn, pt, pl in params]
        self.subprogram_params[name] = param_slots
        self.active_func_ret = (name, f"{name}__ret", ret_ty) if is_func else None

        self.parse_var_section()
        self.code.append(TacInst("label", (entry_label,)))
        self.parse_compound_stmt()
        self.expect("SEMI")

        if self.active_func_ret is not None:
            self.code.append(TacInst("return", (Operand(False, self.active_func_ret[1]),)))
            self.active_func_ret = None
        else:
            self.code.append(TacInst("return", (None,)))
        self.sym.pop_scope()

    def parse_compound_stmt(self) -> None:
        self.expect("BEGIN")
        if self.peek().kind != "END":
            self.parse_stmt()
            while self.eat("SEMI"):
                if self.peek().kind == "END":
                    break
                self.parse_stmt()
        self.expect("END")

    def parse_stmt(self) -> None:
        k = self.peek().kind
        if k == "BEGIN":
            self.parse_compound_stmt()
            return
        if k == "IF":
            self.advance()
            cond_ln = self.line()
            cond = self.parse_expr()
            if cond.ty != T_BOOL:
                raise ValueError(f"line {cond_ln}: if condition must be boolean, got {cond.ty}")
            self.expect("THEN")
            l_else = self.new_label("else")
            l_end = self.new_label("endif")
            self.code.append(TacInst("ifFalse", (cond.op, l_else)))
            self.parse_stmt()
            if self.eat("ELSE"):
                self.code.append(TacInst("goto", (l_end,)))
                self.code.append(TacInst("label", (l_else,)))
                self.parse_stmt()
                self.code.append(TacInst("label", (l_end,)))
            else:
                self.code.append(TacInst("label", (l_else,)))
            return
        if k == "WHILE":
            self.advance()
            l_top = self.new_label("while")
            l_exit = self.new_label("endwhile")
            self.code.append(TacInst("label", (l_top,)))
            cond_ln = self.line()
            cond = self.parse_expr()
            if cond.ty != T_BOOL:
                raise ValueError(f"line {cond_ln}: while condition must be boolean, got {cond.ty}")
            self.expect("DO")
            self.code.append(TacInst("ifFalse", (cond.op, l_exit)))
            self.parse_stmt()
            self.code.append(TacInst("goto", (l_top,)))
            self.code.append(TacInst("label", (l_exit,)))
            return
        if k == "WRITE":
            self.advance()
            self.expect("LPAREN")
            while True:
                arg = self.parse_expr()
                if arg.ty.kind in ("array", "void"):
                    raise ValueError(f"line {self.line()}: cannot write {arg.ty}")
                self.code.append(TacInst("write", (arg.op,)))
                if not self.eat("COMMA"):
                    break
            self.expect("RPAREN")
            return
        if k == "IDENT":
            name, ln = self.expect_ident()
            if self.active_func_ret is not None and name == self.active_func_ret[0] and self.peek().kind == "ASSIGN":
                self.advance()
                rhs = self.check_assignable(self.active_func_ret[2], self.parse_expr(), ln)
                self.code.append(TacInst("assign", (self.active_func_ret[1], rhs.op)))
                return
            if self.peek().kind == "LPAREN" or self.peek().kind not in ("ASSIGN", "LBRACKET"):
                _, ret = self.parse_call(name, ln, need_value=False)
                if ret != T_VOID:
                    raise ValueError(f"line {ln}: function '{name}' return value discarded")
                return
            sym = self.sym.lookup(name)
            if sym is None:
                raise ValueError(f"line {ln}: undeclared identifier '{name}'")
            if sym.is_subprogram:
                raise ValueError(f"line {ln}: cannot assign directly to subprogram '{name}'")
            if self.eat("LBRACKET"):
                idx = self.parse_expr()
                if idx.ty != T_INT:
                    raise ValueError(f"line {ln}: array index must be integer, got {idx.ty}")
                self.expect("RBRACKET")
                self.expect("ASSIGN")
                if sym.ty.kind != "array" or sym.ty.elem is None:
                    raise ValueError(f"line {ln}: cannot index non-array type {sym.ty}")
                rhs = self.check_assignable(sym.ty.elem, self.parse_expr(), ln)
                zero_idx = self.emit_zero_based_index(idx.op, sym.ty.low)
                self.code.append(TacInst("astore", (sym.slot_name, zero_idx, rhs.op)))
            else:
                self.expect("ASSIGN")
                rhs = self.check_assignable(sym.ty, self.parse_expr(), ln)
                self.code.append(TacInst("assign", (sym.slot_name, rhs.op)))
            return
        raise ValueError(f"line {self.line()}: expected statement, found {k}")

    def emit_zero_based_index(self, index_op: Operand, low: int) -> Operand:
        """Subtract Pascal lower bound `low` so runtime storage is 0-indexed."""
        if low == 0:
            return index_op
        tmp = self.new_temp()
        self.code.append(TacInst("bin", (tmp, "-", index_op, Operand(True, low))))
        return Operand(False, tmp)

    def check_assignable(self, target_ty: PasType, rhs: TypedExpr, line: int) -> TypedExpr:
        if target_ty == rhs.ty:
            return rhs
        if target_ty == T_REAL and rhs.ty == T_INT:
            return self.widen_to_real(rhs)
        raise ValueError(f"line {line}: type mismatch: cannot assign {rhs.ty} to {target_ty}")

    def parse_call(self, name: str, line: int, need_value: bool) -> tuple[str | None, PasType]:
        sym = self.sym.lookup(name)
        if sym is None:
            raise ValueError(f"line {line}: undeclared subprogram '{name}'")
        if not sym.is_subprogram:
            raise ValueError(f"line {line}: '{name}' is a variable, not a subprogram")
        if need_value and sym.ret == T_VOID:
            raise ValueError(f"line {line}: procedure '{name}' does not return a value")

        args: list[TypedExpr] = []
        if self.eat("LPAREN"):
            if self.peek().kind != "RPAREN":
                while True:
                    args.append(self.parse_expr())
                    if not self.eat("COMMA"):
                        break
            self.expect("RPAREN")

        if len(args) != len(sym.params):
            raise ValueError(f"line {line}: '{name}' expects {len(sym.params)} arguments, got {len(args)}")
        coerced = [self.check_assignable(pt, arg, line).op for arg, pt in zip(args, sym.params)]
        for op in coerced:
            self.code.append(TacInst("param", (op,)))
        param_slots = self.subprogram_params.get(name, [])
        dst = self.new_temp() if sym.ret != T_VOID else None
        self.code.append(TacInst("call", (dst, name, param_slots)))
        return dst, sym.ret

    def parse_expr(self) -> TypedExpr:
        left = self.parse_simple_expr()
        rel_map = {"LT": "<", "LE": "<=", "GT": ">", "GE": ">=", "EQ": "=", "NE": "<>"}
        if self.peek().kind in rel_map:
            ln = self.line()
            op = rel_map[self.advance().kind]
            right = self.parse_simple_expr()
            l, r = self.align_comparable(left, right, op, ln)
            dst = self.new_temp()
            self.code.append(TacInst("bin", (dst, op, l.op, r.op)))
            return TypedExpr(Operand(False, dst), T_BOOL)
        return left

    def align_comparable(self, left: TypedExpr, right: TypedExpr, op: str, line: int) -> tuple[TypedExpr, TypedExpr]:
        if left.ty == right.ty and left.ty in (T_INT, T_REAL):
            return left, right
        if left.ty in (T_INT, T_REAL) and right.ty in (T_INT, T_REAL):
            return self.widen_to_real(left), self.widen_to_real(right)
        if left.ty == T_BOOL and right.ty == T_BOOL and op in ("=", "<>"):
            return left, right
        raise ValueError(f"line {line}: incompatible operand types {left.ty} and {right.ty} for {op}")

    def parse_simple_expr(self) -> TypedExpr:
        unary_neg = self.eat("MINUS")
        if not unary_neg:
            self.eat("PLUS")
        lhs = self.parse_term()
        if unary_neg:
            if lhs.ty not in (T_INT, T_REAL):
                raise ValueError(f"line {self.line()}: unary '-' requires numeric operand")
            dst = self.new_temp()
            self.code.append(TacInst("neg", (dst, lhs.op)))
            lhs = TypedExpr(Operand(False, dst), lhs.ty)
        add_map = {"PLUS": "+", "MINUS": "-", "OR": "or"}
        while self.peek().kind in add_map:
            ln = self.line()
            op = add_map[self.advance().kind]
            rhs = self.parse_term()
            lhs = self.emit_binary(lhs, rhs, op, ln)
        return lhs

    def parse_term(self) -> TypedExpr:
        lhs = self.parse_factor()
        mul_map = {"STAR": "*", "SLASH": "/", "DIV": "div", "MOD": "mod", "AND": "and"}
        while self.peek().kind in mul_map:
            ln = self.line()
            op = mul_map[self.advance().kind]
            rhs = self.parse_factor()
            lhs = self.emit_binary(lhs, rhs, op, ln)
        return lhs

    def emit_binary(self, left: TypedExpr, right: TypedExpr, op: str, line: int) -> TypedExpr:
        if op in ("and", "or"):
            if left.ty != T_BOOL or right.ty != T_BOOL:
                raise ValueError(f"line {line}: '{op}' requires boolean operands")
            dst = self.new_temp()
            self.code.append(TacInst("bin", (dst, op, left.op, right.op)))
            return TypedExpr(Operand(False, dst), T_BOOL)
        if op in ("div", "mod"):
            if left.ty != T_INT or right.ty != T_INT:
                raise ValueError(f"line {line}: '{op}' requires integer operands")
            dst = self.new_temp()
            self.code.append(TacInst("bin", (dst, op, left.op, right.op)))
            return TypedExpr(Operand(False, dst), T_INT)
        if op == "/":
            if left.ty not in (T_INT, T_REAL) or right.ty not in (T_INT, T_REAL):
                raise ValueError(f"line {line}: '/' requires numeric operands")
            l, r = self.widen_to_real(left), self.widen_to_real(right)
            dst = self.new_temp()
            self.code.append(TacInst("bin", (dst, op, l.op, r.op)))
            return TypedExpr(Operand(False, dst), T_REAL)
        if left.ty == T_INT and right.ty == T_INT:
            l, r, res_ty = left, right, T_INT
        elif left.ty in (T_INT, T_REAL) and right.ty in (T_INT, T_REAL):
            l, r, res_ty = self.widen_to_real(left), self.widen_to_real(right), T_REAL
        else:
            raise ValueError(f"line {line}: '{op}' requires numeric operands")
        dst = self.new_temp()
        self.code.append(TacInst("bin", (dst, op, l.op, r.op)))
        return TypedExpr(Operand(False, dst), res_ty)

    def parse_factor(self) -> TypedExpr:
        tok = self.advance()
        ln = tok.line
        if tok.kind == "INT_LIT":
            return TypedExpr(Operand(True, tok.value), T_INT)
        if tok.kind == "REAL_LIT":
            return TypedExpr(Operand(True, tok.value), T_REAL)
        if tok.kind == "TRUE":
            return TypedExpr(Operand(True, True), T_BOOL)
        if tok.kind == "FALSE":
            return TypedExpr(Operand(True, False), T_BOOL)
        if tok.kind == "LPAREN":
            inner = self.parse_expr()
            self.expect("RPAREN")
            return inner
        if tok.kind == "NOT":
            inner = self.parse_factor()
            if inner.ty != T_BOOL:
                raise ValueError(f"line {ln}: 'not' requires boolean operand")
            dst = self.new_temp()
            self.code.append(TacInst("not", (dst, inner.op)))
            return TypedExpr(Operand(False, dst), T_BOOL)
        if tok.kind == "IDENT":
            name = str(tok.value)
            sym = self.sym.lookup(name)
            if sym is None:
                raise ValueError(f"line {ln}: undeclared identifier '{name}'")
            if sym.is_subprogram:
                dst, ret_ty = self.parse_call(name, ln, need_value=True)
                assert dst is not None
                return TypedExpr(Operand(False, dst), ret_ty)
            if self.eat("LBRACKET"):
                idx = self.parse_expr()
                if idx.ty != T_INT:
                    raise ValueError(f"line {ln}: array index must be integer, got {idx.ty}")
                self.expect("RBRACKET")
                if sym.ty.kind != "array" or sym.ty.elem is None:
                    raise ValueError(f"line {ln}: cannot index non-array type {sym.ty}")
                zero_idx = self.emit_zero_based_index(idx.op, sym.ty.low)
                dst = self.new_temp()
                self.code.append(TacInst("aload", (dst, sym.slot_name, zero_idx)))
                return TypedExpr(Operand(False, dst), sym.ty.elem)
            return TypedExpr(Operand(False, sym.slot_name), sym.ty)
        raise ValueError(f"line {ln}: unexpected factor {tok.kind}")


def compile_pascal(src: str) -> CompiledUnit:
    return Parser(lex(src)).parse_program()


# ---------------------------------------------------------------------------
# 5. Minimal TAC Interpreter
# ---------------------------------------------------------------------------

def exec_tac(unit: CompiledUnit) -> list[object]:
    labels = {inst.args[0]: idx for idx, inst in enumerate(unit.code) if inst.op == "label"}
    global_names = {n for n, _, _ in unit.global_offsets}
    globals_mem: dict[str, object] = {}
    arrays_mem: dict[str, dict[int, object]] = {}
    call_frames: list[tuple[int, str | None, dict[str, object]]] = []
    param_stack: list[object] = []
    writes: list[object] = []

    def read_var(name: str) -> object:
        if call_frames and name in call_frames[-1][2]:
            return call_frames[-1][2][name]
        return globals_mem.get(name, 0)

    def write_var(name: str, val: object) -> None:
        if call_frames and name not in global_names:
            call_frames[-1][2][name] = val
        else:
            globals_mem[name] = val

    def eval_op(op: Operand) -> object:
        return op.val if op.is_const else read_var(str(op.val))

    pc = 0
    while pc < len(unit.code):
        inst = unit.code[pc]
        if inst.op == "label":
            pc += 1
        elif inst.op == "assign":
            write_var(str(inst.args[0]), eval_op(inst.args[1]))  # type: ignore[arg-type]
            pc += 1
        elif inst.op == "bin":
            dst, bop, l_op, r_op = inst.args
            lv, rv = eval_op(l_op), eval_op(r_op)  # type: ignore[arg-type]
            if bop == "+":
                out = lv + rv  # type: ignore[operator]
            elif bop == "-":
                out = lv - rv  # type: ignore[operator]
            elif bop == "*":
                out = lv * rv  # type: ignore[operator]
            elif bop == "/":
                out = float(lv) / float(rv)  # type: ignore[arg-type]
            elif bop == "div":
                out = int(lv) // int(rv)  # type: ignore[arg-type]
            elif bop == "mod":
                out = int(lv) % int(rv)  # type: ignore[arg-type]
            elif bop == "and":
                out = bool(lv) and bool(rv)
            elif bop == "or":
                out = bool(lv) or bool(rv)
            elif bop == "<":
                out = lv < rv  # type: ignore[operator]
            elif bop == "<=":
                out = lv <= rv  # type: ignore[operator]
            elif bop == ">":
                out = lv > rv  # type: ignore[operator]
            elif bop == ">=":
                out = lv >= rv  # type: ignore[operator]
            elif bop == "=":
                out = lv == rv
            else:
                out = lv != rv
            write_var(str(dst), out)
            pc += 1
        elif inst.op == "neg":
            write_var(str(inst.args[0]), -eval_op(inst.args[1]))  # type: ignore[arg-type,operator]
            pc += 1
        elif inst.op == "not":
            write_var(str(inst.args[0]), not bool(eval_op(inst.args[1])))  # type: ignore[arg-type]
            pc += 1
        elif inst.op == "int_to_real":
            write_var(str(inst.args[0]), float(eval_op(inst.args[1])))  # type: ignore[arg-type]
            pc += 1
        elif inst.op == "aload":
            dst, base, idx_op = inst.args
            idx = int(eval_op(idx_op))  # type: ignore[arg-type]
            write_var(str(dst), arrays_mem.get(str(base), {}).get(idx, 0))
            pc += 1
        elif inst.op == "astore":
            base, idx_op, src_op = inst.args
            idx = int(eval_op(idx_op))  # type: ignore[arg-type]
            arrays_mem.setdefault(str(base), {})[idx] = eval_op(src_op)  # type: ignore[arg-type]
            pc += 1
        elif inst.op == "param":
            param_stack.append(eval_op(inst.args[0]))  # type: ignore[arg-type]
            pc += 1
        elif inst.op == "call":
            dst, callee, param_slots = inst.args
            n_p = len(param_slots)  # type: ignore[arg-type]
            passed = param_stack[len(param_stack) - n_p :]
            del param_stack[len(param_stack) - n_p :]
            locals_map = {slot: val for slot, val in zip(param_slots, passed)}  # type: ignore[arg-type]
            call_frames.append((pc + 1, dst, locals_map))  # type: ignore[arg-type]
            pc = int(labels[f"fn_{callee}"])
        elif inst.op == "return":
            ret_val = eval_op(inst.args[0]) if inst.args[0] is not None else None  # type: ignore[arg-type]
            ret_pc, dst_slot, _ = call_frames.pop()
            if dst_slot is not None and ret_val is not None:
                write_var(dst_slot, ret_val)
            pc = ret_pc
        elif inst.op == "write":
            writes.append(eval_op(inst.args[0]))  # type: ignore[arg-type]
            pc += 1
        elif inst.op == "goto":
            pc = int(labels[str(inst.args[0])])
        elif inst.op == "ifFalse":
            cond_val = bool(eval_op(inst.args[0]))  # type: ignore[arg-type]
            pc = int(labels[str(inst.args[1])]) if not cond_val else pc + 1
        else:
            break
    return writes


SAMPLE_PASCAL = """
program WeightedScores;
var
    scores: array[1..4] of integer;
    i: integer;
    avg: real;

function SumWeighted(limit: integer; bonus: real): real;
var
    k, total: integer;
begin
    total := 0;
    k := 1;
    while k <= limit do
    begin
        if (k mod 2) = 0 then
            total := total + scores[k] * 2
        else
            total := total + scores[k];
        k := k + 1
    end;
    SumWeighted := total + bonus
end;

begin
    i := 1;
    while i <= 4 do
    begin
        scores[i] := i * 10;
        i := i + 1
    end;
    avg := SumWeighted(4, 2.5) / 2;
    write(avg)
end.
"""


def main() -> None:
    unit = compile_pascal(SAMPLE_PASCAL)
    print(f"Program: {unit.program_name}")
    print("Global symbol layout:")
    for name, ty, offset in unit.global_offsets:
        print(f"  {name:<8} : {str(ty):<24} offset={offset}")
    print(f"\nEmitted TAC ({len(unit.code)} instructions):")
    for inst in unit.code:
        print(inst)
    print(f"\nExecution write output: {exec_tac(unit)}")


class PascalFrontendTests(unittest.TestCase):
    def test_compiles_nested_scopes_array_bounds_and_real_coercion(self) -> None:
        unit = compile_pascal(SAMPLE_PASCAL)
        self.assertTrue(any(inst.op == "int_to_real" for inst in unit.code))
        out = exec_tac(unit)
        self.assertEqual(out, [81.25])

    def test_rejects_semantic_and_syntax_errors_cleanly(self) -> None:
        with self.assertRaisesRegex(ValueError, "boolean"):
            compile_pascal("program P; var x: integer; begin x := 1; if x then write(x) end.")
        with self.assertRaisesRegex(ValueError, "low > high"):
            compile_pascal("program P; var a: array[5..2] of integer; begin end.")
        with self.assertRaisesRegex(ValueError, "duplicate"):
            compile_pascal("program P; var a, a: integer; begin end.")
        with self.assertRaisesRegex(ValueError, "type mismatch"):
            compile_pascal("program P; var n: integer; begin n := 3.5 end.")


if __name__ == "__main__":
    main()
