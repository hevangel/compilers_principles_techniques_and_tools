"""Compare matrix-multiply loop orders using a trace of row-major accesses."""

from dataclasses import dataclass
import unittest

I64_MASK = (1 << 64) - 1


def wrap_i64(value: int) -> int:
    """Truncate Python arbitrary-precision int to signed 64-bit two's complement (matching Rust)."""
    masked = value & I64_MASK
    return masked - (1 << 64) if masked >= (1 << 63) else masked


@dataclass
class Trace:
    """Spatial-locality proxy counting consecutive reads where `index == previous + 1` (stride 1)."""

    previous: int | None = None
    adjacent: int = 0
    total: int = 0

    def read(self, index: int) -> None:
        if self.previous is not None and self.previous + 1 == index:
            self.adjacent += 1
        self.previous = index
        self.total += 1


def original(a: list[int], b: list[int], n: int) -> tuple[list[int], Trace]:
    """`i, j, k` order: inner `k` loop strides down a column of row-major `B` by `+n`."""
    c = [0] * (n * n)
    trace = Trace()
    for i in range(n):
        for j in range(n):
            for k in range(n):
                rhs = k * n + j
                trace.read(rhs)
                c[i * n + j] = wrap_i64(c[i * n + j] + a[i * n + k] * b[rhs])
    return c, trace


def interchanged(a: list[int], b: list[int], n: int) -> tuple[list[int], Trace]:
    """`i, k, j` order: legal because `C` is disjoint from `A, B` and each `C[i, j]`
    still sums in increasing `k` order; inner `j` loop walks `B` and `C` with stride 1."""
    c = [0] * (n * n)
    trace = Trace()
    for i in range(n):
        for k in range(n):
            for j in range(n):
                rhs = k * n + j
                trace.read(rhs)
                c[i * n + j] = wrap_i64(c[i * n + j] + a[i * n + k] * b[rhs])
    return c, trace


def main() -> None:
    a = list(range(16))
    b = list(reversed(range(16)))
    reference, before = original(a, b, 4)
    result, after = interchanged(a, b, 4)
    assert reference == result
    print(f"Product: {result}")
    print(f"Adjacent B reads: {before.adjacent} -> {after.adjacent} across {after.total} reads")


class LoopLocalityTests(unittest.TestCase):
    def test_keeps_the_product_and_improves_contiguous_access(self) -> None:
        a = [1, 2, 3, 4]
        b = [5, 6, 7, 8]
        expected, before = original(a, b, 2)
        actual, after = interchanged(a, b, 2)
        self.assertEqual(expected, [19, 22, 43, 50])
        self.assertEqual(actual, expected)
        self.assertEqual(before.total, after.total)
        self.assertGreater(after.adjacent, before.adjacent)
        self.assertEqual(interchanged([], [], 0)[0], [])


if __name__ == "__main__":
    main()
