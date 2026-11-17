# Switch Lowering and Break/Continue Backpatching

Choose linear `if/else`, jump-table, or binary-search dispatch by switch density and backpatch nested `break`/`continue` jumps. See the [Rust source](main.rs) and [Python source](main.py).

```sh
cargo run --example switch_lower
cargo test --example switch_lower
python3 examples/switch_lower/main.py
python3 -m unittest examples/switch_lower/main.py
```

## What the example teaches

Compiling a `switch` statement trades code size for dispatch speed according to arm count and key density, while lowering structured `break` and `continue` uses parallel target stacks so inner constructs never hijack outer loop jumps.

### Strategy selection

1. `LinearIfElse` when there are at most 4 arms.
2. Bounds-checked `JumpTable` when `count / span >= 0.5` across `min_val..=max_val`.
3. Balanced `BinarySearchTree` when many sparse arms make a jump table too sparse.

### Nested `break` and `continue` stacks

| Construct | Pushes `break_list` | Pushes `continue_list` |
| --- | --- | --- |
| `Switch` | Yes (targets `sw_end`) | No (inherits enclosing loop) |
| `Loop` | Yes (targets `loop_exit`) | Yes (targets `loop_step`) |

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
