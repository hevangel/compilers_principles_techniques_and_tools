# Epsilon-NFA to DFA

Turn sets of possible NFA states into deterministic transition rows. See the [Rust source](main.rs) and [Python source](main.py).

```sh
cargo run --example subset_dfa
cargo test --example subset_dfa
python3 examples/subset_dfa/main.py
python3 -m unittest examples/subset_dfa/main.py
```

## What the example teaches

### Why we need this / What problem this solves

Regular-expression builders (like Thompson's construction) naturally produce **nondeterministic** finite automata (NFAs) with epsilon edges (`ε` moves that consume no input) and multiple outgoing transitions on the same character. If a lexer ran an NFA naively with **backtracking** (guess a branch, and rewind if it fails), matching or rejecting a string could revisit states exponentially many times. Even tracking a set of NFA states at runtime requires computing `eps_closure` on every single input byte.

**Subset construction** (`determinize`) eliminates both backtracking and runtime closures up front: **one DFA state represents the entire epsilon-closed set of NFA states** the machine could possibly occupy after reading a prefix. Because transitions between sets are precomputed into a flat integer table `rows`, scanning becomes a single array lookup per character.

### Build and use the transition table

1. `closure` follows all epsilon edges reachable from a set of NFA states. It repeats until the set stops growing (fixed point), so chains and cycles of epsilon edges terminate cleanly.
2. `determinize` seeds `subsets[0]` with `closure({0})`. While `rows.len() < subsets.len()` (acting as a BFS queue of newly discovered subsets), it computes `closure(move(subset, sym))` for every symbol in `ALPHABET = "abc"`.
3. A previously seen subset reuses its row index; a brand-new subset is appended to `subsets` and scheduled for processing. Notice that the **empty set `{}`** has no outgoing NFA moves, so all three of its transitions loop back to itself—acting as the **dead (sink) state**.
4. `accepts` steps through `rows` one character at a time. The final DFA state accepts if its subset contains NFA accepting state `4` (`FINAL`).

### Trace the DFA table and prefixes for `ab* | ac`

The sample NFA has two epsilon branches out of state `0`: `0 -ε-> 1 -a-> 3 -b-> 3 -ε-> 4` (for `ab*`) and `0 -ε-> 2 -a-> 5 -c-> 4` (for `ac`).

| Prefix | DFA State | NFA Subset | Explanation |
| --- | --- | --- | --- |
| `""` (empty) | `0` | `{0, 1, 2}` | Start state `0` reaches `1` and `2` via epsilon edges |
| `"a"` | `1` | `{3, 4, 5}` | `1 -a-> 3` (then `3 -ε-> 4`) and `2 -a-> 5`; `4` is present so `"a"` accepts |
| `"ab"` | `2` | `{3, 4}` | Only `3 -b-> 3` fires (then `3 -ε-> 4`); `ac` branch is dropped |
| `"ac"` | `3` | `{4}` | Only `5 -c-> 4` fires; accepting |
| `"abc"` | `4` | `{}` | After `"ab"` (`{3, 4}`), neither `3` nor `4` has a `c` edge -> dead state `{}` |

### How to read the code

| Component | Rust (`main.rs`) | Python (`main.py`) | Role |
| --- | --- | --- | --- |
| NFA edge | `struct Edge` | `class Edge` | Directed edge `(from, symbol, to)` where `None` means epsilon |
| Epsilon closure | `fn closure` | `def closure` | Fixed-point reachability over `None` (epsilon) edges |
| DFA table | `struct Dfa` (`subsets`, `rows`, `accepts`) | `class Dfa` | Precomputed transition matrix and table-driven string matcher |
| Subset construction | `fn determinize` | `def determinize` | BFS worklist turning reachable NFA subsets into integer DFA rows |
| Sample NFA | `fn sample` | `def sample` | Hand-built epsilon-NFA for `ab* \| ac` with accepting state `FINAL = 4` |

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
