# Region-Based Data-Flow Analysis (T1/T2 Reducible Regions)

Construct a hierarchical region tree via T1/T2 graph reductions, compose affine `(gen, kill)` bitvector transfer functions bottom-up with loop closure `f*`, and propagate concrete `IN[B]` facts top-down. See the [Rust source](main.rs) and [Python source](main.py).

```sh
cargo run --example region_dataflow
cargo test --example region_dataflow
python3 examples/region_dataflow/main.py
python3 -m unittest examples/region_dataflow/main.py
```

## What the example teaches

### Why we need this and when it is sound

Iterative data-flow solvers propagate bitvectors around every loop edge repeatedly until convergence. For **reducible** CFGs (roughly, programs built from structured `if`/`else` and single-entry loops), **region-based data-flow analysis** replaces graph-wide iteration with a two-pass structural algorithm:

1. **Bottom-up reduction (`T1` / `T2`):** Collapse the CFG hierarchically into nested single-entry **regions**, summarizing each region by a single affine transfer function `f(X) = gen | (X & !kill)`.
2. **Top-down propagation:** Starting from `IN[root] = 0`, walk the reduction history in reverse to compute exact `IN[B]` for every basic block in one linear pass.

Why is the Kleene closure `(gen, 0)` sound for `T1` loops in reaching definitions? Because `f*(X) = I(X) U f(X) U f(f(X)) U ...`: taking **0 iterations** (the identity path `I(X) = X`) passes incoming definitions straight through `T1` without killing anything (`kill = 0`), while `>= 1` iterations add all definitions generated inside the loop body (`gen`). Because `f` is monotone and affine over bitvectors, `(gen, 0)` computes the exact fixed point of the self-loop algebraically in `O(1)`!

### Structural T1 / T2 reductions and transfer algebra

| Operation | Structural meaning | Transfer rule (`f(X) = G \| (X & !K)`) |
| --- | --- | --- |
| `T2(u, v)` | Absorb `v != entry` whose sole predecessor is `u` (`u != v`) | Sequential composition `f2(f1(X)) = (G2 \| (G1 & !K2), K2 \| K1)` |
| `T1(u)` | Absorb self-loop edge `u -> u` | Loop Kleene closure `f* = (G, 0)` for union may-analysis |
| Meet at join | Multiple parallel edges merged between regions | Union meet `f1(X) \| f2(X) = (G1 \| G2, K1 & K2)` |

### Trace the nested two-loop CFG

In `multi_loop_reducible_sample()`, `B0(d0) -> B1(d1, -d0) -> {B2(d2), B5}`, `B2 -> {B3(d3, -d1), B4(d4, -d2)}`, with inner loop `B3 -> B2` and outer loop `B4 -> B1`:

| Step | Rule | Regions merged | Structural / transfer effect |
| --- | --- | --- | --- |
| 1 | `T2` | `(B2, B3)` → `R6` | Absorbs inner body `B3`; back-edge `B3 -> B2` becomes self-loop `R6 -> R6`, parallel edge `R6 -> B4` |
| 2 | `T1` | `R6` → `R7` | Closes inner loop `R6 -> R6` via `f* = (gen, 0)`, composing closure onto `R7 -> B4` |
| 3 | `T2` | `(R7, B4)` → `R8` | Absorbs outer latch `B4`, turning `B4 -> B1` into `R8 -> B1` |
| 4 | `T2` | `(B1, R8)` → `R9` | `R8`'s only predecessor is `B1`; `B4 -> B1` becomes self-loop `R9 -> R9` |
| 5 | `T1` | `R9` → `R10` | Closes outer loop `R9 -> R9` via `f*` |
| 6–7 | `T2` | `(R10, B5)` → `R11`, `(B0, R11)` → `R12` | Absorbs exit `B5` and merges entry `B0` into root region `R12` |

Top-down replay then recovers `IN[B0..B5]` matching `iterative_mfp_reaching_defs` bit-for-bit across all 6 blocks. Conversely, the double-entry loop `0 -> {1, 2}, 1 <-> 2` has no self-loop and neither `1` nor `2` has a unique predecessor, so `T1`/`T2` halts and reports irreducibility.

### How to read the code

| Component | Rust ([`main.rs`](main.rs)) | Python ([`main.py`](main.py)) | Purpose |
| --- | --- | --- | --- |
| Transfer algebra | `TransferFn` (`apply`, `compose`, `union_meet`, `star`) | `TransferFn` (`apply`, `compose`, `union_meet`, `star`) | 64-bit / int bitvector affine `(gen, kill)` operators |
| Region tree | `Region` (`Leaf`, `Seq`, `Loop`), `IrreducibleError` | `steps` log + `ValueError` | Hierarchical tree / reduction trace and irreducibility detection |
| T1/T2 solver | `reduce_t1_t2` | `reduce_t1_t2` | Bottom-up `T1`/`T2` reduction followed by reverse top-down `IN` pass |
| Oracle & tests | `iterative_mfp_reaching_defs`, `multi_loop_reducible_sample`, tests | `iterative_mfp_reaching_defs`, `multi_loop_sample`, `RegionDataflowTests` | Cross-checks region `IN[B]` against iterative MFP and tests irreducible loop |

## What to look for

On a nested two-loop CFG (`B0 -> B1 -> {B2, B5}`, `B2 -> {B3 -> B2, B4 -> B1}`), `reduce_t1_t2` collapses the entire graph cleanly and computes reaching-definitions `IN[B]` matching `iterative_mfp_reaching_defs` bit-for-bit across all 6 blocks. On a double-entry loop `0 -> {1, 2}, 1 <-> 2`, neither T1 nor T2 applies, demonstrating irreducibility detection.

## Check your understanding

- Why does a self-loop closure `(gen, 0)` clear `kill` in a reaching-definitions may-analysis?
- How does node splitting convert an irreducible double-entry loop into a reducible CFG at the cost of code duplication?

## Edition mapping and scope

| 1977 Green | 1986 Red | 2006 Purple |
| --- | --- | --- |
| 13–14 | 10 | 9 |

Numbers refer to chapters. Related readings and appendices are marked where applicable. See the [edition guide](../../README.md#edition-guide) for the bibliographic references and chapter coverage.

Bitvector transfer functions pack up to 64 facts per word and illustrate reaching-definitions union meet without node splitting for irreducible graphs.
