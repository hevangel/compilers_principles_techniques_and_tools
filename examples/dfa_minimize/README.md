# DFA State Minimization

Collapse indistinguishable states in a deterministic finite automaton using partition refinement. See the [Rust source](main.rs) and [Python source](main.py).

```sh
cargo run --example dfa_minimize
cargo test --example dfa_minimize
python3 examples/dfa_minimize/main.py
python3 -m unittest examples/dfa_minimize/main.py
```

## What the example teaches

### Why we need this / What problem this solves

Lexical analyzer generators compile dozens of token regexes into one combined DFA. Without minimization, subset construction or naive DFA unions routinely produce **unreachable states** and **redundant duplicate states** (for example, two separate branches that behave identically on all future suffixes). That wastes memory and harms cache locality in generated transition tables.

**DFA minimization** (Myhill–Nerode quotient / Hopcroft-style partition refinement) produces the **unique minimum-state DFA** for a regular language. Two states `p` and `q` are **indistinguishable** (equivalent) if no suffix `w` leads one to an accepting state and the other to a rejecting state. Instead of testing infinitely many suffixes naively, partition refinement separates states that *definitely* differ first (accepting vs. non-accepting), then propagates distinguishability backward one transition at a time until no group can be split further.

### Minimization steps

1. `prune_unreachable` runs breadth-first search from `start` so unreachable sink or duplicate states (like state `5` in the sample) are discarded before partitioning.
2. `minimize` seeds the partition with at most two blocks: accepting states and non-accepting states (omitting any empty block so all-accept or all-reject DFAs don't carry an empty group).
3. On each refinement pass, every state is keyed by its **transition signature**—the tuple of target group IDs `(group[trans[s][0]], group[trans[s][1]], ...)`. States inside the same group with different signatures split into separate subgroups until the partition stabilizes.
4. Each final equivalence class becomes one quotient state in the minimal DFA, ordered deterministically with the start state's class at `q0`.

### Trace partition refinement on the inflated `(a|b)*abb` DFA

Input states: `0` (start), `1` (saw `a`, duplicate branch), `2` (saw `b`), `3` (saw `a`), `4` (saw `ab`), `5` (unreachable dead state), `6` (saw `abb`, accepting).

| Pass | Partition | Why states split or stay together |
| --- | --- | --- |
| After `prune_unreachable` | `{0, 1, 2, 3, 4, 6}` | Unreachable state `5` is dropped |
| Initial (accept vs. non-accept) | `{6}`, `{0, 1, 2, 3, 4}` | `6` accepts empty suffix `""`; the rest do not |
| Pass 1 signatures on `(a, b)` | `{6}`, `{0, 1, 2, 3}`, `{4}` | `4` goes to `(non-accept, accept)` on `(a, b)`, separating from `{0, 1, 2, 3}` |
| Pass 2 signatures on `(a, b)` | `{6}`, `{0, 2}`, `{1, 3}`, `{4}` | `0, 2` transition to `({1,3}, {0,2})`, while `1, 3` transition to `({1,3}, {4})` |
| Pass 3 (fixed point) | `{6}`, `{0, 2}`, `{1, 3}`, `{4}` | Every block is uniform -> algorithm stops |

Reindexed with start class first:

| Merged class | Quotient state | Meaning |
| --- | --- | --- |
| `{0, 2}` | `q0` | Start / saw `b` |
| `{1, 3}` | `q1` | Saw trailing `a` |
| `{4}` | `q2` | Saw trailing `ab` |
| `{6}` | `q3` | Saw `abb` (accepting) |

### How to read the code

| Component | Rust (`main.rs`) | Python (`main.py`) | Role |
| --- | --- | --- | --- |
| DFA table | `struct Dfa` (`accepts`) | `class Dfa` (`accepts`) | Dense transition matrix `trans[state][col]` and accepting set |
| Reachability pass | `fn prune_unreachable` | `def prune_unreachable` | BFS from `start` that removes unreachable states and reindexes `0..k-1` |
| Partition refinement | `fn minimize` | `def minimize` | Splits blocks by transition-group signatures and builds quotient DFA |
| Sample input | `fn inflated_abb_dfa` | `def inflated_abb_dfa` | 7-state DFA with duplicate branches and unreachable sink state `5` |

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
