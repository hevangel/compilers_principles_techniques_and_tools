# Dynamic-Programming Tree Tiling and Peephole Optimization

Cover an expression tree with minimum-cost target tiles using DP cost vectors (`C[node][k]`) and apply sliding-window peephole passes. See the [Rust source](main.rs) and [Python source](main.py).

```sh
cargo run --example dp_tiling
cargo test --example dp_tiling
python3 examples/dp_tiling/main.py
python3 -m unittest examples/dp_tiling/main.py
```

## What the example teaches

Complex target instructions (such as fused indexed `base + scale * idx` addressing or immediate arithmetic) overlap multiple AST operators, so bottom-up dynamic programming chooses the lowest-cost tile cover for each register budget while peephole passes clean up local redundancies afterward.

### Tile catalog and peephole rules

| Optimization | Transformation | Effect |
| --- | --- | --- |
| `LeaIndexedTile` (`§8.11`) | `base + idx * 4` | Replaces `LdImm + Mul + Add` with 1 indexed instruction |
| `AddImmTile` (`§8.11`) | `lhs + 8` | Folds constant leaf without loading an immediate register |
| `peephole_optimize` (`§8.7`) | `St r, x; Ld r, x`, `Add 0`, `Mul 2 -> Shl 1`, dead `Jmp` | Shrinks emitted sequence while preserving value |

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
