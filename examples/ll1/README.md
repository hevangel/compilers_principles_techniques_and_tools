# FIRST, FOLLOW, and LL(1)

Compute a predictive parsing table for balanced brackets. See the [Rust source](main.rs).

```sh
cargo run --example ll1
cargo test --example ll1
```

## What the example teaches

`predict` iterates FIRST and FOLLOW to a fixed point. The grammar `S → [ S ] S | ε` selects the empty production when the lookahead belongs to FOLLOW(S). The parser expands productions on a stack using one token of lookahead.

A **context-free grammar** describes valid sequences with productions. A production replaces a **nonterminal** such as `S` with a sequence of terminals and nonterminals. Terminals are the actual input symbols. In this grammar, `S → [ S ] S | ε` says a sequence is either empty or an opening bracket, a nested sequence, a closing bracket, and another sequence.

An **LL(1)** parser reads left to right, constructs a leftmost derivation, and chooses each production using one lookahead token. Its prediction table is indexed by the current nonterminal and that token.

### Build the table from the grammar

- **FIRST** of a sequence describes the terminals it can begin with. The code separately tracks whether the sequence is **nullable**, meaning it can derive the empty string. `first_of` keeps looking to the right only while the symbols it has passed are nullable.
- **FOLLOW** of a nonterminal describes terminals that can appear immediately after it. The first `S` on the right of `[ S ] S` is followed by `]`. The last `S` inherits the left-hand side's FOLLOW set because nothing follows it. The start symbol can also be followed by end of input, written `$`.
- For each production, `predict` fills cells from FIRST of its right-hand side. If the right-hand side is nullable, it also fills cells from FOLLOW of its left-hand side. If two productions claim one cell, the grammar does not support a unique choice there.

| Lookahead with `S` on top | Production |
| --- | --- |
| `[` | `S → [ S ] S` |
| `]` or `$` | `S → ε` |

### Walk the stack

On input `[]`, the first `[` selects the bracket production. The parser pushes the right-hand side in reverse order so its first symbol is on top. It matches `[`, expands the inner `S` to empty on `]`, matches `]`, then expands the last `S` to empty on `$`. A terminal mismatch, missing table entry, or leftover input rejects the string.

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
