"""Trace a simulated heap from its roots, then reclaim unreachable slots."""

from dataclasses import dataclass
import unittest


@dataclass
class HeapObject:
    """Simulated heap object whose pointer fields are indices into the heap list."""

    references: list[int]


def collect(heap: list[HeapObject | None], roots: list[int]) -> list[int]:
    """Run one stop-the-world mark-sweep GC from `roots` and return reclaimed slot indices."""
    # Fresh mark bit per slot; every slot starts unmarked (presumed garbage).
    marked = [False] * len(heap)
    # Worklist initialized with external roots (e.g., stack-frame and register pointers).
    pending = list(roots)

    # Mark phase: traverse reachable subgraph via explicit stack.
    while pending:
        index = pending.pop()
        if not (0 <= index < len(heap)):
            continue
        obj = heap[index]
        if obj is None:
            continue
        # Invariant: mark *before* queuing children so cycles (e.g. 0 <-> 1) terminate.
        if not marked[index]:
            marked[index] = True
            pending.extend(obj.references)

    # Sweep phase: free every occupied slot that remained unmarked (including dead cycles).
    reclaimed: list[int] = []
    for index, (slot, live) in enumerate(zip(heap, marked)):
        if not live and slot is not None:
            heap[index] = None
            reclaimed.append(index)
    return reclaimed


def sample() -> list[HeapObject | None]:
    """Create 5 heap slots: cycle 0<->1, cycle 2<->3, and leaf 4."""
    return [HeapObject(refs) for refs in ([1], [0], [3], [2], [])]


def main() -> None:
    heap = sample()
    print(f"Reclaimed slots: {collect(heap, [0, 4])}")


class MarkSweepTests(unittest.TestCase):
    def test_preserves_rooted_cycle_and_reclaims_unreachable_cycle(self) -> None:
        heap = sample()
        self.assertEqual(collect(heap, [0, 4]), [2, 3])
        self.assertEqual(collect(heap, [0, 4]), [])
        self.assertEqual(collect(heap, []), [0, 1, 4])


if __name__ == "__main__":
    main()
