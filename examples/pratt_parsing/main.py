"""Modern Extension: Pratt (Top-Down Operator Precedence) & Precedence Climbing.

Explicitly labeled as a modern/practical front-end parsing technique complementing
Dragon Book §4.6 and §4.8:
1. Vaughan Pratt's Top-Down Operator Precedence (`nud` + `led` + `(lbp, rbp)` pairs),
2. Handling prefix `-`, left-assoc binary `+`/`-`/`*`/`/`, right-assoc `^`/`=`,
   postfix factorial `!`, and mixfix ternary `cond ? then : else`,
3. Precedence climbing side-by-side producing identical parenthesized ASTs,
4. Exact evaluator verifying associativity and precedence semantics.
"""

from __future__ import annotations

from dataclasses import dataclass
import math
import unittest


@dataclass(frozen=True)
class Token:
    kind: str
    lexeme: str
    pos: int


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
class PrefixExpr:
    op: str
    inner: Expr

    def __str__(self) -> str:
        return f"({self.op}{self.inner})"


@dataclass(frozen=True)
class PostfixExpr:
    op: str
    inner: Expr

    def __str__(self) -> str:
        return f"({self.inner}{self.op})"


@dataclass(frozen=True)
class BinaryExpr:
    op: str
    left: Expr
    right: Expr

    def __str__(self) -> str:
        return f"({self.left} {self.op} {self.right})"


@dataclass(frozen=True)
class TernaryExpr:
    cond: Expr
    then_e: Expr
    else_e: Expr

    def __str__(self) -> str:
        return f"({self.cond} ? {self.then_e} : {self.else_e})"


Expr = IntExpr | VarExpr | PrefixExpr | PostfixExpr | BinaryExpr | TernaryExpr


def tokenize(src: str) -> list[Token]:
    n = len(src)
    i = 0
    out: list[Token] = []
    single = {
        "+": "PLUS", "-": "MINUS", "*": "STAR", "/": "SLASH", "^": "CARET",
        "!": "BANG", "=": "EQ", "?": "QUESTION", ":": "COLON", "(": "LPAREN", ")": "RPAREN",
    }
    while i < n:
        if src[i].isspace():
            i += 1
            continue
        start = i
        if src[i].isdigit():
            i += 1
            while i < n and src[i].isdigit():
                i += 1
            out.append(Token("INT", src[start:i], start))
            continue
        if src[i].isalpha() or src[i] == "_":
            i += 1
            while i < n and (src[i].isalnum() or src[i] == "_"):
                i += 1
            out.append(Token("IDENT", src[start:i], start))
            continue
        if src[i] in single:
            ch = src[i]
            i += 1
            out.append(Token(single[ch], ch, start))
            continue
        raise ValueError(f"unexpected character '{src[i]}' at {start}")
    out.append(Token("EOF", "$", n))
    return out


class PrattParser:
    LBP = {
        "EQ": 2,
        "QUESTION": 4,
        "PLUS": 10,
        "MINUS": 10,
        "STAR": 20,
        "SLASH": 20,
        "CARET": 30,
        "BANG": 40,
    }

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

    def expect(self, kind: str, label: str) -> Token:
        tok = self.peek()
        if tok.kind == kind:
            return self.advance()
        raise ValueError(f"expected {label}, found '{tok.lexeme}' at {tok.pos}")

    def nud(self, tok: Token) -> Expr:
        if tok.kind == "INT":
            return IntExpr(int(tok.lexeme))
        if tok.kind == "IDENT":
            return VarExpr(tok.lexeme)
        if tok.kind == "MINUS":
            return PrefixExpr("-", self.parse_bp(25))
        if tok.kind == "LPAREN":
            inner = self.parse_bp(0)
            self.expect("RPAREN", "')'")
            return inner
        raise ValueError(f"expected expression, found '{tok.lexeme}' at {tok.pos}")

    def led(self, left: Expr, tok: Token) -> Expr:
        lbp = self.LBP[tok.kind]
        if tok.kind in ("PLUS", "MINUS", "STAR", "SLASH"):
            return BinaryExpr(tok.lexeme, left, self.parse_bp(lbp + 1))
        if tok.kind in ("EQ", "CARET"):
            return BinaryExpr(tok.lexeme, left, self.parse_bp(lbp - 1))
        if tok.kind == "BANG":
            return PostfixExpr("!", left)
        if tok.kind == "QUESTION":
            then_b = self.parse_bp(0)
            self.expect("COLON", "':'")
            else_b = self.parse_bp(lbp - 1)
            return TernaryExpr(left, then_b, else_b)
        raise ValueError(f"unexpected operator '{tok.lexeme}'")

    def parse_bp(self, min_bp: int) -> Expr:
        left = self.nud(self.advance())
        while min_bp < self.LBP.get(self.peek().kind, 0):
            left = self.led(left, self.advance())
        return left


def parse_pratt(src: str) -> Expr:
    parser = PrattParser(tokenize(src))
    expr = parser.parse_bp(0)
    if parser.peek().kind != "EOF":
        raise ValueError(f"unexpected trailing token '{parser.peek().lexeme}'")
    return expr


class ClimbParser:
    INFIX = {
        "EQ": (2, True),
        "QUESTION": (4, True),
        "PLUS": (10, False),
        "MINUS": (10, False),
        "STAR": (20, False),
        "SLASH": (20, False),
        "CARET": (30, True),
    }

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

    def _atom_with_postfix(self) -> Expr:
        tok = self.advance()
        if tok.kind == "INT":
            expr: Expr = IntExpr(int(tok.lexeme))
        elif tok.kind == "IDENT":
            expr = VarExpr(tok.lexeme)
        elif tok.kind == "MINUS":
            return PrefixExpr("-", self.parse_expr(25))
        elif tok.kind == "LPAREN":
            expr = self.parse_expr(0)
            if self.peek().kind != "RPAREN":
                raise ValueError("expected ')'")
            self.advance()
        else:
            raise ValueError(f"expected atom, found '{tok.lexeme}'")
        while self.peek().kind == "BANG":
            self.advance()
            expr = PostfixExpr("!", expr)
        return expr

    def parse_expr(self, min_prec: int) -> Expr:
        lhs = self._atom_with_postfix()
        while self.peek().kind in self.INFIX:
            prec, right_assoc = self.INFIX[self.peek().kind]
            if prec < min_prec:
                break
            op = self.advance()
            next_min = prec if right_assoc else prec + 1
            if op.kind == "QUESTION":
                then_e = self.parse_expr(0)
                if self.peek().kind != "COLON":
                    raise ValueError("expected ':'")
                self.advance()
                else_e = self.parse_expr(next_min)
                lhs = TernaryExpr(lhs, then_e, else_e)
            else:
                lhs = BinaryExpr(op.lexeme, lhs, self.parse_expr(next_min))
        return lhs


def parse_climb(src: str) -> Expr:
    parser = ClimbParser(tokenize(src))
    expr = parser.parse_expr(0)
    if parser.peek().kind != "EOF":
        raise ValueError(f"unexpected trailing token '{parser.peek().lexeme}'")
    return expr


def eval_expr(expr: Expr) -> int:
    if isinstance(expr, IntExpr):
        return expr.val
    if isinstance(expr, VarExpr):
        raise ValueError(f"free variable '{expr.name}'")
    if isinstance(expr, PrefixExpr):
        return -eval_expr(expr.inner)
    if isinstance(expr, PostfixExpr):
        n = eval_expr(expr.inner)
        if not (0 <= n <= 20):
            raise ValueError("factorial domain is 0..20")
        return math.factorial(n)
    if isinstance(expr, BinaryExpr):
        a = eval_expr(expr.left)
        b = eval_expr(expr.right)
        if expr.op == "+":
            return a + b
        if expr.op == "-":
            return a - b
        if expr.op == "*":
            return a * b
        if expr.op == "/":
            return int(a / b)
        if expr.op == "^":
            return a**b
        raise ValueError(f"cannot eval op '{expr.op}'")
    return eval_expr(expr.then_e) if eval_expr(expr.cond) != 0 else eval_expr(expr.else_e)


def main() -> None:
    print("=== Advanced Parsing (Modern): Pratt Parsing & Precedence Climbing ===")
    samples = [
        "1 + 2 * 3 ^ 2 ^ 1",
        "-2 ^ 2 + 4! * 2",
        "1 ? 2 + 3 : 4 ? 5 : 6",
        "x = y = 1 + 2 * 3",
    ]
    for src in samples:
        pratt = parse_pratt(src)
        climb = parse_climb(src)
        assert pratt == climb
        try:
            val: int | None = eval_expr(pratt)
        except ValueError:
            val = None
        print(f"  {src:<24} -> {pratt}  val={val}")


class PrattParsingTests(unittest.TestCase):
    def test_pratt_and_precedence_climbing_agree(self) -> None:
        cases = [
            ("1 - 2 - 3", "((1 - 2) - 3)", -4),
            ("2 ^ 3 ^ 2", "(2 ^ (3 ^ 2))", 512),
            ("a = b = c + 1", "(a = (b = (c + 1)))", None),
            ("0 ? 10 : 1 ? 20 : 30", "(0 ? 10 : (1 ? 20 : 30))", 20),
        ]
        for src, expected, val in cases:
            p = parse_pratt(src)
            c = parse_climb(src)
            self.assertEqual(p, c)
            self.assertEqual(str(p), expected)
            if val is not None:
                self.assertEqual(eval_expr(p), val)

    def test_prefix_vs_pow_and_postfix_binding_powers(self) -> None:
        ast = parse_pratt("-2 ^ 2 + 3! * 4")
        self.assertEqual(ast, parse_climb("-2 ^ 2 + 3! * 4"))
        self.assertEqual(str(ast), "((-(2 ^ 2)) + ((3!) * 4))")
        self.assertEqual(eval_expr(ast), 20)

    def test_rejects_mismatched_parens_or_missing_colon(self) -> None:
        with self.assertRaises(ValueError):
            parse_pratt("(1 + 2")
        with self.assertRaises(ValueError):
            parse_pratt("1 ? 2")


if __name__ == "__main__":
    main()
