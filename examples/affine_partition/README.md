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

### Why we need affine partitioning and unimodular skewing

Two classic compiler transformations fail if applied naively without linear-algebraic legality checks:

1. **Synchronization-free parallelism (Space partitioning)**: To assign iterations `i` to processor `p = c . i` with *no* synchronization or communication between processors, any two dependent iterations `i_1` and `i_2` must land on the same processor: `c . i_1 = c . i_2`, which means `c . (i_1 - i_2) = 0` for every dependence distance vector `d = i_1 - i_2`. Equivalently, `c` must lie in the **nullspace** of the matrix whose rows are the dependence vectors. In the demo's 3D nest with dependences `(1, -1, 0)` and `(0, 1, -1)`, guessing partitions by trial and error is brittle; exact rational Gauss-Jordan elimination (Appendix B) systematically yields `[1, 1, 1]`, proving that iterations sharing `i + j + k = const` are completely independent!
2. **Loop interchange and 2D tiling on wavefronts**: Consider `A[i, j] = f(A[i - 1, j + 1])`. The producer at `(i - 1, j + 1)` is consumed at `(i, j)`, giving dependence vector `d = (1, -1)`:
   - **Naive interchange fails**: Swapping `i` and `j` via `U = [[0, 1], [1, 0]]` maps `d = (1, -1)` to `(-1, 1)`, which is lexicographically negative (`d'[0] = -1 < 0`). Executing that loop reads `A[i - 1, j + 1]` *before* the outer `j` loop has computed `j + 1`!
   - **Naive 2D tiling fails**: Splitting `(i, j)` into rectangular tiles causes a tile at `(ti, tj)` to depend on `(ti - 1, tj + 1)` while `(ti, tj + 1)` depends on `(ti - 1, tj + 2)`—you cannot sweep tiles row-by-row left-to-right because `j + 1` reads ahead in `j`.
   - **Unimodular skewing fixes both**: Applying `S = [[1, 0], [1, 1]]` (`det(S) = 1`, meaning `(u, v) = (i, i + j)` is a 1-to-1 bijection on integer lattice points) transforms `(1, -1)` into `S * (1, -1) = (1, 0)`. Because every entry of `(1, 0)` is `>= 0`, both `interchange * S` and rectangular `(tu, tv)` 2D tiling are immediately legal!

### Exact linear solver and unimodular transforms

| Component | Mathematical rule | Outcome in demo |
| --- | --- | --- |
| Appendix B nullspace | Reduced row-echelon over exact rationals (`A v = 0`) | Extracts `[1, 1, 1]` for dependences `(1, -1, 0)` and `(0, 1, -1)` |
| Loop interchange | `U = [[0, 1], [1, 0]]`, check `U * d >_lex 0` | Illegal on `(1, -1)` because `(-1, 1) <_lex 0` |
| Loop skewing | `S = [[1, 0], [1, 1]]` (`det(S) = 1`) | Maps `(1, -1)` to `(1, 0)`, making both interchange and 2D tiling legal |

### How to read the code

| Symbol | Role in [Rust](main.rs) / [Python](main.py) |
| --- | --- |
| `Rational` / `Fraction` | Exact rational arithmetic (`gcd`-reduced in Rust, `fractions.Fraction` in Python) so RREF never suffers floating-point rounding error |
| `nullspace_basis` | Reduces `M` to Reduced Row-Echelon Form and extracts primitive integer basis vectors for `M v = 0` |
| `det2`, `mat2_mul`, `mat2_vec` | 2×2 integer matrix operations for unimodular loop transforms |
| `is_lex_positive`, `is_legal_unimodular`, `can_tile_2d` | Verifies `\|det(U)\| = 1`, `U d >_lex 0` (legality), and `d >= 0` in every component (2D tileability) |
| `run_original_wavefront`, `run_skewed_tiled_wavefront` | Executes `A[i, j] = update(A[i - 1, j + 1])` in original `(i, j)` order and in skewed `(u, v) = (i, i + j)` tiled order, confirming bit-identical outputs |
| `verify_nullspace` | Asserts `M v = 0` for every extracted basis vector |

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
