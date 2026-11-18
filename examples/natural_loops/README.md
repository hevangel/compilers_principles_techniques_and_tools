# Dominators and natural loops

Find loop headers using dominance and collect each natural loop’s blocks. See the [Rust source](main.rs) and [Python source](main.py).

```sh
cargo run --example natural_loops
cargo test --example natural_loops
python3 examples/natural_loops/main.py
python3 -m unittest examples/natural_loops/main.py
```

## What the example teaches

### Why we need this and when it is sound

Compilers spend most of their optimization budget on loops (code motion, strength reduction, unrolling, vectorization), yet source-level `for`/`while` structure disappears once lowered to basic blocks and `goto` branches—and `goto` can even create tangled multi-entry cycles.

To hoist loop-invariant code safely into a **preheader** placed just before a loop's entry, the compiler needs a **single-entry loop** whose **header** dominates every node in the loop. Why is dominance the exact soundness requirement?

- Block `d` **dominates** block `b` (`d dom b`) when every path from the CFG entry `B0` to `b` passes through `d`. Every reachable block dominates itself.
- An edge `tail -> header` is a **back edge** when `header` dominates `tail`.
- Because `header` dominates `tail` (and every block on any path inside the loop reaching `tail`), every path from outside the loop into the loop *must* enter through `header`! That guarantees a preheader placed before `header` runs before every iteration and cannot be bypassed via a side entry.

### Compute dominance, then find the loop

1. `predecessors` reverses the edge lists so the algorithm can ask which blocks can enter a given block.
2. `dominators` initializes `dom[entry] = {entry}` and all other sets to the full node set `{0..N-1}` (since dominance is a forward **must-analysis**). At each non-entry block, a strict dominator must dominate **every** predecessor, so the solver intersects predecessors' sets, adds the block itself (`{node}`), and repeats until the sets stop shrinking.
3. `find_loops` checks each edge `tail -> header` for `header in dom[tail]`. For each back edge, `natural_loop` seeds `body = {header}` and walks `incoming` predecessors backward from `tail`. Pre-seeding `header` stops the walk at the header so outside blocks like `B0` are never pulled into the loop.

### Follow the sample graph

```text
B0 → B1 → B2 → B3
     ↑         │
     └─────────┘
     B1 → B4
```

| Block | Predecessors | `dom[B]` (Fixed Point) | Back-edge / Loop role |
| --- | --- | --- | --- |
| `B0` | `[]` | `{0}` | Preheader / CFG entry (excluded by `body` stop at `B1`) |
| `B1` | `[0, 3]` | `{0, 1}` | Loop header (`1 in dom[3]`, so `B3 -> B1` is a back edge) |
| `B2` | `[1]` | `{0, 1, 2}` | Loop body (`B3`'s predecessor) |
| `B3` | `[2]` | `{0, 1, 2, 3}` | Loop latch / tail (`B3 -> B1`) |
| `B4` | `[1]` | `{0, 1, 4}` | Loop exit (cannot reach `B3`, excluded from `{1, 2, 3}`) |

Starting the reverse walk at `tail = B3` collects `B3`, walks to predecessor `B2`, reaches `B1` (already in `body`), and terminates with `{1, 2, 3}`.

### How to read the code

| Component | Rust ([`main.rs`](main.rs)) | Python ([`main.py`](main.py)) | Purpose |
| --- | --- | --- | --- |
| Reverse CFG | `predecessors(edges)` | `predecessors(edges)` | Builds incoming adjacency lists from outgoing edge lists |
| Dominance solver | `dominators(incoming)` | `dominators(incoming)` | Forward must-intersection from `{0..N-1}` to compute `dom[b]` |
| Loop collector | `natural_loop(tail, header, incoming)` | `natural_loop(tail, header, incoming)` | Reverse DFS from `tail` stopping at pre-seeded `header` |
| Driver & tests | `find_loops`, `GRAPH`, tests | `find_loops`, `GRAPH`, `NaturalLoopsTests` | Tests `B3 -> B1` loop `{1, 2, 3}`, self-loop `{1}`, and acyclic DAG |

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
