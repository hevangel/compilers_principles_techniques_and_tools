# Loop interchange and spatial locality

Change matrix-multiply traversal while preserving the result. See the [Rust source](main.rs) and [Python source](main.py).

```sh
cargo run --example loop_locality
cargo test --example loop_locality
python3 examples/loop_locality/main.py
python3 -m unittest examples/loop_locality/main.py
```

## What the example teaches

The `i, j, k` order walks a column of the row-major right-hand matrix in its inner loop. The `i, k, j` order walks a row instead. For each output element, both versions still accumulate terms in increasing `k` order.

**Spatial locality** means that nearby memory addresses tend to be accessed close together in time. A processor commonly fetches a cache line containing several adjacent values at once, so traversing a row-major array along a row often gives subsequent accesses a chance to use the same line.

### Why we need loop interchange

Modern CPUs fetch memory in **cache lines** (typically 64 bytes, or 8 consecutive `i64` values), not single array elements. If an inner loop reads `B[0, 0]`, `B[1, 0]`, `B[2, 0]`, ... down a column in a row-major layout (`B[k * n + j]`), each access jumps forward by `n` elements. When `n` is large, every single read hits a different cache line, and those lines get evicted before the outer loop ever comes back for column `j + 1`.

At the same time, a compiler cannot blindly swap loops: if an interchange reverses the order of a true data dependence (for example, computing `A[i, j]` from `A[i - 1, j + 1]`), the transformed loop reads values before they are written! Checking legality first and then interchanging for stride-1 access gives cache-friendly traversal without changing program semantics.

### Compare two traversal orders

Matrix multiplication accumulates `A[i, k] * B[k, j]` into `C[i, j]`. The original order is `i, j, k`. The innermost index `k` changes the row of `B`, so its flat index `k * n + j` increases by `n` on each inner iteration. The changed order is `i, k, j`. Now the innermost index `j` changes the column, so the flat index increases by one.

For a 2 × 2 matrix, the first output row produces these sequences of flat indices into `B`.

| Loop order | Indices read | Adjacent `+1` transitions |
| --- | --- | --- |
| `i, j, k` | `0, 2, 1, 3` | `0` (every step jumps by `+2` or `-1`) |
| `i, k, j` | `0, 1, 2, 3` | `3` (`0->1`, `1->2`, `2->3` are all stride `1`) |

### Check legality separately from locality

For any fixed output cell `C[i, j]`, both versions add the terms in increasing `k` order. The changed schedule interleaves independent output cells. Because `C` has its own storage, an update to one cell cannot change a source matrix entry. The test verifies the product and the trace statistic independently.

In the 4 × 4 run, both versions make `4 * 4 * 4 = 64` reads of `B`. The adjacency count rises from `0` to `60`: each full `16`-element sweep over `B` has `15` adjacent `+1` steps (since rows concatenate contiguously in row-major memory), and the `4` sweeps of `i` give `4 * 15 = 60` adjacent reads (with `3` resets back to index `0` when `i` increments). Actual cache misses also depend on hardware and the other memory accesses.

### How to read the code

| Symbol | Role in [Rust](main.rs) / [Python](main.py) |
| --- | --- |
| `Trace` | Counts total reads of `B` and adjacent stride-1 reads (`previous + 1 == index`) as a cache-line locality proxy |
| `original` | Standard `i, j, k` matrix multiplication; inner loop steps down columns of `B` (stride `n`) |
| `interchanged` | Reordered `i, k, j` matrix multiplication; inner loop steps across rows of `B` (stride `1`) and broadcasts `A[i, k]` |
| `wrap_i64` | (Python) Emulates Rust's 64-bit two's-complement wrapping arithmetic so integer results match bit-for-bit |
| `main` / tests | Compares `original` and `interchanged` on 4 × 4 and 2 × 2 matrices to assert identical products and higher adjacency |

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
