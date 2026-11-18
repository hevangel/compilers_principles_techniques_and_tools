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

### Why we need software pipelining

Basic-block list scheduling can only overlap instructions inside a *single* loop iteration. Consider the first demo kernel (`ld_a` -> `add1` -> `mul2` -> `st_b`):

- A single iteration is a strict chain of latencies `2 + 1 + 1 + 1 = 5` cycles. Running 8 iterations back-to-back takes roughly 40 cycles, leaving the `Mem` and `Alu` units idle most of the time.
- If a compiler naively overlaps iterations at `II = 1` without checking hardware limits, two `Alu` instructions (`add1` and `mul2`) collide on the single `Alu` unit every cycle.
- On the second demo kernel (`ld_a` -> `mul3` -> `accum` -> `st_b`), `accum` feeds its own next iteration (`dist = 1`, latency `3`). Naively starting a new iteration every 1 or 2 cycles reads `accum` before the previous iteration's 3-cycle accumulation finishes, producing wrong sums!
- Even when resources and dependences allow overlap, a value produced by `ld_a` in iteration `i` stays live for multiple cycles while iteration `i + 1` runs its own `ld_a`. Without **Modular Variable Expansion (MVE)** to rotate scalar register banks (`iter % unroll`), iteration `i + 1` clobbers iteration `i`'s loaded value before `add1` or `mul3` can consume it.

**Modulo scheduling** solves these problems systematically: it computes the tightest legal `II` from hardware capacity and loop-carried dependence cycles, packs one repeating `II`-cycle reservation table, and rotates register banks via MVE.

### Walkthrough of the two demo kernels

1. **Resource-bound kernel** (`1 Alu`, `2 Mem`):
   - Ops: `ld_a` (Mem, lat 2), `add1` (Alu, lat 1), `mul2` (Alu, lat 1), `st_b` (Mem, lat 1).
   - `MII_res = max(ceil(2 Mem / 2), ceil(2 Alu / 1)) = 2`; there are no cross-iteration edges, so `MII_rec = 1`.
   - At `II = 2`, the scheduler places `ld_a` at cycle `0`, `add1` at `2`, `mul2` at `3`, `st_b` at `4` (`num_stages = 3` because `4 / 2 + 1 = 3`). Every 2-cycle steady-state beat executes `ld_a(i+2)`, `add1(i+1)`, `mul2(i+1)`, and `st_b(i)` simultaneously without exceeding 1 `Alu` or 2 `Mem` slots per cycle.
2. **Recurrence-bound kernel** (`2 Alu`, `2 Mem`):
   - Ops: `ld_a` (Mem, lat 2), `mul3` (Alu, lat 1), `accum` (Alu, lat 3, self-edge `dist = 1`), `st_b` (Mem, lat 1).
   - `MII_res = 1`, but the self-loop on `accum` requires `S(accum) + 3 - 1 * II <= S(accum)`, forcing `II >= 3` (`MII_rec = 3`).
   - With `max_span = 3` and `II = 3`, `unroll_factor = ceil((3 + 1) / 3) = 2` MVE register banks (`0` and `1`), so overlapping iterations alternate banks and never overwrite each other's live values.

### Initiation interval lower bounds and variable expansion

| Bound or step | Formula / rule | Role in demo |
| --- | --- | --- |
| Resource bound | `MII_res = max_u ceil(ops_u / cap_u)` | Two ALU ops on 1 ALU force `MII_res = 2` |
| Recurrence bound | `MII_rec = max_C ceil(sum L / sum D)` | A 3-cycle loop-carried accumulation (`D = 1, L = 3`) forces `MII_rec = 3` |
| Modular scheduling | `S(u) + L - D * II <= S(v)` and `slot = S(op) % II` | Searches increasing `II >= max(MII_res, MII_rec)` until a valid cyclic table exists |
| Modular variable expansion | Unroll kernel by `ceil((max_span + 1) / II)` | Assigns distinct register banks so overlapping iterations never clobber values |

### How to read the code

| Symbol | Role in [Rust](main.rs) / [Python](main.py) |
| --- | --- |
| `ResourceUnit`, `OpKind`, `LoopOp`, `DepEdge`, `LoopKernel` | Defines the loop body operations, functional-unit capacities, and intra-/cross-iteration dependence edges (`dist = 0` or `dist >= 1`) |
| `compute_mii_res` | Computes the hardware resource lower bound `MII_res` across all functional units |
| `compute_mii_rec` (`dfs_cycles`) | Enumerates elementary dependence cycles to compute the recurrence lower bound `MII_rec` |
| `modulo_schedule` (`try_schedule_for_ii`) | Increments `II` from `max(MII_res, MII_rec)` and searches for valid start cycles `S(op)` with modulo reservation table `c % II` and MVE `unroll_factor` |
| `ScheduledInst`, `PipelinedProgram`, `build_pipelined_program` | Expands `ModuloSchedule` into ramp-up `prologue`, `unroll_factor`-copied steady-state `kernel_body`, and drain `epilogue` |
| `eval_op`, `execute_sequential`, `execute_pipelined` | Interprets both the original sequential loop and the software-pipelined schedule to verify identical memory and accumulator results |
| `verify_schedule_constraints` | (Rust) Independently asserts modulo resource capacities and all `S(u) + L - D * II <= S(v)` inequalities |

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
