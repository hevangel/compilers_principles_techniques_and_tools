# Dominators and natural loops

Find loop headers using dominance and collect each natural loop’s blocks. See the [Rust source](main.rs).

```sh
cargo run --example natural_loops
cargo test --example natural_loops
```

## What the example teaches

A block dominates another when every entry path to the latter passes through the former. `dominators` computes this relation with predecessor intersections. For a back edge whose target dominates its source, `natural_loop` walks predecessors up to the header.

In a control-flow graph with one entry, block `d` **dominates** block `b` when every entry-to-`b) path passes through `d`. Every reachable block dominates itself. An edge `tail → header` is a **back edge** in this example when the header dominates the tail. That relation makes the header a required point of entry into the loop.

### Compute dominance, then find the loop

1. `predecessors` reverses the edge lists so the algorithm can ask which blocks can enter a given block.
2. `dominators` initializes the entry set to `{entry}` and all other sets to every node. At each non-entry block, a dominator other than the block itself must dominate **every** predecessor. The algorithm intersects their sets, adds the block, and repeats until the sets stop shrinking.
3. `find_loops` checks each edge against the dominance sets. For a back edge, `natural_loop` walks backward from the tail and stops expanding paths at the header. Marking the header as visited before the walk keeps outside predecessors of the header out of the loop.

### Follow the sample graph

```text
B0 → B1 → B2 → B3
     ↑         │
     └─────────┘
     B1 → B4
```

Every path to B3 has already passed B1, so B3 → B1 closes a natural loop. The backward walk collects B3 and B2 and stops at B1, giving `{1, 2, 3}`. B0 enters the header from outside, and B4 exits the loop.

## What to look for

The sample finds `B3 → B1` with body `{1, 2, 3}`. The test excludes the preheader and checks a self-loop and an acyclic graph.

## Check your understanding

- Why is B0 excluded even though it dominates the header?
- Imagine adding an edge directly from B0 to B3. Can B1 still dominate B3? What does that imply for B3 → B1?

## Edition mapping and scope

| 1977 Green | 1986 Red | 2006 Purple |
| --- | --- | --- |
| 13 | 10 | 9 |

Numbers refer to chapters. Related readings and appendices are marked where applicable. See the [edition guide](../../README.md#edition-guide) for the bibliographic references and chapter coverage.

The graph is nonempty and every block is reachable from entry. Each result describes one back edge.
