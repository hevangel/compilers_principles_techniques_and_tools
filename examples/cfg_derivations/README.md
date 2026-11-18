# Context-Free Grammars, Derivations, Ambiguity, and Limits (Ch. 4)

Compare leftmost and rightmost derivations, prove grammar ambiguity directly by enumerating multiple distinct parse trees, and contrast context-free `{a^n b^n}` against non-context-free `{a^n b^n c^n}`. See the [Rust source](main.rs) and [Python source](main.py).

```sh
cargo run --example cfg_derivations
cargo test --example cfg_derivations
python3 examples/cfg_derivations/main.py
python3 -m unittest examples/cfg_derivations/main.py
```

## What the example teaches

### Why we need this / What problem this solves

Parsing algorithms (`LL(1)`, `SLR`, `LALR(1)`) restrict grammars so parsing stays linear and deterministic. Before applying those algorithms, Dragon Book **§4.1–4.3** establishes what a context-free grammar actually means:

1. **Leftmost (`=>_lm`) vs. Rightmost (`=>_rm`) derivations:** Expanding the leftmost vs. rightmost nonterminal at each step follows different sentential forms (corresponding to top-down LL vs. reversed bottom-up LR traces) yet yields the exact same unique parse tree when a grammar is unambiguous.
2. **Formal proof of ambiguity:** A grammar is ambiguous iff at least one string has **two or more distinct parse trees** (not merely two derivation orders for the same tree!). `E -> E + E | E * E | id` produces **2** parse trees for `id + id * id`, and `S -> if c then S | if c then S else S | other` produces **2** parse trees for `if c then if c then other else other`.
3. **What CFGs cannot express:** A CFG pushdown automaton balances `{a^n b^n}` cleanly with one stack, but cannot enforce cross-serial equality `{a^n b^n c^n}` or declare-before-use identifier binding `{w ... w}`—which is why compilers hand parsed trees off to Chapter 5 semantic analysis.

### How to read the code

| Component | Rust (`main.rs`) | Python (`main.py`) | Role |
| --- | --- | --- | --- |
| Grammar & parse trees | `struct Cfg`, `enum ParseTree` | `class Cfg`, `ParseNode`, `ParseLeaf` | Defines `(V_N, V_T, P, S)` and concrete syntax trees |
| Derivation tracer | `fn derivation_from_tree` | `def derivation_from_tree` | Produces step-by-step `=>_lm` and `=>_rm` sentential forms |
| All-trees chart solver | `fn enumerate_parse_trees` | `def enumerate_parse_trees` | Memoized span solver enumerating all parse trees to prove ambiguity |
| Non-CFG boundary | `an_bn_cfg` vs. `is_an_bn_cn` | `an_bn_cfg` vs. `is_an_bn_cn` | Contrasts CFG `{a^n b^n}` against non-CFG `{a^n b^n c^n}` |

## What to look for

The output prints both conflicting parse trees for `id + id * id`, then shows the stratified `E -> E + T | T`, `T -> T * F | F`, `F -> id` grammar collapsing ambiguity to **1** parse tree while comparing its `=>_lm` and `=>_rm` derivations side by side.

## Check your understanding

- Why don't a leftmost and a rightmost derivation for the **same** expression under `E -> E + T | T` prove ambiguity?
- How does stratifying `E`, `T`, `F` force `id + id * id` to have only one parse tree while remaining context-free?

## Edition mapping and scope

| 1977 Green | 1986 Red | 2006 Purple |
| --- | --- | --- |
| 4–5 | 4 (§4.1–4.3) | 4 (§4.1–4.3) |

Numbers refer to chapters. Continues directly into [Grammar Rewriting](../grammar_rewrite/README.md), [Recursive-Descent Parsing](../recursive_descent/README.md), and [FIRST, FOLLOW, and LL(1)](../ll1/README.md).
