# Array Dependence Testing (GCD, Banerjee, and Fourier-Motzkin)

Prove loop iterations independent or extract exact dependence distance vectors across affine array subscripts and coupled triangular loop nests. See the [Rust source](main.rs) and [Python source](main.py).

```sh
cargo run --example dependence_test
cargo test --example dependence_test
python3 examples/dependence_test/main.py
python3 -m unittest examples/dependence_test/main.py
```

## What the example teaches

Parallelizing a loop requires proving that different iterations never access the same memory location with at least one write. Compilers layer cheap sufficient independence tests in front of exact linear projection.

### Why we need layered dependence testing

Suppose a compiler wants to vectorize or parallelize a loop over `i`. If iteration `i` writes `A[...]` and another iteration `i'` reads (or writes) the same element, running them concurrently creates a data race and corrupts the output. Conversely, conservatively assuming every two accesses to `A` might collide throws away almost all parallelism.

To avoid solving expensive integer linear programs for every array pair, compilers use a **cascade of progressively stronger tests**. Each test checks a necessary condition for dependence: if *any* test proves no solution `(i, i')` exists, the accesses are guaranteed **independent** and the pipeline stops early.

### Walkthrough of the four demo queries

1. **GCD test** (`A[2i]` vs `A[2i + 1]` on `0 <= i <= 10`):
   - Same memory element requires `2i = 2i' + 1`, i.e. `2i - 2i' = 1`.
   - Since `gcd(2, -2) = 2` does not divide `1`, no integers `(i, i')` can ever make the subscripts equal, regardless of loop bounds. Stopped immediately as `IndependentByGcd`.
2. **Banerjee bounds test** (`A[i]` vs `A[i + 5]` on `0 <= i <= 3`):
   - `i - i' = 5` passes GCD (`gcd(1, -1) = 1 | 5`).
   - On the rectangular box `0 <= i, i' <= 3`, `i - i'` attains minimum `0 - 3 = -3` and maximum `3 - 0 = 3`. Because `5` lies outside `[-3, 3]`, stopped as `IndependentByBanerjee`.
3. **Fourier-Motzkin projection** (triangular `0 <= i <= j <= 4`: `A[i, j]` vs `A[j', i' - 1]`):
   - Subscript equalities give `i = j'` and `j = i' - 1`. Both pass GCD, and on the bounding box `[0, 4] x [0, 4]` Banerjee sees `[0 - 4, 4 - 0]` containing `0` and `-1`, so Banerjee conservatively says "maybe dependent".
   - Fourier-Motzkin includes the **coupled triangular inequalities** `i <= j` and `i' <= j'`. Substituting `i = j'` and `j = i' - 1` into `i <= j` yields `j' <= i' - 1`, which contradicts `i' <= j'` (`0 <= -1`)! Stopped as `IndependentByFourierMotzkin`.
4. **Dependent wavefront** (`1 <= i, j <= 4`: `A[i, j]` vs `A[i - 1, j + 1]`):
   - All three checks find feasible solutions `i' = i + 1, j' = j - 1`. Enumerating points inside the nest extracts the exact dependence distance vector `i' - i = [1, -1]`.

### Layered dependence decision pipeline

| Test | What it checks | Strength / limitation |
| --- | --- | --- |
| GCD test | `gcd(coeffs) \| (b0 - a0)` | Fast; ignores loop bounds completely |
| Banerjee bounds | Extreme values of affine sum on `[L_k, U_k]` | Exact for single linear equations over rectangular boxes |
| Fourier-Motzkin | Eliminates variables across `Ax <= c` | Handles coupled triangular bounds (`i <= j`) and multi-D subscripts |

### How to read the code

| Symbol | Role in [Rust](main.rs) / [Python](main.py) |
| --- | --- |
| `AffineSubscript`, `LinearIneq`, `LoopNestQuery` | Represents affine array indices (`offset + sum coeffs[k]*i_k`), half-space constraints `coeffs . x <= rhs`, and rectangular/coupled loop bounds |
| `gcd_test` | Checks Diophantine divisibility `gcd(w_coeffs, r_coeffs) \| (r_offset - w_offset)` per subscript dimension |
| `banerjee_test` | Computes exact `[lo, hi]` extreme values of `sum_k (w_k i_k - r_k i'_k)` on `rect_bounds` |
| `build_fm_system`, `fourier_motzkin_feasible` | Encodes subscript equalities as paired `<=` / `>=` inequalities and eliminates variables `[i, i']` one by one to detect `0 <= negative` contradictions |
| `enumerate_dependence_vectors` | Searches integer points in `rect_bounds` satisfying `sys` to collect unique distance vectors `i' - i` |
| `analyze_dependence` | Orchestrates the GCD -> Banerjee -> Fourier-Motzkin -> witness pipeline |

## What to look for

- `A[2i]` vs `A[2i + 1]` fails the GCD test immediately (`gcd = 2` does not divide `1`).
- `A[i]` vs `A[i + 5]` on `0 <= i < 4` passes GCD (`gcd = 1 | 5`), but fails bounds and Fourier-Motzkin projection.
- `A[i, j]` vs `A[j', i' - 1]` on triangular `0 <= i <= j <= 4` passes both GCD and rectangular Banerjee bounds, yet Fourier-Motzkin proves empty intersection.
- `A[i, j]` vs `A[i - 1, j + 1]` is dependent with exact distance vector `[1, -1]`.

## Check your understanding

- Why does the Banerjee test miss the independence of `A[i, j]` and `A[j', i' - 1]` on `0 <= i <= j <= 4`?
- Under what condition can Fourier-Motzkin report feasibility even when no integer solution exists?

## Edition mapping and scope

| 1977 Green | 1986 Red | 2006 Purple |
| --- | --- | --- |
| — | — | 11 |

Numbers refer to chapters. Related readings and appendices are marked where applicable. See the [edition guide](../../README.md#edition-guide) for the bibliographic references and chapter coverage.

Fourier-Motzkin checks real-space feasibility over rational inequalities (`gcd`-normalized `i64` rows) and enumerates bounded loop vectors when feasible.
