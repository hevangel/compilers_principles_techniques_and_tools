# Switch Lowering and Break/Continue Backpatching

Choose linear `if/else`, jump-table, or binary-search dispatch by switch density and backpatch nested `break`/`continue` jumps. See the [Rust source](main.rs) and [Python source](main.py).

```sh
cargo run --example switch_lower
cargo test --example switch_lower
python3 examples/switch_lower/main.py
python3 -m unittest examples/switch_lower/main.py
```

## What the example teaches

### Why we need this

A parser checks that `switch`, `loop`, `break`, and `continue` statements are grammatically nested, but machine CPUs do not have structured `switch` or `break` instructions—they only have flat conditional branches, indirect jumps, and unconditional `goto`s. Lowering these constructs raises two classic code-generation problems:

1. **Dispatch strategy trade-offs**: A naive compiler lowers every `switch` to a chain of `if (key == c_k)` checks. That works well for 2–4 arms, but on a 100-arm interpreter dispatch loop it performs `O(n)` comparisons per lookup. Conversely, blindly emitting a flat jump table for sparse keys like `{0, 1000, 2000, ...}` wastes kilobytes of memory on empty default slots. Selecting between linear checks, dense jump tables (`O(1)`), and binary search trees (`O(log n)`) balances **code size** against **dispatch time**.
2. **Forward-target & scoping ambiguity for `break`/`continue`**: When lowering `break` or `continue` in one pass, the destination label (`sw_end`, `loop_step`, `loop_exit`) lies ahead in the instruction stream and has not been emitted yet. Worse, `break` inside a `switch` exits **the switch**, whereas `continue` inside that very same `switch` skips **to the enclosing loop's increment step**. A single stack would confuse the two! Keeping separate `break_lists` and `continue_lists` stacks guarantees each jump gets backpatched to the right enclosing construct.

### Strategy selection

`lower_switch` sorts and deduplicates the `(value, label)` arms by key, then `choose_strategy` picks:

1. `LinearIfElse` when there are at most 4 arms (`arms.len() <= 4`).
2. Bounds-checked `JumpTable` when the arm density `count / span >= 0.5` (checked cleanly in integer arithmetic as `2 * count >= span`, where `span = max_val - min_val + 1`).
3. Balanced `BinarySearchTree` when there are more than 4 arms and the keys are too sparse for a jump table (`emit_bst` recursively splits at the midpoint `mid`).

| Sample switch | Arm count | Key span | Density (`count / span`) | Chosen strategy |
| --- | --- | --- | --- | --- |
| `tiny`: `1, 2` | `2` | `2` | `1.00` (<= 4 arms) | `LinearIfElse` |
| `dense`: `0..10` | `10` | `10` | `1.00` (>= 0.5) | `JumpTable` |
| `sparse`: `0, 100, ..., 900` | `10` | `901` | `0.011` (< 0.5) | `BinarySearchTree` |

### Nested `break` and `continue` stacks

| Construct | Pushes `break_list` | Pushes `continue_list` |
| --- | --- | --- |
| `Switch` | Yes (targets `sw_end`) | No (inherits enclosing loop) |
| `Loop` | Yes (targets `loop_exit`) | Yes (targets `loop_step`) |

### Trace the nested `Loop` + `Switch` program

In `nested_loop_and_switch_backpatches_break_and_continue_distinctly`, each case label `val` emits `Emit(val * 10)` on entry before running its body, and `EmitCaseValue` lowers to `Emit(1)`:

| Iteration `var` | Switch arm entered | Instructions executed | Backpatched jump target | Values appended to `trace` |
| --- | --- | --- | --- | --- |
| `0` | `case 0` | `Emit(0)`, `Break` | `sw_end` (exits switch only), then post-switch `Emit(1)` | `0, 1` |
| `1` | `case 1` | `Emit(10)`, `Continue` | `loop_step` (skips post-switch `Emit(1)`, increments `var`) | `10` |
| `2` | `case 2` | `Emit(20)`, `Emit(1)`, `Break` | `sw_end` (exits switch only), then post-switch `Emit(1)` | `20, 1, 1` |
| `3` | `default` | `Emit(1)` (falls through `sw_end`) | Post-switch `Emit(1)` -> `loop_step` (`var` becomes `4`, exits loop) | `1, 1` |

Total result: `final_var = 4` and `trace = [0, 1, 10, 20, 1, 1, 1, 1]`.

## How to read the code

| Concept | Rust (`main.rs`) | Python (`main.py`) | Role |
| --- | --- | --- | --- |
| Dispatch strategy | `enum SwitchStrategy` | `class SwitchStrategy` | `LinearIfElse`, `JumpTable`, or `BinarySearchTree` |
| Target IR | `enum LoweredInstr` | `Instr` union dataclasses | Dispatch ops, `BreakPlaceholder`/`ContinuePlaceholder`, `PatchedJump` |
| Density heuristic | `fn choose_strategy` | `def choose_strategy` | Selects linear, dense jump table, or BST dispatch |
| Switch lowering | `fn lower_switch` | `def lower_switch` | Sorts/dedups arms and emits linear, table, or recursive BST code |
| Structured AST | `enum StmtAst` | `StmtAst` union dataclasses | `Switch`, `Loop`, `Break`, `Continue`, `EmitCaseValue` |
| Control-flow stacks | `struct PatchContext` | `class PatchContext` | Parallel `break_lists` and `continue_lists` stacks + fresh labels |
| Statement lowerer | `fn lower_stmt_list`, `backpatch` | `def lower_stmt_list`, `backpatch` | Emits placeholders and patches `break`/`continue` on block exit |
| Interpreters | `fn eval_switch_dispatch`, `run_lowered_program` | `def eval_switch_dispatch`, `run_lowered_program` | Simulate dispatch tables and full lowered loop/switch programs |

## What to look for

Inside `Loop { Switch { case 1: continue; case 0: break; } Emit(1) }`, `continue` inside `case 1` skips the post-switch `Emit(1)` and jumps straight to the loop increment step, whereas `break` inside `case 0` exits only the `switch` and executes the post-switch statement.

## Check your understanding

- Why does a bounds-checked jump table still need a default check at both `key < min_val` and gaps within `[min_val, max_val]`?
- What happens if a language does not allow `continue` inside a standalone `switch` outside any loop?

## Edition mapping and scope

| 1977 Green | 1986 Red | 2006 Purple |
| --- | --- | --- |
| 7 | 8 | 6 |

Numbers refer to chapters. Related readings and appendices are marked where applicable. See the [edition guide](../../README.md#edition-guide) for the bibliographic references and chapter coverage.

Switch values are 64-bit signed integers without fall-through between cases except when control reaches the end of a non-breaking block; range cases and Duff's device fall-through patterns are omitted.
