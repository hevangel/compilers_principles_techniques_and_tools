"""Chapter 4 (§4.1–4.3): Context-Free Grammars, Derivations, Ambiguity & Limits.

Demonstrates:
1. Formal CFG definition `(V_N, V_T, P, S)` and sentential-form rewriting,
2. Step-by-step Leftmost (`=>_lm`) vs. Rightmost (`=>_rm`) derivations producing the
   exact same parse tree on an unambiguous grammar (`E -> E + T | T`, `T -> T * F | F`, `F -> id`),
3. Proving ambiguity directly by enumerating ALL parse trees for a token sequence via
   general CFG chart recognition (`id + id * id` under `E -> E + E | E * E | id` yields 2 trees;
   dangling-else `if c then if c then s else s` yields 2 trees),
4. Demonstrating the limits of Context-Free Grammars:
   - Balanced nesting `{ a^n b^n }` IS context-free,
   - Cross-serial counting `{ a^n b^n c^n }` is NOT context-free (motivating Ch. 5 semantic checks).
"""

from __future__ import annotations

from dataclasses import dataclass
import unittest


@dataclass(frozen=True, order=True)
class Sym:
    is_nt: bool
    name: str


def nt(name: str) -> Sym:
    return Sym(True, name)


def term(name: str) -> Sym:
    return Sym(False, name)


@dataclass(frozen=True)
class Prod:
    lhs: str
    rhs: tuple[Sym, ...]


@dataclass(frozen=True)
class Cfg:
    start: str
    prods: tuple[Prod, ...]


@dataclass(frozen=True, order=True)
class ParseNode:
    label: str
    children: tuple[ParseTree, ...]


@dataclass(frozen=True, order=True)
class ParseLeaf:
    tok: str


ParseTree = ParseNode | ParseLeaf


def format_tree(tree: ParseTree, indent: int = 1) -> list[str]:
    pad = "  " * indent
    if isinstance(tree, ParseLeaf):
        return [f"{pad}{tree.tok}"]
    lines = [f"{pad}{tree.label}"]
    if not tree.children:
        lines.append(f"{pad}  ε")
    else:
        for ch in tree.children:
            lines.extend(format_tree(ch, indent + 1))
    return lines


def yield_tokens(tree: ParseTree) -> list[str]:
    if isinstance(tree, ParseLeaf):
        return [tree.tok]
    out: list[str] = []
    for ch in tree.children:
        out.extend(yield_tokens(ch))
    return out


def derivation_from_tree(tree: ParseTree, leftmost: bool) -> list[str]:
    frontier: list[tuple[bool, ParseTree]] = [(False, tree)]

    def render(items: list[tuple[bool, ParseTree]]) -> str:
        if not items:
            return "ε"
        parts: list[str] = []
        for expanded, node in items:
            if isinstance(node, ParseLeaf):
                parts.append(node.tok)
            elif expanded:
                parts.append(node.label)
            else:
                parts.append(node.label)
        return " ".join(parts)

    steps = [render(frontier)]
    while True:
        indices = [
            i
            for i, (expanded, node) in enumerate(frontier)
            if not expanded and isinstance(node, ParseNode)
        ]
        if not indices:
            break
        idx = indices[0] if leftmost else indices[-1]
        _, node = frontier.pop(idx)
        assert isinstance(node, ParseNode)
        replacements = [
            (isinstance(c, ParseLeaf), c) for c in node.children
        ]
        for off, item in enumerate(replacements):
            frontier.insert(idx + off, item)
        steps.append(render(frontier))
    return steps


def enumerate_parse_trees(cfg: Cfg, tokens: list[str]) -> list[ParseTree]:
    memo: dict[tuple[str, int, int], list[ParseTree]] = {}
    visiting: set[tuple[str, int, int]] = set()

    def solve_nt(sym_nt: str, l: int, r: int) -> list[ParseTree]:
        key = (sym_nt, l, r)
        if key in memo:
            return memo[key]
        if key in visiting:
            return []
        visiting.add(key)
        trees: set[ParseTree] = set()
        for prod in cfg.prods:
            if prod.lhs != sym_nt:
                continue
            for children in solve_seq(prod.rhs, 0, l, r):
                trees.add(ParseNode(sym_nt, tuple(children)))
        visiting.remove(key)
        res = sorted(trees, key=repr)
        memo[key] = res
        return res

    def solve_seq(rhs: tuple[Sym, ...], idx: int, l: int, r: int) -> list[list[ParseTree]]:
        if idx == len(rhs):
            return [[]] if l == r else []
        sym = rhs[idx]
        if not sym.is_nt:
            if l < r and tokens[l] == sym.name:
                return [[ParseLeaf(sym.name), *tail] for tail in solve_seq(rhs, idx + 1, l + 1, r)]
            return []
        res: list[list[ParseTree]] = []
        for mid in range(l, r + 1):
            ltrees = solve_nt(sym.name, l, mid)
            if not ltrees:
                continue
            rseqs = solve_seq(rhs, idx + 1, mid, r)
            for lt in ltrees:
                for rs in rseqs:
                    res.append([lt, *rs])
        return res

    return solve_nt(cfg.start, 0, len(tokens))


def ambiguous_expr_cfg() -> Cfg:
    return Cfg(
        "E",
        (
            Prod("E", (nt("E"), term("+"), nt("E"))),
            Prod("E", (nt("E"), term("*"), nt("E"))),
            Prod("E", (term("id"),)),
        ),
    )


def unambiguous_expr_cfg() -> Cfg:
    return Cfg(
        "E",
        (
            Prod("E", (nt("E"), term("+"), nt("T"))),
            Prod("E", (nt("T"),)),
            Prod("T", (nt("T"), term("*"), nt("F"))),
            Prod("T", (nt("F"),)),
            Prod("F", (term("id"),)),
        ),
    )


def dangling_else_cfg() -> Cfg:
    return Cfg(
        "S",
        (
            Prod("S", (term("if"), term("c"), term("then"), nt("S"))),
            Prod("S", (term("if"), term("c"), term("then"), nt("S"), term("else"), nt("S"))),
            Prod("S", (term("other"),)),
        ),
    )


def an_bn_cfg() -> Cfg:
    return Cfg(
        "S",
        (
            Prod("S", (term("a"), nt("S"), term("b"))),
            Prod("S", ()),
        ),
    )


def is_an_bn_cn(s: str) -> bool:
    n = len(s)
    i = 0
    while i < n and s[i] == "a":
        i += 1
    na = i
    nb = 0
    while i < n and s[i] == "b":
        i += 1
        nb += 1
    nc = 0
    while i < n and s[i] == "c":
        i += 1
        nc += 1
    return i == n and na >= 1 and na == nb == nc


def main() -> None:
    print("=== CFGs, Leftmost/Rightmost Derivations, Ambiguity & Limits (Ch. 4) ===")
    tokens = ["id", "+", "id", "*", "id"]
    amb_trees = enumerate_parse_trees(ambiguous_expr_cfg(), tokens)
    print(f"\n1. Ambiguous grammar E -> E + E | E * E | id on {' '.join(tokens)!r}: found {len(amb_trees)} parse trees!")
    for idx, tree in enumerate(amb_trees, 1):
        print(f"  Tree #{idx}:\n" + "\n".join(format_tree(tree, 2)))

    unamb_trees = enumerate_parse_trees(unambiguous_expr_cfg(), tokens)
    print(f"\n2. Stratified unambiguous grammar on {' '.join(tokens)!r}: found {len(unamb_trees)} unique parse tree")
    tree = unamb_trees[0]
    lm = derivation_from_tree(tree, True)
    rm = derivation_from_tree(tree, False)
    print("  Leftmost derivation (=>_lm):")
    for step in lm:
        print(f"    =>_lm {step}")
    print("  Rightmost derivation (=>_rm):")
    for step in rm:
        print(f"    =>_rm {step}")

    else_tokens = ["if", "c", "then", "if", "c", "then", "other", "else", "other"]
    else_trees = enumerate_parse_trees(dangling_else_cfg(), else_tokens)
    print(f"\n3. Dangling-else ambiguity on {' '.join(else_tokens)!r}: {len(else_trees)} parse trees")

    ab_trees = enumerate_parse_trees(an_bn_cfg(), ["a", "a", "b", "b"])
    print(
        f"\n4. CFG vs. non-CFG boundary: {{a^n b^n}} has {len(ab_trees)} CFG parse tree on 'aabb'; "
        f"{{a^n b^n c^n}} ('aabbcc' valid? {is_an_bn_cn('aabbcc')}) exceeds one stack -> handled semantically."
    )


class CfgDerivationsTests(unittest.TestCase):
    def test_ambiguous_grammars_produce_multiple_distinct_parse_trees(self) -> None:
        self.assertEqual(len(enumerate_parse_trees(ambiguous_expr_cfg(), ["id", "+", "id", "*", "id"])), 2)
        self.assertEqual(
            len(enumerate_parse_trees(dangling_else_cfg(), ["if", "c", "then", "if", "c", "then", "other", "else", "other"])),
            2,
        )

    def test_unambiguous_grammar_produces_single_tree_with_matching_yields(self) -> None:
        toks = ["id", "+", "id", "*", "id"]
        trees = enumerate_parse_trees(unambiguous_expr_cfg(), toks)
        self.assertEqual(len(trees), 1)
        self.assertEqual(yield_tokens(trees[0]), toks)
        lm = derivation_from_tree(trees[0], True)
        rm = derivation_from_tree(trees[0], False)
        self.assertEqual(lm[0], "E")
        self.assertEqual(rm[0], "E")
        self.assertEqual(lm[-1], "id + id * id")
        self.assertEqual(rm[-1], "id + id * id")
        self.assertNotEqual(lm, rm)

    def test_an_bn_is_cfg_while_an_bn_cn_requires_cross_serial_checking(self) -> None:
        self.assertEqual(len(enumerate_parse_trees(an_bn_cfg(), ["a", "a", "a", "b", "b", "b"])), 1)
        self.assertEqual(enumerate_parse_trees(an_bn_cfg(), ["a", "a", "b"]), [])
        self.assertTrue(is_an_bn_cn("aaabbbccc"))
        self.assertFalse(is_an_bn_cn("aaabbbcc"))


if __name__ == "__main__":
    main()
