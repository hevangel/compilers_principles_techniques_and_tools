# LR(0) items and SLR parsing

Generate item sets and shift/reduce actions for a left-recursive list. See the [Rust source](main.rs).

```sh
cargo run --example slr
cargo test --example slr
```

## What the example teaches

`closure` expands items whose dot precedes a nonterminal. Advancing the dot and closing the result produces the transitions between item sets. Completed productions add reduce actions only on their left-hand side’s FOLLOW tokens.

## What to look for

Two names separated by a comma produce reductions `[3, 2, 3, 1]` in the example’s rule numbering. The test rejects a trailing comma, an embedded end marker, and an empty list.

## Edition mapping and scope

| 1977 Green | 1986 Red | 2006 Purple |
| --- | --- | --- |
| 5–6 | 4 | 4 |

Numbers refer to chapters. Related readings and appendices are marked where applicable. See the [edition guide](../../README.md#edition-guide) for the bibliographic references and chapter coverage.

The program computes item sets and actions for one grammar with hand-supplied FOLLOW sets. A conflicting action is reported as a table construction error.
