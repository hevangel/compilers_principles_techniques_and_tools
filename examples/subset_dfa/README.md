# Epsilon-NFA to DFA

Turn sets of possible NFA states into deterministic transition rows. See the [Rust source](main.rs).

```sh
cargo run --example subset_dfa
cargo test --example subset_dfa
```

## What the example teaches

Each DFA state represents an epsilon-closed set of NFA states. For each input symbol, `determinize` moves from every state in the current set, takes the epsilon closure, and reuses the resulting set if it already has a row. The empty set is the dead state.

## What to look for

The hand-built NFA for `ab*|ac` accepts `a`, `abbb`, and `ac` and rejects `abc`. The test also checks empty input and characters outside the alphabet.

## Edition mapping and scope

| 1977 Green | 1986 Red | 2006 Purple |
| --- | --- | --- |
| 3 | 3 | 3 |

Numbers refer to chapters. Related readings and appendices are marked where applicable. See the [edition guide](../../README.md#edition-guide) for the bibliographic references and chapter coverage.

The NFA and alphabet are supplied directly. The program performs subset construction and leaves the resulting DFA unminimized.
