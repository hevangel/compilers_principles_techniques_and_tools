# Generational, Tri-Color Incremental, and Reference-Counting GC

Track inter-generational write barriers, preserve incremental tri-color invariants, and contrast reference-counting cycle leaks. See the [Rust source](main.rs) and [Python source](main.py).

```sh
cargo run --example generational_gc
cargo test --example generational_gc
python3 examples/generational_gc/main.py
python3 -m unittest examples/generational_gc/main.py
```

## What the example teaches

Most allocations die young, so collecting only the young nursery (`Gen0`) avoids scanning long-lived (`Gen1`) memory provided a write barrier logs old-to-young stores in a `remembered_set`. Incremental marking likewise relies on a write barrier so mutators never leave a `Black` object pointing directly to an unvisited `White` object.

### Three memory-management invariants

| Mechanism | Role | Failure without it |
| --- | --- | --- |
| Remembered set barrier | Tracks `Old -> Young` pointers | Minor GC falsely reclaims reachable `Young` object |
| Tri-color barrier | Re-grays `White` target when linked from `Black` | Incremental collector drops newly linked object |
| Tracing vs naive RC | Detects unreachable cyclic islands | Mutual cycles retain nonzero counts forever |

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
