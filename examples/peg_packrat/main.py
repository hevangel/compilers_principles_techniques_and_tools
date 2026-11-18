"""Modern Extension: Parsing Expression Grammars (PEGs) & Packrat Memoization.

Explicitly labeled as a modern parsing technique (Bryan Ford 2004; CPython's `pegen`
foundation) complementing Dragon Book Chapter 4:
1. Ordered choice (`/`), greedy repetition (`*`/`+`/`?`), and zero-width syntactic
   predicates (`&e` positive lookahead, `!e` negative lookahead),
2. Contrasting deterministic ordered choice `/` with CFG unordered `|` (why `"ab"` fails
   under `'a' / 'ab'` when left-prefix comes first),
3. Proving exponential `O(2^d)` naive backtracking blowup vs. linear `O(n)` Packrat
   memoization `(rule, pos) -> MemoResult` on deeply nested expressions,
4. Recognizing the non-context-free language `{ a^n b^n c^n | n >= 1 }` cleanly using
   zero-width lookahead predicates `&(A "c") "a"+ B !.`.
"""

from __future__ import annotations

from dataclasses import dataclass
import unittest


@dataclass(frozen=True)
class Lit:
    text: str


@dataclass(frozen=True)
class RuleRef:
    name: str


@dataclass(frozen=True)
class Seq:
    items: tuple[Peg, ...]


@dataclass(frozen=True)
class Choice:
    alts: tuple[Peg, ...]


@dataclass(frozen=True)
class Opt:
    inner: Peg


@dataclass(frozen=True)
class Plus:
    inner: Peg


@dataclass(frozen=True)
class AndPred:
    inner: Peg


@dataclass(frozen=True)
class NotPred:
    inner: Peg


@dataclass(frozen=True)
class AnyChar:
    pass


Peg = Lit | RuleRef | Seq | Choice | Opt | Plus | AndPred | NotPred | AnyChar


@dataclass(frozen=True)
class PegGrammar:
    start: str
    rules: dict[str, Peg]


_ACTIVE = object()


class PegRunner:
    def __init__(self, grammar: PegGrammar, src: str, memoize: bool) -> None:
        self.grammar = grammar
        self.src = src
        self.memoize = memoize
        self.memo: dict[tuple[str, int], int | None | object] = {}
        self.rule_calls = 0
        self.cache_hits = 0

    def parse(self) -> int | None:
        return self._eval_rule(self.grammar.start, 0)

    def _eval_rule(self, name: str, pos: int) -> int | None:
        if self.memoize:
            key = (name, pos)
            if key in self.memo:
                self.cache_hits += 1
                cached = self.memo[key]
                return None if cached is _ACTIVE else cached  # type: ignore[return-value]
            self.memo[key] = _ACTIVE
        self.rule_calls += 1
        res = self._eval_expr(self.grammar.rules[name], pos)
        if self.memoize:
            self.memo[(name, pos)] = res
        return res

    def _eval_expr(self, expr: Peg, pos: int) -> int | None:
        if isinstance(expr, Lit):
            return pos + len(expr.text) if self.src.startswith(expr.text, pos) else None
        if isinstance(expr, AnyChar):
            return pos + 1 if pos < len(self.src) else None
        if isinstance(expr, RuleRef):
            return self._eval_rule(expr.name, pos)
        if isinstance(expr, Seq):
            cur = pos
            for item in expr.items:
                nxt = self._eval_expr(item, cur)
                if nxt is None:
                    return None
                cur = nxt
            return cur
        if isinstance(expr, Choice):
            for alt in expr.alts:
                end = self._eval_expr(alt, pos)
                if end is not None:
                    return end
            return None
        if isinstance(expr, Opt):
            end = self._eval_expr(expr.inner, pos)
            return pos if end is None else end
        if isinstance(expr, Plus):
            cur = self._eval_expr(expr.inner, pos)
            if cur is None:
                return None
            while True:
                nxt = self._eval_expr(expr.inner, cur)
                if nxt is None or nxt == cur:
                    break
                cur = nxt
            return cur
        if isinstance(expr, AndPred):
            return pos if self._eval_expr(expr.inner, pos) is not None else None
        assert isinstance(expr, NotPred)
        return pos if self._eval_expr(expr.inner, pos) is None else None


def expr_peg() -> PegGrammar:
    return PegGrammar(
        start="Top",
        rules={
            "Top": Seq((RuleRef("Expr"), NotPred(AnyChar()))),
            "Expr": Choice((Seq((RuleRef("Prod"), Lit("+"), RuleRef("Expr"))), RuleRef("Prod"))),
            "Prod": Choice((Seq((RuleRef("Atom"), Lit("*"), RuleRef("Prod"))), RuleRef("Atom"))),
            "Atom": Choice((Seq((Lit("("), RuleRef("Expr"), Lit(")"))), Lit("id"))),
        },
    )


def an_bn_cn_peg() -> PegGrammar:
    return PegGrammar(
        start="S",
        rules={
            "S": Seq(
                (
                    AndPred(Seq((RuleRef("A"), Lit("c")))),
                    Plus(Lit("a")),
                    RuleRef("B"),
                    NotPred(AnyChar()),
                )
            ),
            "A": Seq((Lit("a"), Opt(RuleRef("A")), Lit("b"))),
            "B": Seq((Lit("b"), Opt(RuleRef("B")), Lit("c"))),
        },
    )


def ordered_choice_trap_peg() -> PegGrammar:
    return PegGrammar(
        start="Trap",
        rules={"Trap": Seq((Choice((Lit("a"), Lit("ab"))), NotPred(AnyChar())))},
    )


def run_peg(grammar: PegGrammar, src: str, memoize: bool) -> tuple[bool, int, int]:
    runner = PegRunner(grammar, src, memoize)
    ok = runner.parse() == len(src)
    return ok, runner.rule_calls, runner.cache_hits


def main() -> None:
    print("=== Advanced Parsing (Modern): PEG and Packrat Memoization ===")
    g_expr = expr_peg()
    nested = f"{'(' * 10}id{')' * 10}"
    ok_naive, calls_naive, _ = run_peg(g_expr, nested, False)
    ok_pack, calls_pack, hits_pack = run_peg(g_expr, nested, True)
    assert ok_naive and ok_pack

    print(f"\n1. Pathological backtracking on depth-10 nested `{nested}`:")
    print(f"   Unmemoized recursive PEG calls: {calls_naive:>8} (exponential O(2^d))")
    print(f"   Packrat memoized PEG calls:     {calls_pack:>8} (linear, cache hits = {hits_pack})")

    g_abc = an_bn_cn_peg()
    print("\n2. Syntactic predicates `&e` / `!e` recognizing non-CFG {a^n b^n c^n}:")
    for sample in ("abc", "aaabbbccc", "aaabbccc", "aabbbcc", "aabbcc!"):
        ok, calls, _ = run_peg(g_abc, sample, True)
        print(f"   {sample:<12} -> matched={str(ok):<5} (Packrat rule evals={calls})")

    g_trap = ordered_choice_trap_peg()
    trap_ab, _, _ = run_peg(g_trap, "ab", True)
    print(f"\n3. Ordered choice `/` vs CFG `|`: rule `'a' / 'ab' !.` on \"ab\" -> {trap_ab} (first alt commits!)")


class PegPackratTests(unittest.TestCase):
    def test_packrat_prevents_exponential_backtracking_on_nested_parens(self) -> None:
        g = expr_peg()
        nested = f"{'(' * 8}id+id*id{')' * 8}"
        ok_naive, calls_naive, _ = run_peg(g, nested, False)
        ok_pack, calls_pack, hits = run_peg(g, nested, True)
        self.assertTrue(ok_naive and ok_pack)
        self.assertGreater(calls_naive, calls_pack * 10)
        self.assertGreater(hits, 0)

    def test_syntactic_predicates_recognize_exact_an_bn_cn(self) -> None:
        g = an_bn_cn_peg()
        for n in range(1, 7):
            valid = "a" * n + "b" * n + "c" * n
            self.assertTrue(run_peg(g, valid, True)[0])
        for bad in ("", "ab", "aabb", "aaabbcc", "aabbbcc", "aabbccc", "abcabc"):
            self.assertFalse(run_peg(g, bad, True)[0])

    def test_ordered_choice_commits_to_first_matching_branch(self) -> None:
        g = ordered_choice_trap_peg()
        self.assertTrue(run_peg(g, "a", True)[0])
        self.assertFalse(run_peg(g, "ab", True)[0])


if __name__ == "__main__":
    main()
