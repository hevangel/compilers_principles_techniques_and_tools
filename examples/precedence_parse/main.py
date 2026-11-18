"""Resolve shift/reduce ambiguities via precedence, associativity, and shift-beats-reduce."""

from dataclasses import dataclass
from enum import Enum
import unittest


class Op(Enum):
    """Precedence-table keys (separating binary `-` from unary `-`)."""

    LT = "Lt"
    ADD = "Add"
    SUB = "Sub"
    MUL = "Mul"
    POW = "Pow"
    UNARY_NEG = "UnaryNeg"


class Assoc(Enum):
    """Associativity tie-breaker when stack-top and lookahead operators share precedence."""

    LEFT = "Left"
    RIGHT = "Right"
    NON_ASSOC = "NonAssoc"


@dataclass(frozen=True)
class IdExpr:
    name: str

    def __str__(self) -> str:
        return self.name


@dataclass(frozen=True)
class UnaryNegExpr:
    inner: "Expr"

    def __str__(self) -> str:
        return f"(-{self.inner})"


@dataclass(frozen=True)
class BinaryExpr:
    op: str
    left: "Expr"
    right: "Expr"

    def __str__(self) -> str:
        return f"({self.left} {self.op} {self.right})"


Expr = IdExpr | UnaryNegExpr | BinaryExpr


@dataclass(frozen=True)
class LeafStmt:
    name: str

    def __str__(self) -> str:
        return self.name


@dataclass(frozen=True)
class IfStmt:
    cond: str
    then_branch: "Stmt"
    else_branch: "Stmt | None"

    def __str__(self) -> str:
        if self.else_branch is None:
            return f"(if {self.cond} then {self.then_branch})"
        return f"(if {self.cond} then {self.then_branch} else {self.else_branch})"


Stmt = LeafStmt | IfStmt


def default_prec_table() -> dict[Op, tuple[int, Assoc]]:
    """Return `(precedence, associativity)` table with `%prec` unary `-` above `*`."""
    return {
        Op.LT: (1, Assoc.NON_ASSOC),
        Op.ADD: (2, Assoc.LEFT),
        Op.SUB: (2, Assoc.LEFT),
        Op.MUL: (3, Assoc.LEFT),
        Op.UNARY_NEG: (4, Assoc.RIGHT),
        Op.POW: (5, Assoc.RIGHT),
    }


BINARY_MAP = {"<": Op.LT, "+": Op.ADD, "-": Op.SUB, "*": Op.MUL, "^": Op.POW}


def should_reduce(top: Op, lookahead: Op, table: dict[Op, tuple[int, Assoc]]) -> bool:
    """Return True to reduce stack-top operator `top`, False to shift `lookahead`."""
    p_top, a_top = table[top]
    p_la, _ = table[lookahead]
    if p_top > p_la:
        return True
    if p_top < p_la:
        return False
    if a_top == Assoc.LEFT:
        return True
    if a_top == Assoc.RIGHT:
        return False
    # Chained non-associative operators (e.g. `a < b < c`) are rejected.
    raise ValueError(f"non-associative operator conflict: {top.value} vs {lookahead.value}")


def tokenize_expr(text: str) -> list[tuple[str, str]]:
    tokens: list[tuple[str, str]] = []
    i = 0
    while i < len(text):
        ch = text[i]
        if ch.isspace():
            i += 1
        elif ch.isalpha():
            start = i
            while i < len(text) and text[i].isalnum():
                i += 1
            tokens.append(("ID", text[start:i]))
        elif ch == "(":
            tokens.append(("LPAREN", ch))
            i += 1
        elif ch == ")":
            tokens.append(("RPAREN", ch))
            i += 1
        elif ch in BINARY_MAP:
            tokens.append(("OP", ch))
            i += 1
        else:
            raise ValueError(f"invalid character {ch!r}")
    return tokens


def parse_expr(tokens: list[tuple[str, str]], table: dict[Op, tuple[int, Assoc]]) -> Expr:
    """Parse an expression with precedence/associativity via value and operator stacks.

    `expect_operand` tracks grammar state so prefix `'-'` receives `Op.UNARY_NEG`.
    """
    values: list[Expr] = []
    ops: list[tuple[str, str, Op | None]] = []
    expect_operand = True

    def reduce_top() -> None:
        kind, ch, _ = ops.pop()
        if kind == "UNARY":
            values.append(UnaryNegExpr(values.pop()))
        elif kind == "BIN":
            right = values.pop()
            left = values.pop()
            values.append(BinaryExpr(ch, left, right))
        else:
            raise ValueError("unclosed '('")

    for kind, lex in tokens:
        if kind == "ID":
            if not expect_operand:
                raise ValueError(f"unexpected identifier {lex!r}")
            values.append(IdExpr(lex))
            expect_operand = False
        elif kind == "LPAREN":
            if not expect_operand:
                raise ValueError("unexpected '(' after operand")
            ops.append(("LPAREN", "(", None))
        elif kind == "RPAREN":
            if expect_operand:
                raise ValueError("unexpected ')'")
            while ops and ops[-1][0] != "LPAREN":
                reduce_top()
            if not ops or ops.pop()[0] != "LPAREN":
                raise ValueError("unmatched ')'")
            expect_operand = False
        elif kind == "OP" and lex == "-" and expect_operand:
            # Unary negation sits at precedence 4 (above `*`, below `^`).
            ops.append(("UNARY", "-", Op.UNARY_NEG))
        elif kind == "OP":
            if expect_operand:
                raise ValueError(f"unexpected binary operator {lex!r}")
            la_op = BINARY_MAP[lex]
            while ops and ops[-1][0] != "LPAREN":
                top_op = ops[-1][2]
                assert top_op is not None
                if should_reduce(top_op, la_op, table):
                    reduce_top()
                else:
                    break
            ops.append(("BIN", lex, la_op))
            expect_operand = True

    if expect_operand:
        raise ValueError("unexpected end of expression")
    while ops:
        reduce_top()
    if len(values) != 1:
        raise ValueError("expression did not reduce to a single root")
    return values[0]


def parse_if_stmts(tokens: list[tuple[str, str]]) -> Stmt:
    """Shift-reduce parser resolving dangling-else via shift-beats-reduce on `ELSE`."""
    stack: list[tuple[str, object]] = []
    pos = 0
    while True:
        lookahead = tokens[pos] if pos < len(tokens) else None
        if stack and stack[-1][0] == "REDUCED" and len(stack) >= 2:
            prev_kind, prev_val = stack[-2]
            if prev_kind == "ELSE":
                _, else_s = stack.pop()
                _, (cond, then_s) = stack.pop()  # type: ignore[misc]
                stack.append(("REDUCED", IfStmt(cond, then_s, else_s)))  # type: ignore[arg-type]
                continue
            if prev_kind == "IF":
                # Shift `ELSE` when present so it attaches to the innermost open `IF`.
                if lookahead is not None and lookahead[0] == "ELSE":
                    _, then_s = stack.pop()
                    _, cond = stack.pop()
                    stack.append(("ELSE", (cond, then_s)))
                    pos += 1
                    continue
                _, then_s = stack.pop()
                _, cond = stack.pop()
                stack.append(("REDUCED", IfStmt(cond, then_s, None)))  # type: ignore[arg-type]
                continue
            raise ValueError("adjacent statements without separator")

        if lookahead is None:
            break
        if lookahead[0] == "IF":
            cond_tok = tokens[pos + 1]
            then_tok = tokens[pos + 2]
            if cond_tok[0] != "ID" or then_tok[0] != "THEN":
                raise ValueError("expected 'if <id> then'")
            stack.append(("IF", cond_tok[1]))
            pos += 3
        elif lookahead[0] == "STMT":
            stack.append(("REDUCED", LeafStmt(lookahead[1])))
            pos += 1
        else:
            raise ValueError(f"unexpected token {lookahead}")

    if len(stack) == 1 and stack[0][0] == "REDUCED":
        stmt = stack[0][1]
        assert isinstance(stmt, (LeafStmt, IfStmt))
        return stmt
    raise ValueError("incomplete statement")


def main() -> None:
    table = default_prec_table()
    for src in ["id + id * id", "id ^ id ^ id", "id - id - id", "-id * id"]:
        ast = parse_expr(tokenize_expr(src), table)
        print(f"{src:16} => {ast}")
    dangling = [
        ("IF", "if"),
        ("ID", "c1"),
        ("THEN", "then"),
        ("IF", "if"),
        ("ID", "c2"),
        ("THEN", "then"),
        ("STMT", "s1"),
        ("ELSE", "else"),
        ("STMT", "s2"),
    ]
    print(f"dangling-else    => {parse_if_stmts(dangling)}")


class PrecedenceParseTests(unittest.TestCase):
    def test_resolves_precedence_associativity_and_unary_prec(self) -> None:
        table = default_prec_table()
        self.assertEqual(str(parse_expr(tokenize_expr("id + id * id"), table)), "(id + (id * id))")
        self.assertEqual(str(parse_expr(tokenize_expr("id ^ id ^ id"), table)), "(id ^ (id ^ id))")
        self.assertEqual(str(parse_expr(tokenize_expr("id - id - id"), table)), "((id - id) - id)")
        self.assertEqual(str(parse_expr(tokenize_expr("-id * id"), table)), "((-id) * id)")
        self.assertEqual(str(parse_expr(tokenize_expr("-id ^ id"), table)), "(-(id ^ id))")
        with self.assertRaises(ValueError):
            parse_expr(tokenize_expr("a < b < c"), table)

    def test_resolves_dangling_else_by_shifting_else(self) -> None:
        tokens = [
            ("IF", "if"),
            ("ID", "c1"),
            ("THEN", "then"),
            ("IF", "if"),
            ("ID", "c2"),
            ("THEN", "then"),
            ("STMT", "s1"),
            ("ELSE", "else"),
            ("STMT", "s2"),
        ]
        self.assertEqual(str(parse_if_stmts(tokens)), "(if c1 then (if c2 then s1 else s2))")


if __name__ == "__main__":
    main()
