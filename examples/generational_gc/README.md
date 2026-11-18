# Generational, Tri-Color Incremental, and Reference-Counting GC

Track inter-generational write barriers, preserve incremental tri-color invariants, and contrast reference-counting cycle leaks. See the [Rust source](main.rs) and [Python source](main.py).

```sh
cargo run --example generational_gc
cargo test --example generational_gc
python3 examples/generational_gc/main.py
python3 -m unittest examples/generational_gc/main.py
```

## What the example teaches

### Why we need this

Full-heap stop-the-world collections pause the program for time proportional to all live memory, even though most programs obey the **weak generational hypothesis**: newly allocated objects die quickly, while objects that survive a few collections tend to live for a long time. Splitting the heap into a small `Young` nursery (`Gen0`) and a large `Old` generation (`Gen1`) lets frequent **minor collections** scan only `Young` objects—ignoring `Gen1` completely.

However, skipping `Gen1` and interleaving GC with program execution introduce two correctness traps, alongside reference counting's cycle limitation:

1. **Old-to-young pointers in generational GC:** Suppose `old_holder` (in `Gen1`) stores a pointer to `young_kept` (in `Gen0`), and no stack root directly points to `young_kept`. Because minor GC refuses to trace `Gen1`, it will falsely reclaim `young_kept` unless every store into an `Old` object passes through a **write barrier** that logs the owner into `remembered_set`.
2. **Mutator writes during incremental marking:** In an incremental collector, nodes are `White` (unvisited), `Gray` (discovered, children pending), or `Black` (scanned, all direct children visited). If the mutator links a `Black` node `0` to a `White` node `2` while marking is paused, node `0` will never be rescanned, so node `2` would be lost unless Dijkstra's **tri-color insertion barrier** re-grays node `2` on the store.
3. **Cycles under reference counting:** Naive reference counting reclaims objects when their count hits zero, so two mutually referencing objects (`0 <-> 1`) retain count `1` after all external roots are dropped.

Most allocations die young, so collecting only the young nursery (`Gen0`) avoids scanning long-lived (`Gen1`) memory provided a write barrier logs old-to-young stores in a `remembered_set`. Incremental marking likewise relies on a write barrier so mutators never leave a `Black` object pointing directly to an unvisited `White` object.

### Three memory-management invariants

| Mechanism | Role | Failure without it |
| --- | --- | --- |
| Remembered set barrier | Tracks `Old -> Young` pointers | Minor GC falsely reclaims reachable `Young` object |
| Tri-color barrier | Re-grays `White` target when linked from `Black` | Incremental collector drops newly linked object |
| Tracing vs naive RC | Detects unreachable cyclic islands | Mutual cycles retain nonzero counts forever |

In the demo (`age_threshold = 2`), `old_holder` writes a pointer to `young_kept` while `young_dead` is unreferenced: Minor GC #1 sweeps `young_dead` (`swept=1, promoted=0`) and preserves `young_kept` via `remembered_set`; Minor GC #2 ages `young_kept` to `2` and promotes it into `Gen1` (`swept=0, promoted=1`), clearing `remembered_set` because no `Old -> Young` edge remains.

### How to read the code

| Component (`main.rs` / `main.py`) | Role |
| --- | --- |
| `Gen`, `GenObj`, `GenerationalHeap` | Models `Young`/`Old` objects, `remembered_set`, `write_field` write barrier, and `minor_collect` with aging & promotion. |
| `Color`, `TriColorHeap` | Models incremental marking (`step_mark`), Dijkstra's insertion barrier (`mutator_add_edge`), and invariant checker (`has_black_to_white_violation`). |
| `naive_refcount_cycle_leak` | Simulates creating `0 <-> 1` and dropping external roots, leaving 2 leaked objects with `rc == 1`. |
| `main` / tests | Exercises minor GC with barrier off vs on, promotion to `Gen1`, tri-color marking, and RC cycle leak. |

## What to look for

Disabling `use_write_barrier` causes `minor_collect` to drop `young_lost` even though `old` still points to it; enabling the barrier retains `young_kept` across minor GC #1, ignores unreferenced `Gen1` objects, and promotes `young_kept` into `Gen1` on minor GC #2 once `age >= age_threshold`.

## Check your understanding

- Why can `old_unreferenced` remain allocated during a minor collection?
- How does Dijkstra's insertion barrier (`Black -> White` re-grays the target) prevent lost objects when mutators interleave with GC marking?

## Edition mapping and scope

| 1977 Green | 1986 Red | 2006 Purple |
| --- | --- | --- |
| — | 7 | 7 |

Numbers refer to chapters. Related readings and appendices are marked where applicable. See the [edition guide](../../README.md#edition-guide) for the bibliographic references and chapter coverage.

Objects are indexed explicitly without card-table byte compression or multi-threaded stop-the-world safepoints.
