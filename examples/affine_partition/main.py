"""Solve affine space partitions (Appendix B nullspace) and verify unimodular skewing + tiling."""

from fractions import Fraction
import math
import unittest


def lcm(a: int, b: int) -> int:
    return 0 if a == 0 or b == 0 else abs(a * b) // math.gcd(abs(a), abs(b))


def nullspace_basis(matrix: list[list[int]], cols: int) -> list[list[int]]:
    rows = len(matrix)
    rref = [[Fraction(x) for x in row] for row in matrix]
    pivot_cols: list[int] = []
    r = 0
    for c in range(cols):
        if r >= rows:
            break
        piv = next((i for i in range(r, rows) if rref[i][c] != 0), None)
        if piv is None:
            continue
        rref[r], rref[piv] = rref[piv], rref[r]
        scale = rref[r][c]
        for j in range(c, cols):
            rref[r][j] /= scale
        for i in range(rows):
            if i != r and rref[i][c] != 0:
                factor = rref[i][c]
                for j in range(c, cols):
                    rref[i][j] -= factor * rref[r][j]
        pivot_cols.append(c)
        r += 1

    pivot_set = set(pivot_cols)
    basis: list[list[int]] = []
    for free_col in range(cols):
        if free_col in pivot_set:
            continue
        vec_r = [Fraction(0)] * cols
        vec_r[free_col] = Fraction(1)
        for row_idx, piv_col in enumerate(pivot_cols):
            vec_r[piv_col] = -rref[row_idx][free_col]
        mult = 1
        for val in vec_r:
            mult = lcm(mult, val.denominator)
        int_vec = [int(val * mult) for val in vec_r]
        g = 0
        for x in int_vec:
            g = math.gcd(g, abs(x))
        if g > 1:
            int_vec = [x // g for x in int_vec]
        basis.append(int_vec)
    return basis


def det2(m: list[list[int]]) -> int:
    return m[0][0] * m[1][1] - m[0][1] * m[1][0]


def mat2_mul(a: list[list[int]], b: list[list[int]]) -> list[list[int]]:
    return [
        [
            a[0][0] * b[0][0] + a[0][1] * b[1][0],
            a[0][0] * b[0][1] + a[0][1] * b[1][1],
        ],
        [
            a[1][0] * b[0][0] + a[1][1] * b[1][0],
            a[1][0] * b[0][1] + a[1][1] * b[1][1],
        ],
    ]


def mat2_vec(m: list[list[int]], v: tuple[int, int]) -> tuple[int, int]:
    return (m[0][0] * v[0] + m[0][1] * v[1], m[1][0] * v[0] + m[1][1] * v[1])


def is_lex_positive(v: tuple[int, int]) -> bool:
    return v[0] > 0 or (v[0] == 0 and v[1] > 0)


def is_legal_unimodular(u: list[list[int]], deps: list[tuple[int, int]]) -> bool:
    return abs(det2(u)) == 1 and all(is_lex_positive(mat2_vec(u, d)) for d in deps)


def can_tile_2d(deps: list[tuple[int, int]]) -> bool:
    return all(d[0] >= 0 and d[1] >= 0 for d in deps)


def run_original_wavefront(rows: int, cols: int) -> list[list[int]]:
    a = [[r * 7 + c * 3 for c in range(cols + 2)] for r in range(rows + 2)]
    for i in range(1, rows + 1):
        for j in range(1, cols + 1):
            a[i][j] = (a[i - 1][j + 1] * 3 + i + 2 * j) % 1009
    return a


def run_skewed_tiled_wavefront(
    rows: int, cols: int, tile_u: int, tile_v: int
) -> list[list[int]]:
    a = [[r * 7 + c * 3 for c in range(cols + 2)] for r in range(rows + 2)]
    for tu in range(1, rows + 1, tile_u):
        tu_end = min(tu + tile_u - 1, rows)
        for tv in range(2, rows + cols + 1, tile_v):
            tv_end = min(tv + tile_v - 1, rows + cols)
            for u in range(tu, tu_end + 1):
                j_lo = max(1, tv - u)
                j_hi = min(cols, tv_end - u)
                for v in range(u + j_lo, u + j_hi + 1):
                    i, j = u, v - u
                    a[i][j] = (a[i - 1][j + 1] * 3 + i + 2 * j) % 1009
    return a


def verify_nullspace(matrix: list[list[int]], basis: list[list[int]]) -> bool:
    return all(
        sum(a * b for a, b in zip(row, v)) == 0
        for v in basis
        for row in matrix
    )


def main() -> None:
    deps_3d = [[1, -1, 0], [0, 1, -1]]
    basis = nullspace_basis(deps_3d, 3)
    print(
        f"Appendix B sync-free affine partition basis for {deps_3d}: "
        f"{basis} (valid={verify_nullspace(deps_3d, basis)})"
    )

    deps = [(1, -1)]
    interchange = [[0, 1], [1, 0]]
    skew = [[1, 0], [1, 1]]
    skewed_interchange = mat2_mul(interchange, skew)
    skewed_dep = mat2_vec(skew, deps[0])
    print(
        f"Original dep={deps[0]}: tileable={can_tile_2d(deps)}, "
        f"interchange_legal={is_legal_unimodular(interchange, deps)}"
    )
    print(
        f"After skew S=[[1,0],[1,1]]: det={det2(skew)}, dep'={skewed_dep}, "
        f"tileable={can_tile_2d([skewed_dep])}, "
        f"skew+interchange_legal={is_legal_unimodular(skewed_interchange, deps)}"
    )

    seq = run_original_wavefront(8, 8)
    tiled = run_skewed_tiled_wavefront(8, 8, 3, 4)
    print(f"8x8 wavefront skewed+tiled exact match: {seq == tiled}")


class AffinePartitionTests(unittest.TestCase):
    def test_appendix_b_nullspace(self) -> None:
        mat = [[1, -1, 0], [0, 1, -1]]
        basis = nullspace_basis(mat, 3)
        self.assertEqual(basis, [[1, 1, 1]])
        self.assertTrue(verify_nullspace(mat, basis))

        mat2 = [[2, 1, -1, 0], [4, 2, 0, -2]]
        basis2 = nullspace_basis(mat2, 4)
        self.assertEqual(len(basis2), 2)
        self.assertTrue(verify_nullspace(mat2, basis2))

    def test_skewing_enables_tiling_and_preserves_output(self) -> None:
        deps = [(1, -1)]
        interchange = [[0, 1], [1, 0]]
        skew = [[1, 0], [1, 1]]
        self.assertFalse(can_tile_2d(deps))
        self.assertFalse(is_legal_unimodular(interchange, deps))
        self.assertTrue(is_legal_unimodular(skew, deps))
        skewed = mat2_vec(skew, deps[0])
        self.assertEqual(skewed, (1, 0))
        self.assertTrue(can_tile_2d([skewed]))

        seq = run_original_wavefront(10, 12)
        for tu, tv in ((2, 2), (3, 5), (4, 4)):
            self.assertEqual(run_skewed_tiled_wavefront(10, 12, tu, tv), seq)


if __name__ == "__main__":
    main()
