# Backpatching and short-circuit jumps

Fill forward jump targets after their destination instructions are emitted. See the [Rust source](main.rs).

```sh
cargo run --example backpatch
cargo test --example backpatch
```

## What the example teaches

`emit` returns unresolved true and false jump sites. AND patches its left true exits to the right operand. OR patches its left false exits there. `compile` connects the remaining exits to the two return instructions.

## What to look for

The tests compare all eight inputs against the truth table for `(a && b) || c`. They also inspect which inputs were read to verify both short-circuit paths.

## Edition mapping and scope

| 1977 Green | 1986 Red | 2006 Purple |
| --- | --- | --- |
| 7 (Boolean translation) | 8 | 6 |

Numbers refer to chapters. Related readings and appendices are marked where applicable. See the [edition guide](../../README.md#edition-guide) for the bibliographic references and chapter coverage.

The input is an in-code condition tree. The generated code branches to Boolean return values on a small interpreter.
