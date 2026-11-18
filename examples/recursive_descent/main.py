"""Chapter 4 (§4.4.1): Recursive-Descent Parsing.

Demonstrates handwritten top-down predictive parsing in direct 1-to-1
correspondence with an EBNF grammar (rewritten to eliminate left recursion):
- Mapping each nonterminal (`Program`, `Stmt`, `Expr`, `Term`, `Power`, `Factor`)
  to a recursive parsing function,
- Using `while` loops for left-associative repetition (`+`, `-`, `*`, `/`)
  without stack overflow, and right recursion for right-associative `^`,
- Comparing 1-token (`LL(1)`) lookahead vs. 2-token (`LL(2)`) lookahead to
  disambiguate `ident := expr;` (assignment) from `ident(args);` (call),
- Dangling-else resolution inside nested `if ... then ... else ...` blocks,
- Crisp error diagnostics, `;`/`}` panic-mode recovery across statements,
  and explicit recursion-depth limits against stack overflow.
"""

from __future__ import annotations

from dataclasses import dataclass
import unittest


# ---------------------------------------------------------------------------
# 1. Lexer (Characters -> Typed Tokens with Byte Offsets)
# ---------------------------------------------------------------------------


@dataclass(frozen=True)
class Token:
    kind: str  # LET | IF | THEN | ELSE | IDENT | INT | ASSIGN | PLUS | ...
    lexeme: str
    pos: int
    int_val: int | None = None

    def label(self) -> str:
        if self.kind == "IDENT":
            return f"ident '{self.lexeme}'"
        if self.kind == "INT":
            return f"int {self.int_val}"
        if self.kind == "EOF":
            return "end-of-input"
        return f"'{self.lexeme}'"


@dataclass(frozen=True)
class Diagnostic:
    pos: int
    msg: str

    def __str__(self) -> str:
        return f"offset {self.pos}: {self.msg}"


class ParseError(Exception):
    def __init__(self, diag: Diagnostic) -> None:
        super().__init__(str(diag))
        self.diag = diag


def lex(src: str) -> list[Token]:
    keywords = {"let": "LET", "if": "IF", "then": "THEN", "else": "ELSE"}
    single = {
        "+": "PLUS",
        "-": "MINUS",
        "*": "STAR",
        "/": "SLASH",
        "^": "CARET",
        "(": "LPAREN",
        ")": "RPAREN",
        "{": "LBRACE",
        "}": "RBRACE",
        ",": "COMMA",
        ";": "SEMI",
    }
    i = 0
    n = len(src)
    out: list[Token] = []

    while i < n:
        ch = src[i]
        if ch.isspace():
            i += 1
            continue
        start = i
        if ch.isalpha() or ch == "_":
            i += 1
            while i < n and (src[i].isalnum() or src[i] == "_"):
                i += 1
            word = src[start:i]
            out.append(Token(kind=keywords.get(word, "IDENT"), lexeme=word, pos=start))
            continue
        if ch.isdigit():
            i += 1
            while i < n and src[i].isdigit():
                i += 1
            text = src[start:i]
            out.append(Token(kind="INT", lexeme=text, pos=start, int_val=int(text)))
            continue
        if src.startswith(":=", i):
            out.append(Token(kind="ASSIGN", lexeme=":=", pos=start))
            i += 2
            continue
        if ch in single:
            out.append(Token(kind=single[ch], lexeme=ch, pos=start))
            i += 1
            continue
        raise ParseError(Diagnostic(pos=start, msg=f"unexpected character '{ch}'"))

    out.append(Token(kind="EOF", lexeme="", pos=n))
    return out


# ---------------------------------------------------------------------------
# 2. AST Definitions
# ---------------------------------------------------------------------------


@dataclass(frozen=True)
class IntExpr:
    val: int

    def __str__(self) -> str:
        return str(self.val)


@dataclass(frozen=True)
class VarExpr:
    name: str

    def __str__(self) -> str:
        return self.name


@dataclass(frozen=True)
class NegExpr:
    inner: Expr

    def __str__(self) -> str:
        return f"(-{self.inner})"


@dataclass(frozen=True)
class BinExpr:
    op: str
    left: Expr
    right: Expr

    def __str__(self) -> str:
        return f"({self.left} {self.op} {self.right})"


Expr = IntExpr | VarExpr | NegExpr | BinExpr


@dataclass(frozen=True)
class LetStmt:
    name: str
    expr: Expr


@dataclass(frozen=True)
class AssignStmt:
    name: str
    expr: Expr


@dataclass(frozen=True)
class CallStmt:
    callee: str
    args: list[Expr]


@dataclass(frozen=True)
class IfStmt:
    cond: Expr
    then_br: Stmt
    else_br: Stmt | None


@dataclass(frozen=True)
class BlockStmt:
    body: list[Stmt]


Stmt = LetStmt | AssignStmt | CallStmt | IfStmt | BlockStmt


# ---------------------------------------------------------------------------
# 3. EBNF Grammar -> Handwritten Recursive-Descent Parser (§4.4.1)
# ---------------------------------------------------------------------------
#
# Rewritten (left-recursion-free) EBNF grammar implemented below:
#
#   Program -> Stmt* Eof
#   Stmt    -> 'let' Ident ':=' Expr ';'
#           |  Ident ':=' Expr ';'                  (LL(2): Ident followed by ':=')
#           |  Ident '(' [Expr (',' Expr)*] ')' ';' (LL(2): Ident followed by '(')
#           |  'if' Expr 'then' Stmt ['else' Stmt]  (dangling-else eager match)
#           |  '{' Stmt* '}'
#   Expr    -> Term   { ('+' | '-') Term }          (while loop -> left-assoc)
#   Term    -> Unary  { ('*' | '/') Unary }         (while loop -> left-assoc)
#   Unary   -> '-' Unary | Power                    (prefix negation)
#   Power   -> Atom   [ '^' Unary ]                 (right recursion -> right-assoc)
#   Atom    -> Int | Ident | '(' Expr ')'


class Parser:
    def __init__(self, tokens: list[Token], max_depth: int) -> None:
        self.tokens = tokens
        self.pos = 0
        self.depth = 0
        self.max_depth = max_depth
        self.lookahead_trace: list[str] = []
        self.errors: list[Diagnostic] = []

    def peek(self, k: int = 0) -> Token:
        """Look `k` tokens ahead (k=0 is LL(1); k=1 gives LL(2))."""
        idx = min(self.pos + k, len(self.tokens) - 1)
        return self.tokens[idx]

    def advance(self) -> Token:
        tok = self.tokens[self.pos]
        if self.pos + 1 < len(self.tokens):
            self.pos += 1
        return tok

    def expect(self, kind: str, label: str) -> Token:
        tok = self.peek(0)
        if tok.kind == kind:
            return self.advance()
        raise ParseError(
            Diagnostic(pos=tok.pos, msg=f"expected {label}, found {tok.label()}")
        )

    def expect_ident(self) -> str:
        tok = self.peek(0)
        if tok.kind == "IDENT":
            self.advance()
            return tok.lexeme
        raise ParseError(
            Diagnostic(pos=tok.pos, msg=f"expected identifier, found {tok.label()}")
        )

    def enter(self, rule: str) -> None:
        self.depth += 1
        if self.depth > self.max_depth:
            tok = self.peek(0)
            raise ParseError(
                Diagnostic(
                    pos=tok.pos,
                    msg=f"recursion depth limit ({self.max_depth}) exceeded while parsing {rule}",
                )
            )

    def leave(self) -> None:
        self.depth -= 1

    def synchronize_to_stmt(self) -> None:
        while self.peek(0).kind not in ("SEMI", "RBRACE", "EOF"):
            self.advance()
        if self.peek(0).kind == "SEMI":
            self.advance()

    def parse_program(self) -> list[Stmt]:
        stmts: list[Stmt] = []
        while self.peek(0).kind != "EOF":
            try:
                stmts.append(self.parse_stmt())
            except ParseError as err:
                self.errors.append(err.diag)
                self.synchronize_to_stmt()
        return stmts

    def parse_stmt(self) -> Stmt:
        self.enter("Stmt")
        try:
            tok0 = self.peek(0)
            if tok0.kind == "LET":
                self.lookahead_trace.append(
                    f"LL(1) peek(0)='let' -> Let stmt @{tok0.pos}"
                )
                self.advance()
                name = self.expect_ident()
                self.expect("ASSIGN", "':='")
                expr = self.parse_expr()
                self.expect("SEMI", "';'")
                return LetStmt(name=name, expr=expr)

            if tok0.kind == "IDENT":
                name = tok0.lexeme
                tok1 = self.peek(1)
                if tok1.kind == "ASSIGN":
                    self.lookahead_trace.append(
                        f"LL(2) peek(0)=ident '{name}', peek(1)=':=' -> Assign stmt @{tok0.pos}"
                    )
                    self.advance()
                    self.advance()
                    expr = self.parse_expr()
                    self.expect("SEMI", "';'")
                    return AssignStmt(name=name, expr=expr)
                if tok1.kind == "LPAREN":
                    self.lookahead_trace.append(
                        f"LL(2) peek(0)=ident '{name}', peek(1)='(' -> Call stmt @{tok0.pos}"
                    )
                    self.advance()
                    self.advance()
                    args: list[Expr] = []
                    if self.peek(0).kind != "RPAREN":
                        while True:
                            args.append(self.parse_expr())
                            if self.peek(0).kind == "COMMA":
                                self.advance()
                            else:
                                break
                    self.expect("RPAREN", "')'")
                    self.expect("SEMI", "';'")
                    return CallStmt(callee=name, args=args)
                raise ParseError(
                    Diagnostic(
                        pos=tok1.pos,
                        msg=f"expected ':=' or '(' after identifier '{name}', found {tok1.label()}",
                    )
                )

            if tok0.kind == "IF":
                self.lookahead_trace.append(
                    f"LL(1) peek(0)='if' -> If stmt @{tok0.pos}"
                )
                self.advance()
                cond = self.parse_expr()
                self.expect("THEN", "'then'")
                then_br = self.parse_stmt()
                else_br: Stmt | None = None
                if self.peek(0).kind == "ELSE":
                    self.advance()
                    else_br = self.parse_stmt()
                return IfStmt(cond=cond, then_br=then_br, else_br=else_br)

            if tok0.kind == "LBRACE":
                self.lookahead_trace.append(
                    f"LL(1) peek(0)='{{' -> Block stmt @{tok0.pos}"
                )
                self.advance()
                body: list[Stmt] = []
                while self.peek(0).kind not in ("RBRACE", "EOF"):
                    try:
                        body.append(self.parse_stmt())
                    except ParseError as err:
                        self.errors.append(err.diag)
                        self.synchronize_to_stmt()
                self.expect("RBRACE", "'}'")
                return BlockStmt(body=body)

            raise ParseError(
                Diagnostic(
                    pos=tok0.pos,
                    msg=f"unexpected token {tok0.label()} starting statement",
                )
            )
        finally:
            self.leave()

    def parse_expr(self) -> Expr:
        self.enter("Expr")
        try:
            acc = self.parse_term()
            while self.peek(0).kind in ("PLUS", "MINUS"):
                op = "+" if self.advance().kind == "PLUS" else "-"
                rhs = self.parse_term()
                acc = BinExpr(op=op, left=acc, right=rhs)
            return acc
        finally:
            self.leave()

    def parse_term(self) -> Expr:
        self.enter("Term")
        try:
            acc = self.parse_unary()
            while self.peek(0).kind in ("STAR", "SLASH"):
                op = "*" if self.advance().kind == "STAR" else "/"
                rhs = self.parse_unary()
                acc = BinExpr(op=op, left=acc, right=rhs)
            return acc
        finally:
            self.leave()

    def parse_unary(self) -> Expr:
        self.enter("Unary")
        try:
            if self.peek(0).kind == "MINUS":
                self.advance()
                return NegExpr(inner=self.parse_unary())
            return self.parse_power()
        finally:
            self.leave()

    def parse_power(self) -> Expr:
        self.enter("Power")
        try:
            base = self.parse_atom()
            if self.peek(0).kind == "CARET":
                self.advance()
                exp = self.parse_unary()
                return BinExpr(op="^", left=base, right=exp)
            return base
        finally:
            self.leave()

    def parse_atom(self) -> Expr:
        self.enter("Atom")
        try:
            tok = self.peek(0)
            if tok.kind == "INT":
                self.advance()
                assert tok.int_val is not None
                return IntExpr(val=tok.int_val)
            if tok.kind == "IDENT":
                self.advance()
                return VarExpr(name=tok.lexeme)
            if tok.kind == "LPAREN":
                self.advance()
                inner = self.parse_expr()
                self.expect("RPAREN", "')'")
                return inner
            raise ParseError(
                Diagnostic(
                    pos=tok.pos, msg=f"expected expression operand, found {tok.label()}"
                )
            )
        finally:
            self.leave()


def parse_source(
    src: str, max_depth: int = 64
) -> tuple[list[Stmt], list[str], list[Diagnostic]]:
    try:
        tokens = lex(src)
    except ParseError as err:
        return [], [], [err.diag]
    parser = Parser(tokens, max_depth=max_depth)
    stmts = parser.parse_program()
    return stmts, parser.lookahead_trace, parser.errors


# ---------------------------------------------------------------------------
# 4. AST Evaluator
# ---------------------------------------------------------------------------


def eval_expr(expr: Expr, env: dict[str, int]) -> int:
    if isinstance(expr, IntExpr):
        return expr.val
    if isinstance(expr, VarExpr):
        if expr.name not in env:
            raise KeyError(f"undefined variable '{expr.name}'")
        return env[expr.name]
    if isinstance(expr, NegExpr):
        return -eval_expr(expr.inner, env)
    l = eval_expr(expr.left, env)
    r = eval_expr(expr.right, env)
    if expr.op == "+":
        return l + r
    if expr.op == "-":
        return l - r
    if expr.op == "*":
        return l * r
    if expr.op == "/":
        if r == 0:
            raise ZeroDivisionError("division by zero")
        return int(l / r)
    if expr.op == "^":
        if r < 0:
            raise ValueError("negative exponent")
        return int(l**r)
    raise ValueError(f"unknown op: {expr.op}")


def exec_stmts(
    stmts: list[Stmt], env: dict[str, int], calls: list[tuple[str, list[int]]]
) -> None:
    for s in stmts:
        if isinstance(s, (LetStmt, AssignStmt)):
            env[s.name] = eval_expr(s.expr, env)
        elif isinstance(s, CallStmt):
            calls.append((s.callee, [eval_expr(a, env) for a in s.args]))
        elif isinstance(s, IfStmt):
            if eval_expr(s.cond, env) != 0:
                exec_stmts([s.then_br], env, calls)
            elif s.else_br is not None:
                exec_stmts([s.else_br], env, calls)
        else:
            exec_stmts(s.body, env, calls)


def main() -> None:
    sample = """\
let a := 10 - 3 - 2;
let p := 2 ^ 3 ^ 2;
a := a + 1 * 4;
if a - 9 then {
    log(a, p);
} else {
    log(0, p);
}
"""
    stmts, trace, errors = parse_source(sample, max_depth=64)
    assert not errors, f"unexpected diagnostics: {errors}"

    env: dict[str, int] = {}
    calls: list[tuple[str, list[int]]] = []
    exec_stmts(stmts, env, calls)

    print("=== Handwritten Recursive-Descent Parsing (§4.4.1) ===")
    print(f"Parsed AST statements ({len(stmts)}):")
    for i, s in enumerate(stmts):
        print(f"  [{i}] {s}")
    print("\nLookahead decisions (LL(1) vs LL(2)):")
    for item in trace:
        print(f"  {item}")
    print("\nExecution verification:")
    print(f"  env={env}, calls={calls}")


class RecursiveDescentTests(unittest.TestCase):
    def test_precedence_and_associativity(self) -> None:
        src = """\
let left_sub := 10 - 3 - 2;
let right_pow := 2 ^ 3 ^ 2;
let mixed := 1 + 2 * 3 ^ 2 - 4 / 2;
"""
        stmts, _, errors = parse_source(src, max_depth=64)
        self.assertEqual(errors, [])
        assert isinstance(stmts[0], LetStmt)
        assert isinstance(stmts[1], LetStmt)
        self.assertEqual(str(stmts[0].expr), "((10 - 3) - 2)")
        self.assertEqual(str(stmts[1].expr), "(2 ^ (3 ^ 2))")

        env: dict[str, int] = {}
        calls: list[tuple[str, list[int]]] = []
        exec_stmts(stmts, env, calls)
        self.assertEqual(env["left_sub"], 5)
        self.assertEqual(env["right_pow"], 512)
        self.assertEqual(env["mixed"], 17)

    def test_ll2_lookahead_and_dangling_else_nesting(self) -> None:
        src = """\
let x := 1;
x := 5;
if 0 then if 1 then emit(1); else emit(2);
emit(x);
"""
        stmts, trace, errors = parse_source(src, max_depth=64)
        self.assertEqual(errors, [])
        self.assertTrue(any("LL(2)" in t and "':='" in t for t in trace))
        self.assertTrue(any("LL(2)" in t and "'('" in t for t in trace))

        env: dict[str, int] = {}
        calls: list[tuple[str, list[int]]] = []
        exec_stmts(stmts, env, calls)
        self.assertEqual(calls, [("emit", [5])])

    def test_syntax_error_recovery_and_recursion_depth_limit(self) -> None:
        bad_src = """\
let ok1 := 1 + 2;
let broken := 3 + ;
let ok2 := 4 * 5;
"""
        stmts, _, errors = parse_source(bad_src, max_depth=64)
        self.assertEqual(len(errors), 1)
        self.assertIn("expected expression operand", errors[0].msg)
        self.assertEqual(len(stmts), 2)

        deep = f"let d := {'(' * 25}1{')' * 25};"
        _, _, deep_errs = parse_source(deep, max_depth=12)
        self.assertEqual(len(deep_errs), 1)
        self.assertIn("recursion depth limit", deep_errs[0].msg)


if __name__ == "__main__":
    main()
