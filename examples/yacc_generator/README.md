# Yacc-Style Parser Generator (`.y` DSL Compiler)

Compile a `.y` grammar specification—with `%token`, `%left`/`%right`/`%nonassoc`, `%prec`, `%empty`, and `{ ... }` semantic actions—into a merged LALR(1) parser table and evaluate input programs directly from the generated tables. See the [Rust source](main.rs) and [Python source](main.py).

```sh
cargo run --example yacc_generator
cargo test --example yacc_generator
python3 examples/yacc_generator/main.py
python3 -m unittest examples/yacc_generator/main.py
```

## What the example teaches

### Why we need this / What problem this solves

Earlier parsing examples study LALR(1) state merging on small conflict-free grammars (`lalr`) or precedence rules on hand-written operator stacks (`precedence_parse`). A real **`.y` parser generator** (Yacc / Bison, `§4.9`) automates the whole pipeline from one grammar DSL file:

1. **Concise ambiguous grammars instead of stratified boilerplate:** Writing `expr : expr '+' expr | expr '*' expr | '-' expr %prec UMINUS` is short and readable, even though it produces shift/reduce conflicts on nearly every operator state.
2. **Declarative disambiguation during LALR(1) table generation:** Comparing each rule's precedence (from `%prec`, or its rightmost terminal) against the lookahead token's `%left`/`%right`/`%nonassoc` tier decides whether an ambiguous state **shifts** (`*` after `+`), **reduces** (`-` before `-`), or emits a **non-associative syntax error** (`1 < 2 < 3`).
3. **Default Yacc conflict recovery & semantic actions (`$1`, `$3`):** Unannotated shift/reduce conflicts default to **shift**, reduce/reduce conflicts pick the **earliest-listed** production rule, and every reduction pops `k` values from the semantic value stack to run its `{ ... }` action.

### Precedence & conflict resolution on `x = 2 + 3 * -4; x * 2 + 1`

| Conflict situation | Rule precedence vs. lookahead | Yacc decision | Result on sample |
| --- | --- | --- | --- |
| After `expr '+' expr`, see `'*'` | `'+'` (level 3) < `'*'` (level 4) | **Shift** `'*'` | `3 * -4` binds before `+` |
| After `'-' expr` (`%prec UMINUS`), see `'*'` | `UMINUS` (level 5) > `'*'` (level 4) | **Reduce** `neg($2)` | `-4` binds tighter than `*` |
| After `expr '-' expr`, see `'-'` | Equal level 3, `%left` | **Reduce** `sub($1, $3)` | `10 - 3 - 2` evaluates as `(10 - 3) - 2 = 5` |
| After `expr '<' expr`, see `'<'` | Equal level 2, `%nonassoc` | **`NonAssocError`** | Rejects `1 < 2 < 3` cleanly |

### How to read the code

| Component | Rust (`main.rs`) | Python (`main.py`) | Role |
| --- | --- | --- | --- |
| `.y` DSL & action parser | `parse_yacc_spec`, `ActionParser` | `parse_yacc_spec`, `ActionParser` | Parses `%token`/`%left`/`%right`/`%nonassoc`/`%start`, `%prec`, `%empty`, and `{ ... }` `$N` calls |
| LR(1) -> LALR(1) builder | `closure_lr1`, `goto_lr1`, `compile_yacc_spec` | `closure_lr1`, `goto_lr1`, `compile_yacc_spec` | Builds canonical LR(1) states, merges shared LR(0) cores, and populates `ACTION`/`GOTO` |
| Conflict resolver | `production_prec` + table loop | `production_prec` + table loop | Resolves S/R via precedence tiers and logs unresolved S/R (shift) or R/R (earlier rule) warnings |
| Shift-reduce runtime | `eval_action`, `parse_with_yacc` | `eval_action`, `parse_with_yacc` | Drives `(state_stack, val_stack)` across shifts and semantic action reductions |

## What to look for

Running the demo reports the canonical LR(1) vs. merged LALR(1) state counts, `0` unresolved conflicts on the precedence-annotated expression grammar, and evaluates `x = 2 + 3 * -4; x * 2 + 1` to `env = { "x": -10 }` and `result = -19`. Unit tests also verify `%nonassoc` rejection on `1 < 2 < 3`, unresolved shift/reduce warnings on unannotated operator grammars, and earlier-rule priority on reduce/reduce conflicts.

## Check your understanding

- Why does Yacc examine the **rightmost terminal** of `expr '+' expr` when determining that production's precedence unless `%prec` overrides it?
- What happens at parse time when `ACTION[state, '<']` is cleared by `%nonassoc` rather than choosing shift or reduce?

## Edition mapping and scope

| 1977 Green | 1986 Red | 2006 Purple |
| --- | --- | --- |
| 5–6, 8 | 4 (§4.8–4.9) | 4 (§4.9) |

Numbers refer to chapters. Related readings and appendices are marked where applicable. See the [edition guide](../../README.md#edition-guide) for the bibliographic references and chapter coverage.

This self-contained Yacc compiler compiles `.y` specs directly into in-memory LALR(1) `ACTION`/`GOTO` tables with a pure expression/environment action evaluator rather than emitting external C source files or `$N` typed unions.
