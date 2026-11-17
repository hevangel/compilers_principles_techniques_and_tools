# Lazy Code Motion (Partial Redundancy Elimination)

Eliminate partially redundant computations safely without speculating onto cold paths using critical-edge splitting and Knoop-Rüthing-Steffen's four data-flow passes. See the [Rust source](main.rs) and [Python source](main.py).

```sh
cargo run --example lazy_code_motion
cargo test --example lazy_code_motion
python3 examples/lazy_code_motion/main.py
python3 -m unittest examples/lazy_code_motion/main.py
```

## What the example teaches

A computation that is redundant along one path to a join block (partial redundancy) can be made totally redundant by placing copies on the remaining incoming paths, while `Postponable` delays placement as late as possible so register lifetimes stay minimal.

### Four passes after critical-edge splitting

| Pass | Direction | Meet | Role |
| --- | --- | --- | --- |
| `Anticipated` | Backward | Must (`&&`) | Ensures every exit path from a point evaluates the expression |
| `Available` | Forward | Must (`&&`) | Defines `earliest = anticipated && !available` |
| `Postponable` | Forward | Must (`&&`) | Pushes placement forward toward uses without duplicating |
| `Used` | Backward | May (`||`) | Suppresses dead placements where value is never reused |

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
