# Global Register Allocation by Graph Coloring

Build an interference graph from multi-block CFG liveness weighted by loop depth and color registers using Kempe's `simplify`/`select` heuristic. See the [Rust source](main.rs) and [Python source](main.py).

```sh
cargo run --example graph_color_regalloc
cargo test --example graph_color_regalloc
python3 examples/graph_color_regalloc/main.py
python3 -m unittest examples/graph_color_regalloc/main.py
```

## What the example teaches

Register allocation reduces to $K$-coloring the interference graph whose edges connect variables live at the same program point; weighting spill costs by $10^{\text{loop\_depth}}$ steers spills out of hot inner loops whenever degree-$K$ nodes force a potential spill.

### Kempe heuristic stages

1. **Interference & cost:** Run backward CFG liveness, add edges `(d, l)` for every `l` in `live_out` at each definition, and weight each use/def by $10^{\text{depth}}$.
2. **Simplify:** Repeatedly remove any node of degree `< K` onto a stack; if all remaining nodes have degree `>= K`, pick the node minimizing `cost(v) / degree(v)` as a potential spill.
3. **Select:** Pop nodes off the stack, assigning the first color in `0..K` absent from already-colored neighbors or marking the variable as an actual spill.

## What to look for

When `outer_cold` (defined/used only at loop depth 0) remains live across a depth-2 inner loop holding `a`, `b`, and `c`, all four form a $K_4$ clique; with $K = 3$ registers, `kempe_color` pushes `outer_cold` first as the lowest `cost / degree` node so `a`, `b`, and `c` get hardware registers and `outer_cold` takes the stack slot.

## Check your understanding

- Why does optimistic coloring wait until the `Select` phase before declaring a potential spill an actual spill?
- Why should a copy `x = y` *avoid* adding an interference edge `(x, y)` unless `x` is rewritten differently later?

## Edition mapping and scope

| 1977 Green | 1986 Red | 2006 Purple |
| --- | --- | --- |
| 15 | 9 | 8 |

Numbers refer to chapters. Related readings and appendices are marked where applicable. See the [edition guide](../../README.md#edition-guide) for the bibliographic references and chapter coverage.

Copy coalescing and live-range splitting are omitted to highlight interference construction and cost-weighted `simplify`/`select` coloring.
