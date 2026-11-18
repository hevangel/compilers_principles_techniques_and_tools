# Tracing and mark-sweep collection

Trace reachable heap objects and reclaim an unrooted cycle. See the [Rust source](main.rs) and [Python source](main.py).

```sh
cargo run --example mark_sweep
cargo test --example mark_sweep
python3 examples/mark_sweep/main.py
python3 -m unittest examples/mark_sweep/main.py
```

## What the example teaches

### Why we need this

Heap-allocated objects outlive the function calls that create them, so they cannot simply be popped like stack frames. Manual `free` calls risk **use-after-free** bugs (freeing too early) or **memory leaks** (forgetting to free). A simpler automatic alternative—naive **reference counting**—tracks how many pointers point to each object, which fails on **reference cycles**: in slots 2 and 3 below (`2 → 3 → 2`), both objects have reference count 1 even after the program drops every variable pointing to them, so they would leak forever.

**Tracing mark-sweep garbage collection** solves both problems by computing true **root reachability**. A **root** is a reference held outside the heap (for example, in an active stack frame or register). Anything reachable along a directed path from a root is live; everything else—including isolated cycles—is dead and can be safely reclaimed.

### Work through a collection

`collect` starts from root indices and traverses references, marking an object before queuing its outgoing edges. The sweep removes every occupied slot without a mark.

1. **Mark.** Start with a work list containing the root indices and a mark bit for each heap slot. On visiting an occupied, unmarked slot, set its mark and add its outgoing references. Marking before adding edges ensures a cycle (`0 → 1 → 0`) cannot keep the traversal running indefinitely.
2. **Sweep.** Scan the heap slots. A marked object is kept. An unmarked occupied slot is emptied, and its index is reported as reclaimed. The marks are fresh for every call.

| Slots | References | With roots `[0, 4]` |
| --- | --- | --- |
| 0 and 1 | `0 → 1 → 0` | Both reachable through root 0 |
| 2 and 3 | `2 → 3 → 2` | Both unreachable and reclaimed |
| 4 | None | Reachable directly from root 4 |

Keeping track of references inside a cycle is insufficient to decide whether the program can still access it. The deciding property is reachability **from the roots**. The second collection with the same roots reclaims nothing because the dead objects have already been removed.

### How to read the code

| Component (`main.rs` / `main.py`) | Role |
| --- | --- |
| `Object` / `HeapObject` | Represents one heap-allocated node holding outgoing `references` (heap slot indices). |
| `collect` | Runs the stop-the-world mark worklist from `roots`, then sweeps all unmarked occupied slots and returns their indices. |
| `sample` | Allocates 5 slots containing a rooted cycle (`0 <-> 1`), an unrooted cycle (`2 <-> 3`), and a rooted leaf (`4`). |
| `main` / test | Collects with roots `[0, 4]` (reclaims `[2, 3]`), collects again (`[]`), and finally collects with `[]` (reclaims `[0, 1, 4]`). |

## What to look for

The sample keeps a rooted cycle and reclaims an unrooted cycle in slots 2 and 3. The test checks a repeated collection and collection after all roots are removed.

## Check your understanding

- Change the root list to `[2]`. Predict exactly which slot indices will be reclaimed.
- Add an edge from object 1 to object 3. With the original roots, what happens to the second cycle?

## Edition mapping and scope

| 1977 Green | 1986 Red | 2006 Purple |
| --- | --- | --- |
| — | 7 (storage reclamation) | 7 |

Numbers refer to chapters. Related readings and appendices are marked where applicable. See the [edition guide](../../README.md#edition-guide) for the bibliographic references and chapter coverage.

This is a simulated heap with integer handles, explicit roots, and one full stop-the-world pass.
