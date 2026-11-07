# FIRST, FOLLOW, and LL(1)

Compute a predictive parsing table for balanced brackets. See the [Rust source](main.rs).

```sh
cargo run --example ll1
cargo test --example ll1
```

## What the example teaches

`predict` iterates FIRST and FOLLOW to a fixed point. The grammar `S → [ S ] S | ε` selects the empty production when the lookahead belongs to FOLLOW(S). The parser expands productions on a stack using one token of lookahead.

## What to look for

`[[]][]` and the empty string are accepted. `[][` is rejected. Tests also check that two productions competing for the same table cell produce a conflict.

## Edition mapping and scope

| 1977 Green | 1986 Red | 2006 Purple |
| --- | --- | --- |
| 4–5 | 4 | 4 |

Numbers refer to chapters. Related readings and appendices are marked where applicable. See the [edition guide](../../README.md#edition-guide) for the bibliographic references and chapter coverage.

The demo uses a small, in-code grammar and reserves `$` as the end marker.
