# Cheney Semispace Copying and Sliding Mark-Compact GC

Compare Cheney breadth-first semispace copying collection against three-pass sliding Mark-Compact collection. See the [Rust source](main.rs) and [Python source](main.py).

```sh
cargo run --example copying_gc
cargo test --example copying_gc
python3 examples/copying_gc/main.py
python3 -m unittest examples/copying_gc/main.py
```

## What the example teaches

Both copying and compacting collectors eliminate heap fragmentation and handle cyclic graphs cleanly, while differing in space overhead versus pass count.

### Two compacting designs

| Collector | Traversal mechanism | Space trade-off |
| --- | --- | --- |
| Cheney Semispace (`§7.6.5`) | `scan` and `free` cursors over `to_space` (BFS queue in the heap itself) | Halves usable heap; touches only live objects |
| Sliding Mark-Compact (`§7.6.4`) | 3 passes (mark, compute `forwarded`, update & slide) | Uses 100% of heap; preserves allocation order |

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
