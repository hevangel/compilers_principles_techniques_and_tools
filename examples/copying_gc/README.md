# Cheney Semispace Copying and Sliding Mark-Compact GC

Compare Cheney breadth-first semispace copying collection against three-pass sliding Mark-Compact collection. See the [Rust source](main.rs) and [Python source](main.py).

```sh
cargo run --example copying_gc
cargo test --example copying_gc
python3 examples/copying_gc/main.py
python3 -m unittest examples/copying_gc/main.py
```

## What the example teaches

### Why we need this

A basic mark-sweep collector frees dead objects **in place**, leaving live objects scattered across memory with free gaps between them. Over time this **heap fragmentation** causes two problems:

1. **Slow allocation:** The allocator must search free lists for a gap large enough for each new object instead of bumping a single pointer (`free += size`).
2. **Failed allocations and poor locality:** Large allocations can fail even when total free memory is sufficient, and traversing linked objects jumps across sparse cache lines.

Compacting collectors solve fragmentation by packing all live objects into a contiguous prefix (`0..live`), restoring `O(1)` bump-pointer allocation. In doing so, they must also preserve **sharing** and **cycles**—in the demo's 10-slot heap, `diamond` branches `d_left` and `d_right` both point to `shared`, and `cyc_a <-> cyc_b` form a live cycle alongside 4 dead objects (`dead_a`, `dead_b`, `dead_x <-> dead_y`). Without forwarding addresses, copying `diamond` would either duplicate `shared` (breaking pointer identity) or loop forever on `cyc_a <-> cyc_b`.

### Two compacting designs

Both copying and compacting collectors eliminate heap fragmentation and handle cyclic graphs cleanly, while differing in space overhead versus pass count.

| Collector | Traversal mechanism | Space trade-off |
| --- | --- | --- |
| Cheney Semispace (`§7.6.5`) | `scan` and `free` cursors over `to_space` (BFS queue in the heap itself) | Halves usable heap; touches only live objects |
| Sliding Mark-Compact (`§7.6.4`) | 3 passes (mark, compute `forwarded`, update & slide) | Uses 100% of heap; preserves allocation order |

On the 10-slot sample (`roots = [diamond, cyc_a]`), both collectors pack the 6 live objects into slots `0..6` and leave `6..10` empty, while `canonical_graph` verifies the reachable pointer graph is isomorphic before and after collection.

### How to read the code

| Component (`main.rs` / `main.py`) | Role |
| --- | --- |
| `HeapObj` | Object node storing `tag`, pointer `fields`, `marked` bit, and `forwarded` relocation slot. |
| `SemispaceHeap::cheney_collect` | Copies roots then advances `scan` toward `free` in `to_space`, setting `forwarded` in `from_space` so shared nodes and cycles copy once, then swaps spaces. |
| `SlidingHeap::mark_compact` | Runs Pass 1 (DFS mark), Pass 2 (assign dense `forwarded` targets), and Pass 3 (rewrite all pointers then slide live objects down). |
| `canonical_graph` | Produces a deterministic BFS encoding of the reachable subgraph to verify graph isomorphism across compaction. |
| `build_diamond_and_cycle_sample` | Builds the 10-slot test heap with dead leaves, a shared diamond DAG, a live cycle, and a dead cycle. |

## What to look for

Both `d_left` and `d_right` point to the same `shared` node alongside a live cycle `cyc_a <-> cyc_b` and unreachable garbage. Cheney copying sets the `forwarded` pointer on the first visit so `shared` is copied strictly once and both branches point to the same compacted address; Mark-Compact achieves an isomorphic compacted prefix `0..6`.

## Check your understanding

- Why does Cheney's algorithm need no auxiliary mark stack even on arbitrary cyclic object graphs?
- Why must Mark-Compact update pointers in Pass 3 *before* (or using precomputed `forwarded` targets while) sliding objects?

## Edition mapping and scope

| 1977 Green | 1986 Red | 2006 Purple |
| --- | --- | --- |
| — | 7 (reclamation) | 7 |

Numbers refer to chapters. Related readings and appendices are marked where applicable. See the [edition guide](../../README.md#edition-guide) for the bibliographic references and chapter coverage.

Objects hold fixed-size reference vectors without raw interior pointers, weak references, or finalizers.
