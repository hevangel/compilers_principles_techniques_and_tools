# Row-major array addresses

Lower a two-dimensional index to a byte offset. See the [Rust source](main.rs).

```sh
cargo run --example array_layout
cargo test --example array_layout
```

## What the example teaches

In row-major storage, advancing one row skips `cols` elements. `row_major` computes `(row * cols + col) * width` after checking the bounds, with checked arithmetic for the byte offset.

## What to look for

Element `[2, 1]` of a four-by-five array with eight-byte elements is 88 bytes from its base. The test covers the first element, an out-of-bounds column, and overflow.

## Edition mapping and scope

| 1977 Green | 1986 Red | 2006 Purple |
| --- | --- | --- |
| 2, 8 | 8–9 | 6, 8 |

Numbers refer to chapters. Related readings and appendices are marked where applicable. See the [edition guide](../../README.md#edition-guide) for the bibliographic references and chapter coverage.

The example uses a zero-based rectangular array and models address arithmetic without accessing memory.
