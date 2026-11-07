# Tracing and mark-sweep collection

Trace reachable heap objects and reclaim an unrooted cycle. See the [Rust source](main.rs).

```sh
cargo run --example mark_sweep
cargo test --example mark_sweep
```

## What the example teaches

`collect` starts from root indices and traverses references, marking an object before queuing its outgoing edges. The sweep removes every occupied slot without a mark.

A **tracing garbage collector** treats heap objects and their references as a directed graph. **Roots** are references from outside the heap, such as values held in active stack frames. An object is reachable if there is a path from a root to it. A group of objects can point to one another and still be unreachable from every root.

### Work through a collection

1. **Mark.** Start with a work list containing the root indices and a mark bit for each heap slot. On visiting an occupied, unmarked slot, set its mark and add its outgoing references. Marking before adding edges ensures a cycle cannot keep the traversal running indefinitely.
2. **Sweep.** Scan the heap slots. A marked object is kept. An unmarked occupied slot is emptied, and its index is reported as reclaimed. The marks are fresh for every call.

| Slots | References | With roots `[0, 4]` |
| --- | --- | --- |
| 0 and 1 | `0 → 1 → 0` | Both reachable through root 0 |
| 2 and 3 | `2 → 3 → 2` | Both unreachable and reclaimed |
| 4 | None | Reachable directly from root 4 |

Keeping track of references inside a cycle is insufficient to decide whether the program can still access it. The deciding property is reachability **from the roots**. The second collection with the same roots reclaims nothing because the dead objects have already been removed.

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
