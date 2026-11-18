"""Solve backward liveness equations on a control-flow graph with a loop."""

from dataclasses import dataclass
import unittest


@dataclass(frozen=True)
class Block:
    """CFG basic block summarized by upward-exposed `uses`, local `defs`, and `successors`."""

    uses: frozenset[str]
    defs: frozenset[str]
    successors: list[int]


def liveness(blocks: list[Block]) -> tuple[list[set[str]], list[set[str]]]:
    """Compute `(live_in, live_out)` sets using backward may-analysis.

    A variable is live if *some* path from that point reads it before redefinition.
    Initializing all sets to empty (`set()`) and taking set unions across successors
    monotonically computes the least fixed point; under-approximating would be unsound.
    """
    live_in: list[set[str]] = [set() for _ in blocks]
    live_out: list[set[str]] = [set() for _ in blocks]
    while True:
        changed = False
        # Reverse order accelerates backward propagation from exits toward entry.
        for index in range(len(blocks) - 1, -1, -1):
            block = blocks[index]
            # Confluence (may / union): live-out combines everything needed by any successor.
            outgoing: set[str] = set()
            for succ in block.successors:
                outgoing |= live_in[succ]
            # Transfer: `IN[B] = USE[B] | (OUT[B] - DEF[B])`.
            # Local writes kill incoming demands, unless an earlier read in B uses the old value.
            incoming = set(block.uses) | (outgoing - block.defs)
            if incoming != live_in[index]:
                changed = True
            live_in[index] = incoming
            live_out[index] = outgoing
        if not changed:
            return live_in, live_out


def sample() -> list[Block]:
    """Build `B0: x = ...; B1: while (again) { B2: x = x + step } B3: return x`."""
    return [
        Block(uses=frozenset(), defs=frozenset({"x"}), successors=[1]),
        Block(uses=frozenset({"again"}), defs=frozenset(), successors=[2, 3]),
        # In B2 (`x = x + step`), `x` is read before being written, so it appears in both sets.
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
        # B0's write to `x` kills the demand for `x` coming out of B0, leaving `again` and `step`.
        self.assertEqual(entry[0], {"again", "step"})
        self.assertEqual(exit_sets[0], {"again", "step", "x"})
        # Back-edge B2 -> B1 carries `again` back into B2 on the second iteration.
        self.assertEqual(entry[2], {"again", "step", "x"})
        # Exit block B3 has no successors, so its live-out set is empty.
        self.assertEqual(exit_sets[3], set())


if __name__ == "__main__":
    main()

