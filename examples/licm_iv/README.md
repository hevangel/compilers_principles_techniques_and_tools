# Loop-Invariant Code Motion and Induction-Variable Strength Reduction

Identify safe loop invariants, hoist them to the preheader, replace derived induction-variable multiplications with incremental additions, and eliminate dead basic IVs. See the [Rust source](main.rs) and [Python source](main.py).

```sh
cargo run --example licm_iv
cargo test --example licm_iv
python3 examples/licm_iv/main.py
python3 -m unittest examples/licm_iv/main.py
```

## What the example teaches

Hoisting code out of a loop must respect both invariance and control-flow safety—if a statement sits inside a conditional branch and defines a variable live after the loop, moving it to the preheader can corrupt the value observed when the branch is bypassed. Strength reduction similarly replaces loop-body multiplications `t = i * 4` with additive updates and updates the loop exit test to `t_sr < n * 4`.

### Loop optimization stages

1. **Invariant marking:** Iterate until fixed point marking statements whose operands are constants, external, or uniquely defined invariants.
2. **Safety gate:** Hoist to the preheader only when the defining block dominates all loop exits or the target variable is dead outside the loop.
3. **Strength reduction + exit test replacement:** Replace `t = i * c` with `t_sr += c * step`, rewrite `i < n` to `t_sr < n * c`, and delete `i = i + step` once unused.

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
