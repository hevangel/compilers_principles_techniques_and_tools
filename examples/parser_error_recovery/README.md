# LL(1) Synchronization Sets and LR Error Recovery (Ch. 4)

Integrate error recovery directly into LL(1) tables (`FOLLOW` synchronization entries + virtual delimiter insertion) and SLR(1) shift-reduce tables (`error` synchronization productions and stack unwinding) while suppressing error cascades. See the [Rust source](main.rs) and [Python source](main.py).

```sh
cargo run --example parser_error_recovery
cargo test --example parser_error_recovery
python3 examples/parser_error_recovery/main.py
python3 -m unittest examples/parser_error_recovery/main.py
```

## What the example teaches

### Why we need this / What problem this solves

The simpler [`recovery`](../recovery/README.md) lesson splits source text cleanly at top-level `;` characters. Real compilers work inside **parser tables and state stacks**: a missing `;`, extra `+`, or stray token leaves nonterminals and LR state numbers sitting on the stack, and naive skipping causes a single typo to emit a cascade of 5–10 spurious errors.

### Two table-integrated recovery designs

1. **LL(1) `FOLLOW`-based synchronization + phrase repair (`Ll1Table`):**
   - Populates empty `M[A, b]` cells with `Synch` whenever `b ∈ FOLLOW(A) ∪ { ";", "$" }`.
   - On `Synch`: pops the unmatchable nonterminal `A` and resumes at `b`.
   - On an empty cell: skips the unexpected input token until a `FIRST(A)` or `Synch` token arrives.
   - On a missing terminal `;` or `)`: performs phrase-level **virtual token insertion** instead of aborting the next statement.
   - Tracks an error cooldown flag so each broken construct emits **one** primary diagnostic.
2. **SLR(1) `error`-production stack unwinding (`S -> error ;`):**
   - Adds an explicit `error` pseudo-token production `Stmt -> error ;` to the grammar before building the SLR automaton.
   - On `ACTION[state, la] == Error`: pops stack states until reaching a state with `ACTION[s, error] == Shift(s_err)`, pushes `s_err`, discards tokens until `;` shifts and reduces `Stmt -> error ;`, and resumes parsing remaining statements cleanly.

### How to read the code

| Component | Rust (`main.rs`) | Python (`main.py`) | Role |
| --- | --- | --- | --- |
| LL(1) `Synch` table | `struct Ll1Table`, `build_ll1_table` | `class Ll1Table` | Computes `FIRST`, `FOLLOW`, production cells, and `Synch` markers |
| LL(1) recovery driver | `ll1_parse_with_recovery` | `ll1_parse_with_recovery` | Coordinates `Synch` pops, token skipping, virtual `;` insertion, and cooldown |
| SLR(1) `error` builder | `struct SlrParser`, `build_slr_with_error_prod` | `class SlrParser` | Builds SLR(1) states & `ACTION`/`GOTO` tables for `S -> error ;` |
| LR stack unwind driver | `slr_parse_with_recovery` | `slr_parse_with_recovery` | Unwinds stack to nearest `error`-accepting state and discards to `;` |

## What to look for

Both recovery engines parse `"let x = + 5; let = 9; emit (1 + ); emit 42;"`, recovering `emit 42;` after three broken statements without error floods, and unit tests verify missing `;` phrase insertion (`"let a = 1 emit 2; ..."`).

## Check your understanding

- Why does an LL(1) `Synch` entry **pop** the nonterminal `A` rather than **skipping** the lookahead token `b`?
- Why must Yacc/SLR grammars place a synchronizing delimiter (like `;` or `}`) right after `error` in `S -> error ;`?

## Edition mapping and scope

| 1977 Green | 1986 Red | 2006 Purple |
| --- | --- | --- |
| 11 | 4 (§4.4, §4.7–4.8) | 4 (§4.4.5, §4.7–4.8) |

Numbers refer to chapters. Expands upon [Panic-mode recovery](../recovery/README.md) with real parser table and stack integration.
