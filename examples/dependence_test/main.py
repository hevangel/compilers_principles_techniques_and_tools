"""Test affine array subscript dependences via GCD, Banerjee bounds, and Fourier-Motzkin elimination."""

from dataclasses import dataclass
import math
import unittest


@dataclass(frozen=True)
class AffineSubscript:
    """One subscript dimension: `offset + sum_k coeffs[k] * i_k`."""

    offset: int
    coeffs: list[int]


@dataclass(frozen=True)
class LinearIneq:
    """Linear half-space `sum_j coeffs[j] * x_j <= rhs` over `[i_0..i_{m-1}, i'_0..i'_{m-1}]`."""

    coeffs: list[int]
    rhs: int

    def normalize(self) -> "LinearIneq":
        """Divide by positive GCD to keep coefficients small without changing feasibility."""
        g = abs(self.rhs)
        for c in self.coeffs:
            g = math.gcd(g, abs(c))
        if g > 1:
            return LinearIneq([c // g for c in self.coeffs], self.rhs // g)
        return self

    def is_all_zero(self) -> bool:
        return all(c == 0 for c in self.coeffs)


@dataclass
class LoopNestQuery:
    """Dependence query pairing write and read subscripts with rectangular + coupled bounds."""

    dims: int
    bounds: list[LinearIneq]
    rect_bounds: list[tuple[int, int]]
    write_sub: list[AffineSubscript]
    read_sub: list[AffineSubscript]


def gcd_test(write_sub: list[AffineSubscript], read_sub: list[AffineSubscript]) -> bool:
    """Necessary Diophantine check: `gcd(w_coeffs, r_coeffs)` must divide `r.offset - w.offset`."""
    for w, r in zip(write_sub, read_sub):
        g = 0
        for c in w.coeffs + r.coeffs:
            g = math.gcd(g, abs(c))
        delta = r.offset - w.offset
        if g == 0:
            if delta != 0:
                return False
        elif delta % g != 0:
            return False
    return True


def banerjee_test(query: LoopNestQuery) -> bool:
    """Extreme-value interval check on rectangular bounds `[L_k, U_k]`.

    Computes `[lo, hi]` of `sum_k (w_k i_k - r_k i'_k)` by picking each variable's min/max
    endpoint independently; returns False if `r.offset - w.offset` falls outside `[lo, hi]`.
    """
    for w, r in zip(query.write_sub, query.read_sub):
        lo = 0
        hi = 0
        for k in range(query.dims):
            lk, uk = query.rect_bounds[k]
            wk = w.coeffs[k]
            rk = r.coeffs[k]
            lo += wk * lk if wk >= 0 else wk * uk
            hi += wk * uk if wk >= 0 else wk * lk
            neg_rk = -rk
            lo += neg_rk * lk if neg_rk >= 0 else neg_rk * uk
            hi += neg_rk * uk if neg_rk >= 0 else neg_rk * lk
        target = r.offset - w.offset
        if target < lo or target > hi:
            return False
    return True


def fourier_motzkin_feasible(num_vars: int, system: list[LinearIneq]) -> bool:
    """Eliminate variables one by one via Fourier-Motzkin projection over the reals.

    Pairs every upper bound (`coeffs[var] > 0`) with every lower bound (`coeffs[var] < 0`)
    using positive multipliers `cn * p + cp * n` so `var` cancels. A derived `0 <= negative`
    row proves the linear system (including coupled triangular bounds) is infeasible.
    """
    sys = list(system)
    for var in range(num_vars):
        pos = [iq for iq in sys if iq.coeffs[var] > 0]
        neg = [iq for iq in sys if iq.coeffs[var] < 0]
        rest = [iq for iq in sys if iq.coeffs[var] == 0]
        for p in pos:
            cp = p.coeffs[var]
            for n in neg:
                cn = -n.coeffs[var]
                coeffs = [cn * p.coeffs[j] + cp * n.coeffs[j] for j in range(num_vars)]
                coeffs[var] = 0
                rhs = cn * p.rhs + cp * n.rhs
                comb = LinearIneq(coeffs, rhs).normalize()
                if comb.is_all_zero():
                    if comb.rhs < 0:
                        return False
                else:
                    rest.append(comb)
        sys = rest
    return all(not (iq.is_all_zero() and iq.rhs < 0) for iq in sys)


def build_fm_system(query: LoopNestQuery) -> list[LinearIneq]:
    """Combine loop bounds with subscript equalities split into `<=` and `>=` half-spaces."""
    nvars = query.dims * 2
    sys = list(query.bounds)
    for w, r in zip(query.write_sub, query.read_sub):
        le_coeffs = [0] * nvars
        ge_coeffs = [0] * nvars
        for k in range(query.dims):
            le_coeffs[k] = w.coeffs[k]
            le_coeffs[query.dims + k] = -r.coeffs[k]
            ge_coeffs[k] = -w.coeffs[k]
            ge_coeffs[query.dims + k] = r.coeffs[k]
        rhs_le = r.offset - w.offset
        sys.append(LinearIneq(le_coeffs, rhs_le).normalize())
        sys.append(LinearIneq(ge_coeffs, -rhs_le).normalize())
    return sys


def enumerate_dependence_vectors(
    query: LoopNestQuery, sys: list[LinearIneq]
) -> list[list[int]]:
    """Enumerate integer `(i, i')` solutions inside `rect_bounds` to extract distance `i' - i`."""
    doubled = query.rect_bounds + query.rect_bounds
    out: set[tuple[int, ...]] = set()

    def rec(dim: int, curr: list[int]) -> None:
        if dim == len(doubled):
            if all(sum(c * x for c, x in zip(iq.coeffs, curr)) <= iq.rhs for iq in sys):
                m = query.dims
                out.add(tuple(curr[m + k] - curr[k] for k in range(m)))
            return
        lo, hi = doubled[dim]
        for v in range(lo, hi + 1):
            curr.append(v)
            rec(dim + 1, curr)
            curr.pop()

    rec(0, [])
    return [list(t) for t in sorted(out)]


def analyze_dependence(query: LoopNestQuery) -> tuple[str, list[list[int]]]:
    """Layered dependence pipeline: GCD -> Banerjee -> Fourier-Motzkin -> integer witness search."""
    if not gcd_test(query.write_sub, query.read_sub):
        return "IndependentByGcd", []
    if not banerjee_test(query):
        return "IndependentByBanerjee", []
    sys = build_fm_system(query)
    if not fourier_motzkin_feasible(query.dims * 2, sys):
        return "IndependentByFourierMotzkin", []
    vecs = enumerate_dependence_vectors(query, sys)
    if not vecs:
        return "IndependentByFourierMotzkin", []
    return "Dependent", vecs


def rect_bounds_ineqs(rect: list[tuple[int, int]]) -> list[LinearIneq]:
    m = len(rect)
    nvars = m * 2
    sys: list[LinearIneq] = []
    for k, (lo, hi) in enumerate(rect):
        for base in (0, m):
            ub = [0] * nvars
            ub[base + k] = 1
            sys.append(LinearIneq(ub, hi))
            lb = [0] * nvars
            lb[base + k] = -1
            sys.append(LinearIneq(lb, -lo))
    return sys


def query_even_vs_odd() -> LoopNestQuery:
    rect = [(0, 10)]
    return LoopNestQuery(
        1,
        rect_bounds_ineqs(rect),
        rect,
        [AffineSubscript(0, [2])],
        [AffineSubscript(1, [2])],
    )


def query_out_of_range_shift() -> LoopNestQuery:
    rect = [(0, 3)]
    return LoopNestQuery(
        1,
        rect_bounds_ineqs(rect),
        rect,
        [AffineSubscript(0, [1])],
        [AffineSubscript(5, [1])],
    )


def query_triangular_fm_infeasible() -> LoopNestQuery:
    rect = [(0, 4), (0, 4)]
    bounds = rect_bounds_ineqs(rect) + [
        LinearIneq([1, -1, 0, 0], 0),
        LinearIneq([0, 0, 1, -1], 0),
    ]
    return LoopNestQuery(
        2,
        bounds,
        rect,
        [AffineSubscript(0, [1, 0]), AffineSubscript(0, [0, 1])],
        [AffineSubscript(0, [0, 1]), AffineSubscript(-1, [1, 0])],
    )


def query_feasible_wavefront() -> LoopNestQuery:
    rect = [(1, 4), (1, 4)]
    return LoopNestQuery(
        2,
        rect_bounds_ineqs(rect),
        rect,
        [AffineSubscript(0, [1, 0]), AffineSubscript(0, [0, 1])],
        [AffineSubscript(-1, [1, 0]), AffineSubscript(1, [0, 1])],
    )


def main() -> None:
    cases = [
        ("A[2i] vs A[2i+1] on 0<=i<=10", query_even_vs_odd()),
        ("A[i] vs A[i+5] on 0<=i<4", query_out_of_range_shift()),
        ("Triangular A[i,j] vs A[j',i'-1]", query_triangular_fm_infeasible()),
        ("Wavefront A[i,j] vs A[i-1,j+1]", query_feasible_wavefront()),
    ]
    for label, q in cases:
        outcome, vecs = analyze_dependence(q)
        print(f"{label}: {outcome}, dist_vectors={vecs}")


class DependenceTestCases(unittest.TestCase):
    def test_gcd_even_odd(self) -> None:
        outcome, vecs = analyze_dependence(query_even_vs_odd())
        self.assertEqual(outcome, "IndependentByGcd")
        self.assertEqual(vecs, [])

    def test_out_of_range_shift(self) -> None:
        q = query_out_of_range_shift()
        self.assertTrue(gcd_test(q.write_sub, q.read_sub))
        self.assertFalse(banerjee_test(q))
        self.assertFalse(fourier_motzkin_feasible(q.dims * 2, build_fm_system(q)))

    def test_coupled_triangular_fourier_motzkin(self) -> None:
        q = query_triangular_fm_infeasible()
        self.assertTrue(gcd_test(q.write_sub, q.read_sub))
        self.assertTrue(banerjee_test(q))
        outcome, vecs = analyze_dependence(q)
        self.assertEqual(outcome, "IndependentByFourierMotzkin")
        self.assertEqual(vecs, [])

    def test_wavefront_dependence_vector(self) -> None:
        outcome, vecs = analyze_dependence(query_feasible_wavefront())
        self.assertEqual(outcome, "Dependent")
        self.assertEqual(vecs, [[1, -1]])


if __name__ == "__main__":
    main()
