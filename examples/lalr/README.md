# Canonical LR(1), LALR(1), and Table Compaction

Compare SLR(1), LALR(1), and canonical LR(1) table generation, conflict detection, and default-action table compaction. See the [Rust source](main.rs) and [Python source](main.py).

```sh
cargo run --example lalr
cargo test --example lalr
python3 examples/lalr/main.py
python3 -m unittest examples/lalr/main.py
```

## What the example teaches

LR(1) items pair each LR(0) production dot position with a single-token lookahead. Canonical LR(1) keeps states with identical LR(0) cores separate when their lookaheads differ; LALR(1) merges states sharing the same LR(0) core while uniting their lookahead sets; SLR(1) falls back to `FOLLOW(LHS)`.

### Comparing parser power and compaction

1. `closure_lr1` propagates lookaheads using `FIRST(beta a)` for every `[A -> alpha . B beta, a]`.
2. `build_lalr1_table` merges canonical LR(1) states that share identical `(prod, dot)` cores, shrinking the automaton without losing lookahead precision on most practical grammars.
3. `compact_table` replaces reduce-only rows with a single default reduction per state, reducing the number of stored action entries.

### Grammar hierarchy on two test grammars

| Grammar | SLR(1) | LALR(1) | LR(1) |
| --- | --- | --- | --- |
| L-value / pointer (`S -> L = R \| R`) | Shift/reduce conflict | Conflict-free | Conflict-free |
| Core-merge conflict (`S -> a E c \| ...`) | Conflict | Reduce/reduce conflict | Conflict-free |

## What to look for

The pointer/l-value grammar fails SLR(1) because `=` belongs to `FOLLOW(R)` even in a state that can only shift `=`, whereas LALR(1) and LR(1) parse `*id = id` cleanly with fewer states in LALR(1). A second test grammar constructs the classic case where LALR(1) core merging creates a reduce/reduce conflict that canonical LR(1) avoids.

## Check your understanding

- Why does merging two LR(1) states with the same LR(0) core never introduce a new shift/reduce conflict, even though it can introduce a reduce/reduce conflict?
- Why can default-reduction compaction safely omit error entries on states that only have reduce actions?

## Edition mapping and scope

| 1977 Green | 1986 Red | 2006 Purple |
| --- | --- | --- |
| 5–6 | 4 | 4 |

Numbers refer to chapters. Related readings and appendices are marked where applicable. See the [edition guide](../../README.md#edition-guide) for the bibliographic references and chapter coverage.

LALR(1) is constructed by merging canonical LR(1) collections rather than the iterative lookahead-propagation optimization, and compaction uses default reductions rather than two-dimensional row displacement.
