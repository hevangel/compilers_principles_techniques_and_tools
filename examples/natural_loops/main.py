"""Identify back edges using dominators and walk each natural loop backward."""

import unittest


def predecessors(edges: list[list[int]]) -> list[list[int]]:
    result: list[list[int]] = [[] for _ in edges]
    for source, targets in enumerate(edges):
        for target in targets:
            result[target].append(source)
    return result


def dominators(incoming: list[list[int]]) -> list[set[int]]:
    n = len(incoming)
    dom = [set(range(n)) for _ in range(n)]
    dom[0] = {0}
    while True:
        previous = [set(s) for s in dom]
        for node in range(1, n):
            parents = incoming[node]
            common = set(dom[parents[0]])
            for parent in parents[1:]:
                common &= dom[parent]
            common.add(node)
            dom[node] = common
        if dom == previous:
            return dom


def natural_loop(tail: int, header: int, incoming: list[list[int]]) -> set[int]:
    body = {header}
    pending = [tail]
    while pending:
        node = pending.pop()
        if node not in body:
            body.add(node)
            pending.extend(incoming[node])
    return body


def find_loops(edges: list[list[int]]) -> list[tuple[int, int, set[int]]]:
    incoming = predecessors(edges)
    dom = dominators(incoming)
    result: list[tuple[int, int, set[int]]] = []
    for tail, targets in enumerate(edges):
        for header in targets:
            if header in dom[tail]:
                result.append((tail, header, natural_loop(tail, header, incoming)))
    return result


GRAPH = [[1], [2, 4], [3], [1], []]


def main() -> None:
    for tail, header, body in find_loops(GRAPH):
        print(f"Back edge B{tail} -> B{header}, loop blocks {sorted(body)}")


class NaturalLoopsTests(unittest.TestCase):
    def test_excludes_preheader_and_handles_self_loop(self) -> None:
        self.assertEqual(find_loops(GRAPH), [(3, 1, {1, 2, 3})])
        self.assertEqual(find_loops([[1], [1]]), [(1, 1, {1})])
        self.assertEqual(find_loops([[1, 2], [2], []]), [])


if __name__ == "__main__":
    main()
