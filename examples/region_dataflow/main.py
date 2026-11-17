"""Construct T1/T2 reducible region trees and compose affine (GEN, KILL) transfer functions."""

from dataclasses import dataclass
import unittest


@dataclass(frozen=True)
class TransferFn:
    gen: int
    kill: int

    def apply(self, x: int) -> int:
        return self.gen | (x & ~self.kill)

    @staticmethod
    def compose(second: "TransferFn", first: "TransferFn") -> "TransferFn":
        return TransferFn(second.gen | (first.gen & ~second.kill), second.kill | first.kill)

    @staticmethod
    def union_meet(a: "TransferFn", b: "TransferFn") -> "TransferFn":
        return TransferFn(a.gen | b.gen, a.kill & b.kill)

    def star(self) -> "TransferFn":
        return TransferFn(self.gen, 0)


def reduce_t1_t2(
    num_blocks: int, edges: list[tuple[int, int]], block_tfs: list[TransferFn]
) -> tuple[list[int], int]:
    regions = set(range(num_blocks))
    out_edges: dict[int, dict[int, TransferFn]] = {b: {} for b in range(num_blocks)}
    in_preds: dict[int, set[int]] = {b: set() for b in range(num_blocks)}
    for u, v in edges:
        out_edges[u][v] = block_tfs[u]
        in_preds[v].add(u)

    steps: list[tuple] = []
    next_id = num_blocks
    entry = 0

    while not (len(regions) == 1 and not out_edges[entry]):
        t1_node = next((u for u in sorted(regions) if u in out_edges[u]), None)
        if t1_node is not None:
            u = t1_node
            self_tf = out_edges[u].pop(u)
            in_preds[u].discard(u)
            closure = self_tf.star()
            regions.remove(u)
            new_id = next_id
            next_id += 1
            regions.add(new_id)

            old_out = out_edges.pop(u)
            new_out: dict[int, TransferFn] = {}
            for w, tf in old_out.items():
                in_preds[w].discard(u)
                in_preds[w].add(new_id)
                new_out[w] = TransferFn.compose(tf, closure)
            out_edges[new_id] = new_out

            preds_u = list(in_preds.pop(u))
            in_preds[new_id] = set(preds_u)
            for p in preds_u:
                out_edges[p][new_id] = out_edges[p].pop(u)
            if entry == u:
                entry = new_id
            steps.append(("T1", new_id, u, self_tf))
            continue

        t2_pair = None
        for v in sorted(regions):
            if v == entry:
                continue
            ps = in_preds[v]
            if len(ps) == 1:
                u = next(iter(ps))
                if u != v:
                    t2_pair = (u, v)
                    break

        if t2_pair is not None:
            u, v = t2_pair
            u_to_v = out_edges[u].pop(v)
            in_preds.pop(v)
            regions.remove(u)
            regions.remove(v)
            new_id = next_id
            next_id += 1
            regions.add(new_id)

            v_out = out_edges.pop(v)
            u_out = out_edges.pop(u)
            new_out = {}
            for w, tf_uw in u_out.items():
                target = new_id if w in (u, v) else w
                if target != new_id:
                    in_preds[w].discard(u)
                    in_preds[w].add(new_id)
                new_out[target] = (
                    TransferFn.union_meet(new_out[target], tf_uw) if target in new_out else tf_uw
                )
            for w, tf_vw in v_out.items():
                composed = TransferFn.compose(tf_vw, u_to_v)
                target = new_id if w in (u, v) else w
                if target != new_id:
                    in_preds[w].discard(v)
                    in_preds[w].add(new_id)
                new_out[target] = (
                    TransferFn.union_meet(new_out[target], composed)
                    if target in new_out
                    else composed
                )

            new_preds: set[int] = set()
            for p in in_preds.pop(u):
                if p in (u, v):
                    continue
                if u in out_edges[p]:
                    tf_pu = out_edges[p].pop(u)
                    out_edges[p][new_id] = (
                        TransferFn.union_meet(out_edges[p][new_id], tf_pu)
                        if new_id in out_edges[p]
                        else tf_pu
                    )
                    new_preds.add(p)
            if new_id in new_out:
                new_preds.add(new_id)
            in_preds[new_id] = new_preds
            out_edges[new_id] = new_out
            if entry == u:
                entry = new_id
            steps.append(("T2", new_id, u, v, u_to_v))
            continue

        raise ValueError(f"Irreducible CFG: remaining={sorted(regions)}")

    region_in: dict[int, int] = {entry: 0}
    for step in reversed(steps):
        if step[0] == "T1":
            _, new_id, old_id, self_tf = step
            region_in[old_id] = self_tf.star().apply(region_in[new_id])
        else:
            _, new_id, u, v, u_to_v = step
            parent_in = region_in[new_id]
            region_in[u] = parent_in
            region_in[v] = u_to_v.apply(parent_in)
    return [region_in[b] for b in range(num_blocks)], len(steps)


def iterative_mfp_reaching_defs(
    num_blocks: int, edges: list[tuple[int, int]], block_tfs: list[TransferFn]
) -> list[int]:
    preds: list[list[int]] = [[] for _ in range(num_blocks)]
    for u, v in edges:
        preds[v].append(u)
    in_b = [0] * num_blocks
    out_b = [tf.apply(0) for tf in block_tfs]
    while True:
        changed = False
        for b in range(num_blocks):
            new_in = 0
            for p in preds[b]:
                new_in |= out_b[p]
            new_out = block_tfs[b].apply(new_in)
            if new_in != in_b[b] or new_out != out_b[b]:
                in_b[b] = new_in
                out_b[b] = new_out
                changed = True
        if not changed:
            break
    return in_b


def multi_loop_sample() -> tuple[int, list[tuple[int, int]], list[TransferFn]]:
    edges = [(0, 1), (1, 2), (1, 5), (2, 3), (2, 4), (3, 2), (4, 1)]
    tfs = [
        TransferFn(0b00001, 0b00000),
        TransferFn(0b00010, 0b00001),
        TransferFn(0b00100, 0b00000),
        TransferFn(0b01000, 0b00010),
        TransferFn(0b10000, 0b00100),
        TransferFn(0, 0),
    ]
    return 6, edges, tfs


def main() -> None:
    n, edges, tfs = multi_loop_sample()
    in_blocks, steps = reduce_t1_t2(n, edges, tfs)
    mfp = iterative_mfp_reaching_defs(n, edges, tfs)
    assert in_blocks == mfp
    print(f"T1/T2 reduction steps={steps}, matches MFP={[bin(x) for x in in_blocks]}")


class RegionDataflowTests(unittest.TestCase):
    def test_region_based_matches_iterative_mfp_on_nested_loops(self) -> None:
        n, edges, tfs = multi_loop_sample()
        in_blocks, _ = reduce_t1_t2(n, edges, tfs)
        self.assertEqual(in_blocks, iterative_mfp_reaching_defs(n, edges, tfs))

    def test_detects_irreducible_double_entry_loop(self) -> None:
        with self.assertRaises(ValueError):
            reduce_t1_t2(3, [(0, 1), (0, 2), (1, 2), (2, 1)], [TransferFn(0, 0)] * 3)


if __name__ == "__main__":
    main()
