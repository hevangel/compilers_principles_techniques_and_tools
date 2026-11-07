# Local value numbering

Reuse a previously computed value within one basic block. See the [Rust source](main.rs).

```sh
cargo run --example value_numbering
cargo test --example value_numbering
```

## What the example teaches

`number` tracks the representative of each value and indexes expressions by operator and operand representatives. It canonicalizes operands for addition, where order is irrelevant, and preserves operand order for subtraction.

A **basic block** is a straight-line sequence of instructions with entry at its beginning and no branch out until the end. Inside a block, an expression can be computed once and reused when the same operation sees the same values again. **Local value numbering** gives equivalent values a common representative so the compiler can recognize this situation.

### Give each value a representative

`number` uses one table to map each input or temporary to its representative and another to map an expression key to the first temporary that computed it. A key consists of an operator and the representatives of its two operands. If the key is already present, the new instruction becomes a copy. Future expressions use the original representative for both temporary names.

For addition, sorting operand representatives gives `a + b` and `b + a` the same key. Subtraction retains its order. The example uses wrapping integer arithmetic, where addition is commutative even at the numeric limits.

### Trace the numbered temporaries

Inputs occupy indices 0 and 1. Each expression adds one temporary, so the first instruction writes index 2.

| Destination | Original expression | After value numbering |
| --- | --- | --- |
| 2 | `0 + 1` | Compute and record the key |
| 3 | `1 + 0` | Copy 2 |
| 4 | `3 - 0` | Compute `2 - 0` |
| 5 | `0 - 2` | Compute separately |
| 6 | `2 - 0` | Copy 4 |

The interpreter evaluates the original and optimized forms with the same input values. The tests compare every temporary, including values produced by copies.

## What to look for

The second addition becomes a copy. Two subtractions with reversed operands stay distinct. Tests compare execution before and after rewriting, including wrapping arithmetic.

## Check your understanding

- Add an expression `5 + 4` and then `4 + 5` to `CODE`. Predict the optimized form of the second expression.
- Why would using the variable's spelling alone as a representative be unsafe in a language that lets the same variable be assigned more than once?

## Edition mapping and scope

| 1977 Green | 1986 Red | 2006 Purple |
| --- | --- | --- |
| 12 | 9–10 | 8–9 |

Numbers refer to chapters. Related readings and appendices are marked where applicable. See the [edition guide](../../README.md#edition-guide) for the bibliographic references and chapter coverage.

Temporary destinations are implicit and assigned once. The example assumes straight-line, side-effect-free code.
