# Inherited and synthesized attributes

Pass an origin down a tree and synthesize widths on the way back up. See the [Rust source](main.rs).

```sh
cargo run --example attributes
cargo test --example attributes
```

## What the example teaches

A strip synthesizes its width. A join passes its inherited origin to the left child, then adds the left child’s width to obtain the right child’s inherited origin. The left-to-right evaluation order satisfies both dependencies.

## What to look for

Three strips of widths 2, 5, and 4 starting at 10 have origins 10, 12, and 17. The test checks those placements and total width 11.

## Edition mapping and scope

| 1977 Green | 1986 Red | 2006 Purple |
| --- | --- | --- |
| 7 (translation schemes) | 5 | 5 |

Numbers refer to chapters. Related readings and appendices are marked where applicable. See the [edition guide](../../README.md#edition-guide) for the bibliographic references and chapter coverage.

The tree and its evaluation order are written directly in Rust.
