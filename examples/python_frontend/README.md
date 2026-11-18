# Python-Subset Compiler Front End (Appendix A Extension)

Compile an indentation-scoped Python-subset program—with off-side rule `INDENT`/`DEDENT`/`NEWLINE` scanning, bracketed multi-line joining, compile-time local-vs-`global` scope pre-scan (`UnboundLocalError` analysis), `/` vs. `//` division, and negative list indexing `arr[-1]`—into labeled Three-Address Code (TAC) and verify it on a Python-semantics TAC VM. See the [Rust source](main.rs) and [Python source](main.py).

```sh
cargo run --example python_frontend
cargo test --example python_frontend
python3 examples/python_frontend/main.py
python3 -m unittest examples/python_frontend/main.py
```

## What the example teaches

### Why we need this / What problem this solves

Adapting the Dragon Book's **Appendix A** front-end architecture to Python illuminates three front-end challenges absent from C, Pascal, or Java:

1. **Turning indentation into explicit delimiter tokens (`INDENT` / `DEDENT` / `NEWLINE`):** Because Python has no `{` or `}` block delimiters, the scanner maintains an indentation stack (`[0]`) and a bracket nesting counter (`bracket_depth`). Blank lines and `#`-comments are skipped, new spaces pushed beyond the top emit `INDENT`, outdents pop until matching an outer column (raising `IndentationError` if misaligned), and open `(...)`/`[...]` suppress line breaks cleanly.
2. **Whole-function scope pre-scan before binding (`LEGB` & `UnboundLocalError`):** In Python, *any* variable assigned inside `def`—even *after* a line that reads it—is bound as a function-local slot across the entire body unless listed in `global`. A pre-pass collects assignment targets before code generation so references resolve to `fn::name` vs. `global::name`, catching `y = x + 1; x = 20` with `UnboundLocalError` instead of silently reading a global `x`.
3. **Runtime-distinct division (`/` vs. `//`) and negative list indexing:** Even without static type annotations, the compiler emits distinct operators (`/` for true float division -> `295.0` vs. `//` for floor-toward-`-inf` division -> `-7 // 2 == -4`), and the VM normalizes negative indices from list length (`arr[-1]` -> element `len - 1`).

### Worked trace on `weighted_sum`

| Python construct | Front-end action | Lowered TAC & VM effect |
| --- | --- | --- |
| `global calls` / `calls = calls + 1` | Symtable pre-scan excludes `calls` from `locals` and keeps it in `globals` | Reads and updates `global::calls` (`0 -> 1`) |
| `w = (\n i + 1\n ) * 2` | `bracket_depth > 0` suppresses `NEWLINE`/`INDENT`/`DEDENT` across lines | Parses `(i + 1) * 2` seamlessly inside `else:` |
| `(acc / scale) + bonus + last_bonus` | Resolves `acc`/`last_bonus` locally and `scale`/`bonus` to `global::*` | Evaluates `590 / 2 -> 295.0`, `40 // 16 * 0.5 -> 1.0`, returning `298.5` |

### How to read the code

| Component | Rust (`main.rs`) | Python (`main.py`) | Role |
| --- | --- | --- | --- |
| Off-side scanner | `lex`, `enum TokKind` | `lex`, `Token` | Tracks `indent_stack` and `bracket_depth` to emit `INDENT`, `DEDENT`, `NEWLINE` |
| Parser | `Parser::parse_suite`, `parse_*` | `Parser.parse_suite`, `parse_*` | Parses `def`, `global`, `if`/`elif`/`else`, `while`, `and`/`or`/`not`, lists |
| Scope pre-scan | `analyze_fn_scope`, `FnSymtable` | `analyze_fn_scope`, `FnSymtable` | Separates function-local slots from `global::` names before TAC emission |
| TAC IR & VM | `Emitter`, `TacInst`, `exec_tac` | `Emitter`, `TacInst`, `exec_tac` | Emits labeled `fn_*` and `__module__` blocks; checks `UnboundLocalError` and negative bounds |

## What to look for

Running the demo prints the compile-time symtable for `weighted_sum` (`locals: [acc, arr, i, last_bonus, limit, w]`, `global decls: [calls]`), the `54` emitted TAC instructions across `fn_weighted_sum:` and `__module__:`, and verifies `Emitted output: [298.5, 1]`. Unit tests verify `-7 // 2 == -4` vs. `-7 / 2 == -3.5`, `UnboundLocalError` detection from the scope pre-scan, `IndentationError` on misaligned dedents, and `IndexError` on `xs[-3]`.

## Check your understanding

- Why can't Python's parser treat leading spaces like ordinary whitespace tokens skipped inside the scanner?
- Why does assigning `x = 20` at the end of a function turn an earlier `y = x + 1` inside the same function into an `UnboundLocalError` even when global `x = 10` exists?

## Edition mapping and scope

| 1977 Green | 1986 Red | 2006 Purple |
| --- | --- | --- |
| — | 2, 3, 6 | app. A (Python front end) |

Numbers refer to chapters. Related readings and appendices are marked where applicable. See the [edition guide](../../README.md#edition-guide) for the bibliographic references and chapter coverage.

This original Python-subset front end keeps code self-contained in one file per language and targets labeled three-address code rather than emitting CPython bytecode.
