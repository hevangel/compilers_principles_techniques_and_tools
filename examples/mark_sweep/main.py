"""Trace a simulated heap from its roots, then reclaim unreachable slots."""

from dataclasses import dataclass
import unittest


@dataclass
class HeapObject:
    references: list[int]


def collect(heap: list[HeapObject | None], roots: list[int]) -> list[int]:
    marked = [False] * len(heap)
    pending = list(roots)
    while pending:
        index = pending.pop()
        if not (0 <= index < len(heap)):
            continue
        obj = heap[index]
        if obj is None:
            continue
        if not marked[index]:
            marked[index] = True
            pending.extend(obj.references)
    reclaimed: list[int] = []
    for index, (slot, live) in enumerate(zip(heap, marked)):
        if not live and slot is not None:
            heap[index] = None
            reclaimed.append(index)
    return reclaimed


def sample() -> list[HeapObject | None]:
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
