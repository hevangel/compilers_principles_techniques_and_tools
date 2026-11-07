# Loop interchange and spatial locality

Change matrix-multiply traversal while preserving the result. See the [Rust source](main.rs).

```sh
cargo run --example loop_locality
cargo test --example loop_locality
```

## What the example teaches

The `i, j, k` order walks a column of the row-major right-hand matrix in its inner loop. The `i, k, j` order walks a row instead. For each output element, both versions still accumulate terms in increasing `k` order.

**Spatial locality** means that nearby memory addresses tend to be accessed close together in time. A processor commonly fetches a cache line containing several adjacent values at once, so traversing a row-major array along a row often gives subsequent accesses a chance to use the same line.

### Compare two traversal orders

Matrix multiplication accumulates `A[i, k] * B[k, j]` into `C[i, j]`. The original order is `i, j, k`. The innermost index `k` changes the row of `B`, so its flat index `k * n + j` increases by `n` on each inner iteration. The changed order is `i, k, j`. Now the innermost index `j` changes the column, so the flat index increases by one.

For a 2 × 2 matrix, the first output row produces these sequences of flat indices into `B`.

| Loop order | Indices read |
| --- | --- |
| `i, j, k` | `0, 2, 1, 3` |
| `i, k, j` | `0, 1, 2, 3` |

### Check legality separately from locality

For any fixed output cell `C[i, j]`, both versions add the terms in increasing `k` order. The changed schedule interleaves independent output cells. Because `C` has its own storage, an update to one cell cannot change a source matrix entry. The test verifies the product and the trace statistic independently.

In the 4 × 4 run, both versions make 64 reads of `B`. The adjacency count rises from 0 to 60. It counts pairs of successive accesses where the second index is one larger, including pairs that cross loop boundaries. Actual cache misses also depend on hardware and the other memory accesses.

## What to look for

The demo prints identical output values and counts adjacent right-hand-matrix reads. The test checks a known 2 × 2 product and an increase in adjacent reads with the same total number of reads.

## Check your understanding

- Trace the first six accesses to `B` in each order for `n = 3`. Which order uses a whole row first?
- Explain why a change that reordered the `k` terms for each output cell would need extra care for floating-point arithmetic.

## Edition mapping and scope

| 1977 Green | 1986 Red | 2006 Purple |
| --- | --- | --- |
| — | — | 11 |

Numbers refer to chapters. Related readings and appendices are marked where applicable. See the [edition guide](../../README.md#edition-guide) for the bibliographic references and chapter coverage.

The example counts contiguous accesses as a proxy for spatial locality. It uses square matrices and models no hardware cache.
