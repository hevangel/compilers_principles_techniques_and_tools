# DFA Optimization: Equivalence Classes, Comb Tables, and Direct-Coded Scanners (§3.9)

Shrink a sparse `states × 128` DFA table using character equivalence classes and `default / base / next / check` row-displacement compression, and compare against a direct-coded scanner. See the [Rust source](main.rs) and [Python source](main.py).

```sh
cargo run --example dfa_optimization
cargo test --example dfa_optimization
python3 examples/dfa_optimization/main.py
python3 -m unittest examples/dfa_optimization/main.py
```

## What the example teaches

### Why we need this / What problem this solves

DFA minimization (`dfa_minimize`) merges indistinguishable **rows** (states), but a scanner DFA still stores a wide column for every possible byte (`128` ASCII or `256` UTF-8 bytes), most of which hold `DEAD` transitions. On real language grammars with 150+ DFA states, a naive `states × 256` array wastes tens of kilobytes and pollutes CPU caches.

Dragon Book **§3.9** applies three progressive optimizations without changing the accepted language:

1. **Character Equivalence Classes:** Bytes with identical transition columns across every state (e.g., `a`..`z` excluding letters appearing in keywords) collapse into one equivalence class, shrinking `128` columns down to ~`12`.
2. **Row-Displacement ("Comb") Compression (`default / base / next / check`):** Similar states (like keyword prefixes `l`, `le`, `let` and generic `IDENT`) share almost identical rows. Each state `s` picks an earlier state as `default[s]` and packs only the differing transitions into shared flat `next` and `check` arrays via `base[s]`.
3. **Direct-Coded Scanner:** Instead of indexing arrays in memory at every byte, a generated `match state` / `match byte` function turns DFA edges directly into machine branches.

### How to read the code

| Component | Rust (`main.rs`) | Python (`main.py`) | Role |
| --- | --- | --- | --- |
| Full 2D DFA | `struct DenseDfa`, `build_scanner_dfa` | `class DenseDfa`, `build_scanner_dfa` | 19-state multi-pattern scanner over 128 ASCII bytes (`2432` cells) |
| Equivalence partitioning | `build_equiv_classes` | `build_equiv_classes` | Collapses identical transition columns into `char_class[128]` + `C`-column table |
| Comb compression | `struct CompressedCombDfa` | `class CompressedCombDfa` | Computes acyclic `default[]`, packs diffs into `next[]`/`check[]`, implements `step()` |
| Direct-coded runner | `direct_coded_step`, `emit_direct_coded_snippet` | `direct_coded_step`, `emit_direct_coded_snippet` | Executes and previews table-free control-flow dispatch |

## What to look for

Running the example prints the exact cell-count drop from full 2D (`2432` cells) → equivalence-classed 2D → `default/base/next/check` comb encoding, verifies byte-for-byte transition parity across all `19 × 128` pairs in unit tests, and asserts all four scanner engines emit the exact same tokens.

## Check your understanding

- Why must `default[s]` only point to states with fewer diffs (and never form a cycle)?
- When might a **table-driven** equivalence-classed scanner beat a **direct-coded** one in practice, and vice versa?

## Edition mapping and scope

| 1977 Green | 1986 Red | 2006 Purple |
| --- | --- | --- |
| — | 3 (§3.9) | 3 (§3.9) |

Numbers refer to chapters. Builds on [DFA State Minimization](../dfa_minimize/README.md) and [Lex-Style Scanner Generator](../lex_generator/README.md).
