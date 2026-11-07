# Forward reaching definitions

Find which assignments can supply a value at a control-flow join. See the [Rust source](main.rs).

```sh
cargo run --example reaching_defs
cargo test --example reaching_defs
```

## What the example teaches

`reaching` unions predecessor outputs. Each assignment removes older definitions of the same variable and adds its own definition ID. Iteration propagates information through loops until the sets stabilize.

## What to look for

At the join, both branch definitions of `x` reach the block, along with the entry definition of `y`. A second test case checks that a loop-carried definition reaches the loop header.

## Edition mapping and scope

| 1977 Green | 1986 Red | 2006 Purple |
| --- | --- | --- |
| 12, 14 | 10 | 9 |

Numbers refer to chapters. Related readings and appendices are marked where applicable. See the [edition guide](../../README.md#edition-guide) for the bibliographic references and chapter coverage.

The program uses scalar variables and a control-flow graph specified in Rust.
