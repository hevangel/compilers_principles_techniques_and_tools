# Context-insensitive points-to analysis

Propagate possible allocation sites through assignments and calls. See the [Rust source](main.rs).

```sh
cargo run --example points_to
cargo test --example points_to
```

## What the example teaches

Each inclusion edge adds the source’s possible targets to the destination’s set. Both calls to `relay` share one formal parameter and one return node, so the solver merges their facts as it iterates to a fixed point.

## What to look for

Both callers’ result variables are conservatively associated with both allocation sites. The test also propagates an allocation-site fact around a cycle.

## Edition mapping and scope

| 1977 Green | 1986 Red | 2006 Purple |
| --- | --- | --- |
| 14 (related interprocedural flow) | 10 (related alias analysis) | 12 |

Numbers refer to chapters. Related readings and appendices are marked where applicable. See the [edition guide](../../README.md#edition-guide) for the bibliographic references and chapter coverage.

The example models allocation, copy, and statically known calls with shared callee nodes. Fields, indirect loads and stores, and context-sensitive analysis are outside this model.
