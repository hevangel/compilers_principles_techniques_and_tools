# Available Expressions, Global CSE, and Copy Propagation

Compute available expressions via forward must-analysis initialized to the universal set, eliminate global common subexpressions, and propagate copies. See the [Rust source](main.rs) and [Python source](main.py).

```sh
cargo run --example available_exprs
cargo test --example available_exprs
python3 examples/available_exprs/main.py
python3 -m unittest examples/available_exprs/main.py
```

## What the example teaches

Unlike reaching definitions (a forward *may* analysis using set union initialized to empty), available expressions is a forward *must* analysis using set intersection initialized to the universal set $\mathcal{U}$ so back-edges do not prematurely kill invariants.

### Two paired global passes

1. **Global CSE:** Any expression in `in_avail[B]` (and not locally killed) reuses a single `_cse_k` temporary materialized at earlier defining blocks.
2. **Copy Propagation:** A separate must-analysis tracks `(dst, src)` copies valid along all paths and substitutes `src` for `dst` at downstream uses before `dst` or `src` is overwritten.

## What to look for

In the diamond CFG, `x + y` (canonicalized from `y + x` on the right branch) is available at join block `B3` because *both* branches compute it, whereas `a * b` is computed only in `B1` and therefore omitted from `in_avail[3]`. In the loop test, initializing `out[B]` to $\mathcal{U}$ correctly preserves `b + c` around the back-edge while killing `a + b` when `a` is reassigned in `B2`.

## Check your understanding

- Why would initializing `out[B]` to the empty set break loop-invariant available expressions at a loop header?
- Why does copy propagation follow CSE instead of running only before it?

## Edition mapping and scope

| 1977 Green | 1986 Red | 2006 Purple |
| --- | --- | --- |
| 12, 14 | 10 | 9 |

Numbers refer to chapters. Related readings and appendices are marked where applicable. See the [edition guide](../../README.md#edition-guide) for the bibliographic references and chapter coverage.

Expressions canonicalize commutative `+` and `*` operands alphabetically over scalar variables without alias-aware memory loads.
