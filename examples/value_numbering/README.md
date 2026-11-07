# Local value numbering

Reuse a previously computed value within one basic block. See the [Rust source](main.rs).

```sh
cargo run --example value_numbering
cargo test --example value_numbering
```

## What the example teaches

`number` tracks the representative of each value and indexes expressions by operator and operand representatives. It canonicalizes operands for addition, where order is irrelevant, and preserves operand order for subtraction.

## What to look for

The second addition becomes a copy. Two subtractions with reversed operands stay distinct. Tests compare execution before and after rewriting, including wrapping arithmetic.

## Edition mapping and scope

| 1977 Green | 1986 Red | 2006 Purple |
| --- | --- | --- |
| 12 | 9–10 | 8–9 |

Numbers refer to chapters. Related readings and appendices are marked where applicable. See the [edition guide](../../README.md#edition-guide) for the bibliographic references and chapter coverage.

Temporary destinations are implicit and assigned once. The example assumes straight-line, side-effect-free code.
