# Array Dependence Testing (GCD, Banerjee, and Fourier-Motzkin)

Prove loop iterations independent or extract exact dependence distance vectors across affine array subscripts and coupled triangular loop nests. See the [Rust source](main.rs) and [Python source](main.py).

```sh
cargo run --example dependence_test
cargo test --example dependence_test
python3 examples/dependence_test/main.py
python3 -m unittest examples/dependence_test/main.py
```

## What the example teaches

Parallelizing a loop requires proving that different iterations never access the same memory location with at least one write. compilers layer cheap sufficient independence tests in front of exact linear projection.

### Layered dependence decision pipeline

| Test | What it checks | Strength / limitation |
| --- | --- | --- |
| GCD test | `gcd(coeffs) \| (b0 - a0)` | Fast; ignores loop bounds completely |
| Banerjee bounds | Extreme values of affine sum on `[L_k, U_k]` | Exact for single linear equations over rectangular boxes |
| Fourier-Motzkin | Eliminates variables across `Ax <= c` | Handles coupled triangular bounds (`i <= j`) and multi-D subscripts |

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
