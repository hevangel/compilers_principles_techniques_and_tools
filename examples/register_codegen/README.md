# Register needs for expression trees

Choose an evaluation order that uses fewer registers. See the [Rust source](main.rs).

```sh
cargo run --example register_codegen
cargo test --example register_codegen
```

## What the example teaches

`Plan::for_expr` labels subtrees with their register needs. Equal needs require one extra register to retain the first result. `generate` evaluates the more demanding subtree first and records the original operand order in the emitted instruction.

## What to look for

For `a - (b + c)`, the right subtree runs first and the code uses two registers. The test confirms both the evaluation order and the subtraction result.

## Edition mapping and scope

| 1977 Green | 1986 Red | 2006 Purple |
| --- | --- | --- |
| 15 | 9 | 8 |

Numbers refer to chapters. Related readings and appendices are marked where applicable. See the [edition guide](../../README.md#edition-guide) for the bibliographic references and chapter coverage.

The model uses pure expressions and an abstract register machine with enough registers for the chosen tree.
