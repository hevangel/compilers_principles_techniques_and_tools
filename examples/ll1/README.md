# FIRST, FOLLOW, and LL(1)

Compute a predictive parsing table for balanced brackets. See the [Rust source](main.rs) and [Python source](main.py).

```sh
cargo run --example ll1
cargo test --example ll1
python3 examples/ll1/main.py
python3 -m unittest examples/ll1/main.py
```

## What the example teaches

### Why we need this / What problem this solves

Given a grammar such as balanced brackets `S → [ S ] S | ε`, a top-down parser standing at `S` must decide *which* production to expand using only the next input token (**1 token of lookahead**). A naive backtracking parser tries `[ S ] S` first and rewinds if it fails, risking exponential blowup on nested brackets. Conversely, guessing based only on the first terminal of each right-hand side breaks down completely on **nullable** (`ε`) productions:

- When the input has `[`, `S` must expand to `[ S ] S`.
- When the input has `]` (closing an enclosing pair, as in `[ ]`) or `$` (end of input), `S` cannot match `[`, so it *must* take `S → ε`. How does the parser know which tokens legitimately mean "stop expanding `S` here" versus "syntax error" (like an extra unmatched `]`)?

**LL(1) predictive parsing** solves this deterministically in `O(n)` time with zero backtracking by precomputing **FIRST** (what terminals can *start* a phrase, plus whether it is **nullable**) and **FOLLOW** (what terminals can legally appear *immediately after* a nonterminal). See [`recursive_descent`](../recursive_descent/README.md) for the handwritten function-per-nonterminal counterpart that replaces this explicit stack and table with direct calls, `while` loops, and multi-token `peek(1)` lookahead.

### Build the table from the grammar

- **FIRST** of a sequence describes the terminals it can begin with. The code separately tracks whether the sequence is **nullable**, meaning it can derive the empty string. `first_of` keeps looking to the right only while the symbols it has passed are nullable (`FIRST(S) = {'['}, nullable = true`).
- **FOLLOW** of a nonterminal describes terminals that can appear immediately after it. The first `S` on the right of `[ S ] S` is followed by `]`. The last `S` inherits the left-hand side's FOLLOW set (`FOLLOW(S)`). The start symbol `S` is seeded with end-of-input `$`, so `FOLLOW(S) = {']', '$'}`.
- For each production `A → α`, `predict` fills table cells `(A, t)` for every `t ∈ FIRST(α)`. If `α` is nullable, it also fills `(A, t)` for every `t ∈ FOLLOW(A)`. If two productions claim the same `(A, t)` cell, the grammar is not LL(1) and `predict` reports a collision.

| Lookahead with `S` on top | Production | Source set |
| --- | --- | --- |
| `[` | `S → [ S ] S` | `FIRST([ S ] S) = {'['}` |
| `]` | `S → ε` | `FOLLOW(S) = {']', '$'}` (production 1 is nullable) |
| `$` | `S → ε` | `FOLLOW(S) = {']', '$'}` (production 1 is nullable) |

### Walk the stack on `[]`

The parser initializes `stack = ['$', S]` (top on the right) and appends `'$'` to the input. When expanding a production, it pushes the right-hand side **in reverse order** so the leftmost symbol sits on top:

| Step | Stack (top on right) | Remaining input | Action |
| --- | --- | --- | --- |
| 1 | `['$', S]` | `"[]$"` | Lookahead `[`: expand `S → [ S ] S` (push `S ] S [` ) |
| 2 | `['$', S, ']', S, '[']` | `"[]$"` | Match terminal `[` |
| 3 | `['$', S, ']', S]` | `"]$"` | Lookahead `]`: expand inner `S → ε` (push nothing) |
| 4 | `['$', S, ']']` | `"]$"` | Match terminal `]` |
| 5 | `['$', S]` | `"$"` | Lookahead `$`: expand trailing `S → ε` |
| 6 | `['$']` | `"$"` | Match terminal `$` -> stack empty & input exhausted -> **accept** |

A terminal mismatch, missing table entry, or leftover input rejects the string.

### How to read the code

| Component | Rust (`main.rs`) | Python (`main.py`) | Role |
| --- | --- | --- | --- |
| Grammar symbols | `enum Symbol` (`Terminal`, `Rule`) | `Terminal`, `Rule` dataclasses | Distinguishes input tokens from nonterminal indices |
| Sequence FIRST | `fn first_of` | `def first_of` | Computes `(FIRST(seq), nullable)` stopping at first non-nullable symbol |
| Table builder | `fn predict` | `def predict` | Fixed-point `FIRST` and `FOLLOW` loops, then LL(1) table population & conflict check |
| Predictive driver | `fn parse` | `def parse` | Stack-based top-down recognizer using the `(nonterminal, lookahead)` table |

## What to look for

`[[]][]` and the empty string are accepted. `[][` is rejected. Tests also check that two productions competing for the same table cell produce a conflict.

## Check your understanding

- Hand-trace `[][]`. The last `S` of the first bracket pair sees `[`, so it starts another pair.
- For `[]]`, explain why the table can select an empty production on the final bracket yet the parser still rejects the input.

## Edition mapping and scope

| 1977 Green | 1986 Red | 2006 Purple |
| --- | --- | --- |
| 4–5 | 4 | 4 |

Numbers refer to chapters. Related readings and appendices are marked where applicable. See the [edition guide](../../README.md#edition-guide) for the bibliographic references and chapter coverage.

The demo uses a small, in-code grammar and reserves `$` as the end marker.
