"""Cheney breadth-first semispace copying GC and 3-pass sliding Mark-Compact GC."""

from dataclasses import dataclass, field
import unittest


@dataclass
class HeapObj:
    tag: str
    fields: list[int | None]
    marked: bool = False
    forwarded: int | None = None

    def clone_fresh(self) -> "HeapObj":
        return HeapObj(self.tag, list(self.fields), False, None)


class SemispaceHeap:
    def __init__(self, capacity: int) -> None:
        self.from_space: list[HeapObj | None] = [None] * capacity
        self.to_space: list[HeapObj | None] = [None] * capacity

    def alloc_in_from(self, tag: str, fields: list[int | None]) -> int:
        for i, slot in enumerate(self.from_space):
            if slot is None:
                self.from_space[i] = HeapObj(tag, list(fields))
                return i
        raise RuntimeError("from_space exhausted")

    def cheney_collect(self, roots: list[int]) -> tuple[int, int]:
        self.to_space = [None] * len(self.to_space)
        free = 0
        scan = 0
        copies = 0

        def copy_ref(old_idx: int) -> int:
            nonlocal free, copies
            slot = self.from_space[old_idx]
            assert slot is not None
            if slot.forwarded is not None:
                return slot.forwarded
            new_idx = free
            free += 1
            copies += 1
            slot.forwarded = new_idx
            self.to_space[new_idx] = slot.clone_fresh()
            return new_idx

        for i, r in enumerate(roots):
            roots[i] = copy_ref(r)

        while scan < free:
            obj = self.to_space[scan]
            assert obj is not None
            for fi, f in enumerate(obj.fields):
                if f is not None:
                    obj.fields[fi] = copy_ref(f)
            scan += 1

        self.from_space, self.to_space = self.to_space, [None] * len(self.to_space)
        return free, copies


@dataclass
class SlidingHeap:
    slots: list[HeapObj | None] = field(default_factory=list)

    def mark_compact(self, roots: list[int]) -> int:
        for obj in self.slots:
            if obj is not None:
                obj.marked = False
                obj.forwarded = None

        stack = list(roots)
        while stack:
            idx = stack.pop()
            obj = self.slots[idx]
            if obj is not None and not obj.marked:
                obj.marked = True
                for child in obj.fields:
                    if child is not None:
                        stack.append(child)

        free = 0
        for obj in self.slots:
            if obj is not None and obj.marked:
                obj.forwarded = free
                free += 1

        def new_addr(idx: int) -> int:
            obj = self.slots[idx]
            assert obj is not None and obj.forwarded is not None
            return obj.forwarded

        for i, r in enumerate(roots):
            roots[i] = new_addr(r)

        new_slots: list[HeapObj | None] = [None] * len(self.slots)
        for obj in self.slots:
            if obj is not None and obj.forwarded is not None:
                new_slots[obj.forwarded] = HeapObj(
                    obj.tag,
                    [new_addr(f) if f is not None else None for f in obj.fields],
                    False,
                    None,
                )
        self.slots = new_slots
        return free


def canonical_graph(
    slots: list[HeapObj | None], roots: list[int]
) -> list[tuple[str, list[int | None]]]:
    visited: dict[int, int] = {}
    queue: list[int] = []
    for r in roots:
        if r not in visited:
            visited[r] = len(visited)
            queue.append(r)
    head = 0
    result: list[tuple[str, list[int | None]]] = []
    while head < len(queue):
        cur = queue[head]
        head += 1
        obj = slots[cur]
        assert obj is not None
        children: list[int | None] = []
        for c in obj.fields:
            if c is None:
                children.append(None)
            else:
                if c not in visited:
                    visited[c] = len(visited)
                    queue.append(c)
                children.append(visited[c])
        result.append((obj.tag, children))
    return result


def build_diamond_and_cycle_sample() -> tuple[SemispaceHeap, SlidingHeap, list[int]]:
    semi = SemispaceHeap(10)
    semi.alloc_in_from("dead_a", [None])
    semi.alloc_in_from("dead_b", [None])
    shared = semi.alloc_in_from("shared", [None])
    d_left = semi.alloc_in_from("d_left", [shared])
    d_right = semi.alloc_in_from("d_right", [shared])
    root_diamond = semi.alloc_in_from("diamond", [d_left, d_right])
    cyc_a = semi.alloc_in_from("cyc_a", [None])
    cyc_b = semi.alloc_in_from("cyc_b", [cyc_a])
    assert semi.from_space[cyc_a] is not None
    semi.from_space[cyc_a].fields[0] = cyc_b
    dead_x = semi.alloc_in_from("dead_x", [None])
    dead_y = semi.alloc_in_from("dead_y", [dead_x])
    assert semi.from_space[dead_x] is not None
    semi.from_space[dead_x].fields[0] = dead_y

    sliding = SlidingHeap([o.clone_fresh() if o is not None else None for o in semi.from_space])
    return semi, sliding, [root_diamond, cyc_a]


def main() -> None:
    semi, sliding, roots_semi = build_diamond_and_cycle_sample()
    roots_slide = list(roots_semi)
    before = canonical_graph(semi.from_space, roots_semi)
    live_semi, copies = semi.cheney_collect(roots_semi)
    after_semi = canonical_graph(semi.from_space, roots_semi)
    live_slide = sliding.mark_compact(roots_slide)
    after_slide = canonical_graph(sliding.slots, roots_slide)
    assert before == after_semi == after_slide
    print(f"Cheney live={live_semi}, objects copied once={copies}, roots={roots_semi}")
    print(f"Mark-Compact live={live_slide}, roots={roots_slide}")


class CopyingGcTests(unittest.TestCase):
    def test_cheney_copies_shared_diamond_once_and_handles_cycles(self) -> None:
        semi, _, roots = build_diamond_and_cycle_sample()
        before = canonical_graph(semi.from_space, roots)
        live, copies = semi.cheney_collect(roots)
        self.assertEqual((live, copies), (6, 6))
        self.assertEqual(before, canonical_graph(semi.from_space, roots))
        self.assertTrue(all(s is not None for s in semi.from_space[:live]))
        self.assertTrue(all(s is None for s in semi.from_space[live:]))
        d = semi.from_space[roots[0]]
        assert d is not None and d.fields[0] is not None and d.fields[1] is not None
        left_obj = semi.from_space[d.fields[0]]
        right_obj = semi.from_space[d.fields[1]]
        assert left_obj is not None and right_obj is not None
        self.assertEqual(left_obj.fields[0], right_obj.fields[0])

    def test_mark_compact_slides_live_objects_preserving_graph_isomorphism(self) -> None:
        _, sliding, roots = build_diamond_and_cycle_sample()
        before = canonical_graph(sliding.slots, roots)
        live = sliding.mark_compact(roots)
        self.assertEqual(live, 6)
        self.assertEqual(before, canonical_graph(sliding.slots, roots))
        self.assertTrue(all(s is not None for s in sliding.slots[:live]))
        self.assertTrue(all(s is None for s in sliding.slots[live:]))


if __name__ == "__main__":
    main()
