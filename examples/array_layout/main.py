"""Lower a checked row-major array index to a byte offset."""

import unittest

# Upper bound of a 64-bit unsigned pointer/offset (`usize::MAX` in Rust) so Python's
# arbitrary-precision integers still detect hardware-width overflow.
USIZE_MAX = (1 << 64) - 1


def row_major(row: int, col: int, rows: int, cols: int, width: int) -> int | None:
    """Lower a 2D `(row, col)` index to a row-major byte offset, or `None` if invalid."""
    # Validate per-axis bounds before flattening so `col >= cols` cannot alias the next row.
    if row < 0 or col < 0 or row >= rows or col >= cols or width <= 0:
        return None
    # Each complete row holds `cols` elements; scale element index `(row * cols + col)` by `width`.
    offset = (row * cols + col) * width
    # Bounds-valid indices can still overflow 64-bit address arithmetic when `cols * width` is huge.
    return offset if offset <= USIZE_MAX else None


def main() -> None:
    # Keep the relative offset calculation independent of where the array starts in memory.
    base = 1000
    offset = row_major(2, 1, 4, 5, 8)
    assert offset is not None
    print(f"Element [2, 1]: byte offset {offset}, address {base + offset}")


class ArrayLayoutTests(unittest.TestCase):
    def test_applies_row_stride_and_checks_bounds_and_overflow(self) -> None:
        self.assertEqual(row_major(2, 1, 4, 5, 8), 88)
        self.assertEqual(row_major(0, 0, 4, 5, 8), 0)
        self.assertIsNone(row_major(0, 5, 4, 5, 8))
        self.assertIsNone(row_major(1, 0, 2, USIZE_MAX, 8))


if __name__ == "__main__":
    main()
