# Tracing and mark-sweep collection

Trace reachable heap objects and reclaim an unrooted cycle. See the [Rust source](main.rs).

```sh
cargo run --example mark_sweep
cargo test --example mark_sweep
```

## What the example teaches

`collect` starts from root indices and traverses references, marking an object before queuing its outgoing edges. The sweep removes every occupied slot without a mark.

## What to look for

The sample keeps a rooted cycle and reclaims an unrooted cycle in slots 2 and 3. The test checks a repeated collection and collection after all roots are removed.

## Edition mapping and scope

| 1977 Green | 1986 Red | 2006 Purple |
| --- | --- | --- |
| — | 7 (storage reclamation) | 7 |

Numbers refer to chapters. Related readings and appendices are marked where applicable. See the [edition guide](../../README.md#edition-guide) for the bibliographic references and chapter coverage.

This is a simulated heap with integer handles, explicit roots, and one full stop-the-world pass.
