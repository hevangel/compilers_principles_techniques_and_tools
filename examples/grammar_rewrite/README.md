# Grammar Rewriting: Left Recursion Elimination and Left Factoring

Rewrite context-free grammars to eliminate indirect and direct left recursion and factor shared prefixes for top-down parsing. See the [Rust source](main.rs) and [Python source](main.py).

```sh
cargo run --example grammar_rewrite
cargo test --example grammar_rewrite
python3 examples/grammar_rewrite/main.py
python3 -m unittest examples/grammar_rewrite/main.py
```

## What the example teaches

A recursive-descent or LL(1) parser loops forever on left-recursive rules and cannot decide between productions that share a leading prefix. Two mechanical grammar rewrites eliminate these obstacles without changing the generated language.

### Two rewriting passes

1. `eliminate_left_recursion` orders nonterminals `A_1 ... A_n`, substitutes earlier heads `A_i -> A_j gamma` (`j < i`) to expose hidden indirect cycles, and converts direct left recursion `A -> A alpha | beta` into right-recursive `A -> beta A'`, `A' -> alpha A' | eps`.
2. `left_factor` repeatedly finds the longest non-empty prefix shared by at least two alternatives of a nonterminal and moves their differing suffixes into a fresh tail nonterminal.

### Rewriting `S -> A a | b`, `A -> S d | c`

| Step | Grammar transformation |
| --- | --- |
| Substitute `S` into `A` | `A -> A a d \| b d \| c` |
| Eliminate direct recursion on `A` | `A -> b d A' \| c A'`, `A' -> a d A' \| eps` |

## What to look for

The worked trace removes indirect left recursion from `S -> A a | b, A -> S d | c` and factors the common prefix `i E t S` out of the dangling-else grammar. Tests verify that left recursion is completely eliminated while preserving the bounded generated language, and that unit cycles or base-less recursive rules are rejected cleanly.

## Check your understanding

- Why does ordered substitution `j < i` eliminate indirect left recursion without creating new indirect cycles among already processed nonterminals?
- Why can left factoring still leave an LL(1) conflict even when all prefixes are factored?

## Edition mapping and scope

| 1977 Green | 1986 Red | 2006 Purple |
| --- | --- | --- |
| 4–5 | 4 | 4 |

Numbers refer to chapters. Related readings and appendices are marked where applicable. See the [edition guide](../../README.md#edition-guide) for the bibliographic references and chapter coverage.

The rewriter operates purely on CFG productions and does not carry semantic actions across substitutions.
