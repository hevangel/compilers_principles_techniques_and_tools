# DFA State Minimization

Collapse indistinguishable states in a deterministic finite automaton using partition refinement. See the [Rust source](main.rs) and [Python source](main.py).

```sh
cargo run --example dfa_minimize
cargo test --example dfa_minimize
python3 examples/dfa_minimize/main.py
python3 -m unittest examples/dfa_minimize/main.py
```

## What the example teaches

Two DFA states are distinguishable if some input string leads one to an accepting state and the other to a rejecting state. Partition refinement starts by separating accepting and non-accepting states, then repeatedly splits any group whose members transition to different groups on the same alphabet symbol.

### Minimization steps

1. `prune_unreachable` runs breadth-first search from `start` so unreachable sink or duplicate states are discarded first.
2. `minimize` seeds the partition with accepting and non-accepting state sets (omitting any empty block).
3. On each pass, every state is keyed by the target group indices of its outgoing transitions; states with distinct transition signatures split into separate subgroups until the partition stabilizes.
4. Each final equivalence class becomes one quotient state in the minimal DFA.

### Trace on the inflated `(a|b)*abb` DFA

| Initial states | Merged class | Meaning |
| --- | --- | --- |
| `{0, 2}` | `q0` | Start / saw `b` |
| `{1, 3}` | `q1` | Saw trailing `a` |
| `{4}` | `q2` | Saw trailing `ab` |
| `{6}` | `q3` | Saw `abb` (accepting) |

## What to look for

The 7-state input DFA (with redundant branches and unreachable state 5) collapses to 4 minimal states while accepting and rejecting the exact same strings of length up to 6. Edge-case tests also verify single-state all-accept and all-reject quotients.

## Check your understanding

- Why must unreachable states be removed before interpreting the final number of partition blocks as the minimal state count?
- Why can an accepting state and a non-accepting state never share a group in the initial partition?

## Edition mapping and scope

| 1977 Green | 1986 Red | 2006 Purple |
| --- | --- | --- |
| 3 | 3 | 3 |

Numbers refer to chapters. Related readings and appendices are marked where applicable. See the [edition guide](../../README.md#edition-guide) for the bibliographic references and chapter coverage.

The implementation operates on an explicit dense transition table and sorts final equivalence classes deterministically rather than compressing sparse rows.
