# Static Single-Assignment (SSA) Construction and Phi Placement

Convert a control-flow graph into minimal SSA form using dominance frontiers, rename variables along the dominator tree, fold constants, and lower back out of SSA. See the [Rust source](main.rs) and [Python source](main.py).

```sh
cargo run --example ssa
cargo test --example ssa
python3 examples/ssa/main.py
python3 -m unittest examples/ssa/main.py
```

## What the example teaches

### Why we need this

In an ordinary control-flow graph (CFG), variables like `i` and `s` are overwritten in place on different branches and loop iterations. Before an optimizer can fold constants, eliminate dead code, or detect induction variables, it must answer: *which assignments to `x` can reach this use of `x`?* Solving that dataflow problem repeatedly across arbitrary branches and loops is expensive and fragile.

**Static Single-Assignment (SSA) form** solves this by giving every definition its own unique version (`i_1`, `i_2`, `s_1`). Wherever two control-flow paths with different versions merge (such as a loop header or an `if/else` join), a `phi` instruction selects the version matching the incoming predecessor edge. Naively inserting `phi` nodes at every merge for every variable creates huge bloat, so compilers compute **dominance frontiers** (plus liveness for **pruned SSA**) to place `phi` nodes *only* where live definitions actually collide.

In Static Single-Assignment form, every variable is assigned at most once, and control-flow merges join competing versions with explicit `phi` nodes. Minimal SSA places `phi` nodes only at the iterated dominance frontier of blocks that define each variable.

### SSA construction and lowering pipeline

1. Compute reachable blocks from entry, dominator sets, immediate dominators (`idom`), and **dominance frontiers** `DF(n)` (the first blocks where `n`'s dominance ends—exactly where merge `phi`s may be needed).
2. Run backward liveness analysis (`live_in`) so block-local temporaries (like `cond` or `one` in the loop) do not get useless dead `phi` nodes (**pruned SSA**).
3. For each variable `v`, run a worklist over `DF` starting from `Defs(v)` to place `phi` nodes where `v` is live-in and two paths with different definitions meet.
4. Walk the dominator tree depth-first with per-variable version stacks to rename definitions and uses, filling `phi` incoming `(pred_block, version)` arguments along CFG successor edges.
5. Verify the strict SSA dominance invariant (`verify_ssa`), fold constants through `phi`s (`fold_ssa_constants`), and lower `phi`s back to sequential edge copies (`de_ssa`), using temporary variables for parallel `phi` semantics and splitting critical edges with trampoline blocks when a predecessor has multiple successors.

### Loop `phi` placement (`B0` preheader -> `B1` header -> `B2` latch)

For `loop_sample` (`sum(0..n)`): `B0` initializes `i = 0, s = 0` and jumps to `B1`; `B1` tests `i < n` and branches to `B2` or `B3`; `B2` updates `s = s + i; i = i + 1` and jumps back to `B1`.

| Block | Role | `idom` | `DF` | `phi`s placed |
| --- | --- | --- | --- | --- |
| `B0` | Preheader (`i_1 = 0`, `s_1 = 0`) | — | `{}` | None |
| `B1` | Loop header (`cond_1 = i_2 < n`) | `B0` | `{B1}` | `i_2 = phi(B0: i_1, B2: i_3)`, `s_2 = phi(B0: s_1, B2: s_3)` |
| `B2` | Loop body / latch (`s_3 = s_2 + i_2`, `i_3 = i_2 + 1`) | `B1` | `{B1}` | None |
| `B3` | Exit (`return s_2`) | `B1` | `{}` | None |

Notice why `B1` gets `phi`s for `i` and `s`: `B2` defines `i` and `s`, and `B1` is in `DF(B2)` because `B2` jumps to `B1` without dominating `B1`. Meanwhile `one` (defined in `B2`) is not live-in at `B1`, so pruned SSA skips placing a `phi` for `one`.

## How to read the code

| Concept | Rust (`main.rs`) | Python (`main.py`) | Role |
| --- | --- | --- | --- |
| Non-SSA CFG | `BasicBlock`, `Stmt`, `Term`, `Rhs` | `BasicBlock`, `Stmt`, `Term*`, `Rhs*` | Input/output mutable-variable CFG |
| SSA IR | `SsaProgram`, `SsaBlock`, `Phi`, `SsaVar` | `SsaProgram`, `SsaBlock`, `Phi`, `SsaVar` | Versioned IR with `phi` nodes and dominance metadata |
| Dominance & frontiers | `compute_dominators`, `compute_dominance_frontiers` | `compute_dominators`, `compute_dominance_frontiers` | Builds `dom`, `idom`, dominator tree, and `DF` |
| SSA construction | `to_ssa` (plus `rename_block`) | `to_ssa` (plus `rename`) | Liveness + DF worklist + dominator-tree DFS renaming |
| Verification & opt | `verify_ssa`, `fold_ssa_constants` | `verify_ssa`, `fold_ssa_constants` | Checks dominance invariant; folds constants across `phi`s |
| Out-of-SSA lowering | `de_ssa` | `de_ssa` | Parallel-copy `__phi_tmp` emission and critical-edge splitting |
| Reference interpreters | `eval_cfg`, `eval_ssa` | `eval_cfg`, `eval_ssa` | Execute pre-SSA, SSA, and de-SSA programs to verify equivalence |

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
