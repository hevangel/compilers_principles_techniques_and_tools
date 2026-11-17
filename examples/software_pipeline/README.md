# Software Pipelining and Modular Variable Expansion

Overlap consecutive iterations of an inner loop via cyclic modulo scheduling while honoring resource capacities, cross-iteration recurrences, and scalar register lifetimes. See the [Rust source](main.rs) and [Python source](main.py).

```sh
cargo run --example software_pipeline
cargo test --example software_pipeline
python3 examples/software_pipeline/main.py
python3 -m unittest examples/software_pipeline/main.py
```

## What the example teaches

Instead of scheduling one loop iteration completely before starting the next, software pipelining launches a new iteration every `II` (initiation interval) cycles, emitting a ramp-up prologue, a repeating steady-state kernel, and a drain epilogue.

### Initiation interval lower bounds and variable expansion

| Bound or step | Formula / rule | Role in demo |
| --- | --- | --- |
| Resource bound | `MII_res = max_u ceil(ops_u / cap_u)` | Two ALU ops on 1 ALU force `MII_res = 2` |
| Recurrence bound | `MII_rec = max_C ceil(sum L / sum D)` | A 3-cycle loop-carried accumulation (`D = 1, L = 3`) forces `MII_rec = 3` |
| Modular scheduling | `S(u) + L - D * II <= S(v)` and `slot = S(op) % II` | Searches increasing `II >= max(MII_res, MII_rec)` until a valid cyclic table exists |
| Modular variable expansion | Unroll kernel by `ceil(max_lifetime / II)` | Assigns distinct register banks so overlapping iterations never clobber values |

## What to look for

The resource-bound kernel overlaps its 2-cycle load, two ALU ops, and store across `stages = 3` at `II = 2`. The recurrence-bound kernel respects the `accum -> accum` cross-iteration edge at `II = 3` and matches an unpipelined sequential run element-for-element across all 8 iterations.

## Check your understanding

- Why does a loop-carried dependence with latency `L = 5` and distance `D = 2` force an initiation interval of at least `ceil(5 / 2) = 3`?
- Why is modular variable expansion needed when an operation's result stays live longer than `II` cycles?

## Edition mapping and scope

| 1977 Green | 1986 Red | 2006 Purple |
| --- | --- | --- |
| — | — | 10 |

Numbers refer to chapters. Related readings and appendices are marked where applicable. See the [edition guide](../../README.md#edition-guide) for the bibliographic references and chapter coverage.

Modulo scheduling uses bounded backtracking over cycle offsets on integer loop kernels without predicated early exits.
