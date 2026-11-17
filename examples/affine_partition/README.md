# Affine Partitioning, Unimodular Transforms, and Appendix B Linear Algebra

Compute exact nullspace bases for synchronization-free space partitioning and prove loop skewing makes negative-distance wavefront nests legal for interchange and 2D tiling. See the [Rust source](main.rs) and [Python source](main.py).

```sh
cargo run --example affine_partition
cargo test --example affine_partition
python3 examples/affine_partition/main.py
python3 -m unittest examples/affine_partition/main.py
```

## What the example teaches

Parallelizing loop nests often requires reshaping the iteration space via integer linear algebra so dependences either vanish across processors (`c . (i_1 - i_2) = 0`) or become non-negative in every dimension (`d' >= 0`).

### Exact linear solver and unimodular transforms

| Component | Mathematical rule | Outcome in demo |
| --- | --- | --- |
| Appendix B nullspace | Reduced row-echelon over exact rationals (`A v = 0`) | Extracts `[1, 1, 1]` for dependences `(1, -1, 0)` and `(0, 1, -1)` |
| Loop interchange | `U = [[0, 1], [1, 0]]`, check `U * d >_lex 0` | Illegal on `(1, -1)` because `(-1, 1) <_lex 0` |
| Loop skewing | `S = [[1, 0], [1, 1]]` (`det(S) = 1`) | Maps `(1, -1)` to `(1, 0)`, making both interchange and 2D tiling legal |

## What to look for

The unmodified wavefront kernel `A[i, j] = f(A[i - 1, j + 1])` cannot be tiled directly because `j + 1` reads ahead in `j`. Skewing by `v = i + j` rotates `(1, -1)` into `(1, 0)`, and the skewed 2D tiled loop produces an identical `10 x 12` matrix across multiple tile sizes.

## Check your understanding

- Why must a valid unimodular loop transformation matrix have `|det(U)| = 1`?
- Why does skewing `v = i + j` turn `(1, -1)` into `(1, 0)` without changing the set of executed `(i, j)` points?

## Edition mapping and scope

| 1977 Green | 1986 Red | 2006 Purple |
| --- | --- | --- |
| — | — | 11, app. B |

Numbers refer to chapters. Related readings and appendices are marked where applicable. See the [edition guide](../../README.md#edition-guide) for the bibliographic references and chapter coverage.

Rational arithmetic in Rust uses normalized `gcd`-reduced `i64` pairs on small integer matrices; Python uses `fractions.Fraction`.
