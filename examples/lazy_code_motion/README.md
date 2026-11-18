# Lazy Code Motion (Partial Redundancy Elimination)

Eliminate partially redundant computations safely without speculating onto cold paths using critical-edge splitting and Knoop-Rüthing-Steffen's four data-flow passes. See the [Rust source](main.rs) and [Python source](main.py).

```sh
cargo run --example lazy_code_motion
cargo test --example lazy_code_motion
python3 examples/lazy_code_motion/main.py
python3 -m unittest examples/lazy_code_motion/main.py
```

## What the example teaches

### Why we need this and when it is sound

Global CSE (`available_exprs`) only eliminates expressions computed on **every** path to a join. Often an expression `e` is computed on only *one* incoming branch (`hot(1) -> join(2)`), making `e` at `join(2)` **partially redundant**: along `entry(0) -> hot(1) -> join(2)` it runs twice, while along `entry(0) -> join(2)` it runs once.

**Partial Redundancy Elimination (PRE)** makes `e` totally redundant at `join(2)` by placing `t = e` on the missing incoming edge `0 -> 2`, then replacing `join(2)`'s copy with `t`. **Lazy Code Motion (LCM)** solves two subtle problems with naive PRE:

1. **No speculation (soundness):** Never insert `e` on a path that did not originally evaluate `e` (otherwise an unused branch or a 0-trip `while` loop would execute `e` unnecessarily or even trigger a fault). **Critical-edge splitting** ensures every edge from a multi-successor block to a multi-predecessor block gets its own landing block (`split_0_2`), and the `Anticipated` pass verifies `e` is needed on *every* exit path from an insertion point.
2. **Minimal register lifetimes:** Hoisting `e` as early as possible keeps `t` live in a register across unrelated code. The `Postponable` pass pushes placement as *late* as possible toward uses without duplicating work, and `Used` suppresses writing `t` if no downstream block ever reads it.

### Four passes after critical-edge splitting

| Pass | Direction | Meet | Role |
| --- | --- | --- | --- |
| `Anticipated` | Backward | Must (`&&`) | Ensures every exit path from a point evaluates the expression before any kill |
| `Available` | Forward | Must (`&&`) | Marks blocks reachable from an earlier anticipation; `earliest = anticipated && !available` |
| `Postponable` | Forward | Must (`&&`) | Pushes `earliest` placement forward toward uses; `latest` stops at `e_use` or branch points |
| `Used` | Backward | May (`||`) | Tracks whether `t` is actually read after `latest` so dead placements are skipped |

### Trace `partial_diamond_sample`

In `partial_diamond_sample()`, `entry(0)` branches to `hot(1)` or `join(2)` (`e_use = true` in `hot` and `join`). Critical edge `0 -> 2` splits into `split_0_2(4)`:

| Block | `e_use` | `anticipated` | `available` | `earliest` | `postponable` | `latest` | `used_out` | `insert` / `replace` |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| `B0 (entry)` | `false` | `true` | `false` | `true` | `false` | `false` | `true` | `false` / `false` |
| `B1 (hot)` | `true` | `true` | `true` | `false` | `true` | `true` | `true` | `true` / `true` |
| `B4 (split_0_2)` | `false` | `true` | `true` | `false` | `true` | `true` | `true` | `true` / `false` |
| `B2 (join)` | `true` | `true` | `true` | `false` | `false` | `false` | `false` | `false` / `true` |
| `B3 (exit)` | `false` | `false` | `true` | `false` | `false` | `false` | `false` | `false` / `false` |

Notice how `earliest` starts at `entry(0)`, `Postponable` pushes placement forward across `entry(0)` down into `hot(1)` and `split_0_2(4)`, and `join(2)` simply reuses `t` (`replace = true`).

### How to read the code

| Component | Rust ([`main.rs`](main.rs)) | Python ([`main.py`](main.py)) | Purpose |
| --- | --- | --- | --- |
| CFG & output | `LcmBlock`, `LcmSolution` | `LcmBlock`, `LcmSolution` | Single-expression block flags (`e_use`, `e_kill`) and all 4 pass vectors |
| Edge splitter | `split_critical_edges` | `split_critical_edges` | Inserts `split_b_s` landing pads on edges with `|succ(b)| > 1` and `|pred(s)| > 1` |
| 4-pass solver | `solve_lcm` | `solve_lcm` | Runs `Anticipated`, `Available`, `Postponable`, `Used` and derives `insert`/`replace` |
| Path counter & tests | `count_evals_along_path`, tests | `count_evals_along_path`, `LazyCodeMotionTests` | Compares dynamic evaluations on partial diamond, `while`, and `do_while` CFGs |

## What to look for

In `partial_diamond_sample`, edge `entry(0) -> join(2)` is critical and is automatically split into `split_0_2`. Lazy Code Motion places `t = e` in `hot(1)` and `split_0_2`, removes the redundant recomputation from `join(2)`, and cuts hot-path evaluations from 2 to 1. In `do_while`, loop-invariant `e` hoists to `preheader(0)`, cutting 3 iterations from 3 evaluations to 1 without hoisting when a 0-trip exit exists (`while` loop).

## Check your understanding

- Why does splitting critical edges prevent hoisting a calculation onto a branch that never needed it?
- What changes if `Postponable` is omitted and insertions are placed directly at `earliest`?

## Edition mapping and scope

| 1977 Green | 1986 Red | 2006 Purple |
| --- | --- | --- |
| — | — | 9 |

Numbers refer to chapters. Related readings and appendices are marked where applicable. See the [edition guide](../../README.md#edition-guide) for the bibliographic references and chapter coverage.

Bitvector flags track one normalized expression across basic blocks (`e_use`, `e_kill`) after edge splitting rather than full instruction rewriting.
