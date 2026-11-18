# Complete Structured Front End: Lexer, Scoped AST, and TAC (2006 Appendix A)

Compile a block-scoped, statically typed imperative program—with lexical `{ ... }` shadowing, multidimensional `float[2][3]` arrays, `&&`/`||` short-circuit control flow, `while`, `do ... while`, and `break`—into labeled Three-Address Code (TAC) and verify it on a byte-offset VM. See the [Rust source](main.rs) and [Python source](main.py).

```sh
cargo run --example complete_frontend
cargo test --example complete_frontend
python3 examples/complete_frontend/main.py
python3 -m unittest examples/complete_frontend/main.py
```

## What the example teaches

### Why we need this / What problem this solves

The 2006 Purple Dragon Book's **Appendix A** shows how a real compiler front end combines separate components—lexer, chained lexical symbol tables (`Env`), types, AST nodes, and labeled intermediate-code generation—rather than jamming everything into one parser function:

1. **Nested block scoping without clobbering outer names:** When an inner `{ int i; i = 100; }` shadows an outer `int i`, a flat symbol table would fail with a duplicate-name error or overwrite the outer loop counter. Chaining lexical block environments (`EnvStack`) maps inner `i` to `i@b2` and outer `i` to `i@b0`, preserving both cleanly.
2. **Short-circuit control flow without dummy booleans (`jumping`):** Evaluating `if (i == 1 && j == 2) break;` into a `t1 = ...` boolean first wastes instructions and breaks short-circuiting. Passing `(true_label, false_label)` targets down through `jumping(expr, t, f)` (with `0` meaning fall-through) lets `&&`, `||`, `!`, and comparisons emit direct conditional branches.
3. **Multidimensional row-major offsets:** Declaring `float[2][3] grid` computes recursive type widths (`float` = `8` bytes, `float[3]` = `24` bytes, `float[2][3]` = `48` bytes), so `grid[i][j]` lowers directly to `i * 24 + j * 8`.

### Lowering pipeline on the sample program

| Source construct | Semantic & type work | Lowered TAC |
| --- | --- | --- |
| `{ int i; i = 100; sum = sum + i; }` | Pushes block `b2`; resolves inner `i` to `i@b2` and `sum` outward to `sum@b0` | Leaves outer loop variable `i@b0` untouched (`200` in `sum@b0`) |
| `if (i == 1 && j == 2) break;` | `jumping` emits direct branch targets and resolves `break` to enclosing `do-while` exit label | Skips `grid[1][2]` (byte offset `40`) cleanly |
| `grid[i][j] = (i * 3 + j) + 0.5;` | Computes `i * 24 + j * 8` byte offset; coerces `int` RHS sum against `0.5` (`float`) | Stores `2.5` at `grid@b0[16]` and `4.5` at `grid@b0[32]` |

### How to read the code

| Component | Rust (`main.rs`) | Python (`main.py`) | Role |
| --- | --- | --- | --- |
| Scanner | `lex`, `enum TokKind` | `lex`, `Token` | Scans keywords, two-char operators (`&&`, `\|\|`, `<=`), numbers, and `//`/`/*...*/` comments |
| Scope chain & types | `EnvStack`, `Type` | `EnvStack`, `Type` | Tracks nested `{ ... }` variable slots, recursive array widths, and numeric widening |
| AST & parser | `Parser`, `Expr`, `Stmt` | `Parser`, `Expr`, `Stmt` | Validates loop nesting for `break;`, builds typed expressions, and computes array byte strides |
| Label-driven TAC | `Emitter::jumping`, `gen_stmt`, `exec_tac` | `Emitter.jumping`, `gen_stmt`, `exec_tac` | Generates fall-through-aware branches and verifies execution on a byte-offset VM |

## What to look for

Running the demo prints the lexical symbol bindings (`48` bytes for `grid@b0`, total `64` bytes), the emitted labeled TAC, and verifies `outer i@b0 = 2`, `inner i@b2 = 100`, `sum@b0 = 200`, `grid[0][2] = 2.5` (offset `16`), and `grid[1][1] = 4.5` (offset `32`). Tests check unenclosed `break;`, duplicate same-block declarations, non-boolean conditions, and indexing non-array types.

## Check your understanding

- Why does `jumping(expr, t, f)` use `0` to mean "fall through to the very next instruction"?
- Why must `dims` in `float[2][3]` be folded from right to left (`array(2, array(3, float))`)?

## Edition mapping and scope

| 1977 Green | 1986 Red | 2006 Purple |
| --- | --- | --- |
| — | 2, 6, 8 | app. A (complete front end) |

Numbers refer to chapters. Related readings and appendices are marked where applicable. See the [edition guide](../../README.md#edition-guide) for the bibliographic references and chapter coverage.

This original modular front end mirrors the 2006 Appendix A architecture in self-contained Rust and Python without external packages or multi-file build boilerplate.
