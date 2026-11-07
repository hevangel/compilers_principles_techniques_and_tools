# Dominators and natural loops

Find loop headers using dominance and collect each natural loop’s blocks. See the [Rust source](main.rs).

```sh
cargo run --example natural_loops
cargo test --example natural_loops
```

## What the example teaches

A block dominates another when every entry path to the latter passes through the former. `dominators` computes this relation with predecessor intersections. For a back edge whose target dominates its source, `natural_loop` walks predecessors up to the header.

## What to look for

The sample finds `B3 → B1` with body `{1, 2, 3}`. The test excludes the preheader and checks a self-loop and an acyclic graph.

## Edition mapping and scope

| 1977 Green | 1986 Red | 2006 Purple |
| --- | --- | --- |
| 13 | 10 | 9 |

Numbers refer to chapters. Related readings and appendices are marked where applicable. See the [edition guide](../../README.md#edition-guide) for the bibliographic references and chapter coverage.

The graph is nonempty and every block is reachable from entry. Each result describes one back edge.
