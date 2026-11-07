# LR(0) items and SLR parsing

Generate item sets and shift/reduce actions for a left-recursive list. See the [Rust source](main.rs).

```sh
cargo run --example slr
cargo test --example slr
```

## What the example teaches

`closure` expands items whose dot precedes a nonterminal. Advancing the dot and closing the result produces the transitions between item sets. Completed productions add reduce actions only on their left-hand side’s FOLLOW tokens.

An LL parser predicts a production before reading its right-hand side. A **bottom-up** parser works in the other direction. It **shifts** tokens onto a stack and **reduces** a recognized right-hand side to its left-hand nonterminal. This makes the left-recursive rule `List → List , Item` useful for extending a list that has already been recognized.

### From productions to a parser

1. An **LR(0) item** is a production with a dot marking progress. `List → List · , Item` means a list has been recognized and the comma is expected next. The program stores an item as `(rule number, dot position)`.
2. **Closure** adds the starting items of any nonterminal after a dot. For instance, `Start → · List` brings in both productions of `List`. To build an automaton transition, advance dots across one symbol, then take closure again.
3. `actions` turns terminal transitions into shifts. A completed production becomes a reduction on its left-hand side's FOLLOW tokens. This is the SLR restriction. In the list grammar, a completed item may reduce before a comma or end of input. Completing the augmented start production on end of input gives acceptance.
4. `parse` keeps automaton states on its stack. Shifting consumes a token and pushes a state. Reducing pops one state per right-hand symbol, consults the nonterminal transition (called **goto**) from the uncovered state, and pushes the result. The lookahead remains available for the next decision.

### Follow the reductions for `name , name`

| Rule | Recognized phrase |
| --- | --- |
| 3 | First `name` becomes an `Item` |
| 2 | That `Item` becomes a `List` |
| 3 | Second `name` becomes another `Item` |
| 1 | `List , Item` becomes the longer `List` |

The parser then accepts on end of input. The leading `Start` rule is an extra production used to distinguish acceptance from an ordinary reduction.

## What to look for

Two names separated by a comma produce reductions `[3, 2, 3, 1]` in the example’s rule numbering. The test rejects a trailing comma, an embedded end marker, and an empty list.

## Check your understanding

- Predict the reduction trace for a single `name`. Then change the sample token array and check your prediction.
- For `name ,`, identify which token the parser still needs after shifting the comma. Why can it not reduce the list at end of input?

## Edition mapping and scope

| 1977 Green | 1986 Red | 2006 Purple |
| --- | --- | --- |
| 5–6 | 4 | 4 |

Numbers refer to chapters. Related readings and appendices are marked where applicable. See the [edition guide](../../README.md#edition-guide) for the bibliographic references and chapter coverage.

The program computes item sets and actions for one grammar with hand-supplied FOLLOW sets. A conflicting action is reported as a table construction error.
