# Backward liveness analysis

Find which values may still be read along a future path. See the [Rust source](main.rs).

```sh
cargo run --example liveness
cargo test --example liveness
```

## What the example teaches

A block’s live-out set is the union of its successors’ live-in sets. Its live-in set adds reads that occur before local definitions, then propagates the live-out values that the block does not overwrite. The calculation repeats until stable.

## What to look for

The loop keeps `step` and `x` live on its back edge. At the entry block, the first assignment kills the incoming need for `x`. The test also checks that the exit has no live-out values.

## Edition mapping and scope

| 1977 Green | 1986 Red | 2006 Purple |
| --- | --- | --- |
| 12, 14 | 10 | 9 |

Numbers refer to chapters. Related readings and appendices are marked where applicable. See the [edition guide](../../README.md#edition-guide) for the bibliographic references and chapter coverage.

The example starts with explicit control-flow edges and per-block use/definition sets.
