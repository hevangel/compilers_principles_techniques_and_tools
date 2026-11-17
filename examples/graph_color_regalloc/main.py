"""Global register allocation via interference graph construction and Kempe simplify/select coloring."""

from dataclasses import dataclass
import unittest


@dataclass(frozen=True)
class Instr:
    defn: str | None
    uses: list[str]


@dataclass(frozen=True)
class CfgBlock:
    loop_depth: int
    instrs: list[Instr]
    succs: list[int]


@dataclass
class ColoringResult:
    colors: dict[str, int]
    spills: set[str]
    total_spill_cost: int
    interference: dict[str, set[str]]


def build_interference_and_costs(
    blocks: list[CfgBlock],
) -> tuple[dict[str, set[str]], dict[str, int]]:
    n = len(blocks)
    use_blk: list[set[str]] = [set() for _ in range(n)]
    def_blk: list[set[str]] = [set() for _ in range(n)]
    all_vars: set[str] = set()
    costs: dict[str, int] = {}

    for b, blk in enumerate(blocks):
        weight = 10 ** blk.loop_depth
        for ins in blk.instrs:
            for u in ins.uses:
                all_vars.add(u)
                costs[u] = costs.get(u, 0) + weight
                if u not in def_blk[b]:
                    use_blk[b].add(u)
            if ins.defn is not None:
                all_vars.add(ins.defn)
                costs[ins.defn] = costs.get(ins.defn, 0) + weight
                def_blk[b].add(ins.defn)

    live_in: list[set[str]] = [set() for _ in range(n)]
    live_out: list[set[str]] = [set() for _ in range(n)]
    while True:
        changed = False
        for b in range(n - 1, -1, -1):
            new_out: set[str] = set()
            for s in blocks[b].succs:
                new_out |= live_in[s]
            new_in = use_blk[b] | (new_out - def_blk[b])
            if new_out != live_out[b] or new_in != live_in[b]:
                live_out[b] = new_out
                live_in[b] = new_in
                changed = True
        if not changed:
            break

    interference: dict[str, set[str]] = {v: set() for v in sorted(all_vars)}

    def add_edge(u: str, v: str) -> None:
        if u != v:
            interference[u].add(v)
            interference[v].add(u)

    for b, blk in enumerate(blocks):
        live = set(live_out[b])
        exit_list = sorted(live)
        for i, u in enumerate(exit_list):
            for v in exit_list[i + 1 :]:
                add_edge(u, v)
        for ins in reversed(blk.instrs):
            if ins.defn is not None:
                for l in live:
                    add_edge(ins.defn, l)
                live.discard(ins.defn)
            for u in ins.uses:
                live.add(u)
            cur_list = sorted(live)
            for i, u in enumerate(cur_list):
                for v in cur_list[i + 1 :]:
                    add_edge(u, v)
    return interference, costs


def kempe_color(
    interference: dict[str, set[str]], costs: dict[str, int], k: int
) -> ColoringResult:
    active: dict[str, set[str]] = {u: set(vs) for u, vs in interference.items()}
    stack: list[str] = []

    while active:
        trivial = next((v for v in sorted(active) if len(active[v]) < k), None)
        if trivial is not None:
            chosen = trivial
        else:
            chosen = min(
                sorted(active),
                key=lambda v: (costs.get(v, 1) / max(len(active[v]), 1), v),
            )
        nbrs = active.pop(chosen)
        for nbr in nbrs:
            if nbr in active:
                active[nbr].discard(chosen)
        stack.append(chosen)

    colors: dict[str, int] = {}
    spills: set[str] = set()
    total_spill_cost = 0

    while stack:
        v = stack.pop()
        used = {colors[n] for n in interference[v] if n in colors}
        avail = next((c for c in range(k) if c not in used), None)
        if avail is not None:
            colors[v] = avail
        else:
            spills.add(v)
            total_spill_cost += costs.get(v, 1)

    return ColoringResult(colors, spills, total_spill_cost, interference)


def verify_coloring(res: ColoringResult) -> None:
    for u, nbrs in res.interference.items():
        if u in res.colors:
            for v in nbrs:
                if v in res.colors:
                    assert res.colors[u] != res.colors[v]


def sample_cfg() -> list[CfgBlock]:
    return [
        CfgBlock(0, [Instr("outer_cold", [])], [1]),
        CfgBlock(
            2,
            [
                Instr("a", []),
                Instr("b", ["a"]),
                Instr("c", ["a", "b"]),
                Instr(None, ["a", "b", "c"]),
            ],
            [1, 2],
        ),
        CfgBlock(0, [Instr(None, ["c", "outer_cold"])], []),
    ]


def main() -> None:
    ig, costs = build_interference_and_costs(sample_cfg())
    res = kempe_color(ig, costs, 3)
    verify_coloring(res)
    print(f"Weighted spill costs: {costs}")
    print(f"K=3 coloring: {res.colors}, spills={sorted(res.spills)}, cost={res.total_spill_cost}")


class GraphColorRegallocTests(unittest.TestCase):
    def test_colors_planar_k3_graph_without_spills(self) -> None:
        ig = {
            "a": {"b", "c"},
            "b": {"a", "c", "d"},
            "c": {"a", "b", "d"},
            "d": {"b", "c"},
        }
        costs = {"a": 10, "b": 10, "c": 10, "d": 10}
        res = kempe_color(ig, costs, 3)
        verify_coloring(res)
        self.assertEqual(res.spills, set())
        self.assertEqual(len(res.colors), 4)

    def test_spills_lowest_cost_outer_variable_on_k4_clique(self) -> None:
        ig, costs = build_interference_and_costs(sample_cfg())
        self.assertEqual(len(ig["outer_cold"]), 3)
        res = kempe_color(ig, costs, 3)
        verify_coloring(res)
        self.assertEqual(res.spills, {"outer_cold"})
        self.assertEqual(len(res.colors), 3)


if __name__ == "__main__":
    main()
