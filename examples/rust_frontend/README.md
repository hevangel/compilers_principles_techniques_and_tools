# Rust-Subset Compiler Front End (Appendix A Extension)

Compile a statically typed Rust-subset program—with nested `/* /* ... */ */` block comments, `let` / `let mut` immutability checks, hygienic variable shadowing (`weight#5` -> `weight#6`), strict `as` numeric casts, `[T; N]` fixed arrays, and expression-oriented blocks/`if`/`else`—into labeled Three-Address Code (TAC) and verify it on a TAC VM. See the [Rust source](main.rs) and [Python source](main.py).

```sh
cargo run --example rust_frontend
cargo test --example rust_frontend
python3 examples/rust_frontend/main.py
python3 -m unittest examples/rust_frontend/main.py
```

## What the example teaches

### Why we need this / What problem this solves

Extending the Dragon Book's **Appendix A** front-end architecture to Rust highlights three front-end design guarantees that C, Pascal, and early Java compilers omit:

1. **Nested block comments during lexical analysis:** Unlike C/Java where the first `*/` closes `/* ...`, Rust tracks comment nesting depth (`depth += 1` on `/*`, `depth -= 1` on `*/`). Commenting out a whole block that already contains `/* inner */` would otherwise terminate early and feed fragments to the parser.
2. **Hygienic `let` shadowing paired with `let mut` enforcement:** Rebinding `let weight: i64 = if idx == 0 { weight } else { weight * 2 };` inside the same block allocates a distinct TAC slot (`weight#6`) instead of clobbering `weight#5`, whereas assigning `x = ...` on a non-`mut` binding fails during type checking.
3. **Expression-oriented blocks and zero implicit numeric widening:** In Rust, missing a trailing semicolon makes `{ ...; tail_expr }` evaluate to `tail_expr`'s type, `if ... { ... } else { ... }` requires identical arm types, and mixed arithmetic like `1 + 2.5` is rejected unless explicitly written `(acc as f64)`.

### Worked trace on `weighted_sum`

| Rust construct | Front-end action | Lowered TAC effect |
| --- | --- | --- |
| `let weight: i64 = if idx == 0 { weight } else { weight * 2 };` | Reads `weight#5`, allocates fresh `weight#6`, checks both `if`/`else` arms yield `i64` | Emits conditional branches writing into temporary `t3`, then `weight#6 = t3` |
| `(acc as f64) / scale + bonus` | Rejects implicit widening; emits explicit `Cast` from `i64` to `f64` | `t10 = acc#4 as f64`, `t11 = t10 / scale#7`, `t12 = t11 + bonus#2` -> `297.5` |
| `let rounded: i64 = total as i64;` | Truncates `f64` `297.5` cleanly and sets block tail expression `rounded` | Emits `emit total#9` (`297.5`) and returns `rounded#10` (`297`) |

### How to read the code

| Component | Rust (`main.rs`) | Python (`main.py`) | Role |
| --- | --- | --- | --- |
| Scanner | `lex`, `enum TokKind` | `lex`, `Token` | Nested `/* /* ... */ */` comment depth, `->`, `&&`/`\|\|`, keywords (`let`, `mut`, `as`, `i64`, `f64`) |
| Scope & mutability table | `ScopeEnv`, `VarInfo`, `RsType` | `ScopeEnv`, `VarInfo`, `RsType` | Tracks `mut` flags, creates fresh `name#N` slots on shadowing, and allocates stack byte offsets |
| Recursive-descent checker | `Parser::parse_block`, `parse_*` | `Parser.parse_block`, `parse_*` | Two-pass signature collection, expression-oriented blocks, `if/else` arm typing, and `as` casts |
| TAC IR & VM | `Emitter`, `TacInst`, `exec_tac` | `Emitter`, `TacInst`, `exec_tac` | Short-circuit `&&`/`\|\|` jumps, labeled frames, explicit `Cast`, and array bounds checks |

## What to look for

Running the demo prints the two collected signatures (`main`, `weighted_sum`), the `136`-byte slot allocation showing shadowed `weight#5` (`@ byte 64`) and `weight#6` (`@ byte 72`), the `42` emitted TAC instructions, and verifies `Emitted output : [F64(297.5)]` with `main() returned: 297`. Tests verify short-circuit `&&` bounds guards, runtime out-of-bounds errors, immutable reassignment rejection, implicit widening rejection, and `if`/`else` arm type mismatches.

## Check your understanding

- Why does `parse_program` scan all `fn` headers in a pre-pass before parsing function bodies?
- How does evaluating `{ let weight = if ... { weight } ... }` read the previous `weight#5` on the right-hand side before `weight#6` enters scope?

## Edition mapping and scope

| 1977 Green | 1986 Red | 2006 Purple |
| --- | --- | --- |
| — | 2, 6, 8 | app. A (Rust front end) |

Numbers refer to chapters. Related readings and appendices are marked where applicable. See the [edition guide](../../README.md#edition-guide) for the bibliographic references and chapter coverage.

This original Rust-subset front end keeps code self-contained in one file per language and targets labeled three-address code rather than emitting LLVM IR or machine code.
