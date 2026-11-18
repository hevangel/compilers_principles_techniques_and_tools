"""Convert an epsilon-NFA into a table-driven deterministic recognizer."""

from dataclasses import dataclass
import unittest


@dataclass(frozen=True)
class Edge:
    """Directed NFA transition (`symbol=None` represents an epsilon edge)."""

    src: int
    symbol: str | None
    dst: int


ALPHABET = ("a", "b", "c")
FINAL = 4


def closure(states: set[int], edges: list[Edge]) -> tuple[int, ...]:
    """Return the sorted epsilon-closure of `states` (fixed point over `symbol=None` edges)."""
    current = set(states)
    while True:
        previous = len(current)
        for edge in edges:
            if edge.symbol is None and edge.src in current:
                current.add(edge.dst)
        # Terminates even when epsilon edges form cycles because `current` only grows.
        if len(current) == previous:
            return tuple(sorted(current))


@dataclass
class Dfa:
    """Table-driven DFA where state `i` corresponds to NFA subset `subsets[i]`."""

    subsets: list[tuple[int, ...]]
    rows: list[list[int]]

    def accepts(self, text: str) -> bool:
        state = 0
        for ch in text:
            # Characters outside ALPHABET cannot match any transition.
            if ch not in ALPHABET:
                return False
            column = ALPHABET.index(ch)
            state = self.rows[state][column]
        # Accepting iff the active subset contains NFA accepting state FINAL.
        return FINAL in self.subsets[state]


def determinize(edges: list[Edge]) -> Dfa:
    """Build a DFA from an epsilon-NFA via subset construction (BFS over reachable sets)."""
    subsets = [closure({0}, edges)]
    rows: list[list[int]] = []
    # Invariant: `rows` holds completed rows for indices `0..len(rows)`;
    # `subsets[len(rows)..]` is the worklist of discovered states still needing rows.
    while len(rows) < len(subsets):
        current = len(rows)
        row: list[int] = []
        for symbol in ALPHABET:
            moved = {
                edge.dst
                for edge in edges
                if edge.symbol == symbol and edge.src in subsets[current]
            }
            # Empty `moved` yields `()`, creating an explicit self-looping dead state.
            nxt = closure(moved, edges)
            if nxt in subsets:
                index = subsets.index(nxt)
            else:
                index = len(subsets)
                subsets.append(nxt)
            row.append(index)
        rows.append(row)
    return Dfa(subsets=subsets, rows=rows)


def sample() -> Dfa:
    """Construct the sample NFA for `ab* | ac` and determinize it."""
    edges = [
        Edge(0, None, 1),
        Edge(0, None, 2),
        Edge(1, "a", 3),
        Edge(3, "b", 3),
        Edge(3, None, FINAL),
        Edge(2, "a", 5),
        Edge(5, "c", FINAL),
    ]
    return determinize(edges)


def main() -> None:
    dfa = sample()
    for index, (subset, row) in enumerate(zip(dfa.subsets, dfa.rows)):
        print(f"{index}: {set(subset)} -> {row} on a, b, c")
    for word in ["a", "abbb", "ac", "abc"]:
        print(f"{word}: {dfa.accepts(word)}")


class SubsetDfaTests(unittest.TestCase):
    def test_follows_epsilon_branches_and_handles_dead_state(self) -> None:
        dfa = sample()
        for word in ["a", "ab", "abbb", "ac"]:
            self.assertTrue(dfa.accepts(word), word)
        for word in ["", "b", "abc", "acc", "\u00e9"]:
            self.assertFalse(dfa.accepts(word), word)


if __name__ == "__main__":
    main()
