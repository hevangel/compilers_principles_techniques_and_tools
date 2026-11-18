"""Minimize a DFA by pruning unreachable states and refining equivalence partitions."""

from collections import deque
from dataclasses import dataclass
import unittest


@dataclass(frozen=True)
class Dfa:
    """Table-driven DFA with dense transition rows `trans[state][col]`."""

    start: int
    alphabet: list[str]
    accepting: frozenset[int]
    trans: list[list[int]]

    def accepts(self, text: str) -> bool:
        state = self.start
        for ch in text:
            if ch not in self.alphabet:
                return False
            state = self.trans[state][self.alphabet.index(ch)]
        return state in self.accepting


def prune_unreachable(dfa: Dfa) -> Dfa:
    """Discard states unreachable from `dfa.start` and reindex reachable states `0..k-1`.

    Pruned first so unreachable disconnected components cannot survive as extra blocks.
    """
    visited = {dfa.start}
    queue = deque([dfa.start])
    order: list[int] = []
    while queue:
        state = queue.popleft()
        order.append(state)
        for nxt in dfa.trans[state]:
            if nxt not in visited:
                visited.add(nxt)
                queue.append(nxt)
    remap = {old: new for new, old in enumerate(order)}
    trans = [[remap[t] for t in dfa.trans[s]] for s in order]
    accepting = frozenset(remap[s] for s in dfa.accepting if s in remap)
    return Dfa(start=0, alphabet=list(dfa.alphabet), accepting=accepting, trans=trans)


def minimize(dfa: Dfa) -> tuple[Dfa, list[frozenset[int]]]:
    """Minimize `dfa` by partition refinement and return `(minimal_dfa, classes)`."""
    reachable = prune_unreachable(dfa)
    num_states = len(reachable.trans)
    accepting = set(reachable.accepting)
    non_accepting = set(range(num_states)) - accepting

    # Initial split separates accepting from non-accepting states; drop empty blocks
    # so all-accepting or all-rejecting DFAs collapse cleanly to one quotient state.
    partition: list[frozenset[int]] = [
        frozenset(block) for block in (accepting, non_accepting) if block
    ]

    # Refine until every block's states share the same destination-group signature.
    while True:
        state_group = [0] * num_states
        for gid, group in enumerate(partition):
            for s in group:
                state_group[s] = gid

        next_partition: list[frozenset[int]] = []
        for group in partition:
            subgroups: dict[tuple[int, ...], set[int]] = {}
            for state in sorted(group):
                sig = tuple(state_group[t] for t in reachable.trans[state])
                subgroups.setdefault(sig, set()).add(state)
            for sig in sorted(subgroups):
                next_partition.append(frozenset(subgroups[sig]))

        if next_partition == partition:
            break
        partition = next_partition

    # Put the start state's equivalence class first for deterministic output.
    partition.sort(key=lambda g: (0 if reachable.start in g else 1, min(g)))
    group_of = [0] * num_states
    for gid, group in enumerate(partition):
        for s in group:
            group_of[s] = gid

    # Construct quotient transitions from one representative per equivalence class.
    trans = []
    min_accepting: set[int] = set()
    for gid, group in enumerate(partition):
        rep = min(group)
        trans.append([group_of[t] for t in reachable.trans[rep]])
        if rep in reachable.accepting:
            min_accepting.add(gid)

    return (
        Dfa(
            start=group_of[reachable.start],
            alphabet=list(reachable.alphabet),
            accepting=frozenset(min_accepting),
            trans=trans,
        ),
        partition,
    )


def inflated_abb_dfa() -> Dfa:
    return Dfa(
        start=0,
        alphabet=["a", "b"],
        accepting=frozenset({6}),
        trans=[
            [1, 2],
            [3, 4],
            [1, 2],
            [3, 4],
            [1, 6],
            [5, 5],
            [1, 2],
        ],
    )


def main() -> None:
    original = inflated_abb_dfa()
    minimized, groups = minimize(original)
    print(f"States reduced: {len(original.trans)} -> {len(minimized.trans)}")
    print(f"Equivalence classes: {[sorted(g) for g in groups]}")
    for idx, row in enumerate(minimized.trans):
        mark = " (accept)" if idx in minimized.accepting else ""
        print(f"  q{idx}{mark}: a -> q{row[0]}, b -> q{row[1]}")


class DfaMinimizeTests(unittest.TestCase):
    def test_minimizes_inflated_dfa_and_preserves_language(self) -> None:
        original = inflated_abb_dfa()
        minimized, groups = minimize(original)
        self.assertEqual(len(minimized.trans), 4)
        self.assertEqual(len(groups), 4)
        words = [""]
        for _ in range(7):
            for w in words:
                self.assertEqual(original.accepts(w), minimized.accepts(w), w)
            words = [w + ch for w in words for ch in ("a", "b")]

    def test_handles_single_state_and_unreachable_dfas(self) -> None:
        all_accept = Dfa(0, ["a", "b"], frozenset({0, 1}), [[1, 0], [0, 1]])
        min_accept, _ = minimize(all_accept)
        self.assertEqual(len(min_accept.trans), 1)
        self.assertTrue(min_accept.accepts("abba"))

        all_reject = Dfa(0, ["a", "b"], frozenset(), [[1, 1], [0, 0], [2, 2]])
        min_reject, _ = minimize(all_reject)
        self.assertEqual(len(min_reject.trans), 1)
        self.assertFalse(min_reject.accepts("ab"))


if __name__ == "__main__":
    main()
