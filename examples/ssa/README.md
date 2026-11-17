# Static Single-Assignment (SSA) Construction and Phi Placement

Convert a control-flow graph into minimal SSA form using dominance frontiers, rename variables along the dominator tree, fold constants, and lower back out of SSA. See the [Rust source](main.rs) and [Python source](main.py).

```sh
cargo run --example ssa
cargo test --example ssa
python3 examples/ssa/main.py
python3 -m unittest examples/ssa/main.py
```

## What the example teaches

In Static Single-Assignment form, every variable is assigned at most once, and control-flow merges join competing versions with explicit `phi` nodes. Minimal SSA places `phi` nodes only at the iterated dominance frontier of blocks that define each variable.

### SSA construction and lowering pipeline

1. Compute reachable blocks from entry, dominator sets, immediate dominators (`idom`), and **dominance frontiers** `DF(n)`.
2. For each variable `v`, run a worklist over `DF` starting from `Defs(v)` to place `phi` nodes only where two paths with different definitions meet.
3. Walk the dominator tree depth-first with per-variable version stacks to rename definitions and uses, filling `phi` incoming `(pred_block, version)` arguments on outgoing edges.
4. Verify the strict SSA dominance invariant, perform sparse constant folding (including `phi` nodes with identical constant inputs), and lower `phi`s back to sequential edge copies (`de_ssa`).

### Loop `phi` placement (`B0` preheader -> `B1` header -> `B2` latch)

| Block | Role | `phi`s placed |
| --- | --- | --- |
| `B0` | Preheader (`i = 0`, `s = 0`) | None |
| `B1` | Loop header (`i < n`) | `i` and `s` merging `B0` and `B2` |
| `B2` | Loop body / latch | None |
| `B3` | Exit (`return s`) | None |

## What to look for

The worked trace prints dominance frontiers, `phi` nodes at loop header `B1`, and verifies that original CFG execution, SSA interpretation, and de-SSA lowered CFG execution all return `10` for `sum(0..5)`. Unit tests also check that an unreachable block's dead assignment does not pollute join `phi`s in a diamond CFG.

## Check your understanding

- Why does a `phi` operand from predecessor `P` have to be dominated by its definition at the end of `P` rather than at the join block itself?
- Why does lowering `phi`s out of SSA require temporary copies when two `phi` nodes in the same block read each other's variables?

## Edition mapping and scope

| 1977 Green | 1986 Red | 2006 Purple |
| --- | --- | --- |
| — | — | 6, 9 |

Numbers refer to chapters. Related readings and appendices are marked where applicable. See the [edition guide](../../README.md#edition-guide) for the bibliographic references and chapter coverage.

Dominance sets are computed iteratively on small CFGs rather than using the Lengauer-Tarjan near-linear algorithm.
