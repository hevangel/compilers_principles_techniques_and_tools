# Loop-Invariant Code Motion and Induction-Variable Strength Reduction

Identify safe loop invariants, hoist them to the preheader, replace derived induction-variable multiplications with incremental additions, and eliminate dead basic IVs. See the [Rust source](main.rs) and [Python source](main.py).

```sh
cargo run --example licm_iv
cargo test --example licm_iv
python3 examples/licm_iv/main.py
python3 -m unittest examples/licm_iv/main.py
```

## What the example teaches

### Why we need this and when it is sound

Loops dominate program execution time, so compilers target two classic optimizations once natural loops and preheaders are identified:

1. **Loop-Invariant Code Motion (LICM):** Compute expressions whose operands never change inside the loop *once* in the **preheader** before entering the loop, rather than recomputing them on every iteration.
2. **Induction-Variable Strength Reduction:** Replace expensive multiplications derived from a loop counter (`t = i * 4`) with cheap additions (`t_sr = t_sr + 4`), rewrite the exit comparison to use `t_sr`, and delete the original counter `i` once nothing else reads it.

Both transformations have strict **soundness gates**:

- **LICM invariance vs. control-flow safety:** Being loop-invariant (all operands constant, defined outside the loop, or uniquely defined invariants) is *not* enough! If a pure statement `s: dst = expr` sits inside a conditional branch inside the loop (`B3` guarded by `flag`) or after a `0`-trip exit test (`B1 -> B5`), hoisting `s` to the preheader executes it unconditionally. That is sound **only if** (a) `s`'s block dominates all loop exit blocks, **or** (b) `dst` is **dead after the loop** (not in `live_in` of any exit target), and `s` has no side effects or traps. Furthermore, `dst` must have a unique definition in the loop so hoisting does not clobber another assignment to `dst`.
- **Strength reduction & dead-IV elimination:** Rewriting `i < n` to `t_sr < n * scale` requires `scale > 0` (so `<` direction is preserved) and no integer overflow; deleting `i = i + step` is sound only when `i` has no remaining loop uses and is dead at all loop exits.

### Loop optimization stages

1. **Invariant marking (`licm_hoist`):** Compute dominators, natural loop nodes `{1, 2, 3, 4}`, and backward `live_in` sets; iterate until fixed point marking statements whose operands are external (`def_count == 0`) or already-marked unique invariants.
2. **Safety gate (`licm_hoist`):** Hoist to `preheader(0)` only when the defining block dominates all exit blocks (`exits = [B1]`) or `dst` is dead in `live_in[B5]`.
3. **Strength reduction + exit test replacement (`strength_reduce_iv`):** Detect basic IV `i = i + step` and derived IV `t = i * scale`, initialize `t_sr = i * scale` in `preheader(0)`, update `t_sr += scale * step`, rewrite `i < n` to `t_sr < n_scaled`, and delete `i = i + 1` once dead.

### Trace the sample loop

In `sample_loop_with_guarded_invariant()` (`B0` preheader -> `B1` header (`i < n ? B2 : B5`) -> `B2` -> `{B3, B4}` -> `B4 -> B1`):

| Statement / Variable | Block | Invariant / IV? | Dominates exit `B1`? | Live at `B5`? | Optimization decision (`n=6, flag=0`) |
| --- | --- | --- | --- | --- | --- |
| `safe_inv = a * b` | `B2` | Invariant (`a, b` external) | No (`n == 0` skips `B2`) | No (`dead_after = true`) | **Hoisted** to `B0` (`6` muls → `1` mul) |
| `unsafe_inv = a * 10` | `B3` | Invariant (`a, 10` external) | No (`flag == 0` skips `B3`) | **Yes** (`res = sum + unsafe_inv`) | **Skipped** (stays in `B3`, preserves `unsafe_inv = 77`) |
| `t = i * 4` | `B4` | Derived IV (`scale = 4`) | — | — | **Strength-reduced** to `t = t_sr` + `t_sr += 4` |
| `cond = i < n` / `i = i + 1` | `B1` / `B4` | Basic IV (`step = 1`) | — | `i` dead at `B5` | Exit test → `t_sr < n_scaled`; `i = i + 1` **deleted** |

Overall, on `n = 6, flag = 0`, loop multiplications drop from `12` (`safe_inv` × 6 + `t` × 6) down to `3` preheader multiplications (`safe_inv`, `t_init_mul`, `n_scaled`).

### How to read the code

| Component | Rust ([`main.rs`](main.rs)) | Python ([`main.py`](main.py)) | Purpose |
| --- | --- | --- | --- |
| CFG & analyses | `compute_doms`, `natural_loop`, `compute_live_in` | `compute_doms`, `natural_loop`, `compute_live_in` | Dominance, natural-loop membership, and exit liveness |
| LICM pass | `licm_hoist` | `licm_hoist` | Fixed-point invariant detection + dominance/dead-after-loop safety gate |
| Strength reduction | `strength_reduce_iv`, `DerivedIv` | `strength_reduce_iv` | Affine IV detection, `t_sr` updates, exit test rewrite, and dead-IV cleanup |
| Evaluator & tests | `eval_cfg`, `sample_loop_with_guarded_invariant`, tests | `eval_cfg`, `sample_loop`, `LicmIvTests` | Checks `unsafe_inv` preservation (`flag=0/1`) and mul-count reduction |

## What to look for

In the sample loop, `safe_inv = a * b` (dead after the loop) hoists cleanly to `preheader(0)`, whereas `unsafe_inv = a * 10` sits inside the `flag`-guarded branch `B3` and is live at exit `B5`, so `licm_hoist` refuses to hoist it—preserving `res` when `flag = 0`. Strength reduction then turns `t = i * 4` into `t_sr = t_sr + 4`, cutting multiplications on `n = 6` iterations from 12 down to 3.

## Check your understanding

- Why is `safe_inv` safe to hoist even though `B2` does not dominate exit `B1` when `n == 0`?
- Why must the preheader initialize `n_scaled = n * 4` before deleting the basic induction variable `i`?

## Edition mapping and scope

| 1977 Green | 1986 Red | 2006 Purple |
| --- | --- | --- |
| 12–13 | 10 | 9 |

Numbers refer to chapters. Related readings and appendices are marked where applicable. See the [edition guide](../../README.md#edition-guide) for the bibliographic references and chapter coverage.

Strength reduction targets affine integer induction variables (`i = i + c`, `t = i * scale`) over reducible natural loops without floating-point rounding subtleties.
