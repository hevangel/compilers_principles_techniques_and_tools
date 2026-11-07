# Row-major array addresses

Lower a two-dimensional index to a byte offset. See the [Rust source](main.rs).

```sh
cargo run --example array_layout
cargo test --example array_layout
```

## What the example teaches

In row-major storage, advancing one row skips `cols` elements. `row_major` computes `(row * cols + col) * width` after checking the bounds, with checked arithmetic for the byte offset.

An array expression such as `a[row, col]` gives the compiler two indices. A machine usually needs a single address. **Row-major** layout stores all the elements of a row together, then places the next row immediately after it. For an array with five columns, moving down one row skips five elements. That distance is called the **row stride**.

### Turn the index into an address

1. Verify that `row < rows` and `col < cols`. The valid indices start at zero. The example also requires a nonzero element width.
2. Compute the element index `row * cols + col`. Two complete rows of five elements account for ten elements. Column 1 of the next row therefore has index 11.
3. Multiply the element index by the element width to get a byte offset. With eight-byte elements, index 11 has byte offset 88. Adding it to the sample base address 1000 gives address 1088.

`row_major` returns only the offset. Its checked arithmetic returns `None` if a multiplication or addition cannot fit in `usize`, even if the supplied indices are individually in bounds. Bounds checks and arithmetic overflow checks guard different properties.

### Connect the formula to generated code

For a fixed-width array type, the compiler can often obtain the column count and element width from type information. When the indices are runtime values, generated arithmetic computes the offset at runtime. Consecutive columns differ by one element width. Consecutive rows at a fixed column differ by a full row stride.

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
