"""Propagate assignment identities forward through branches and loops."""

from dataclasses import dataclass
import unittest


@dataclass(frozen=True)
class Block:
    """CFG basic block storing incoming edge indices and ordered definition site IDs."""

    predecessors: list[int]
    assignments: list[int]


def reaching(
    blocks: list[Block], variable_of: list[str]
) -> tuple[list[set[int]], list[set[int]]]:
    """Compute `(entry, exit_sets)` reaching definitions via forward may-analysis.

    Each integer ID represents one assignment site; `variable_of[id]` gives the
    assigned variable. Initializing sets to empty and taking unions across
    predecessors computes the least fixed point safely (no reaching write is missed).
    """
    entry: list[set[int]] = [set() for _ in blocks]
    exit_sets: list[set[int]] = [set() for _ in blocks]
    while True:
        changed = False
        for index, block in enumerate(blocks):
            # Confluence (may / union): combine definitions surviving out of any predecessor.
            incoming: set[int] = set()
            for parent in block.predecessors:
                incoming |= exit_sets[parent]
            entry[index] = incoming
            outgoing = set(incoming)
            # Sequential transfer: each assignment kills older definitions of `var` and adds itself.
            for def_id in block.assignments:
                var = variable_of[def_id]
                outgoing = {old for old in outgoing if variable_of[old] != var}
                outgoing.add(def_id)
            if outgoing != exit_sets[index]:
                changed = True
            exit_sets[index] = outgoing
        if not changed:
            return entry, exit_sets


def sample() -> list[Block]:
    """Diamond CFG: B0 sets x(0), y(1); B1 sets x(2); B2 sets x(3); B3 joins."""
    return [
        Block(predecessors=[], assignments=[0, 1]),
        Block(predecessors=[0], assignments=[2]),
        Block(predecessors=[0], assignments=[3]),
        Block(predecessors=[1, 2], assignments=[]),
    ]


def main() -> None:
    entry, exit_sets = reaching(sample(), ["x", "y", "x", "x"])
    for index, (e, x) in enumerate(zip(entry, exit_sets)):
        print(f"B{index}: reaching in {sorted(e)}, reaching out {sorted(x)}")


class ReachingDefsTests(unittest.TestCase):
    def test_joins_branch_definitions_and_kills_overwritten_assignments(self) -> None:
        entry, exit_sets = reaching(sample(), ["x", "y", "x", "x"])
        self.assertEqual(exit_sets[0], {0, 1})
        # B1 overwrites `x` (id 2), killing id 0 while keeping `y` (id 1).
        self.assertEqual(exit_sets[1], {1, 2})
        # Both arms reach join B3, so `entry[3]` contains {1, 2, 3}.
        self.assertEqual(entry[3], {1, 2, 3})
        # Loop back-edge B1 -> B1 carries definition 1 back into B1's entry set on pass 2.
        loop_blocks = [
            Block(predecessors=[], assignments=[0]),
            Block(predecessors=[0, 1], assignments=[1]),
        ]
        self.assertEqual(reaching(loop_blocks, ["x", "x"])[0][1], {0, 1})


if __name__ == "__main__":
    main()

