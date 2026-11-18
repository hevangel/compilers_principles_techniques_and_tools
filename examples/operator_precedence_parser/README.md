# Classical Operator-Precedence Parsing (1977 §5.3 / 1986 §4.6)

Verify operator-grammar invariants, compute `LEADING` and `TRAILING` sets, construct the Floyd `<·`, `=·`, `·>` precedence-relation matrix, compact it into `f(a)`/`g(b)` precedence functions, and parse expressions by prime-phrase handle reduction. See the [Rust source](main.rs) and [Python source](main.py).

```sh
cargo run --example operator_precedence_parser
cargo test --example operator_precedence_parser
python3 examples/operator_precedence_parser/main.py
python3 -m unittest examples/operator_precedence_parser/main.py
```

## What the example teaches

### Why we need this / What problem this solves

Classical **operator-precedence parsing** (Robert Floyd; 1977 Green §5.3, 1986 Red §4.6) is *not* the same technique as resolving shift/reduce conflicts in an LR table ([`precedence_parse`](../precedence_parse/README.md)) or shunting-yard operator stacks. Before LR item sets became standard in 1970s toolchains, operator-precedence parsers drove expression parsing directly from **pairwise relations between adjacent terminals**, completely ignoring nonterminals during handle boundary checks.

### Classical pipeline implemented here

1. **Operator-grammar invariant (`verify_operator_grammar`):** Every production must be `ε`-free and contain no two adjacent nonterminals `A B`.
2. **`LEADING(A)` and `TRAILING(A)` sets:** Fixpoint sets of terminals that can appear first (or last) in a sentential form derived from `A` (ignoring at most one leading/trailing nonterminal).
3. **Precedence relations (`<·`, `=·`, `·>`):**
   - `a =· b` for adjacent terminals `a b` or `a B b`,
   - `a <· b` for every `b ∈ LEADING(B)` in `a B`,
   - `c ·> a` for every `c ∈ TRAILING(B)` in `B a`,
   - `$ <· LEADING(S)` and `TRAILING(S) ·> $`.
4. **Precedence functions (`f(a)`, `g(b)`):** Merges `=·` pairs into equivalence groups and solves longest-path depths over the directed constraint DAG (`a ·> b ⇒ f(a) > g(b)`, `a <· b ⇒ f(a) < g(b)`), collapsing an `N × N` table into two `O(N)` integer arrays—or flagging a cycle if no linear ordering exists.
5. **Prime-phrase handle reduction:** Shifts while `top_term <· lookahead` or `=·`, and pops back across `prev_term <· popped_term` whenever `top_term ·> lookahead`.

### How to read the code

| Component | Rust (`main.rs`) | Python (`main.py`) | Role |
| --- | --- | --- | --- |
| Grammar check | `verify_operator_grammar` | `verify_operator_grammar` | Rejects `ε`-rules and adjacent nonterminals |
| Terminal boundary sets | `compute_leading_trailing` | `compute_leading_trailing` | Computes `LEADING`/`TRAILING` sets per nonterminal |
| Relation matrix & DAG | `build_precedence_matrix`, `build_precedence_functions` | `build_precedence_matrix`, `build_precedence_functions` | Constructs `<· / =· / ·>` matrix and `f(a)`/`g(b)` integer tables |
| Prime-phrase driver | `parse_operator_precedence` | `parse_operator_precedence` | Shift-reduce trace comparing matrix and `f`/`g` decisions |

## What to look for

Running the example prints the derived `LEADING`/`TRAILING` sets, the `6 × 6` precedence matrix for `+, *, (, ), id, $`, the compacted `f`/`g` values, and the shift-reduce trace turning `"a + b * (c + d)"` into `((a + (b * (c + d))))`.

## Check your understanding

- Why does `(a + b) * c` give `(` `=·` `)` even though an operator like `+` sits inside `(...)`?
- What property of a relation graph causes `build_precedence_functions` to fail even when the `<· / =· / ·>` matrix itself has no conflicts?

## Edition mapping and scope

| 1977 Green | 1986 Red | 2006 Purple |
| --- | --- | --- |
| 5 (§5.3) | 4 (§4.6) | — *(removed; superseded by §4.8 LR precedence)* |

Numbers refer to chapters. Compare directly with [Ambiguous Precedence and Associativity Resolution](../precedence_parse/README.md) to see why LR `%left`/`%right` declarations superseded classical operator-precedence matrices in the 2006 Purple edition.
