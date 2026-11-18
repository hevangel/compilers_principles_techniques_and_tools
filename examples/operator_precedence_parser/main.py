"""1977 Green §5.3 / 1986 Red §4.6: Classical Operator-Precedence Parsing.

Explicitly contrasts with `precedence_parse` (LR shift/reduce precedence conflict
resolution) by implementing Robert Floyd's classical operator-precedence algorithm:
1. Verifying operator-grammar invariants (no epsilon rules, no adjacent nonterminals),
2. Computing `LEADING(A)` and `TRAILING(A)` sets via fixpoint iteration,
3. Constructing the `<.`, `=.`, `.>` precedence-relation matrix directly from productions,
4. Building integer precedence functions `f(a)` and `g(b)` via longest paths in the
   constraint DAG (and detecting cycles when no precedence functions exist),
5. Running shift-reduce handle recognition using top-most stack terminal relations to
   build and evaluate an expression AST.
"""

from __future__ import annotations

from dataclasses import dataclass
import unittest

YIELDS = "<."
EQUAL = "=."
TAKES = ".>"


@dataclass(frozen=True)
class Sym:
    is_nt: bool
    name: str


def nt(s: str) -> Sym:
    return Sym(True, s)


def term(s: str) -> Sym:
    return Sym(False, s)


@dataclass(frozen=True)
class Prod:
    lhs: str
    rhs: tuple[Sym, ...]


@dataclass(frozen=True)
class IdentExpr:
    name: str

    def __str__(self) -> str:
        return self.name


@dataclass(frozen=True)
class BinExpr:
    op: str
    left: Expr
    right: Expr

    def __str__(self) -> str:
        return f"({self.left} {self.op} {self.right})"


Expr = IdentExpr | BinExpr


def expression_operator_grammar() -> tuple[str, list[Prod]]:
    return (
        "E",
        [
            Prod("E", (nt("E"), term("+"), nt("T"))),
            Prod("E", (nt("T"),)),
            Prod("T", (nt("T"), term("*"), nt("F"))),
            Prod("T", (nt("F"),)),
            Prod("F", (term("("), nt("E"), term(")"))),
            Prod("F", (term("id"),)),
        ],
    )


def verify_operator_grammar(prods: list[Prod]) -> None:
    for p in prods:
        if not p.rhs:
            raise ValueError(f"{p.lhs} -> ε violates operator-grammar no-epsilon rule")
        for a, b in zip(p.rhs, p.rhs[1:]):
            if a.is_nt and b.is_nt:
                raise ValueError(f"{p.lhs} -> ... {a.name} {b.name} ... has adjacent nonterminals")


def compute_leading_trailing(prods: list[Prod]) -> tuple[dict[str, set[str]], dict[str, set[str]]]:
    leading: dict[str, set[str]] = {p.lhs: set() for p in prods}
    trailing: dict[str, set[str]] = {p.lhs: set() for p in prods}

    changed = True
    while changed:
        changed = False
        for p in prods:
            rhs = p.rhs
            if not rhs[0].is_nt:
                if rhs[0].name not in leading[p.lhs]:
                    leading[p.lhs].add(rhs[0].name)
                    changed = True
            else:
                for t in leading[rhs[0].name]:
                    if t not in leading[p.lhs]:
                        leading[p.lhs].add(t)
                        changed = True
                if len(rhs) >= 2 and not rhs[1].is_nt and rhs[1].name not in leading[p.lhs]:
                    leading[p.lhs].add(rhs[1].name)
                    changed = True

            if not rhs[-1].is_nt:
                if rhs[-1].name not in trailing[p.lhs]:
                    trailing[p.lhs].add(rhs[-1].name)
                    changed = True
            else:
                for t in trailing[rhs[-1].name]:
                    if t not in trailing[p.lhs]:
                        trailing[p.lhs].add(t)
                        changed = True
                if len(rhs) >= 2 and not rhs[-2].is_nt and rhs[-2].name not in trailing[p.lhs]:
                    trailing[p.lhs].add(rhs[-2].name)
                    changed = True

    return leading, trailing


def build_precedence_matrix(start: str, prods: list[Prod]) -> dict[tuple[str, str], str]:
    verify_operator_grammar(prods)
    leading, trailing = compute_leading_trailing(prods)
    matrix: dict[tuple[str, str], str] = {}

    def insert_rel(a: str, b: str, rel: str) -> None:
        key = (a, b)
        if key in matrix and matrix[key] != rel:
            raise ValueError(f"operator-precedence conflict at ({a}, {b}): {matrix[key]} vs {rel}")
        matrix[key] = rel

    for p in prods:
        rhs = p.rhs
        for i, sym in enumerate(rhs):
            if not sym.is_nt:
                if i + 1 < len(rhs) and not rhs[i + 1].is_nt:
                    insert_rel(sym.name, rhs[i + 1].name, EQUAL)
                if i + 2 < len(rhs) and rhs[i + 1].is_nt and not rhs[i + 2].is_nt:
                    insert_rel(sym.name, rhs[i + 2].name, EQUAL)
                if i + 1 < len(rhs) and rhs[i + 1].is_nt:
                    for b in leading[rhs[i + 1].name]:
                        insert_rel(sym.name, b, YIELDS)
            else:
                if i + 1 < len(rhs) and not rhs[i + 1].is_nt:
                    for c in trailing[sym.name]:
                        insert_rel(c, rhs[i + 1].name, TAKES)

    for b in leading[start]:
        insert_rel("$", b, YIELDS)
    for a in trailing[start]:
        insert_rel(a, "$", TAKES)
    return matrix


def build_precedence_functions(
    terminals: list[str], matrix: dict[tuple[str, str], str]
) -> tuple[dict[str, int], dict[str, int]]:
    tcount = len(terminals)
    idx_of = {s: i for i, s in enumerate(terminals)}
    parent = list(range(2 * tcount))

    def find(x: int) -> int:
        while parent[x] != x:
            parent[x] = parent[parent[x]]
            x = parent[x]
        return x

    def union(a: int, b: int) -> None:
        ra, rb = find(a), find(b)
        if ra != rb:
            parent[rb] = ra

    for (a, b), rel in matrix.items():
        if rel == EQUAL:
            union(idx_of[a], tcount + idx_of[b])

    reps = sorted({find(u) for u in range(2 * tcount)})
    edges: dict[int, set[int]] = {r: set() for r in reps}
    for (a, b), rel in matrix.items():
        fa = find(idx_of[a])
        gb = find(tcount + idx_of[b])
        if rel == TAKES:
            if fa == gb:
                raise ValueError("precedence graph contains self-loop on .>")
            edges[fa].add(gb)
        elif rel == YIELDS:
            if fa == gb:
                raise ValueError("precedence graph contains self-loop on <.")
            edges[gb].add(fa)

    color: dict[int, int] = {r: 0 for r in reps}
    height: dict[int, int] = {}

    def dfs(u: int) -> int:
        if color[u] == 2:
            return height[u]
        if color[u] == 1:
            raise ValueError("cycle in precedence graph; no precedence functions exist")
        color[u] = 1
        h = 1
        for v in sorted(edges[u]):
            h = max(h, 1 + dfs(v))
        color[u] = 2
        height[u] = h
        return h

    for r in reps:
        dfs(r)

    f = {term_s: height[find(i)] for i, term_s in enumerate(terminals)}
    g = {term_s: height[find(tcount + i)] for i, term_s in enumerate(terminals)}
    return f, g


def tokenize_op_expr(src: str) -> list[tuple[str, str]]:
    i = 0
    out: list[tuple[str, str]] = []
    while i < len(src):
        if src[i].isspace():
            i += 1
            continue
        if src[i].isalpha() or src[i] == "_":
            start = i
            i += 1
            while i < len(src) and (src[i].isalnum() or src[i] == "_"):
                i += 1
            out.append(("id", src[start:i]))
            continue
        if src[i] in ("+", "*", "(", ")"):
            out.append((src[i], src[i]))
            i += 1
            continue
        raise ValueError(f"unexpected character '{src[i]}'")
    out.append(("$", "$"))
    return out


def _reduce_prime_phrase(handle: list[tuple[str, str, Expr | None]]) -> Expr:
    if len(handle) == 1 and handle[0][0] == "id":
        return IdentExpr(handle[0][1])
    if len(handle) == 3 and handle[0][0] == "E" and handle[2][0] == "E" and handle[1][0] in ("+", "*"):
        assert handle[0][2] is not None and handle[2][2] is not None
        return BinExpr(handle[1][0], handle[0][2], handle[2][2])
    if len(handle) == 3 and handle[0][0] == "(" and handle[1][0] == "E" and handle[2][0] == ")":
        assert handle[1][2] is not None
        return handle[1][2]
    raise ValueError(f"invalid prime phrase handle: {handle}")


def parse_operator_precedence(
    src: str,
    matrix: dict[tuple[str, str], str],
    funcs: tuple[dict[str, int], dict[str, int]] | None = None,
) -> tuple[Expr, list[tuple[str, str, str, str]]]:
    tokens = tokenize_op_expr(src)
    stack: list[tuple[str, str, Expr | None]] = [("$", "$", None)]
    ip = 0
    trace: list[tuple[str, str, str, str]] = []

    def fmt_stack() -> str:
        return " ".join(
            f"id({lex})" if kind == "id" else ("E" if kind == "E" else kind)
            for kind, lex, _ in stack
        )

    while True:
        a = next(kind for kind, _, _ in reversed(stack) if kind != "E")
        b, b_lex = tokens[ip]
        if a == "$" and b == "$":
            if len(stack) == 2 and stack[1][0] == "E" and stack[1][2] is not None:
                trace.append((fmt_stack(), "acc", "$", "accept"))
                return stack[1][2], trace
            raise ValueError("unexpected end of input")

        if (a, b) not in matrix:
            raise ValueError(f"syntax error: no precedence relation between '{a}' and '{b}'")
        rel = matrix[(a, b)]
        if funcs is not None:
            f, g = funcs
            if rel == YIELDS:
                assert f[a] < g[b]
            elif rel == EQUAL:
                assert f[a] == g[b]
            else:
                assert f[a] > g[b]

        snap = fmt_stack()
        if rel in (YIELDS, EQUAL):
            trace.append((snap, f"{a} {rel} {b}", b_lex, f"shift {b_lex}"))
            stack.append((b, b_lex, None))
            ip += 1
        else:
            handle: list[tuple[str, str, Expr | None]] = []
            while True:
                while stack and stack[-1][0] == "E":
                    handle.append(stack.pop())
                item = stack.pop()
                popped_t = item[0]
                handle.append(item)
                while stack and stack[-1][0] == "E":
                    handle.append(stack.pop())
                prev_t = next(kind for kind, _, _ in reversed(stack) if kind != "E")
                if matrix.get((prev_t, popped_t)) == YIELDS:
                    break
            handle.reverse()
            reduced = _reduce_prime_phrase(handle)
            trace.append((snap, f"{a} .> {b}", b_lex, f"reduce -> {reduced}"))
            stack.append(("E", "E", reduced))


def main() -> None:
    start, prods = expression_operator_grammar()
    leading, trailing = compute_leading_trailing(prods)
    matrix = build_precedence_matrix(start, prods)
    terminals = ["+", "*", "(", ")", "id", "$"]
    f, g = build_precedence_functions(terminals, matrix)

    print("=== Classical Operator-Precedence Parsing (Green §5.3 / Red §4.6) ===")
    print("LEADING sets:")
    for k in sorted(leading):
        print(f"  LEADING({k})  = {{{', '.join(sorted(leading[k]))}}}")
    print("TRAILING sets:")
    for k in sorted(trailing):
        print(f"  TRAILING({k}) = {{{', '.join(sorted(trailing[k]))}}}")

    print("\nPrecedence Relation Matrix (<., =., .>):")
    print("     " + "".join(f"{col:>4}" for col in terminals))
    for row in terminals:
        cells = "".join(f"{matrix.get((row, col), '·'):>4}" for col in terminals)
        print(f"  {row:<3}{cells}")

    print("\nCompacted Precedence Functions f(a), g(a):")
    print("  f: " + "  ".join(f"{t}={f[t]}" for t in terminals))
    print("  g: " + "  ".join(f"{t}={g[t]}" for t in terminals))

    sample = "a + b * (c + d)"
    ast, trace = parse_operator_precedence(sample, matrix, (f, g))
    print(f"\nShift-reduce trace on {sample!r} -> {ast}:")
    for st, rel, la, act in trace:
        print(f"  {st:<24} | {rel:<10} | {la:<4} | {act}")


class OperatorPrecedenceTests(unittest.TestCase):
    def test_rejects_non_operator_grammars(self) -> None:
        with self.assertRaises(ValueError):
            verify_operator_grammar([Prod("E", ())])
        with self.assertRaises(ValueError):
            verify_operator_grammar([Prod("S", (nt("A"), nt("B")))])

    def test_matrix_and_precedence_functions_agree_and_parse_precedence(self) -> None:
        start, prods = expression_operator_grammar()
        matrix = build_precedence_matrix(start, prods)
        terminals = ["+", "*", "(", ")", "id", "$"]
        funcs = build_precedence_functions(terminals, matrix)

        ast, _ = parse_operator_precedence("a + b * c", matrix, funcs)
        self.assertEqual(str(ast), "(a + (b * c))")
        ast2, _ = parse_operator_precedence("(a + b) * c + d", matrix, funcs)
        self.assertEqual(str(ast2), "(((a + b) * c) + d)")

    def test_detects_cyclic_precedence_relations_without_functions(self) -> None:
        cyclic = {
            ("a", "b"): YIELDS,
            ("a", "a"): TAKES,
            ("b", "b"): TAKES,
            ("b", "a"): YIELDS,
        }
        with self.assertRaises(ValueError):
            build_precedence_functions(["a", "b"], cyclic)


if __name__ == "__main__":
    main()
