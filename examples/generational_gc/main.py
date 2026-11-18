"""Generational GC write barriers, incremental tri-color invariants, and reference-counting cycles."""

from dataclasses import dataclass
from enum import Enum
import unittest


class Gen(Enum):
    """Generational partition: short-lived nursery (`YOUNG`) vs tenured long-lived (`OLD`)."""

    YOUNG = "Young"
    OLD = "Old"


@dataclass
class GenObj:
    """Heap object tracking its generation, minor-GC survival age, and outgoing pointer slots."""

    tag: str
    gen: Gen
    # Incremented on each survived minor GC; promoted to OLD when age >= age_threshold.
    age: int
    fields: list[int | None]


class GenerationalHeap:
    """Two-generation heap with an optional `Old -> Young` write barrier and remembered set."""

    def __init__(self, age_threshold: int, use_write_barrier: bool) -> None:
        self.objects: dict[int, GenObj] = {}
        # Tracks `Old` objects that may hold pointers into `Young` so minor GC can skip scanning `Old`.
        self.remembered_set: set[int] = set()
        self.next_id = 1
        self.age_threshold = age_threshold
        self.use_write_barrier = use_write_barrier

    def alloc(self, tag: str, gen: Gen, arity: int) -> int:
        oid = self.next_id
        self.next_id += 1
        self.objects[oid] = GenObj(tag, gen, 0, [None] * arity)
        return oid

    def write_field(self, owner: int, slot: int, target: int | None) -> None:
        """Store `target` into `owner.fields[slot]`, logging `Old -> Young` stores in `remembered_set`."""
        self.objects[owner].fields[slot] = target
        if self.use_write_barrier and self.objects[owner].gen == Gen.OLD and target is not None:
            if self.objects[target].gen == Gen.YOUNG:
                self.remembered_set.add(owner)

    def minor_collect(self, roots: list[int]) -> tuple[int, int]:
        """Collect ONLY `YOUNG` objects; `OLD` objects are untouched unless in `remembered_set`."""
        # Seed worklist with `YOUNG` stack roots plus `YOUNG` targets referenced by `remembered_set`.
        worklist = [r for r in roots if r in self.objects and self.objects[r].gen == Gen.YOUNG]
        for rem in sorted(self.remembered_set):
            if rem in self.objects:
                for child in self.objects[rem].fields:
                    if child is not None and child in self.objects and self.objects[child].gen == Gen.YOUNG:
                        worklist.append(child)

        # Trace only within the young nursery.
        live_young: set[int] = set()
        while worklist:
            oid = worklist.pop()
            if oid in live_young:
                continue
            live_young.add(oid)
            for child in self.objects[oid].fields:
                if child is not None and child in self.objects and self.objects[child].gen == Gen.YOUNG:
                    worklist.append(child)

        # Sweep: delete dead `YOUNG` objects; `OLD` objects are never freed by a minor GC.
        young_before = sum(1 for o in self.objects.values() if o.gen == Gen.YOUNG)
        for oid in [k for k, o in self.objects.items() if o.gen == Gen.YOUNG and k not in live_young]:
            del self.objects[oid]

        # Age surviving young objects and promote those reaching `age_threshold`.
        promoted = 0
        for oid in sorted(live_young):
            obj = self.objects[oid]
            obj.age += 1
            if obj.age >= self.age_threshold:
                obj.gen = Gen.OLD
                promoted += 1

        # Refresh remembered_set: newly promoted OLD objects might point to YOUNG, and
        # previously remembered OLD objects may no longer have any YOUNG children.
        if self.use_write_barrier:
            self.remembered_set = {
                oid
                for oid, o in self.objects.items()
                if o.gen == Gen.OLD
                and any(c is not None and c in self.objects and self.objects[c].gen == Gen.YOUNG for c in o.fields)
            }
        else:
            self.remembered_set.clear()

        young_after = sum(1 for o in self.objects.values() if o.gen == Gen.YOUNG)
        swept = young_before - young_after - promoted
        return swept, promoted


class Color(Enum):
    """Tri-color marking states: WHITE (unvisited), GRAY (worklist), BLACK (scanned)."""

    WHITE = "White"
    GRAY = "Gray"
    BLACK = "Black"


class TriColorHeap:
    """Incremental marker demonstrating Dijkstra's insertion write barrier."""

    def __init__(self, nodes: list[int], edges: list[tuple[int, int]]) -> None:
        self.colors: dict[int, Color] = {n: Color.WHITE for n in nodes}
        self.edges: dict[int, list[int]] = {n: [] for n in nodes}
        for u, v in edges:
            self.edges[u].append(v)

    def step_mark(self) -> bool:
        """Advance marking by one node: gray its white neighbors, then darken it to BLACK."""
        grays = [k for k in sorted(self.colors) if self.colors[k] == Color.GRAY]
        if not grays:
            return False
        node = grays[0]
        for nbr in self.edges[node]:
            if self.colors[nbr] == Color.WHITE:
                self.colors[nbr] = Color.GRAY
        self.colors[node] = Color.BLACK
        return True

    def mutator_add_edge(self, frm: int, to: int) -> None:
        """Dijkstra insertion barrier: linking BLACK -> WHITE re-grays `to` to preserve reachability."""
        self.edges[frm].append(to)
        if self.colors[frm] == Color.BLACK and self.colors[to] == Color.WHITE:
            self.colors[to] = Color.GRAY

    def has_black_to_white_violation(self) -> bool:
        """Check strong tri-color invariant: no BLACK node may point directly to a WHITE node."""
        return any(
            self.colors[u] == Color.BLACK and self.colors[v] == Color.WHITE
            for u, vs in self.edges.items()
            for v in vs
        )


def naive_refcount_cycle_leak() -> int:
    """Demonstrate why naive reference counting leaks unreachable 2-cycles (`rc` stays 1)."""
    rc = [1, 1]  # external roots to node 0 and node 1
    rc[1] += 1  # edge 0 -> 1
    rc[0] += 1  # edge 1 -> 0
    rc[0] -= 1  # drop external root to 0
    rc[1] -= 1  # drop external root to 1
    return sum(1 for c in rc if c > 0)


def main() -> None:
    heap = GenerationalHeap(2, True)
    old_holder = heap.alloc("old_holder", Gen.OLD, 1)
    young_kept = heap.alloc("young_kept", Gen.Young if False else Gen.YOUNG, 0)
    heap.alloc("young_dead", Gen.YOUNG, 0)
    heap.write_field(old_holder, 0, young_kept)

    s1, p1 = heap.minor_collect([old_holder])
    print(f"Minor GC #1: swept={s1}, promoted={p1}, rem={sorted(heap.remembered_set)}")
    s2, p2 = heap.minor_collect([old_holder])
    print(f"Minor GC #2: swept={s2}, promoted={p2}, gen={heap.objects[young_kept].gen.value}")

    tri = TriColorHeap([0, 1, 2], [(0, 1)])
    tri.colors[0] = Color.GRAY
    tri.step_mark()
    tri.mutator_add_edge(0, 2)
    assert not tri.has_black_to_white_violation()
    while tri.step_mark():
        pass
    print(f"Tri-color final colors: { {k: c.value for k, c in tri.colors.items()} }")
    print(f"Naive RC leaked cycle nodes: {naive_refcount_cycle_leak()}")


class GenerationalGcTests(unittest.TestCase):
    def test_write_barrier_protects_old_to_young_while_skipping_gen1(self) -> None:
        unbarriered = GenerationalHeap(2, False)
        old_u = unbarriered.alloc("old", Gen.OLD, 1)
        young_u = unbarriered.alloc("young_lost", Gen.YOUNG, 0)
        unbarriered.write_field(old_u, 0, young_u)
        unbarriered.minor_collect([old_u])
        self.assertNotIn(young_u, unbarriered.objects)

        heap = GenerationalHeap(2, True)
        old_root = heap.alloc("old_root", Gen.OLD, 1)
        old_unref = heap.alloc("old_unref", Gen.OLD, 0)
        young_k = heap.alloc("young_kept", Gen.YOUNG, 0)
        young_dead = heap.alloc("young_dead", Gen.YOUNG, 0)
        heap.write_field(old_root, 0, young_k)

        self.assertEqual(heap.minor_collect([old_root]), (1, 0))
        self.assertNotIn(young_dead, heap.objects)
        self.assertIn(young_k, heap.objects)
        self.assertIn(old_unref, heap.objects)

        self.assertEqual(heap.minor_collect([old_root]), (0, 1))
        self.assertEqual(heap.objects[young_k].gen, Gen.OLD)
        self.assertEqual(heap.remembered_set, set())

    def test_tricolor_invariant_and_refcount_cycle_leak(self) -> None:
        tri = TriColorHeap([0, 1, 2], [(0, 1)])
        tri.colors[0] = Color.GRAY
        tri.step_mark()
        self.assertEqual(tri.colors[0], Color.BLACK)
        self.assertEqual(tri.colors[1], Color.GRAY)
        self.assertEqual(tri.colors[2], Color.WHITE)

        tri.mutator_add_edge(0, 2)
        self.assertFalse(tri.has_black_to_white_violation())
        self.assertEqual(tri.colors[2], Color.GRAY)
        while tri.step_mark():
            pass
        self.assertTrue(all(c == Color.BLACK for c in tri.colors.values()))
        self.assertEqual(naive_refcount_cycle_leak(), 2)


if __name__ == "__main__":
    main()
