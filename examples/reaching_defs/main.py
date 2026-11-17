"""Propagate assignment identities forward through branches and loops."""

from dataclasses import dataclass
import unittest


@dataclass(frozen=True)
class Block:
    predecessors: list[int]
    assignments: list[int]


def reaching(
    blocks: list[Block], variable_of: list[str]
) -> tuple[list[set[int]], list[set[int]]]:
    entry: list[set[int]] = [set() for _ in blocks]
    exit_sets: list[set[int]] = [set() for _ in blocks]
    while True:
        changed = False
        for index, block in enumerate(blocks):
            incoming: set[int] = set()
            for parent in block.predecessors:
                incoming |= exit_sets[parent]
            entry[index] = incoming
            outgoing = set(incoming)
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
        self.assertEqual(exit_sets[1], {1, 2})
        self.assertEqual(entry[3], {1, 2, 3})
        loop_blocks = [
            Block(predecessors=[], assignments=[0]),
            Block(predecessors=[0, 1], assignments=[1]),
        ]
        self.assertEqual(reaching(loop_blocks, ["x", "x"])[0][1], {0, 1})


if __name__ == "__main__":
    main()
