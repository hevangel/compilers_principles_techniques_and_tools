# Global Register Allocation by Graph Coloring

Build an interference graph from multi-block CFG liveness weighted by loop depth and color registers using Kempe's `simplify`/`select` heuristic. See the [Rust source](main.rs) and [Python source](main.py).

```sh
cargo run --example graph_color_regalloc
cargo test --example graph_color_regalloc
python3 examples/graph_color_regalloc/main.py
python3 -m unittest examples/graph_color_regalloc/main.py
```

## What the example teaches

### Why we need this

Basic-block code generation (`reg_desc_codegen`) optimizes one straight-line block at a time and flushes live variables to memory at every exit. In a real function with branches and loops, flushing and reloading at every block boundary means an inner-loop variable gets loaded from and stored to memory on **every loop iteration**, killing performance.

**Global register allocation** keeps variables in hardware registers across whole control-flow graphs (CFGs) by framing register assignment as **graph `K`-coloring**:
- Each variable is a vertex, and two vertices share an **interference edge** if they are live at the same program point (so they cannot occupy the same register).
- Each color in `0..K` represents one physical hardware register.

Because general graph `K`-coloring is NP-complete, **Kempe's simplify/select heuristic** gives a fast, practical solver. Crucially, when more than `K` variables interfere simultaneously (like the 4-clique `{a, b, c, outer_cold}` with `K = 3` registers in the demo), **some variable must spill to the stack**. Weighting each use/def by `10^loop_depth` ensures the allocator spills `outer_cold` (depth 0, spill cost `2`) instead of `a`, `b`, or `c` inside the depth-2 inner loop (spill costs `200`–`300`)!

### Kempe heuristic stages

Register allocation reduces to `K`-coloring the interference graph whose edges connect variables live at the same program point; weighting spill costs by `10^loop_depth` steers spills out of hot inner loops whenever degree-`K` nodes force a potential spill.

1. **Interference & cost:** Run backward CFG liveness, add edges `(d, l)` for every `l` in `live_out` at each definition, and weight each use/def by `10^depth`.
2. **Simplify:** Repeatedly remove any node of degree `< K` onto a stack; if all remaining nodes have degree `>= K`, pick the node minimizing `cost(v) / degree(v)` as a potential spill.
3. **Select:** Pop nodes off the stack, assigning the first color in `0..K` absent from already-colored neighbors or marking the variable as an actual spill.

In `main`'s 3-block CFG: `B0` (depth 0) defines `outer_cold`; `B1` (depth 2 loop) defines and uses `a`, `b`, `c` while `outer_cold` remains live across `B1` to `B2` (depth 0). All four nodes form a `K4` clique (each has degree 3). Because all nodes have degree `>= 3`, `kempe_color` picks `outer_cold` (minimum `cost / degree = 2 / 3`) as the potential spill, removes it so `a`, `b`, `c` drop to degree `< 3`, and during `Select` assigns `a, b, c` colors `0, 1, 2` before discovering `outer_cold` has no free color left—making `outer_cold` the sole actual spill.

### How to read the code

| Component (`main.rs` / `main.py`) | Role |
| --- | --- |
| `Instr`, `CfgBlock` | Models each block's `loop_depth`, instruction `def`/`uses`, and successor block indices `succs`. |
| `ColoringResult` | Holds register assignment `colors`, `spills`, `total_spill_cost`, and the `interference` adjacency map. |
| `build_interference_and_costs` | Computes loop-weighted spill costs, iterates backward CFG liveness (`live_in`/`live_out`) to a fixed point, and adds interference edges. |
| `kempe_color` | Runs Kempe's `Simplify` (push degree `< K` or lowest `cost/degree` potential spill) and `Select` (pop & color greedily) phases. |
| `verify_coloring` | Asserts no two interfering variables share the same register color. |

## What to look for

When `outer_cold` (defined/used only at loop depth 0) remains live across a depth-2 inner loop holding `a`, `b`, and `c`, all four form a `K4` clique; with `K = 3` registers, `kempe_color` pushes `outer_cold` first as the lowest `cost / degree` node so `a`, `b`, and `c` get hardware registers and `outer_cold` takes the stack slot.

## Check your understanding

- Why does optimistic coloring wait until the `Select` phase before declaring a potential spill an actual spill?
- Why should a copy `x = y` *avoid* adding an interference edge `(x, y)` unless `x` is rewritten differently later?

## Edition mapping and scope

| 1977 Green | 1986 Red | 2006 Purple |
| --- | --- | --- |
| 15 | 9 | 8 |

Numbers refer to chapters. Related readings and appendices are marked where applicable. See the [edition guide](../../README.md#edition-guide) for the bibliographic references and chapter coverage.

Copy coalescing and live-range splitting are omitted to highlight interference construction and cost-weighted `simplify`/`select` coloring.
