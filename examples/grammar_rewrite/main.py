"""Eliminate left recursion and extract common prefixes via left factoring."""

from dataclasses import dataclass
import unittest


@dataclass(frozen=True)
class Terminal:
    """Leaf token symbol in a context-free grammar production."""

    name: str


@dataclass(frozen=True)
class NonTerminal:
    """Rule head symbol in a context-free grammar production."""

    name: str


Symbol = Terminal | NonTerminal


@dataclass
class Cfg:
    """Context-free grammar with ordered nonterminals and production alternatives.

    An empty `list[Symbol]` (`[]`) encodes epsilon (`eps`).
    """

    nonterminals: list[str]
    start: str
    prods: dict[str, list[list[Symbol]]]

    def fresh_name(self, base: str, used: set[str], suffix: str) -> str:
        """Allocate a nonterminal name not present in `used` (e.g., `A'`, `S_f`)."""
        candidate = f"{base}{suffix}"
        counter = 1
        while candidate in used:
            candidate = f"{base}{suffix}{counter}"
            counter += 1
        used.add(candidate)
        return candidate


def validate_cfg(cfg: Cfg) -> None:
    """Ensure start/nonterminals are defined and reject pure unit cycles (`A =>+ A`)."""
    if cfg.start not in cfg.prods:
        raise ValueError(f"missing start symbol {cfg.start}")
    for nt in cfg.nonterminals:
        if not cfg.prods.get(nt):
            raise ValueError(f"nonterminal {nt} has no productions")
    nullable: set[str] = set()
    while True:
        prev = len(nullable)
        for lhs, alts in cfg.prods.items():
            if any(
                all(isinstance(s, NonTerminal) and s.name in nullable for s in rhs)
                for rhs in alts
            ):
                nullable.add(lhs)
        if len(nullable) == prev:
            break
    for start in cfg.nonterminals:
        reachable: set[str] = set()
        stack = [start]
        while stack:
            cur = stack.pop()
            for rhs in cfg.prods.get(cur, []):
                for sym in rhs:
                    if isinstance(sym, Terminal):
                        break
                    if sym.name == start and len(rhs) == 1:
                        raise ValueError(f"unit cycle involving {start}")
                    if sym.name not in reachable:
                        reachable.add(sym.name)
                        stack.append(sym.name)
                    if sym.name not in nullable:
                        break


def eliminate_left_recursion(cfg: Cfg) -> Cfg:
    """Eliminate indirect and direct left recursion via ordered substitution (`j < i`).

    Invariant after outer iteration `i`: no production of `A_k` (`k <= i`) begins with
    `A_m` (`m <= k`), so no indirect left-recursive cycle can remain among processed rules.
    """
    validate_cfg(cfg)
    prods = {k: [list(rhs) for rhs in v] for k, v in cfg.prods.items()}
    order = list(cfg.nonterminals)
    used = set(order)

    for i in range(len(order)):
        ai = order[i]
        # Substitute earlier rules `A_j` (`j < i`) into leading occurrences `A_i -> A_j tail`.
        for j in range(i):
            aj = order[j]
            aj_alts = prods.get(aj, [])
            updated: list[list[Symbol]] = []
            for rhs in prods.get(ai, []):
                if rhs and isinstance(rhs[0], NonTerminal) and rhs[0].name == aj:
                    tail = rhs[1:]
                    for aj_rhs in aj_alts:
                        updated.append([*aj_rhs, *tail])
                else:
                    updated.append(rhs)
            prods[ai] = updated

        # Split remaining alternatives into left-recursive (`A_i alpha`) and base (`beta`).
        alphas: list[list[Symbol]] = []
        betas: list[list[Symbol]] = []
        for rhs in prods.get(ai, []):
            if rhs and isinstance(rhs[0], NonTerminal) and rhs[0].name == ai:
                alphas.append(rhs[1:])
            else:
                betas.append(rhs)
        if alphas:
            # Base-less recursion (`A -> A a`) cannot derive any finite terminal string.
            if not betas:
                raise ValueError(f"nonterminal {ai} has no non-left-recursive base case")
            tail_nt = cfg.fresh_name(ai, used, "'")
            prods[ai] = [[*beta, NonTerminal(tail_nt)] for beta in betas]
            prods[tail_nt] = [[*alpha, NonTerminal(tail_nt)] for alpha in alphas] + [[]]
            order.append(tail_nt)

    return Cfg(nonterminals=order, start=cfg.start, prods=prods)


def left_factor(cfg: Cfg) -> Cfg:
    """Repeatedly extract the longest shared prefix among >= 2 alternatives of a rule."""
    prods = {k: [list(rhs) for rhs in v] for k, v in cfg.prods.items()}
    order = list(cfg.nonterminals)
    used = set(order)

    while True:
        changed = False
        for nt in list(order):
            alts = prods.get(nt, [])
            if len(alts) < 2:
                continue
            best_prefix: list[Symbol] = []
            for i in range(len(alts)):
                for j in range(i + 1, len(alts)):
                    common = 0
                    while (
                        common < len(alts[i])
                        and common < len(alts[j])
                        and alts[i][common] == alts[j][common]
                    ):
                        common += 1
                    if common > len(best_prefix):
                        best_prefix = alts[i][:common]
            if not best_prefix:
                continue
            fresh = cfg.fresh_name(nt, used, "_f")
            prefix_len = len(best_prefix)
            factored_tails: list[list[Symbol]] = []
            remaining: list[list[Symbol]] = []
            # Alternatives equal to `best_prefix` leave an empty (`eps`) suffix in `fresh`.
            for rhs in alts:
                if rhs[:prefix_len] == best_prefix:
                    factored_tails.append(rhs[prefix_len:])
                else:
                    remaining.append(rhs)
            remaining.append([*best_prefix, NonTerminal(fresh)])
            prods[nt] = remaining
            prods[fresh] = factored_tails
            order.append(fresh)
            changed = True
            break
        if not changed:
            break

    return Cfg(nonterminals=order, start=cfg.start, prods=prods)


def has_left_recursion(cfg: Cfg) -> bool:
    """Return True if any nonterminal can reach itself via leftmost nonterminals."""
    for start in cfg.nonterminals:
        seen: set[str] = set()
        stack = [start]
        while stack:
            cur = stack.pop()
            for rhs in cfg.prods.get(cur, []):
                if rhs and isinstance(rhs[0], NonTerminal):
                    head = rhs[0].name
                    if head == start:
                        return True
                    if head not in seen:
                        seen.add(head)
                        stack.append(head)
    return False


def derive_bounded(cfg: Cfg, max_len: int, max_steps: int) -> set[tuple[str, ...]]:
    forms: set[tuple[Symbol, ...]] = {(NonTerminal(cfg.start),)}
    strings: set[tuple[str, ...]] = set()
    for _ in range(max_steps):
        next_forms: set[tuple[Symbol, ...]] = set()
        for form in forms:
            nt_pos = next(
                (i for i, sym in enumerate(form) if isinstance(sym, NonTerminal)),
                None,
            )
            if nt_pos is None:
                strings.add(tuple(sym.name for sym in form if isinstance(sym, Terminal)))
            else:
                nt_sym = form[nt_pos]
                assert isinstance(nt_sym, NonTerminal)
                for rhs in cfg.prods[nt_sym.name]:
                    expanded = form[:nt_pos] + tuple(rhs) + form[nt_pos + 1 :]
                    if sum(1 for s in expanded if isinstance(s, Terminal)) <= max_len:
                        next_forms.add(expanded)
        if not next_forms:
            break
        forms = next_forms
    return strings


def format_cfg(cfg: Cfg) -> str:
    lines = []
    for nt in cfg.nonterminals:
        alts = []
        for rhs in cfg.prods[nt]:
            alts.append("eps" if not rhs else " ".join(sym.name for sym in rhs))
        lines.append(f"  {nt} -> {' | '.join(alts)}")
    return "\n".join(lines)


def indirect_left_recursive_sample() -> Cfg:
    return Cfg(
        nonterminals=["S", "A"],
        start="S",
        prods={
            "S": [[NonTerminal("A"), Terminal("a")], [Terminal("b")]],
            "A": [[NonTerminal("S"), Terminal("d")], [Terminal("c")]],
        },
    )


def dangling_else_sample() -> Cfg:
    return Cfg(
        nonterminals=["S"],
        start="S",
        prods={
            "S": [
                [Terminal("i"), Terminal("E"), Terminal("t"), Terminal("S")],
                [
                    Terminal("i"),
                    Terminal("E"),
                    Terminal("t"),
                    Terminal("S"),
                    Terminal("e"),
                    Terminal("S"),
                ],
                [Terminal("a")],
            ]
        },
    )


def main() -> None:
    lr = indirect_left_recursive_sample()
    lr_free = eliminate_left_recursion(lr)
    print(f"Original indirect left-recursive grammar:\n{format_cfg(lr)}")
    print(
        f"After left-recursion elimination (left recursive={has_left_recursion(lr_free)}):\n"
        f"{format_cfg(lr_free)}"
    )
    df = dangling_else_sample()
    factored = left_factor(df)
    print(f"After left factoring dangling-else:\n{format_cfg(factored)}")


class GrammarRewriteTests(unittest.TestCase):
    def test_eliminates_indirect_left_recursion_and_preserves_bounded_language(self) -> None:
        cfg = indirect_left_recursive_sample()
        self.assertTrue(has_left_recursion(cfg))
        rewritten = eliminate_left_recursion(cfg)
        self.assertFalse(has_left_recursion(rewritten))
        self.assertEqual(derive_bounded(cfg, 5, 10), derive_bounded(rewritten, 5, 10))

    def test_factors_common_prefixes_and_rejects_invalid_grammars(self) -> None:
        df = dangling_else_sample()
        factored = left_factor(df)
        self.assertEqual(len(factored.prods["S"]), 2)
        self.assertGreaterEqual(len(factored.nonterminals), 2)
        self.assertEqual(derive_bounded(df, 6, 4), derive_bounded(factored, 6, 4))

        broken = Cfg(
            nonterminals=["S"],
            start="S",
            prods={"S": [[NonTerminal("S"), Terminal("a")]]},
        )
        with self.assertRaises(ValueError):
            eliminate_left_recursion(broken)

        unit_cycle = Cfg(
            nonterminals=["S"],
            start="S",
            prods={"S": [[NonTerminal("S")]]},
        )
        with self.assertRaises(ValueError):
            eliminate_left_recursion(unit_cycle)


if __name__ == "__main__":
    main()
