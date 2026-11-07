# Epsilon-NFA to DFA

Turn sets of possible NFA states into deterministic transition rows. See the [Rust source](main.rs).

```sh
cargo run --example subset_dfa
cargo test --example subset_dfa
```

## What the example teaches

Each DFA state represents an epsilon-closed set of NFA states. For each input symbol, `determinize` moves from every state in the current set, takes the epsilon closure, and reuses the resulting set if it already has a row. The empty set is the dead state.

A **finite automaton** recognizes strings by following labeled edges from a start state. A nondeterministic finite automaton (NFA) can have several possible next states. An **epsilon** edge can also be taken without consuming a symbol. A deterministic finite automaton (DFA) has exactly one next state for each state and alphabet symbol.

Subset construction turns that apparent choice into bookkeeping. One DFA state stands for the entire set of NFA states the machine could be in after reading a prefix.

### Build and use the transition table

1. `closure` follows all epsilon edges reachable from a set. It repeats until the set stops growing, so chains and cycles of epsilon edges work.
2. `determinize` starts with the closure of NFA state 0. For each known set and alphabet symbol, it takes every matching labeled edge, then closes the resulting destination set under epsilon edges.
3. A previously seen set reuses its row number. A new set gets its own row, which will be filled on a later iteration. The empty set has no outgoing NFA moves, so all three of its transitions lead back to itself.
4. `accepts` runs the resulting table. The final DFA row is accepting if its subset contains NFA accepting state 4.

### Trace two prefixes

| Prefix | Possible NFA states | Explanation |
| --- | --- | --- |
| Empty | `{0, 1, 2}` | Both epsilon branches are available |
| `a` | `{3, 4, 5}` | Both branches match; state 3 can reach 4 by epsilon |
| `ab` | `{3, 4}` | The repetition branch remains possible |
| `abc` | `{}` | After `ab`, there is no `c` edge |
| `ac` | `{4}` | Taking `c` immediately after `a` reaches the accepting state |

## What to look for

The hand-built NFA for `ab*|ac` accepts `a`, `abbb`, and `ac` and rejects `abc`. The test also checks empty input and characters outside the alphabet.

## Check your understanding

- Predict the subset for `abb` and for `acc`. Locate the corresponding transitions in the printed table.
- Why does the machine accept `a` before reading any `b`? Identify the epsilon edge that makes the zero-repetition case possible.

## Edition mapping and scope

| 1977 Green | 1986 Red | 2006 Purple |
| --- | --- | --- |
| 3 | 3 | 3 |

Numbers refer to chapters. Related readings and appendices are marked where applicable. See the [edition guide](../../README.md#edition-guide) for the bibliographic references and chapter coverage.

The NFA and alphabet are supplied directly. The program performs subset construction and leaves the resulting DFA unminimized.
