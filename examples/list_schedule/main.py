"""Schedule instructions subject to dependencies, latency, and issue width."""

from dataclasses import dataclass
import unittest


@dataclass(frozen=True)
class Task:
    name: str
    latency: int
    depends_on: tuple[int, ...]


def schedule(tasks: list[Task], width: int) -> list[int] | None:
    if width <= 0 or any(
        task.latency <= 0 or any(dep < 0 or dep >= len(tasks) for dep in task.depends_on)
        for task in tasks
    ):
        return None
    issued: list[int | None] = [None] * len(tasks)
    pending = len(tasks)
    cycle = 0
    while pending > 0:
        slots = width
        next_ready: int | None = None
        for index, task in enumerate(tasks):
            if issued[index] is not None:
                continue
            ready_at: int | None = 0
            for dep in task.depends_on:
                start = issued[dep]
                if start is None:
                    ready_at = None
                    break
                assert ready_at is not None
                ready_at = max(ready_at, start + tasks[dep].latency)
            if ready_at is not None:
                if ready_at <= cycle and slots > 0:
                    issued[index] = cycle
                    slots -= 1
                    pending -= 1
                else:
                    candidate = cycle + 1 if ready_at <= cycle else ready_at
                    next_ready = candidate if next_ready is None else min(next_ready, candidate)
        if pending > 0:
            if slots < width:
                cycle += 1
            else:
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
