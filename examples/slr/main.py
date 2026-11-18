"""Construct an LR(0) automaton and drive a small SLR shift/reduce parser."""

from dataclasses import dataclass
from enum import Enum
import unittest


class Symbol(Enum):
    """Grammar symbols: nonterminals (`START`, `LIST`, `ITEM`), terminals, and `END`."""

    START = "Start"
    LIST = "List"
    ITEM = "Item"
    NAME = "Name"
    COMMA = "Comma"
    END = "End"


# Augmented grammar: rule 0 (`START -> LIST`) gives acceptance its own completed item.
RULES: list[tuple[Symbol, list[Symbol]]] = [
    (Symbol.START, [Symbol.LIST]),
    (Symbol.LIST, [Symbol.LIST, Symbol.COMMA, Symbol.ITEM]),
    (Symbol.LIST, [Symbol.ITEM]),
    (Symbol.ITEM, [Symbol.NAME]),
]


def after_dot(item: tuple[int, int]) -> Symbol | None:
    """Return the symbol immediately following the dot, or None if at the end of RHS."""
    rule, dot = item
    rhs = RULES[rule][1]
    return rhs[dot] if dot < len(rhs) else None


def closure(items: set[tuple[int, int]]) -> frozenset[tuple[int, int]]:
    """Expand items whose dot precedes a nonterminal until reaching a fixed point."""
    current = set(items)
    while True:
        previous = len(current)
        expanded: set[tuple[int, int]] = set()
        for item in current:
            sym = after_dot(item)
            if sym is not None:
                for idx, (lhs, _) in enumerate(RULES):
                    if lhs == sym:
                        expanded.add((idx, 0))
        current |= expanded
        if len(current) == previous:
            return frozenset(current)


def automaton() -> tuple[list[frozenset[tuple[int, int]]], dict[tuple[int, Symbol], int]]:
    """Build the canonical LR(0) item-set collection and state transition table."""
    sets = [closure({(0, 0)})]
    transitions: dict[tuple[int, Symbol], int] = {}
    state = 0
    while state < len(sets):
        symbols = sorted(
            {sym for item in sets[state] if (sym := after_dot(item)) is not None},
            key=lambda s: s.name,
        )
        for sym in symbols:
            # Advance only items waiting on `sym`, then close under nonterminal expansions.
            advanced = {(r, d + 1) for (r, d) in sets[state] if after_dot((r, d)) == sym}
            nxt = closure(advanced)
            if nxt in sets:
                idx = sets.index(nxt)
            else:
                idx = len(sets)
                sets.append(nxt)
            transitions[(state, sym)] = idx
        state += 1
    return sets, transitions


@dataclass(frozen=True)
class Action:
    """SLR parsing action (`Shift`, `Reduce`, or `Accept`)."""

    kind: str
    value: int | None = None


def build_actions(
    sets: list[frozenset[tuple[int, int]]],
    transitions: dict[tuple[int, Symbol], int],
) -> dict[tuple[int, Symbol], Action]:
    """Build the SLR(1) action table by gating reductions on FOLLOW(LHS)."""
    actions: dict[tuple[int, Symbol], Action] = {}
    for (state, sym), target in transitions.items():
        if sym in (Symbol.NAME, Symbol.COMMA):
            actions[(state, sym)] = Action("Shift", target)
    for state, items in enumerate(sets):
        for rule, dot in sorted(items):
            if after_dot((rule, dot)) is not None:
                continue
            # FOLLOW(List) == FOLLOW(Item) == {Comma, End}.
            if rule == 0:
                lookaheads, action = [Symbol.END], Action("Accept")
            else:
                lookaheads, action = [Symbol.COMMA, Symbol.END], Action("Reduce", rule)
            for token in lookaheads:
                # Overlapping shift/reduce or reduce/reduce entries signal an SLR conflict.
                if (state, token) in actions:
                    raise ValueError(f"conflict at state {state}, token {token}")
                actions[(state, token)] = action
    return actions


def parse(
    tokens: list[Symbol],
    actions: dict[tuple[int, Symbol], Action],
    goto: dict[tuple[int, Symbol], int],
) -> list[int] | None:
    """Run the shift-reduce parser and return reduced rule numbers, or None on error."""
    if any(tok not in (Symbol.NAME, Symbol.COMMA) for tok in tokens):
        return None
    stream = [*tokens, Symbol.END]
    pos = 0
    stack = [0]
    reductions: list[int] = []
    while True:
        action = actions.get((stack[-1], stream[pos]))
        if action is None:
            return None
        if action.kind == "Shift":
            assert action.value is not None
            stack.append(action.value)
            pos += 1
        elif action.kind == "Reduce":
            assert action.value is not None
            rule = action.value
            lhs, rhs = RULES[rule]
            if rhs:
                del stack[-len(rhs) :]
            # Consult `goto` from the uncovered state; lookahead `stream[pos]` stays unconsumed.
            nxt = goto.get((stack[-1], lhs))
            if nxt is None:
                return None
            stack.append(nxt)
            reductions.append(rule)
        else:
            return reductions


def main() -> None:
    sets, goto = automaton()
    actions = build_actions(sets, goto)
    print(f"LR(0) states: {[sorted(s) for s in sets]}")
    print(f"Actions: {actions}")
    print(f"Reductions: {parse([Symbol.NAME, Symbol.COMMA, Symbol.NAME], actions, goto)}")


class SlrTests(unittest.TestCase):
    def test_handles_left_recursion_and_rejects_trailing_separator(self) -> None:
        sets, goto = automaton()
        actions = build_actions(sets, goto)
        self.assertEqual(
            parse([Symbol.NAME, Symbol.COMMA, Symbol.NAME], actions, goto),
            [3, 2, 3, 1],
        )
        self.assertIsNone(parse([Symbol.NAME, Symbol.COMMA], actions, goto))
        self.assertIsNone(parse([Symbol.NAME, Symbol.END, Symbol.NAME], actions, goto))
        self.assertIsNone(parse([], actions, goto))


if __name__ == "__main__":
    main()
