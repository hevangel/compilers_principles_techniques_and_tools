# Basic-block list scheduling

Issue ready instructions subject to dependencies and per-cycle capacity. See the [Rust source](main.rs).

```sh
cargo run --example list_schedule
cargo test --example list_schedule
```

## What the example teaches

`schedule` scans pending instructions in input order. An instruction is ready after every predecessor has issued and its latency has elapsed. At most the configured number of instructions can issue per cycle.

Modern processors can overlap work. An instruction **issues** when it starts execution, while its **latency** is the number of cycles until its result is available. The **issue width** caps how many instructions can start in one cycle. A compiler can rearrange independent instructions to use cycles that would otherwise go idle.

### Decide what is ready

Each `Task` records which other tasks produce its inputs. If any producer has yet to issue, the task is not ready. Otherwise its earliest start is the maximum of `issue_cycle + latency` across its producers. The scheduler scans the tasks in input order and issues a ready task when a slot is available. A task with no predecessors can start immediately.

| Cycle | Instructions issued at width 2 | Reason |
| --- | --- | --- |
| 0 | Load `a`, load `b` | Two independent tasks fill both slots |
| 1 | Independent multiply | The loads are still in flight |
| 2 | None | The add is waiting for `a` |
| 3 | Add `a, b` | Both loads are ready |
| 4 | Consume sum and product | Both producers are ready |

The load of `a` issues at cycle 0 with latency 3, so the add cannot issue at cycle 2 even though the second load is already ready. The independent multiply finishes at cycle 3 and can be consumed at cycle 4.

### Understand a stall

If a scan issues nothing but identifies a future ready time, `schedule` jumps to that time. If tasks remain and none have a known future ready time, their dependencies cannot be resolved. The cycle test exercises that case.

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
