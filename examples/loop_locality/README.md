# Loop interchange and spatial locality

Change matrix-multiply traversal while preserving the result. See the [Rust source](main.rs).

```sh
cargo run --example loop_locality
cargo test --example loop_locality
```

## What the example teaches

The `i, j, k` order walks a column of the row-major right-hand matrix in its inner loop. The `i, k, j` order walks a row instead. For each output element, both versions still accumulate terms in increasing `k` order.

## What to look for

The demo prints identical output values and counts adjacent right-hand-matrix reads. The test checks a known 2 × 2 product and an increase in adjacent reads with the same total number of reads.

## Edition mapping and scope

| 1977 Green | 1986 Red | 2006 Purple |
| --- | --- | --- |
| — | — | 11 |

Numbers refer to chapters. Related readings and appendices are marked where applicable. See the [edition guide](../../README.md#edition-guide) for the bibliographic references and chapter coverage.

The example counts contiguous accesses as a proxy for spatial locality. It uses square matrices and models no hardware cache.
