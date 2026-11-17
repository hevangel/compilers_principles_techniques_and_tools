"""Solve backward liveness equations on a control-flow graph with a loop."""

from dataclasses import dataclass
import unittest


@dataclass(frozen=True)
class Block:
    uses: frozenset[str]
    defs: frozenset[str]
    successors: list[int]


def liveness(blocks: list[Block]) -> tuple[list[set[str]], list[set[str]]]:
    live_in: list[set[str]] = [set() for _ in blocks]
    live_out: list[set[str]] = [set() for _ in blocks]
    while True:
        changed = False
        for index in range(len(blocks) - 1, -1, -1):
            block = blocks[index]
            outgoing: set[str] = set()
            for succ in block.successors:
                outgoing |= live_in[succ]
            incoming = set(block.uses) | (outgoing - block.defs)
            if incoming != live_in[index]:
                changed = True
            live_in[index] = incoming
            live_out[index] = outgoing
        if not changed:
            return live_in, live_out


def sample() -> list[Block]:
    return [
        Block(uses=frozenset(), defs=frozenset({"x"}), successors=[1]),
        Block(uses=frozenset({"again"}), defs=frozenset(), successors=[2, 3]),
        Block(uses=frozenset({"x", "step"}), defs=frozenset({"x"}), successors=[1]),
        Block(uses=frozenset({"x"}), defs=frozenset(), successors=[]),
    ]


def main() -> None:
    live_in, live_out = liveness(sample())
    for index, (entry, exit_set) in enumerate(zip(live_in, live_out)):
        print(f"B{index}: in {sorted(entry)}, out {sorted(exit_set)}")


class LivenessTests(unittest.TestCase):
    def test_propagates_around_loop_and_kills_entry_definition(self) -> None:
        entry, exit_sets = liveness(sample())
        self.assertEqual(entry[0], {"again", "step"})
        self.assertEqual(exit_sets[0], {"again", "step", "x"})
        self.assertEqual(entry[2], {"again", "step", "x"})
        self.assertEqual(exit_sets[3], set())


if __name__ == "__main__":
    main()
