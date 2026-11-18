# Parsing Expression Grammars (PEGs) and Packrat Memoization *(Modern Extension)*

Execute ordered-choice PEGs (`/`), zero-width syntactic predicates (`&e` and `!e`), and Packrat memoization `(rule, pos) -> result`, avoiding exponential backtracking while recognizing non-context-free `{a^n b^n c^n}`. See the [Rust source](main.rs) and [Python source](main.py).

> **Edition note:** Parsing Expression Grammars and Packrat memoization (Ford 2002/2004; used by CPython's `pegen` parser generator) are modern parsing extensions placed here under **Advanced parsing techniques** rather than classical Dragon Book editions.

```sh
cargo run --example peg_packrat
cargo test --example peg_packrat
python3 examples/peg_packrat/main.py
python3 -m unittest examples/peg_packrat/main.py
```

## What the example teaches

### Why we need this / What problem this solves

Context-free grammars use *unordered* alternation `|` where ambiguity can produce multiple parse trees, while recursive-descent backtracking without memoization explodes to `O(2^d)` runtime on nested `( ... )` expressions when an outer fallback re-parses an inner subexpression from scratch.

A **PEG** replaces `|` with deterministic **ordered choice (`/`):** if the first branch succeeds, it commits permanently—eliminating ambiguity by construction. Adding **Packrat memoization** caches `(rule_name, pos) -> result` so every rule executes at most once per input offset, bounding total parsing work to linear `O(n · |G|)` time.

### Syntactic predicates & ordered choice pitfalls

1. **Linear-time nested backtracking:** On `((((((((((id))))))))))` with `Expr <- Prod '+' Expr / Prod`, every unmemoized nesting layer doubles failed `Prod` re-evaluations (`O(2^d)`), whereas Packrat memoization turns repeat parses at each offset into cache hits.
2. **Beyond CFGs with `&e` and `!e`:** Zero-width lookahead predicates check conditions without consuming input. Rule `S <- &(A "c") "a"+ B !.` enforces equal `a`/`b` counts (`A`) overlapped with equal `b`/`c` counts (`B`), matching exact `{a^n b^n c^n | n >= 1}` with zero imperative counters.
3. **The `'a' / 'ab'` prefix trap:** Because `/` commits greedily on the first success, `'a' / 'ab' !.` rejects `"ab"`—showing why longest/more-specific alternatives must appear first in PEGs.

### How to read the code

| Component | Rust (`main.rs`) | Python (`main.py`) | Role |
| --- | --- | --- | --- |
| PEG AST | `enum Peg`, `struct PegGrammar` | `Lit`, `Seq`, `Choice`, `AndPred`, `NotPred` | Models literals, rule calls, `/`, `*`/`+`/`?`, `&e`, `!e` |
| Memoizing VM | `struct PegRunner`, `eval_rule`, `eval_expr` | `class PegRunner` | Toggles naive backtracking vs. `(rule, pos)` Packrat memo table |
| Built-in grammars | `expr_peg`, `an_bn_cn_peg`, `ordered_choice_trap_peg` | `expr_peg`, `an_bn_cn_peg`, `ordered_choice_trap_peg` | Demonstrates `O(2^d) -> O(n)`, `{a^n b^n c^n}`, and `/` ordering |

## What to look for

The sample prints the call-count gap on depth-10 nested parentheses, checks all 5 sample strings against `{a^n b^n c^n}`, and confirms why `"ab"` fails under `'a' / 'ab' !.`.

## Check your understanding

- Why does putting `'ab' / 'a'` instead of `'a' / 'ab'` fix the prefix trap?
- Why can't a classic deterministic `LL(1)` or `LR(1)` CFG recognize `{a^n b^n c^n}` while a PEG with `&e` can?

## Edition mapping and scope

| 1977 Green | 1986 Red | 2006 Purple |
| --- | --- | --- |
| — *(modern extension)* | — *(modern extension)* | — *(modern extension)* |
