# Global Trace Scheduling and Speculative Code Motion (§10.4)

Schedule hot multi-block execution traces across conditional branches and joins—pulling pure non-trapping operations upward into earlier stall slots while blocking unsafe speculative traps (`div`) and stores, checking off-trace register liveness, and inserting compensation code on cold edges. See the [Rust source](main.rs) and [Python source](main.py).

```sh
cargo run --example trace_schedule
cargo test --example trace_schedule
python3 examples/trace_schedule/main.py
python3 -m unittest examples/trace_schedule/main.py
```

## What the example teaches

### Why we need global trace scheduling

Basic-block list scheduling (`list_schedule`) cannot move instructions across branch boundaries. In control-heavy code, basic blocks often contain only 2–4 instructions each, leaving wide issue slots idle whenever a slow memory load stalls inside one block:

1. **`B0` has empty issue cycles while `r1 = load_safe(mem[0])` takes 3 cycles.** An independent multiply `r3 = r_in * 3` sits across the conditional branch in `B1` (`90%` hot path), and `r6 = r_in + 10` sits even further down below the join in `B3`.
2. **Naive code motion breaks cold paths.** Blindly hoisting instructions across branches can cause three classic bugs:
   - **Traps & side effects (`DivChecked`, `Store`):** Hoisting `r4 = 100 / r_guard` above `br_zero r_guard` crashes (`ZeroDivisionError`) when the cold `r_guard == 0` path runs!
   - **Clobbering cold-path registers (`live_in[off_trace]`):** Hoisting `r = ...` speculatively above a split overwrites `r` if the cold block `B2` reads the previous value of `r`.
   - **Missing work on side entrances (`Join`):** Hoisting `r6 = r_in + 10` from join block `B3` up past `B1` leaves the off-trace incoming edge `B2 -> B3` without `r6` unless **compensation code** is inserted in `B2`.

**Trace scheduling (§10.4)** picks the hot path `[B0, B1, B3]` using profile weights, builds one unified DAG containing data, memory, liveness, and control-safety barriers, schedules it globally, and automatically inserts compensation copies on cold edges so both paths compute identical outputs.

### Walkthrough on the diamond CFG (`width = 2`)

| Phase / Cycle | What happens | Why it is legal & profitable |
| --- | --- | --- |
| Trace & liveness | Picks `[B0, B1, B3]`; `live_in[B2] = {r2, r_in}` | Hot path covers `90%` weight; `r3` and `r6` are NOT live on entry to `B2` |
| Cycles `0–1` (inside `B0`) | Co-issues `r3 = r_in * 3` (cyc 0) and `r6 = r_in + 10` (cyc 1) alongside slow `load_safe(mem[0])` | Pure non-trapping ops fill `B0`'s load-stall window without clobbering `B2` |
| Cycle `4–5` (`br_zero` boundary) | `br_zero` finishes `B0`; `r4 = 100 / r_guard [trap]` waits until cycle `5` | `DivChecked` has a control barrier on `br_zero`, so `r_guard == 0` never traps |
| Compensation on `B2` | Tail-duplicates merged `B3` operations into cold block `B2` | Cold path (`r_guard = 0`) still computes `r6`..`mem[1] = r8` (`54`), saving 4 cycles on the hot trace (`9` vs. `13`) |

### How to read the code

| Symbol | Role in [Rust](main.rs) / [Python](main.py) |
| --- | --- |
| `Op`, `Block`, `CfgProgram` | CFG blocks with execution frequencies and `is_speculative_safe()` classification (`DivChecked` & `Store` vs. pure ops) |
| `pick_hot_trace` | Selects the highest-frequency acyclic execution trace (`[B0, B1, B3]`) |
| `compute_liveness` | Backward data-flow solver computing `live_in` / `live_out` so speculation never overwrites cold-live registers |
| `trace_schedule_cfg` | Combines basic-block baseline timing (`bb_cycles`), unified trace DAG scheduling, segment splitting, and §10.4 compensation insertion |
| `simulate_cfg` | Executes both original and trace-scheduled CFGs on hot (`r_guard = 2`) and cold (`r_guard = 0`) inputs to verify register/memory equivalence |

## What to look for

Running the demo prints the hot trace `["B0", "B1", "B3"]`, `cold B2 live_in=["r2", "r_in"]`, the global cycle-by-cycle schedule dropping from `13` (basic-block-only) to `9` cycles, the compensation moves inserted into `B2`, and exact output/memory agreement (`101` / `[20, 101]` hot; `54` / `[20, 54]` cold). Tests also verify that cold-live register clobbers and speculative memory `Store` hoisting above a controlling branch are blocked.

## Check your understanding

- Why can `r3 = r_in * 3` move safely into `B0` without adding compensation to `B2`, whereas `r6 = r_in + 10` hoisted from `B3` requires compensation in `B2`?
- Under what hardware or compiler feature (predication or non-faulting speculative loads) could an otherwise trapping instruction move above a branch?

## Edition mapping and scope

| 1977 Green | 1986 Red | 2006 Purple |
| --- | --- | --- |
| — | — | 10 (§10.4) |

Numbers refer to chapters. Related readings and appendices are marked where applicable. See the [edition guide](../../README.md#edition-guide) for the bibliographic references and chapter coverage.

This original §10.4 trace scheduler models non-predicated acyclic superblock traces with exact off-trace liveness barriers and edge compensation rather than full hardware-predicated hyperblocks.
