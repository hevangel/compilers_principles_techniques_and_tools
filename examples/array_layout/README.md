# Row-major array addresses

Lower a two-dimensional index to a byte offset. See the [Rust source](main.rs) and [Python source](main.py).

```sh
cargo run --example array_layout
cargo test --example array_layout
python3 examples/array_layout/main.py
python3 -m unittest examples/array_layout/main.py
```

## What the example teaches

### Why we need this

A parser recognizes `a[row, col]` as a two-dimensional indexing expression, but syntax alone cannot compute where that element lives in memory. Hardware RAM is a **flat, one-dimensional array of bytes**, so the compiler must translate multidimensional indices `(row, col)` into a single byte offset from the array's base address. It also needs two distinct safety checks:

- **Bounds checking**: `a[0, 5]` on a `4 × 5` matrix has the same flat index (`0 * 5 + 5 = 5`) as `a[1, 0]`, yet `col = 5` is illegal in row 0. Checking bounds on the flattened offset alone would miss out-of-bounds bugs that silently spill into the next row.
- **Overflow checking**: even when `row < rows` and `col < cols`, multiplying by `cols` or `width` on a large array can wrap `usize` arithmetic and point back inside the array (a classic memory-safety vulnerability).

In **row-major** storage (used by C, Rust, and Python NumPy by default), all elements of row 0 sit contiguously in memory, followed immediately by row 1, and so on. Advancing one row skips `cols` elements (the **row stride**), so `row_major` computes `(row * cols + col) * width` after checking both bounds and arithmetic overflow.

### Turn the index into an address

1. Verify that `0 <= row < rows`, `0 <= col < cols`, and `width > 0`. Bounds are checked per axis before flattening.
2. Compute the linear element index `row * cols + col` using overflow-checked arithmetic. Two complete rows of five elements account for `10` elements; column `1` of row `2` therefore has element index `11`.
3. Multiply the element index by `width` (also overflow-checked) to obtain the byte offset (`11 * 8 = 88`), then add it to the array base address (`1000 + 88 = 1088`).

### Trace the sample cases

For a `4 × 5` array of 8-byte elements (`row_stride = 5 * 8 = 40` bytes) at base address `1000`:

| Call | Bounds check | Linear element index | Byte offset | Address (`base + offset`) | Outcome |
| --- | --- | --- | --- | --- | --- |
| `row_major(0, 0, 4, 5, 8)` | `0 < 4`, `0 < 5` | `0 * 5 + 0 = 0` | `0` | `1000` | Valid (`Some(0)`) |
| `row_major(2, 1, 4, 5, 8)` | `2 < 4`, `1 < 5` | `2 * 5 + 1 = 11` | `11 * 8 = 88` | `1088` | Valid (`Some(88)`) |
| `row_major(0, 5, 4, 5, 8)` | `col = 5 >= 5` fails | — | — | — | Rejected (`None`) |
| `row_major(1, 0, 2, MAX, 8)` | `1 < 2`, `0 < MAX` | `1 * MAX + 0 = MAX` | `MAX * 8` overflows | — | Rejected (`None`) |

### Connect the formula to generated code

For a fixed-width array type, the compiler can often obtain the column count and element width from type information. When the indices are runtime values, generated arithmetic computes the offset at runtime. Consecutive columns differ by one element width (`8` bytes). Consecutive rows at a fixed column differ by a full row stride (`cols * width = 40` bytes).

## How to read the code

| Concept | Rust (`main.rs`) | Python (`main.py`) | Role |
| --- | --- | --- | --- |
| 64-bit address limit | `usize::MAX` (`checked_*`) | `USIZE_MAX` (`(1 << 64) - 1`) | Guards against byte-offset integer wraparound |
| Row-major lowering | `fn row_major(...) -> Option<usize>` | `def row_major(...) -> int \| None` | Checks per-axis bounds, flattens `(row * cols + col) * width`, checks overflow |
| Sample driver | `fn main()` | `def main()` | Computes `row_major(2, 1, 4, 5, 8)` and adds `base = 1000` |

## What to look for

Element `[2, 1]` of a four-by-five array with eight-byte elements is 88 bytes from its base. The test covers the first element, an out-of-bounds column, and overflow.

## Check your understanding

- Compute the offsets of `[2, 2]` and `[3, 1]` in the sample. Compare the byte distances from `[2, 1]`.
- Why must `[0, 5]` be rejected even though flattening it would give the same element index as `[1, 0]`?

## Edition mapping and scope

| 1977 Green | 1986 Red | 2006 Purple |
| --- | --- | --- |
| 2, 8 | 8–9 | 6, 8 |

Numbers refer to chapters. Related readings and appendices are marked where applicable. See the [edition guide](../../README.md#edition-guide) for the bibliographic references and chapter coverage.

The example uses a zero-based rectangular array and models address arithmetic without accessing memory.
