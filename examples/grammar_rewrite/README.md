# Grammar Rewriting: Left Recursion Elimination and Left Factoring

Rewrite context-free grammars to eliminate indirect and direct left recursion and factor shared prefixes for top-down parsing. See the [Rust source](main.rs) and [Python source](main.py).

```sh
cargo run --example grammar_rewrite
cargo test --example grammar_rewrite
python3 examples/grammar_rewrite/main.py
python3 -m unittest examples/grammar_rewrite/main.py
```

## What the example teaches

### Why we need this / What problem this solves

Grammar authors write rules in the way that best matches language meaning—such as `S -> A a | b, A -> S d | c` (left recursion for left-associative repetition) or `S -> i E t S | i E t S e S | a` (shared `if cond then stmt` prefix). A naive **top-down (recursive-descent or LL(1)) parser** fails on both:

1. **Infinite loop on left recursion:** If `A()` expands `A -> A alpha` by calling `A()` before consuming any terminal, it enters unbounded recursion and overflows the stack. Worse, the recursion can be **indirect** (`S -> A a`, `A -> S d` forms a cycle `S => A a => S d a`).
2. **Prefix ambiguity with 1-token lookahead:** In `S -> i E t S | i E t S e S`, both productions begin with `i E t S`. With only 1 token of lookahead (`i`), an LL(1) parser cannot see far enough ahead to know whether an `e` (`else`) will follow later.

Two mechanical, language-preserving rewrites fix these issues: **left-recursion elimination** converts left recursion into right-recursive tail rules, and **left factoring** postpones the choice until after the shared prefix has been parsed. Together they make grammars directly executable by both handwritten [`recursive_descent`](../recursive_descent/README.md) (which turns eliminated left recursion into `while` loops) and table-driven [`ll1`](../ll1/README.md) parsers.

### Two rewriting passes

1. `eliminate_left_recursion` first validates the grammar (`validate_cfg`, rejecting missing start symbols or pure unit cycles like `S -> S`). It then orders nonterminals `A_1 ... A_n`, substitutes earlier heads `A_i -> A_j gamma` (`j < i`) so all indirect cycles surface on `A_i`, and rewrites direct left recursion `A -> A alpha | beta` into `A -> beta A'`, `A' -> alpha A' | eps` (rejecting rules with no non-left-recursive base case `beta`).
2. `left_factor` repeatedly finds the longest non-empty prefix shared by at least two alternatives of a nonterminal and moves their differing suffixes into a fresh tail nonterminal `N_f`.

### Rewriting traces on the two sample grammars

**1. Indirect left recursion (`S -> A a | b`, `A -> S d | c` with order `[S, A]`):**

| Step | Nonterminal | Transformation | Productions afterward |
| --- | --- | --- | --- |
| Start | `S, A` | None | `S -> A a \| b`, `A -> S d \| c` |
| `i = 0` (`S`) | `S` | No `j < 0`, no direct `S -> S...` | `S -> A a \| b` unchanged |
| `i = 1`, `j = 0` | `A` | Substitute `S` into `A -> S d` | `A -> A a d \| b d \| c` |
| `i = 1` direct | `A` | Split `alpha = [a d]`, `beta = [b d, c]` | `A -> b d A' \| c A'`, `A' -> a d A' \| eps` |

**2. Left factoring dangling-else (`S -> i E t S | i E t S e S | a`):**

| Step | Shared prefix | Productions afterward |
| --- | --- | --- |
| Before | `i E t S` (length 4) | `S -> i E t S \| i E t S e S \| a` |
| After | Factor into `S_f` | `S -> a \| i E t S S_f`, `S_f -> eps \| e S` |

### How to read the code

| Component | Rust (`main.rs`) | Python (`main.py`) | Role |
| --- | --- | --- | --- |
| CFG representation | `enum Symbol`, `struct Cfg` | `Terminal`, `NonTerminal`, `class Cfg` | Grammar symbols, nonterminal order, productions, and fresh-name generator |
| Pre-check | `fn validate_cfg` | `def validate_cfg` | Computes nullable set and rejects missing rules or pure unit cycles (`A =>+ A`) |
| Left-recursion pass | `fn eliminate_left_recursion` | `def eliminate_left_recursion` | Ordered substitution `j < i` plus `A -> beta A'` rewrite |
| Left-factoring pass | `fn left_factor` | `def left_factor` | Iterative longest-common-prefix extraction into fresh `_f` tail rules |
| Verification helpers | `has_left_recursion`, `derive_bounded` | `has_left_recursion`, `derive_bounded` | Checks cycle freedom and language equivalence up to bounded length |

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

The rewriter operates purely on CFG productions and does not carry semantic actions across substitutions. For formal leftmost/rightmost derivations and parse-tree ambiguity prior to rewriting, see [CFG Derivations, Parse Trees, and Ambiguity](../cfg_derivations/README.md).
