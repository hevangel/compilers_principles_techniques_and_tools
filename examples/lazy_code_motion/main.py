"""Lazy Code Motion (Partial Redundancy Elimination) with critical-edge splitting and 4 data-flow passes."""

from dataclasses import dataclass
import unittest


@dataclass
class LcmBlock:
    name: str
    e_use: bool
    e_kill: bool
    succs: list[int]


@dataclass
class LcmSolution:
    blocks: list[LcmBlock]
    anticipated_in: list[bool]
    available_in: list[bool]
    earliest: list[bool]
    postponable_in: list[bool]
    latest: list[bool]
    used_out: list[bool]
    insert_at_entry: list[bool]
    replace_use: list[bool]


def split_critical_edges(blocks: list[LcmBlock]) -> list[LcmBlock]:
    n = len(blocks)
    preds = [0] * n
    for blk in blocks:
        for s in blk.succs:
            preds[s] += 1
    out = [LcmBlock(b.name, b.e_use, b.e_kill, list(b.succs)) for b in blocks]
    for b in range(n):
        if len(out[b].succs) > 1:
            for slot, s in enumerate(list(out[b].succs)):
                if preds[s] > 1:
                    split_id = len(out)
                    out.append(LcmBlock(f"split_{b}_{s}", False, False, [s]))
                    out[b].succs[slot] = split_id
    return out


def solve_lcm(raw_blocks: list[LcmBlock], entry: int, exit_blk: int) -> LcmSolution:
    blocks = split_critical_edges(raw_blocks)
    n = len(blocks)
    preds: list[list[int]] = [[] for _ in range(n)]
    for b, blk in enumerate(blocks):
        for s in blk.succs:
            preds[s].append(b)

    ant_in = [b != exit_blk for b in range(n)]
    ant_out = [False] * n
    while True:
        changed = False
        for b in range(n - 1, -1, -1):
            if b == exit_blk:
                continue
            new_out = bool(blocks[b].succs) and all(ant_in[s] for s in blocks[b].succs)
            new_in = blocks[b].e_use or (new_out and not blocks[b].e_kill)
            if new_out != ant_out[b] or new_in != ant_in[b]:
                ant_out[b] = new_out
                ant_in[b] = new_in
                changed = True
        if not changed:
            break

    avail_in = [False] * n
    avail_out = [b != entry for b in range(n)]
    while True:
        changed = False
        for b in range(n):
            new_in = False if b == entry or not preds[b] else all(avail_out[p] for p in preds[b])
            new_out = (ant_in[b] or new_in) and not blocks[b].e_kill
            if new_in != avail_in[b] or new_out != avail_out[b]:
                avail_in[b] = new_in
                avail_out[b] = new_out
                changed = True
        if not changed:
            break

    earliest = [ant_in[b] and not avail_in[b] for b in range(n)]

    post_in = [False] * n
    post_out = [b != entry for b in range(n)]
    while True:
        changed = False
        for b in range(n):
            new_in = False if b == entry or not preds[b] else all(post_out[p] for p in preds[b])
            new_out = (earliest[b] or new_in) and not blocks[b].e_use
            if new_in != post_in[b] or new_out != post_out[b]:
                post_in[b] = new_in
                post_out[b] = new_out
                changed = True
        if not changed:
            break

    latest = [
        False
        if b == exit_blk or not blocks[b].succs
        else (earliest[b] or post_in[b])
        and (blocks[b].e_use or not all(earliest[s] or post_in[s] for s in blocks[b].succs))
        for b in range(n)
    ]

    used_in = [False] * n
    used_out = [False] * n
    while True:
        changed = False
        for b in range(n - 1, -1, -1):
            if b == exit_blk:
                continue
            new_out = any(used_in[s] for s in blocks[b].succs)
            new_in = (blocks[b].e_use or new_out) and not latest[b]
            if new_out != used_out[b] or new_in != used_in[b]:
                used_out[b] = new_out
                used_in[b] = new_in
                changed = True
        if not changed:
            break

    insert_at_entry = [latest[b] and used_out[b] for b in range(n)]
    replace_use = [blocks[b].e_use and (not latest[b] or used_out[b]) for b in range(n)]
    return LcmSolution(
        blocks, ant_in, avail_in, earliest, post_in, latest, used_out, insert_at_entry, replace_use
    )


def count_evals_along_path(
    raw_blocks: list[LcmBlock], sol: LcmSolution, path: list[int]
) -> tuple[int, int]:
    before = sum(1 for b in path if raw_blocks[b].e_use)
    split_path = [path[0]]
    for u, v in zip(path, path[1:]):
        if v in sol.blocks[u].succs:
            split_path.append(v)
        else:
            bridge = next(s for s in sol.blocks[u].succs if sol.blocks[s].succs == [v])
            split_path.extend([bridge, v])
    after = sum(
        (1 if sol.insert_at_entry[b] else 0)
        + (1 if sol.blocks[b].e_use and not sol.replace_use[b] else 0)
        for b in split_path
    )
    return before, after


def partial_diamond_sample() -> list[LcmBlock]:
    return [
        LcmBlock("entry", False, False, [1, 2]),
        LcmBlock("hot", True, False, [2]),
        LcmBlock("join", True, False, [3]),
        LcmBlock("exit", False, False, []),
    ]


def main() -> None:
    diamond = partial_diamond_sample()
    sol = solve_lcm(diamond, 0, 3)
    for b, blk in enumerate(sol.blocks):
        print(
            f"B{b} ({blk.name}): latest={sol.latest[b]}, used_out={sol.used_out[b]}, "
            f"insert={sol.insert_at_entry[b]}, replace={sol.replace_use[b]}"
        )
    print(f"Hot path evals before/after: {count_evals_along_path(diamond, sol, [0, 1, 2, 3])}")


class LazyCodeMotionTests(unittest.TestCase):
    def test_partial_diamond_splits_critical_edge_and_eliminates_double_eval(self) -> None:
        diamond = partial_diamond_sample()
        sol = solve_lcm(diamond, 0, 3)
        self.assertEqual(len(sol.blocks), 5)
        self.assertEqual(sol.blocks[4].name, "split_0_2")
        self.assertTrue(sol.insert_at_entry[4])
        self.assertTrue(sol.insert_at_entry[1])
        self.assertFalse(sol.insert_at_entry[2])
        self.assertTrue(sol.replace_use[2])
        self.assertEqual(count_evals_along_path(diamond, sol, [0, 1, 2, 3]), (2, 1))
        self.assertEqual(count_evals_along_path(diamond, sol, [0, 2, 3]), (1, 1))

    def test_loop_invariant_hoisted_only_when_anticipated(self) -> None:
        while_loop = [
            LcmBlock("preheader", False, False, [1]),
            LcmBlock("header", False, False, [2, 3]),
            LcmBlock("body", True, False, [1]),
            LcmBlock("exit", False, False, []),
        ]
        sol = solve_lcm(while_loop, 0, 3)
        self.assertFalse(sol.insert_at_entry[0])

        do_while = [
            LcmBlock("preheader", False, False, [1]),
            LcmBlock("body", True, False, [1, 2]),
            LcmBlock("exit", False, False, []),
        ]
        dw_sol = solve_lcm(do_while, 0, 2)
        self.assertTrue(dw_sol.insert_at_entry[0])
        self.assertFalse(dw_sol.insert_at_entry[1])
        self.assertTrue(dw_sol.replace_use[1])
        self.assertEqual(count_evals_along_path(do_while, dw_sol, [0, 1, 1, 1, 2]), (3, 1))


if __name__ == "__main__":
    main()
