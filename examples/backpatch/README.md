# Backpatching and short-circuit jumps

Fill forward jump targets after their destination instructions are emitted. See the [Rust source](main.rs) and [Python source](main.py).

```sh
cargo run --example backpatch
cargo test --example backpatch
python3 examples/backpatch/main.py
python3 -m unittest examples/backpatch/main.py
```

## What the example teaches

### Why we need this

In source languages like C, Rust, or Python, Boolean operators `&&` and `||` have **short-circuit semantics**: evaluating `a && b` when `a` is false must **not** evaluate `b` (for example, `ptr != null && ptr->ok` would crash if `ptr->ok` ran first!). Compilers implement short-circuiting with conditional jump instructions rather than computing `0`/`1` values eagerly.

The catch is a **forward-reference problem during single-pass code generation**: when emitting the jump for `a` in `a && b`, the compiler has not generated the code for `b` (or for the final `false` target) yet, so it does not know the destination instruction address! A naive compiler could run a second pass over the AST just to compute code sizes first, or emit temporary symbolic labels and resolve them later. **Backpatching** solves this cleanly in one pass:

- Every emitted jump with an unknown destination leaves its target slot as `None` (a "hole").
- Each compiled subexpression returns two lists of instruction indices (`on_true` and `on_false`) recording where those holes live.
- As soon as the destination instruction index becomes known, `patch` fills in the missing targets directly in the instruction array.

### Combine branch fragments

| Condition | When the left side is true | When the left side is false |
| --- | --- | --- |
| `left && right` | Patch `left.on_true` to the start of `right`; result exits are `right.on_true` | Merge `left.on_false` and `right.on_false` (neither evaluates further on false) |
| `left || right` | Merge `left.on_true` and `right.on_true` (either short-circuits to true) | Patch `left.on_false` to the start of `right`; result exits are `right.on_false` |

An input leaf `Input(i)` emits an `IfTrue(i, None)` followed by an unconditional `Jump(None)` for the false case (when `IfTrue` sees `false`, control falls through to `pc + 1`, hitting the `Jump`). Once all fragments are emitted, `compile` / `compile_condition` appends `Return(true)` and `Return(false)` and backpatches the remaining outermost `on_true` and `on_false` lists to them. `patch` asserts that every site was previously `None` so no hole is filled twice or silently overwritten.

### Trace `(a && b) || c`

Generating code for `sample()` (`(Input(0) && Input(1)) || Input(2)`) produces 8 instructions:

| `pc` | Generated instruction | Leaf / role | Final patched target & reason |
| --- | --- | --- | --- |
| `0` | `IfTrue(0, Some(2))` | `Input(0)` true branch | Patched by `And`: if `a` is true, test `b` at `pc = 2` |
| `1` | `Jump(Some(4))` | `Input(0)` false branch | Patched by `Or`: `And` failed, short-circuit to `c` at `pc = 4` |
| `2` | `IfTrue(1, Some(6))` | `Input(1)` true branch | Patched by `compile`: `a && b` true, short-circuit `c` -> `Return(true)` at `pc = 6` |
| `3` | `Jump(Some(4))` | `Input(1)` false branch | Patched by `Or`: `And` failed, fall back to `c` at `pc = 4` |
| `4` | `IfTrue(2, Some(6))` | `Input(2)` true branch | Patched by `compile`: whole `Or` is true -> `Return(true)` at `pc = 6` |
| `5` | `Jump(Some(7))` | `Input(2)` false branch | Patched by `compile`: all paths failed -> `Return(false)` at `pc = 7` |
| `6` | `Return(true)` | True exit landing pad | Target for remaining `exits.on_true` (`[2, 4]`) |
| `7` | `Return(false)` | False exit landing pad | Target for remaining `exits.on_false` (`[5]`) |

Running on `[false, false, true]` jumps `0 -> 1 -> 4 -> 6`, returning `(true, [0, 2])` without reading input `1`. Running on `[true, true, false]` jumps `0 -> 2 -> 6`, returning `(true, [0, 1])` without reading input `2`.

## How to read the code

| Concept | Rust (`main.rs`) | Python (`main.py`) | Role |
| --- | --- | --- | --- |
| Boolean AST | `enum Condition` (`Input`, `And`, `Or`) | `CondInput`, `CondAnd`, `CondOr` (`Condition`) | Short-circuit Boolean expression tree |
| Branching bytecode | `enum Instruction` (`IfTrue`, `Jump`, `Return`) | `IfTrue`, `Jump`, `Return` (`Instruction`) | Target VM with `None` placeholder targets |
| Patch lists | `struct Exits { on_true, on_false }` | `class Exits` | Synthesized lists of unpatched jump indices |
| Backpatcher | `fn patch(code, sites, target)` | `def patch(code, sites, target)` | Fills `None` jump targets once and verifies no double-patch |
| Fragment emitter | `fn emit(condition, code) -> Exits` | `def emit(condition, code) -> Exits` | Emits code bottom-up, patching internal jumps and merging lists |
| Top-level driver | `fn compile(condition)` | `def compile_condition(condition)` | Appends `Return(true)`/`Return(false)` and patches final exits |
| VM interpreter | `fn run(code, inputs)` | `def run(code, inputs)` | Executes jumps and logs which input indices were actually read |

## What to look for

The tests compare all eight inputs against the truth table for `(a && b) || c`. They also inspect which inputs were read to verify both short-circuit paths.

## Check your understanding

- Predict the returned value and read trace for `[true, false, false]`. Check the sequence against the branch table.
- For `a || b`, draw the four instructions for the two leaves and mark which one of `a`'s two exits leads to the start of `b`.

## Edition mapping and scope

| 1977 Green | 1986 Red | 2006 Purple |
| --- | --- | --- |
| 7 (Boolean translation) | 8 | 6 |

Numbers refer to chapters. Related readings and appendices are marked where applicable. See the [edition guide](../../README.md#edition-guide) for the bibliographic references and chapter coverage.

The input is an in-code condition tree. The generated code branches to Boolean return values on a small interpreter.
