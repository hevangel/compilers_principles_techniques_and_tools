# Basic-block list scheduling

Issue ready instructions subject to dependencies and per-cycle capacity. See the [Rust source](main.rs) and [Python source](main.py).

```sh
cargo run --example list_schedule
cargo test --example list_schedule
python3 examples/list_schedule/main.py
python3 -m unittest examples/list_schedule/main.py
```

## What the example teaches

`schedule` scans pending instructions in input order. An instruction is ready after every predecessor has issued and its latency has elapsed. At most the configured number of instructions can issue per cycle.

Modern processors can overlap work. An instruction **issues** when it starts execution, while its **latency** is the number of cycles until its result is available. The **issue width** caps how many instructions can start in one cycle. A compiler can rearrange independent instructions to use cycles that would otherwise go idle.

### Why we need list scheduling

Suppose a compiler emits instructions naively in source order without reordering:

1. `load a` (latency 3) starts at cycle 0.
2. `load b` (latency 2) starts at cycle 0 (if width is 2) or cycle 1 (if width is 1).
3. `add a, b` depends on both loads, so it cannot start until cycle `max(0 + 3, 0 + 2) = 3`.
4. `independent multiply` sits *after* `add a, b` in program order. If an in-order processor or static VLIW schedule refuses to move it ahead of `add a, b`, cycles 1 and 2 are wasted waiting for `load a` even though `independent multiply` needs none of the loads!

Even worse, on statically scheduled architectures without hardware interlocks, issuing `add a, b` before `load a`'s 3-cycle latency expires would silently read a stale register value. **Basic-block list scheduling** solves both problems: it respects every data dependence and functional-unit width limit while pulling independent work forward to fill stall cycles.

### Decide what is ready

Each `Task` records which other tasks produce its inputs. If any producer has yet to issue, the task is not ready. Otherwise its earliest start is the maximum of `issue_cycle + latency` across its producers. The scheduler scans the tasks in input order and issues a ready task when a slot is available. A task with no predecessors can start immediately.

> Note: Production list schedulers often break ties among multiple ready instructions using **critical-path priority** (longest latency chain to an exit node). This demo scans ready tasks in input order so you can trace every step deterministically by hand.

Walking through the 5-task demo at `width = 2`:

| Cycle | Instructions issued at width 2 | Reason |
| --- | --- | --- |
| 0 | `load a` (lat 3), `load b` (lat 2) | Two independent tasks fill both issue slots |
| 1 | `independent multiply` (lat 2) | `add a, b` is blocked until cycle 3, so the scheduler pulls the independent multiply forward |
| 2 | None | `add a, b` is still waiting for `load a`; `consume` is waiting for `add a, b` |
| 3 | `add a, b` (lat 1) | Both `load a` (`0 + 3 = 3`) and `load b` (`0 + 2 = 2`) have completed |
| 4 | `consume sum and product` (lat 1) | Both `add a, b` (`3 + 1 = 4`) and `independent multiply` (`1 + 2 = 3`) are ready |

The load of `a` issues at cycle 0 with latency 3, so the add cannot issue at cycle 2 even though the second load is already ready. The independent multiply finishes at cycle 3 and can be consumed at cycle 4.

### Understand a stall

If a scan issues nothing (`slots == width`) but identifies a future ready time (`next_ready`), `schedule` jumps directly to that cycle instead of spinning one cycle at a time. If tasks remain (`pending > 0`) and none have a known future ready time (`next_ready` is `None`), every remaining task is blocked on another unissued task, which proves the dependency graph has a cycle. The cycle test exercises that case.

### How to read the code

| Symbol | Role in [Rust](main.rs) / [Python](main.py) |
| --- | --- |
| `Task` | Represents one basic-block instruction with its `name`, execution `latency`, and producer indices `depends_on` |
| `schedule` | Main list-scheduling loop: validates inputs, computes `ready_at = max(issued[dep] + latency[dep])`, fills up to `width` slots per cycle, and jumps over idle stall cycles |
| `TASKS` | The 5-instruction demo basic block (`load a`, `load b`, `add a, b`, `independent multiply`, `consume sum and product`) |
| `main` / tests | Runs `TASKS` at width 2 and tests width 1, forward-referenced producers, and cyclic-dependency rejection |

## What to look for

Two loads can issue together. An independent multiply fills a waiting cycle, and the consumer waits for the sum and product. The tests check one- and two-wide issue, a forward reference, and a dependency cycle.

## Check your understanding

- At width 1, predict the issue cycle for the multiply. Does the final consumer need to move later in this sample?
- If the load of `a` has latency 5, calculate the earliest issue cycles for the add and final consumer.

## Edition mapping and scope

| 1977 Green | 1986 Red | 2006 Purple |
| --- | --- | --- |
| — | — | 10 |

Numbers refer to chapters. Related readings and appendices are marked where applicable. See the [edition guide](../../README.md#edition-guide) for the bibliographic references and chapter coverage.

The dependency graph and latencies are supplied. The model uses interchangeable issue slots and fixed latencies.
