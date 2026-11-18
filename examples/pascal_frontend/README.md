# Pascal-Subset Compiler Front End (1986 Appendix A)

Compile a typed Pascal-subset program—with case-insensitive keywords, `array[low..high]` bounds, nested procedure/function scopes, and `integer`-to-`real` coercions—into labeled Three-Address Code (TAC) and verify it on a minimal TAC simulator. See the [Rust source](main.rs) and [Python source](main.py).

```sh
cargo run --example pascal_frontend
cargo test --example pascal_frontend
python3 examples/pascal_frontend/main.py
python3 -m unittest examples/pascal_frontend/main.py
```

## What the example teaches

### Why we need this / What problem this solves

Earlier front-end examples study one mechanism at a time: scanning (`lexer`), recursive-descent parsing ([`recursive_descent`](../recursive_descent/README.md), scaled up in [`tiny_compiler`](../tiny_compiler/README.md)), scope chains (`scope_types`), arrays (`array_layout`), or numeric widening (`overload_coerce`). Real compilers (and the 1986 Red Dragon Book's Appendix A Pascal compiler project) must wire **all of those phases together** into one coherent pipeline:

1. **Disambiguating `..` vs. decimal points in Pascal lexing:** In `array[1..4] of integer`, `1` is followed immediately by a dot. A naive lexer seeing `digit + '.'` greedily parses `1.` as a floating-point literal and fails on `.4`. Looking ahead to require a digit after `.` preserves `IntLit(1)`, `Dots`, `IntLit(4)`.
2. **Scope chains & return-slot semantics:** Subprograms introduce local variables (`k`, `total`) and parameters (`limit`, `bonus`) that shadow or stay separate from global variables (`scores`, `i`, `avg`), while Pascal functions return values by assigning to the function's own name (`SumWeighted := total + bonus`).
3. **Lower-bound normalization & coercion during TAC emission:** Pascal lets programmers declare `array[1..4]`, whereas runtime memory starts at index `0`. Adjusting every index (`k - 1`) and inserting explicit `int_to_real` instructions at compile time guarantees the low-level IR stays type-safe and uniform.

### Worked trace on `WeightedScores`

| Program construct | Front-end action | Lowered TAC effect |
| --- | --- | --- |
| `scores: array[1..4] of integer` | Allocates 16 bytes (`4 * 4`) at global offset `0` | Array reads/writes emit `idx - 1` before `aload`/`astore` |
| `function SumWeighted(limit: integer; bonus: real): real` | Pushes local scope `SumWeighted`, binds params + return slot `SumWeighted__ret` | Emits `fn_sumweighted:` body ending in `return sumweighted__ret` |
| `total + bonus` | Detects `integer + real` mismatch; inserts widening | `tN := int_to_real(sumweighted_total)` then `tM := tN + sumweighted_bonus` |
| `SumWeighted(4, 2.5) / 2` | Emits `param` pushes, `call`, and widens divisor `2` for `/` | Evaluates `(10 + 40 + 30 + 80 + 2.5) / 2.0 = 81.25` |

### How to read the code

| Component | Rust (`main.rs`) | Python (`main.py`) | Role |
| --- | --- | --- | --- |
| Scanner | `lex`, `enum TokKind` | `lex`, `Token` | Case-insensitive keywords, `{...}`/`(*...*)` comments, and `..` vs. real literals |
| Symbol table | `SymTab`, `PasType` | `SymTab`, `PasType` | Nested lexical scopes, byte widths, and function signatures |
| Parser & type checker | `Parser::parse_*`, `widen_to_real` | `Parser.parse_*`, `widen_to_real` | Recursive-descent grammar rules, array bound checks, and `integer -> real` promotion |
| TAC IR & VM | `TacInst`, `exec_tac` | `TacInst`, `exec_tac` | Labeled three-address code emission and stack-frame execution verification |

## What to look for

Running the demo prints the global byte-offset symbol table (`scores` 16 bytes, `i` 4 bytes, `avg` 8 bytes), the complete emitted TAC listing (showing `int_to_real`, `aload`, `astore`, `param`, and `call`), and verifies `Execution write output: [Real(81.25)]`. Tests check non-boolean `if` conditions, inverted array bounds (`5..2`), duplicate declarations, and narrowing assignment errors (`integer := real`).

## Check your understanding

- Why must `SumWeighted := total + bonus` assign to `sumweighted__ret` rather than calling `SumWeighted` recursively?
- Where does the lowered TAC adjust `scores[k]` so `scores[1]` lands at zero-based offset `0`?

## Edition mapping and scope

| 1977 Green | 1986 Red | 2006 Purple |
| --- | --- | --- |
| — | app. A (Pascal project) | 2, 6 |

Numbers refer to chapters. Related readings and appendices are marked where applicable. See the [edition guide](../../README.md#edition-guide) for the bibliographic references and chapter coverage.

This original Pascal-subset compiler keeps code self-contained in one file per language and targets labeled three-address code rather than emitting host assembly.
