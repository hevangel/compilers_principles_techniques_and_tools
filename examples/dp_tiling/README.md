# Dynamic-Programming Tree Tiling and Peephole Optimization

Cover an expression tree with minimum-cost target tiles using DP cost vectors (`C[node][k]`) and apply sliding-window peephole passes. See the [Rust source](main.rs) and [Python source](main.py).

```sh
cargo run --example dp_tiling
cargo test --example dp_tiling
python3 examples/dp_tiling/main.py
python3 -m unittest examples/dp_tiling/main.py
```

## What the example teaches

### Why we need this

Real CPUs provide compound instructions—such as x86-style indexed addressing (`LdIndexed` for `base + scale * idx`) and immediate arithmetic (`AddImm` for `lhs + 8`)—that cover **multiple expression-tree nodes** in a single fast instruction. A naive compiler that translates each AST operator one by one (`naive_postorder_emit`) emits separate `LdImm`, `MulReg`, and `AddReg` instructions for `(base + idx * 4) + 8`, costing **7 instructions (total cost 10)** instead of **4 instructions (total cost 6)**.

Even better than greedy top-down pattern matching ("maximal munch"), **bottom-up dynamic programming** (`solve_dp`) computes an entire **cost vector `best[1..=max_regs]`** at every AST node. Why a vector instead of one scalar cost? Because choosing a larger tile or switching child evaluation order (`left_first = true` vs `false`) changes how many registers remain (`k` vs `k - 1`) for the sibling subtree! Computing optimal solutions for every register budget `1..=max_regs` lets the parent pick the exact combination of tiles and evaluation order that minimizes total cost without spilling.

After instruction selection, a sliding-window **peephole optimizer** (`peephole_optimize`) cleans up local inefficiencies that only become visible once instructions sit side by side—such as redundant store-then-load (`St r, x; Ld r, x`), strength-reducible constant multiplies (`MulImm 2 -> ShlImm 1`), no-op arithmetic (`AddImm 0`), and dead jumps to the immediately following label (`Jmp L; Label L`).

### Tile catalog and peephole rules

Complex target instructions (such as fused indexed `base + scale * idx` addressing or immediate arithmetic) overlap multiple AST operators, so bottom-up dynamic programming chooses the lowest-cost tile cover for each register budget while peephole passes clean up local redundancies afterward.

| Optimization | Transformation | Effect |
| --- | --- | --- |
| `LeaIndexedTile` (`§8.11`) | `base + idx * 4` | Replaces `LdImm + Mul + Add` with 1 indexed instruction |
| `AddImmTile` (`§8.11`) | `lhs + 8` | Folds constant leaf without loading an immediate register |
| `peephole_optimize` (`§8.7`) | `St r, x; Ld r, x`, `Add 0`, `Mul 2 -> Shl 1`, dead `Jmp` | Shrinks emitted sequence while preserving value |

For `(base + idx * 4) + 8` with `base = 1000, idx = 7`:
- `solve_dp` matches `AddImmTile` at the root (`+ 8`, cost +1) and `LeaIndexedTile` at its left child (`base + idx * 4`, loading `base` cost 2, `idx` cost 2, `LdIndexed` cost 1), totaling `cost = 6` in 4 instructions (`Ld r0, base; Ld r1, idx; LdIndexed r0, r0 + 4*r1; AddImm r0, r0, 8`) evaluating to `1036`.
- `peephole_optimize` iterates until a fixed point, reducing the 7-instruction test sequence to 4 instructions while computing the exact same result.

### How to read the code

| Component (`main.rs` / `main.py`) | Role |
| --- | --- |
| `Expr`, `TileCode` | Input integer expression AST (`Mem`, `Imm`, `Add`, `Mul`) and target instruction set (`Ld`, `LdImm`, `LdIndexed`, `AddReg`, `AddImm`, `MulImm`, `ShlImm`, `St`, `Jmp`, `Label`). |
| `SelectedTile`, `DpSolution`, `DpNode` | Records the winning tile, child evaluation order (`left_first`), and cost vector `best[1..=max_regs]` at each tree node. |
| `solve_dp` / `emit_dp` | Bottom-up DP filling `best[k]` across all matching tiles and evaluation orders, followed by top-down code emission. |
| `naive_postorder_emit` | Baseline 1-to-1 AST postorder emitter used to measure DP's cost and instruction-count savings. |
| `peephole_optimize` | Fixed-point sliding-window pass applying redundant `St`/`Ld`, `Jmp`/`Label`, `AddImm 0`, and `MulImm 2 -> ShlImm 1` rewrites. |
| `eval_expr` / `eval_tiles` | AST and target-machine interpreters verifying semantic equivalence before and after optimization. |

## What to look for

For `(base + idx * 4) + 8`, DP tiling (`cost = 6`, 4 instructions) beats naive postorder expansion (`cost = 10`, 7 instructions) by fusing both `* 4` into `LdIndexed` and `+ 8` into `AddImm`. Applying `peephole_optimize` reduces a 7-instruction test snippet to 4 instructions while computing identical machine output.

## Check your understanding

- Why does `solve_dp` compute a separate optimal cost for every register budget `1..=max_regs`?
- Why must the `St r, x; Ld r, x` peephole rule ensure no jump label intervenes between the store and load?

## Edition mapping and scope

| 1977 Green | 1986 Red | 2006 Purple |
| --- | --- | --- |
| 15 | 9 | 8 |

Numbers refer to chapters. Related readings and appendices are marked where applicable. See the [edition guide](../../README.md#edition-guide) for the bibliographic references and chapter coverage.

Tree tiling targets an abstract RISC-plus-indexed-addressing instruction set over integer expressions without floating-point or multi-output patterns.
