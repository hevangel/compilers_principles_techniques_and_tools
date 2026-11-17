"""Solve inclusion constraints for a context-insensitive points-to analysis."""

import unittest


def solve(points_to: list[set[str]], inclusions: list[tuple[int, int]]) -> list[set[str]]:
    state = [set(s) for s in points_to]
    while True:
        changed = False
        for source, destination in inclusions:
            previous = len(state[destination])
            state[destination] |= state[source]
            if len(state[destination]) != previous:
                changed = True
        if not changed:
            return state


def example() -> list[set[str]]:
    a, b, parameter, ret, x, y = 0, 1, 2, 3, 4, 5
    initial: list[set[str]] = [set() for _ in range(6)]
    initial[a].add("alloc_A")
    initial[b].add("alloc_B")
    return solve(
        initial,
        [
            (ret, y),
            (ret, x),
            (parameter, ret),
            (b, parameter),
            (a, parameter),
        ],
    )


def main() -> None:
    names = ["a", "b", "relay.p", "relay.return", "x", "y"]
    for variable, sites in zip(names, example()):
        print(f"{variable} may point to {sorted(sites)}")


class PointsToTests(unittest.TestCase):
    def test_merges_call_sites_and_propagates_around_a_cycle(self) -> None:
        solution = example()
        self.assertEqual(solution[0], {"alloc_A"})
        for sites in solution[2:]:
            self.assertEqual(sites, {"alloc_A", "alloc_B"})
        initial = [{"seed"}, set()]
        self.assertEqual(solve(initial, [(1, 0), (0, 1)]), [{"seed"}, {"seed"}])


if __name__ == "__main__":
    main()
