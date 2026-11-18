"""Construct SLR(1), LALR(1), and canonical LR(1) tables and compact rows."""

from dataclasses import dataclass
import unittest


@dataclass(frozen=True)
class Nt:
    """Nonterminal ID indexing into `first` / `follow` arrays."""

    id: int


@dataclass(frozen=True)
class Term:
    """Terminal ID (`0` is reserved for EOF sentinel `$`)."""

    id: int


Symbol = Nt | Term


@dataclass(frozen=True)
class Lr1Item:
    """Canonical LR(1) item `[prod: A -> alpha . beta, lookahead]`."""

    prod: int
    dot: int
    lookahead: int


@dataclass(frozen=True)
class Action:
    """Parse table action (`Shift`, `Reduce`, or `Accept`)."""

    kind: str  # "Shift", "Reduce", "Accept"
    value: int | None = None


class Grammar:
    """Integer-indexed CFG with augmented start nonterminal `0` and production `0`."""

    def __init__(
        self,
        num_nt: int,
        num_terms: int,
        term_names: list[str],
        prods: list[tuple[int, list[Symbol]]],
    ) -> None:
        self.num_nt = num_nt
        self.num_terms = num_terms
        self.term_names = term_names
        self.prods = prods

    def first_of_seq(
        self, seq: list[Symbol], first: list[tuple[set[int], bool]]
    ) -> tuple[set[int], bool]:
        out: set[int] = set()
        for sym in seq:
            if isinstance(sym, Term):
                out.add(sym.id)
                return out, False
            terms, nullable = first[sym.id]
            out |= terms
            if not nullable:
                return out, False
        return out, True

    def first_sets(self) -> list[tuple[set[int], bool]]:
        first: list[tuple[set[int], bool]] = [(set(), False) for _ in range(self.num_nt)]
        while True:
            changed = False
            for lhs, rhs in self.prods:
                terms, nullable = self.first_of_seq(rhs, first)
                prev_len, prev_null = len(first[lhs][0]), first[lhs][1]
                first[lhs][0].update(terms)
                first[lhs] = (first[lhs][0], prev_null or nullable)
                if len(first[lhs][0]) != prev_len or first[lhs][1] != prev_null:
                    changed = True
            if not changed:
                return first

    def follow_sets(self, first: list[tuple[set[int], bool]]) -> list[set[int]]:
        follow: list[set[int]] = [set() for _ in range(self.num_nt)]
        follow[0].add(0)
        while True:
            changed = False
            for lhs, rhs in self.prods:
                for i, sym in enumerate(rhs):
                    if isinstance(sym, Nt):
                        terms, nullable = self.first_of_seq(rhs[i + 1 :], first)
                        if nullable:
                            terms = terms | follow[lhs]
                        prev = len(follow[sym.id])
                        follow[sym.id].update(terms)
                        if len(follow[sym.id]) != prev:
                            changed = True
            if not changed:
                return follow

    def closure_lr1(
        self, items: set[Lr1Item], first: list[tuple[set[int], bool]]
    ) -> frozenset[Lr1Item]:
        """Expand `[A -> alpha . B beta, a]` with `[B -> . gamma, b]` for `b in FIRST(beta a)`."""
        current = set(items)
        while True:
            prev = len(current)
            additions: list[Lr1Item] = []
            for item in current:
                rhs = self.prods[item.prod][1]
                if item.dot < len(rhs) and isinstance(rhs[item.dot], Nt):
                    b_nt = rhs[item.dot]
                    assert isinstance(b_nt, Nt)
                    # Appending `Term(item.lookahead)` passes `a` through whenever `beta` is nullable.
                    suffix = [*rhs[item.dot + 1 :], Term(item.lookahead)]
                    lookaheads, _ = self.first_of_seq(suffix, first)
                    for pid, (lhs, _) in enumerate(self.prods):
                        if lhs == b_nt.id:
                            for la in lookaheads:
                                additions.append(Lr1Item(pid, 0, la))
            current.update(additions)
            if len(current) == prev:
                return frozenset(current)

    def goto_lr1(
        self,
        items: frozenset[Lr1Item],
        sym: Symbol,
        first: list[tuple[set[int], bool]],
    ) -> frozenset[Lr1Item]:
        """Shift dot past `sym` (keeping item lookaheads) and close under `closure_lr1`."""
        kernel = {
            Lr1Item(it.prod, it.dot + 1, it.lookahead)
            for it in items
            if it.dot < len(self.prods[it.prod][1]) and self.prods[it.prod][1][it.dot] == sym
        }
        return self.closure_lr1(kernel, first) if kernel else frozenset()


@dataclass
class ParseTable:
    """Action/Goto table together with any detected shift/reduce or reduce/reduce conflicts."""

    num_states: int
    actions: dict[tuple[int, int], Action]
    goto: dict[tuple[int, int], int]
    conflicts: list[tuple[int, int, Action, Action]]


@dataclass
class CompactTable:
    """Sparse action table using `default_reduce` on reduce-only rows."""

    dense_entries: int
    compact_entries: int
    default_reduce: list[int | None]
    explicit_actions: dict[tuple[int, int], Action]


def compact_table(table: ParseTable, num_terms: int) -> CompactTable:
    """Hoist the majority reduction per reduce-only state to shrink stored table entries."""
    default_reduce: list[int | None] = [None] * table.num_states
    explicit_actions: dict[tuple[int, int], Action] = {}
    for state in range(table.num_states):
        counts: dict[int, int] = {}
        has_shift_or_accept = False
        for t in range(num_terms):
            act = table.actions.get((state, t))
            if act is not None:
                if act.kind == "Reduce":
                    assert act.value is not None
                    counts[act.value] = counts.get(act.value, 0) + 1
                else:
                    has_shift_or_accept = True
        # Only rows with no Shift or Accept can safely supply a default reduction.
        best_reduce = (
            max(counts.items(), key=lambda kv: (kv[1], -kv[0]))[0]
            if not has_shift_or_accept and counts
            else None
        )
        default_reduce[state] = best_reduce
        for t in range(num_terms):
            act = table.actions.get((state, t))
            if act is not None and act != (
                Action("Reduce", best_reduce) if best_reduce is not None else None
            ):
                explicit_actions[(state, t)] = act
    return CompactTable(
        dense_entries=table.num_states * num_terms,
        compact_entries=len(explicit_actions) + sum(1 for r in default_reduce if r is not None),
        default_reduce=default_reduce,
        explicit_actions=explicit_actions,
    )


def build_lr1_collection(
    g: Grammar,
) -> tuple[list[frozenset[Lr1Item]], dict[tuple[int, Symbol], int]]:
    """Build the canonical LR(1) state collection starting from `[S' -> . S, $]`."""
    first = g.first_sets()
    start = g.closure_lr1({Lr1Item(0, 0, 0)}, first)
    states = [start]
    trans: dict[tuple[int, Symbol], int] = {}
    i = 0
    while i < len(states):
        syms = sorted(
            {
                g.prods[it.prod][1][it.dot]
                for it in states[i]
                if it.dot < len(g.prods[it.prod][1])
            },
            key=lambda s: (0 if isinstance(s, Nt) else 1, s.id),
        )
        for sym in syms:
            nxt = g.goto_lr1(states[i], sym, first)
            if nxt in states:
                idx = states.index(nxt)
            else:
                idx = len(states)
                states.append(nxt)
            trans[(i, sym)] = idx
        i += 1
    return states, trans


def _record_action(
    actions: dict[tuple[int, int], Action],
    conflicts: list[tuple[int, int, Action, Action]],
    state: int,
    term: int,
    act: Action,
) -> None:
    prev = actions.get((state, term))
    if prev is not None:
        if prev != act:
            conflicts.append((state, term, prev, act))
    else:
        actions[(state, term)] = act


def _table_from_lr1_states(
    g: Grammar,
    states: list[frozenset[Lr1Item]],
    trans: dict[tuple[int, Symbol], int],
) -> ParseTable:
    actions: dict[tuple[int, int], Action] = {}
    goto: dict[tuple[int, int], int] = {}
    conflicts: list[tuple[int, int, Action, Action]] = []
    for (src, sym), dst in trans.items():
        if isinstance(sym, Term):
            _record_action(actions, conflicts, src, sym.id, Action("Shift", dst))
        else:
            goto[(src, sym.id)] = dst
    for state, items in enumerate(states):
        for item in sorted(items, key=lambda x: (x.prod, x.dot, x.lookahead)):
            if item.dot == len(g.prods[item.prod][1]):
                act = Action("Accept") if item.prod == 0 else Action("Reduce", item.prod)
                _record_action(actions, conflicts, state, item.lookahead, act)
    return ParseTable(len(states), actions, goto, conflicts)


def build_lr1_table(g: Grammar) -> ParseTable:
    states, trans = build_lr1_collection(g)
    return _table_from_lr1_states(g, states, trans)


def build_lalr1_table(g: Grammar) -> ParseTable:
    """Merge LR(1) states sharing the same `(prod, dot)` core and unite their lookaheads."""
    lr1_states, lr1_trans = build_lr1_collection(g)
    cores: list[frozenset[tuple[int, int]]] = []
    core_of = [0] * len(lr1_states)
    merged: list[set[Lr1Item]] = []
    for idx, st in enumerate(lr1_states):
        core = frozenset((it.prod, it.dot) for it in st)
        if core in cores:
            cid = cores.index(core)
        else:
            cid = len(cores)
            cores.append(core)
            merged.append(set())
        core_of[idx] = cid
        merged[cid].update(st)
    lalr_trans = {
        (core_of[src], sym): core_of[dst] for (src, sym), dst in lr1_trans.items()
    }
    return _table_from_lr1_states(g, [frozenset(m) for m in merged], lalr_trans)


def build_slr1_table(g: Grammar) -> ParseTable:
    """Build SLR(1) over LR(0) cores using `FOLLOW(LHS)` for completed items."""
    first = g.first_sets()
    follow = g.follow_sets(first)
    lr1_states, lr1_trans = build_lr1_collection(g)
    cores: list[frozenset[tuple[int, int]]] = []
    core_of = [0] * len(lr1_states)
    for idx, st in enumerate(lr1_states):
        core = frozenset((it.prod, it.dot) for it in st)
        if core in cores:
            cid = cores.index(core)
        else:
            cid = len(cores)
            cores.append(core)
        core_of[idx] = cid
    actions: dict[tuple[int, int], Action] = {}
    goto: dict[tuple[int, int], int] = {}
    conflicts: list[tuple[int, int, Action, Action]] = []
    for (src, sym), dst in lr1_trans.items():
        csrc, cdst = core_of[src], core_of[dst]
        if isinstance(sym, Term):
            actions[(csrc, sym.id)] = Action("Shift", cdst)
        else:
            goto[(csrc, sym.id)] = cdst
    for state, core in enumerate(cores):
        for prod, dot in sorted(core):
            if dot == len(g.prods[prod][1]):
                if prod == 0:
                    _record_action(actions, conflicts, state, 0, Action("Accept"))
                else:
                    lhs = g.prods[prod][0]
                    for la in sorted(follow[lhs]):
                        _record_action(actions, conflicts, state, la, Action("Reduce", prod))
    return ParseTable(len(cores), actions, goto, conflicts)


def parse(g: Grammar, table: ParseTable, tokens: list[int]) -> list[int]:
    """Run the LR shift-reduce driver on `tokens` using a conflict-free `ParseTable`."""
    if table.conflicts:
        raise ValueError(f"grammar has {len(table.conflicts)} conflicts")
    stream = [*tokens, 0]
    pos = 0
    stack = [0]
    reductions: list[int] = []
    while True:
        act = table.actions.get((stack[-1], stream[pos]))
        if act is None:
            raise ValueError(f"syntax error at token {pos} ({g.term_names[stream[pos]]})")
        if act.kind == "Shift":
            assert act.value is not None
            stack.append(act.value)
            pos += 1
        elif act.kind == "Reduce":
            assert act.value is not None
            lhs, rhs = g.prods[act.value]
            if rhs:
                del stack[-len(rhs) :]
            stack.append(table.goto[(stack[-1], lhs)])
            reductions.append(act.value)
        else:
            return reductions


def lvalue_grammar() -> Grammar:
    return Grammar(
        num_nt=4,
        num_terms=4,
        term_names=["$", "=", "*", "id"],
        prods=[
            (0, [Nt(1)]),
            (1, [Nt(2), Term(1), Nt(3)]),
            (1, [Nt(3)]),
            (2, [Term(2), Nt(3)]),
            (2, [Term(3)]),
            (3, [Nt(2)]),
        ],
    )


def lr1_only_grammar() -> Grammar:
    return Grammar(
        num_nt=4,
        num_terms=6,
        term_names=["$", "a", "b", "c", "d", "e"],
        prods=[
            (0, [Nt(1)]),
            (1, [Term(1), Nt(2), Term(3)]),
            (1, [Term(1), Nt(3), Term(4)]),
            (1, [Term(2), Nt(3), Term(3)]),
            (1, [Term(2), Nt(2), Term(4)]),
            (2, [Term(5)]),
            (3, [Term(5)]),
        ],
    )


def main() -> None:
    lv = lvalue_grammar()
    slr = build_slr1_table(lv)
    lalr = build_lalr1_table(lv)
    lr1 = build_lr1_table(lv)
    compact = compact_table(lalr, lv.num_terms)
    print(
        f"Lvalue grammar: SLR conflicts={len(slr.conflicts)}, "
        f"LALR states={lalr.num_states} (conflicts={len(lalr.conflicts)}), "
        f"LR(1) states={lr1.num_states} (conflicts={len(lr1.conflicts)})"
    )
    print(
        f"LALR compaction: dense {compact.dense_entries} entries -> "
        f"{compact.compact_entries} compact entries (default_reduce={compact.default_reduce})"
    )
    print(f"Parsed '*id = id' via LALR(1), reductions: {parse(lv, lalr, [2, 3, 1, 3])}")


class LalrTests(unittest.TestCase):
    def test_lvalue_grammar_fails_slr_but_passes_lalr_and_lr1(self) -> None:
        lv = lvalue_grammar()
        slr = build_slr1_table(lv)
        lalr = build_lalr1_table(lv)
        lr1 = build_lr1_table(lv)
        self.assertTrue(slr.conflicts)
        self.assertFalse(lalr.conflicts)
        self.assertFalse(lr1.conflicts)
        self.assertLess(lalr.num_states, lr1.num_states)
        parse(lv, lalr, [3])
        parse(lv, lalr, [2, 3, 1, 3])
        parse(lv, lalr, [2, 2, 3, 1, 2, 3])
        with self.assertRaises(ValueError):
            parse(lv, lalr, [3, 1])
        compact = compact_table(lalr, lv.num_terms)
        self.assertLess(compact.compact_entries, compact.dense_entries)

    def test_demonstrates_strict_lr1_over_lalr1_hierarchy(self) -> None:
        g = lr1_only_grammar()
        lalr = build_lalr1_table(g)
        lr1 = build_lr1_table(g)
        self.assertTrue(lalr.conflicts)
        self.assertFalse(lr1.conflicts)
        for tokens in ([1, 5, 3], [1, 5, 4], [2, 5, 3], [2, 5, 4]):
            parse(g, lr1, tokens)
        with self.assertRaises(ValueError):
            parse(g, lr1, [1, 5, 5])


if __name__ == "__main__":
    main()
