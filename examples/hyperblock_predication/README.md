# Hardware-Predicated Execution and If-Converted Hyperblocks (§10.4)

Select balanced multi-block regions into a single **predicated hyperblock**, convert control dependences into explicit boolean predicates (`cmp.zero.un` and OR-type join merges), promote safe operations upward while squashing traps/stores on false predicates, prove predicate disjointness to drop false WAW/memory dependences, and verify every execution path in a predicated hardware simulator. See the [Rust source](main.rs) and [Python source](main.py).

```sh
cargo run --example hyperblock_predication
cargo test --example hyperblock_predication
python3 examples/hyperblock_predication/main.py
python3 -m unittest examples/hyperblock_predication/main.py
```

## What the example teaches

### Why we need if-conversion and hyperblocks

Both basic-block scheduling (`list_schedule`) and single-path trace scheduling (`trace_schedule`) hit a wall on **balanced or unpredictable branches** (e.g. `52%` / `48%`):

1. **Trace scheduling backs only one side.** On a `52%` / `48%` branch, almost half of all executions exit the hot trace, paying branch-misprediction stalls and compensation overhead.
2. **Trapping operations (`100 / r_x`) cannot safely hoist above unpredicated branches.** Executing `100 / r_x` before testing `r_x == 0` crashes with `ZeroDivisionError` unless the hardware can cancel the fault when the guard predicate is `false`.
3. **Full if-conversion without tail selection bloats the schedule.** Inlining a `2%` cold error handler (`B5`) wastes VLIW issue slots on `98%` of normal executions.

A **predicated hyperblock (§10.4)** solves all three:

- **Hyperblock selection (`select_hyperblock`):** merges both arms of balanced branches (`B0`, `B1`, `B2`, `B3`, `B4`) into one straight-line predicated block while leaving ultra-cold handlers (`B5`, `freq = 2%`) outside behind a single `(p4) exit B5` side exit.
- **Hardware predication & trap squashing:** every instruction carries a boolean predicate `(p_k)`. When `P[p_k] == false`, hardware turns the operation—even a `DivChecked` `100 / 0` or `Store`—into a clean no-op with **zero traps** and zero architectural state changes.
- **Disjoint predicate analysis (`PredTree::are_disjoint`):** proves `p1` and `p3` are mutually exclusive, eliminating false WAW/WAR register and memory dependences so `(p1) r5 = r6 * 2` and `(p3) r5 = r3 + r7` co-exist cleanly without renaming.
- **Speculative predicate promotion (`promote_predicates`):** lifts pure, non-trapping operations (`r6 = r_y + 10` and `r7 = r_y * 3`) to `p0` so they execute inside `B0`'s `load_safe(mem[0])` latency stall before branches are even resolved.

### Walkthrough on the diamond-with-cold-exit CFG (`width = 2`)

| Cycle | Issued Predicated Instructions (`width = 2`) | Concept Demonstrated |
| --- | --- | --- |
| `0` | `(p0) cmp.zero.un p1, p2 = r_x` \|\| `(p0) r1 = load_safe(mem[0])` | Unconditional partition compare splits `p0` into disjoint `(p1, p2)` during load |
| `1` | `(p2) cmp.zero.un p3, p4 = r_err` \|\| `(p0) r6 = r_y + 10 [from p1]` | Nested compare (`p3`/`p4`) + promoted `B2` op filling load stall |
| `2` | `(p3) r3 = 100 / r_x [trap]` \|\| `(p0) r7 = r_y * 3 [from p3]` | Trapping `div` stays under `p3` (squashed when `r_x == 0`); pure `mul` promotes to `p0` |
| `3–5` | `(p1) r5 = r6 * 2`, `(p0) p5 = false`, `(p1)/(p3) p5 |= true`, `(p3) r5 = r3 + r7` | Disjoint `(p1, p3)` share `r5` with zero WAW edge; OR-predicate `p5` merges surviving paths |
| `6–8` | `(p5) r8 = r5 + r2` \|\| `(p4) exit B5`, `(p5) mem[1] = r8`, `(p5) ret r8` | Cold error path exits cleanly via `B5`; total `9` cycles vs `12.94` (`14.44` w/ mispredict) expected BB cycles |

### How to read the code

| Symbol | Role in [Rust](main.rs) / [Python](main.py) |
| --- | --- |
| `select_hyperblock` | Picks frequent/balanced CFG blocks (`>= 20%` of entry) while keeping cold exits (`B5`) out |
| `PredTree` | Tracks `PartitionChild` splits and `JoinOr` merges; proves exact mutual exclusion via `are_disjoint` |
| `if_convert` | Converts CFG control dependences into `CmpZeroUn`, `PredClear` + `PredOrTrue` joins, and `SideExit`s |
| `promote_predicates` | Safely lifts pure ops to `p0` while blocking `DivChecked`, `Store`, and live-sibling clobbers |
| `schedule_hyperblock` | Predicate-aware DAG + width-2 list scheduler stripping false WAW/WAR/memory edges across disjoint arms |
| `simulate_hyperblock` | Cycle-accurate predicated VLIW VM proving trap-squashed, side-exit, and memory equivalence |

## What to look for

Running the demo prints the hyperblock (`["B0", "B1", "B2", "B3", "B4"]` excluding `["B5"]`), per-block predicates, the 9-cycle dual-issue VLIW schedule (`12.94` expected BB cycles without misprediction, `14.44` with a 3-cycle cold-arm mispredict penalty), and identical simulation results across `then_path` (`83`), `else_path` (`79`, proving `100 / 0` traps are squashed with `traps=0`), and `cold_exit` (`-1`, `side_exited=true`). Unit tests also verify that promotion stops if `dst` is `live_in` on a sibling branch and that disjoint `(p1, p2)` writes and stores co-issue in the same cycle.

## Check your understanding

- Why does `(p2) cmp.zero.un p3, p4 = r_err` write **both** `p3` and `p4` to `false` when `p2 == false`, rather than only clearing them conditionally?
- When two arms write different values to `r_out` and `mem[0]`, why can a predicate-aware scheduler issue both in the **same cycle**, whereas an unpredicated trace scheduler must serialize them or rename registers?

## Edition mapping and scope

| 1977 Green | 1986 Red | 2006 Purple |
| --- | --- | --- |
| — | — | 10 (§10.4) |

Numbers refer to chapters. Related readings and appendices are marked where applicable. See the [edition guide](../../README.md#edition-guide) for the bibliographic references and chapter coverage.
