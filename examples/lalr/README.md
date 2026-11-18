# Canonical LR(1), LALR(1), and Table Compaction

Compare SLR(1), LALR(1), and canonical LR(1) table generation, conflict detection, and default-action table compaction. See the [Rust source](main.rs) and [Python source](main.py).

```sh
cargo run --example lalr
cargo test --example lalr
python3 examples/lalr/main.py
python3 -m unittest examples/lalr/main.py
```

## What the example teaches

### Why we need this / What problem this solves

SLR(1) is simple, but its `FOLLOW(LHS)` approximation is too coarse for many real-world grammars. Consider the classic C-like pointer/l-value grammar:

```text
S -> L = R | R
L -> * R | id
R -> L
```

Here `=` can appear after `R` *in general* (`S -> L = R` and `R -> L`, so `= ∈ FOLLOW(R)`), even though a top-level `R` at the start of an assignment (`id = id`) **never** legitimately precedes `=`. In the state where `id` was just shifted (`[L -> id ·]`, `[S -> L · = R, R -> L ·]`), SLR(1) sees `=` in `FOLLOW(R)` and tries to *both* shift `=` (to parse `L = R`) *and* reduce `R -> L`—producing a spurious shift/reduce conflict!

- **Canonical LR(1)** fixes this by attaching an exact 1-token lookahead to every item (`[A -> α · B β, a]`), distinguishing states by context. However, LR(1) creates many states with the exact same LR(0) core (`(prod, dot)` pairs) that differ only in lookaheads.
- **LALR(1)** merges LR(1) states that share the same LR(0) core while uniting their lookahead sets—achieving LR(0)-sized tables with far better lookahead precision than SLR(1). (In rare grammars like `lr1_only_grammar`, merging distinct contexts can still cause a reduce/reduce conflict.)
- **Table compaction (`compact_table`)** shrinks sparse `states × terminals` action tables by hoisting a single `default_reduce` action on reduce-only rows.

### Comparing parser power and compaction

1. `closure_lr1` propagates exact lookaheads using `FIRST(beta a)` for every `[A -> alpha . B beta, a]`.
2. `build_lalr1_table` merges canonical LR(1) states that share identical `(prod, dot)` cores, shrinking the automaton without losing lookahead precision on most practical grammars.
3. `compact_table` replaces reduce-only rows with a single default reduction per state, reducing the number of stored action entries.

### Grammar hierarchy and concrete state comparison

| Grammar | SLR(1) | LALR(1) | LR(1) | Why? |
| --- | --- | --- | --- | --- |
| L-value / pointer (`S -> L = R \| R`) | Shift/reduce conflict | Conflict-free | Conflict-free | After `L`, `R -> L ·` only carries lookahead `$` in context, never `=` |
| Core-merge conflict (`S -> a E c \| a F d \| b F c \| b E d`, `E -> e`, `F -> e`) | Conflict | Reduce/reduce conflict | Conflict-free | Merging `[E -> e ·, c]`+`[F -> e ·, d]` (`a e...`) with `[F -> e ·, c]`+`[E -> e ·, d]` (`b e...`) puts `{c, d}` on both `E` and `F` |

### How to read the code

| Component | Rust (`main.rs`) | Python (`main.py`) | Role |
| --- | --- | --- | --- |
| Grammar & LR(1) items | `enum Symbol`, `struct Lr1Item`, `struct Grammar` | `Nt`, `Term`, `Lr1Item`, `class Grammar` | Computes `first_sets`, `follow_sets`, `closure_lr1`, `goto_lr1` |
| Table builders | `build_slr1_table`, `build_lalr1_table`, `build_lr1_table` | `build_slr1_table`, `build_lalr1_table`, `build_lr1_table` | Constructs all three tables and collects `(state, term, prev, act)` conflicts |
| Compaction | `struct CompactTable`, `fn compact_table` | `class CompactTable`, `def compact_table` | Hoists `default_reduce` for rows with no shift/accept entries |
| Driver & samples | `parse`, `lvalue_grammar`, `lr1_only_grammar` | `parse`, `lvalue_grammar`, `lr1_only_grammar` | Shift-reduce parser and the two hierarchy-separating test grammars |

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
