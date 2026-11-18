"""Schedule instructions subject to dependencies, latency, and issue width."""

from dataclasses import dataclass
import unittest


@dataclass(frozen=True)
class Task:
    """One instruction in a basic-block data-dependence DAG."""

    name: str
    latency: int  # Cycles from issue until its result is ready for consumers.
    depends_on: tuple[int, ...]  # Indices of producer tasks that must complete first.


def schedule(tasks: list[Task], width: int) -> list[int] | None:
    """Greedy list scheduler honoring dependence latencies and per-cycle issue width.

    Invariants:
    - For every dependency `dep -> idx`, `issued[idx] >= issued[dep] + tasks[dep].latency`.
    - At most `width` instructions are assigned the same issue cycle.
    - Returns `None` if input constraints are invalid or the graph contains a cycle.
    """
    if width <= 0 or any(
        task.latency <= 0 or any(dep < 0 or dep >= len(tasks) for dep in task.depends_on)
        for task in tasks
    ):
        return None
    # `issued[i]` records the assigned issue cycle for task `i`, or `None` while pending.
    issued: list[int | None] = [None] * len(tasks)
    pending = len(tasks)
    cycle = 0
    while pending > 0:
        slots = width
        # Earliest future cycle at which some unissued task (whose predecessors are all
        # already issued) becomes eligible, letting us skip empty stall cycles.
        next_ready: int | None = None
        for index, task in enumerate(tasks):
            if issued[index] is not None:
                continue
            # Data-readiness is the max of `issue_cycle + latency` across all producers.
            ready_at: int | None = 0
            for dep in task.depends_on:
                start = issued[dep]
                if start is None:
                    ready_at = None
                    break
                assert ready_at is not None
                ready_at = max(ready_at, start + tasks[dep].latency)
            if ready_at is not None:
                # An instruction issues only when both data-ready and an issue slot remain.
                # Input order breaks ties (a full priority list scheduler sorts by critical path).
                if ready_at <= cycle and slots > 0:
                    issued[index] = cycle
                    slots -= 1
                    pending -= 1
                else:
                    candidate = cycle + 1 if ready_at <= cycle else ready_at
                    next_ready = candidate if next_ready is None else min(next_ready, candidate)
        if pending > 0:
            if slots < width:
                # At least one instruction issued, so advance one cycle and rescan.
                cycle += 1
            else:
                # Nothing issued this cycle: fast-forward to `next_ready`.
                # If `next_ready` is `None`, all pending tasks wait on each other (cycle).
                if next_ready is None:
                    return None
                cycle = next_ready
    return [c for c in issued if c is not None]


TASKS = [
    Task("load a", 3, ()),
    Task("load b", 2, ()),
    Task("add a, b", 1, (0, 1)),
    Task("independent multiply", 2, ()),
    Task("consume sum and product", 1, (2, 3)),
]


def main() -> None:
    cycles = schedule(TASKS, 2)
    assert cycles is not None
    for task, cycle in zip(TASKS, cycles):
        print(f"Cycle {cycle}: {task.name}")


class ListScheduleTests(unittest.TestCase):
    def test_respects_width_and_latencies(self) -> None:
        self.assertEqual(schedule(TASKS, 2), [0, 0, 3, 1, 4])
        self.assertEqual(schedule(TASKS, 1), [0, 1, 3, 2, 4])
        reversed_tasks = [
            Task("consumer", 1, (1,)),
            Task("producer", 4, ()),
        ]
        self.assertEqual(schedule(reversed_tasks, 2), [4, 0])

    def test_detects_a_cycle(self) -> None:
        tasks = [Task("a", 1, (1,)), Task("b", 1, (0,))]
        self.assertIsNone(schedule(tasks, 2))


if __name__ == "__main__":
    main()
