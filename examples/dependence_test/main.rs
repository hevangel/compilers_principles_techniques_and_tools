//! Test affine array subscript dependences via GCD, Banerjee bounds, and Fourier-Motzkin elimination.

use std::collections::BTreeSet;

fn gcd(mut a: i64, mut b: i64) -> i64 {
    a = a.abs();
    b = b.abs();
    while b != 0 {
        let r = a % b;
        a = b;
        b = r;
    }
    a
}

/// One dimension of an affine array subscript: `offset + sum_k coeffs[k] * i_k`.
#[derive(Clone, Debug)]
struct AffineSubscript {
    offset: i64,
    coeffs: Vec<i64>,
}

/// Linear half-space constraint `sum_j coeffs[j] * vars[j] <= rhs` over the
/// concatenated iteration pair `vars = [i_0, ..., i_{m-1}, i'_0, ..., i'_{m-1}]`.
#[derive(Clone, Debug, PartialEq, Eq)]
struct LinearIneq {
    coeffs: Vec<i64>,
    rhs: i64,
}

impl LinearIneq {
    /// Divide all coefficients and `rhs` by their positive GCD.
    /// Dividing by a positive integer preserves the sign and solution set while
    /// preventing coefficient blow-up during repeated Fourier-Motzkin projections.
    fn normalize(mut self) -> Self {
        let mut g = self.rhs.abs();
        for &c in &self.coeffs {
            g = gcd(g, c);
        }
        if g > 1 {
            for c in &mut self.coeffs {
                *c /= g;
            }
            self.rhs /= g;
        }
        self
    }

    fn is_all_zero(&self) -> bool {
        self.coeffs.iter().all(|&c| c == 0)
    }
}

/// Result of the layered dependence cascade: either a proof of independence
/// (naming the first test that ruled out collisions) or `Dependent` with distance vectors.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DependenceOutcome {
    IndependentByGcd,
    IndependentByBanerjee,
    IndependentByFourierMotzkin,
    Dependent,
}

/// Query asking whether write `write_sub(i)` and read `read_sub(i')` can ever touch the same
/// array element inside the given loop nest.
#[derive(Clone, Debug)]
struct LoopNestQuery {
    dims: usize,
    // Box or triangular bounds expressed over variables [i_0..i_{m-1}, i'_0..i'_{m-1}].
    bounds: Vec<LinearIneq>,
    // Simple constant bounds `[low, high]` for each original loop dim `0..dims` (for Banerjee / witness search).
    rect_bounds: Vec<(i64, i64)>,
    write_sub: Vec<AffineSubscript>,
    read_sub: Vec<AffineSubscript>,
}

/// GCD (Diophantine) test:
/// By Bezout's identity, `sum_k (w_k i_k - r_k i'_k) = r_0 - w_0` has an integer solution
/// (over unbounded integers) iff `gcd(w_0..w_{m-1}, r_0..r_{m-1})` divides `r_0 - w_0`.
/// Returns `true` if a dependence is still possible, `false` if proven independent.
fn gcd_test(write_sub: &[AffineSubscript], read_sub: &[AffineSubscript]) -> bool {
    for (w, r) in write_sub.iter().zip(read_sub) {
        let mut g = 0i64;
        for &c in w.coeffs.iter().chain(r.coeffs.iter()) {
            g = gcd(g, c);
        }
        let delta = r.offset - w.offset;
        if g == 0 {
            if delta != 0 {
                return false;
            }
        } else if delta % g != 0 {
            return false;
        }
    }
    true
}

/// Banerjee interval test over rectangular loop bounds `L_k <= i_k, i'_k <= U_k`.
/// Computes the exact minimum `lo` and maximum `hi` of `sum_k (w_k * i_k - r_k * i'_k)`
/// over the bounding box `[L, U]^2`. If `target = r_0 - w_0` falls outside `[lo, hi]`,
/// no real (hence no integer) solution exists inside the loop bounds.
fn banerjee_test(query: &LoopNestQuery) -> bool {
    for (w, r) in query.write_sub.iter().zip(&query.read_sub) {
        // Test equation: sum_k (w_k * i_k - r_k * i'_k) = r.offset - w.offset.
        let mut lo = 0i64;
        let mut hi = 0i64;
        for k in 0..query.dims {
            let (lk, uk) = query.rect_bounds[k];
            let wk = w.coeffs[k];
            let rk = r.coeffs[k];
            // Positive coefficient minimizes at `lk`, maximizes at `uk`; negative swaps them.
            lo += if wk >= 0 { wk * lk } else { wk * uk };
            hi += if wk >= 0 { wk * uk } else { wk * lk };
            // Min/max of (-rk) * i'_k on [lk, uk].
            let neg_rk = -rk;
            lo += if neg_rk >= 0 {
                neg_rk * lk
            } else {
                neg_rk * uk
            };
            hi += if neg_rk >= 0 {
                neg_rk * uk
            } else {
                neg_rk * lk
            };
        }
        let target = r.offset - w.offset;
        if target < lo || target > hi {
            return false;
        }
    }
    true
}

/// Fourier-Motzkin elimination over `Ax <= b`:
/// To project out variable `var`, partition constraints by the sign of `coeffs[var]`:
/// - `pos`: `cp * var + rest_p <= p.rhs`  (so `var <= (p.rhs - rest_p) / cp`)
/// - `neg`: `-cn * var + rest_n <= n.rhs` (so `(rest_n - n.rhs) / cn <= var`)
///
/// Pairing every `(p, n)` via `cn * p + cp * n` eliminates `var` exactly over the reals.
/// If any eliminated row collapses to `0 <= negative`, the original system is infeasible.
/// Note: FM decides *real* feasibility; `enumerate_dependence_vectors` confirms integer points.
fn fourier_motzkin_feasible(num_vars: usize, mut system: Vec<LinearIneq>) -> bool {
    for var in 0..num_vars {
        let mut pos = Vec::new();
        let mut neg = Vec::new();
        let mut rest = Vec::new();

        for ineq in system {
            let c = ineq.coeffs[var];
            if c > 0 {
                pos.push(ineq);
            } else if c < 0 {
                neg.push(ineq);
            } else {
                rest.push(ineq);
            }
        }

        for p in &pos {
            let cp = p.coeffs[var];
            for n in &neg {
                let cn = -n.coeffs[var]; // positive multiplier
                let mut coeffs: Vec<i64> = p
                    .coeffs
                    .iter()
                    .zip(&n.coeffs)
                    .map(|(&pj, &nj)| cn * pj + cp * nj)
                    .collect();
                coeffs[var] = 0;
                let rhs = cn * p.rhs + cp * n.rhs;
                let combined = LinearIneq { coeffs, rhs }.normalize();
                if combined.is_all_zero() {
                    if combined.rhs < 0 {
                        return false;
                    }
                } else {
                    rest.push(combined);
                }
            }
        }
        system = rest;
    }

    for ineq in &system {
        if ineq.is_all_zero() && ineq.rhs < 0 {
            return false;
        }
    }
    true
}

/// Encode subscript equalities `write_sub(i) == read_sub(i')` as pairs of `<=` and `>=`
/// inequalities and combine them with the loop-nest bounds.
fn build_fm_system(query: &LoopNestQuery) -> Vec<LinearIneq> {
    let nvars = query.dims * 2;
    let mut sys = query.bounds.clone();
    for (w, r) in query.write_sub.iter().zip(&query.read_sub) {
        // w.offset + sum w.coeffs[k]*i_k - (r.offset + sum r.coeffs[k]*i'_k) == 0
        let mut le_coeffs = vec![0i64; nvars];
        let mut ge_coeffs = vec![0i64; nvars];
        for k in 0..query.dims {
            le_coeffs[k] = w.coeffs[k];
            le_coeffs[query.dims + k] = -r.coeffs[k];
            ge_coeffs[k] = -w.coeffs[k];
            ge_coeffs[query.dims + k] = r.coeffs[k];
        }
        let rhs_le = r.offset - w.offset;
        sys.push(
            LinearIneq {
                coeffs: le_coeffs,
                rhs: rhs_le,
            }
            .normalize(),
        );
        sys.push(
            LinearIneq {
                coeffs: ge_coeffs,
                rhs: -rhs_le,
            }
            .normalize(),
        );
    }
    sys
}

/// Run the layered dependence pipeline: GCD -> Banerjee -> Fourier-Motzkin -> integer witness search.
fn analyze_dependence(query: &LoopNestQuery) -> (DependenceOutcome, Vec<Vec<i64>>) {
    if !gcd_test(&query.write_sub, &query.read_sub) {
        return (DependenceOutcome::IndependentByGcd, Vec::new());
    }
    if !banerjee_test(query) {
        return (DependenceOutcome::IndependentByBanerjee, Vec::new());
    }
    let sys = build_fm_system(query);
    if !fourier_motzkin_feasible(query.dims * 2, sys.clone()) {
        return (DependenceOutcome::IndependentByFourierMotzkin, Vec::new());
    }
    let vectors = enumerate_dependence_vectors(query, &sys);
    if vectors.is_empty() {
        return (DependenceOutcome::IndependentByFourierMotzkin, Vec::new());
    }
    (DependenceOutcome::Dependent, vectors)
}

fn satisfies_all(point: &[i64], sys: &[LinearIneq]) -> bool {
    sys.iter().all(|ineq| {
        let lhs: i64 = ineq.coeffs.iter().zip(point).map(|(&c, &x)| c * x).sum();
        lhs <= ineq.rhs
    })
}

fn enumerate_points(
    dim: usize,
    bounds: &[(i64, i64)],
    curr: &mut Vec<i64>,
    sys: &[LinearIneq],
    out: &mut BTreeSet<Vec<i64>>,
) {
    let total = bounds.len();
    if dim == total {
        if satisfies_all(curr, sys) {
            let m = total / 2;
            let dist: Vec<i64> = (0..m).map(|k| curr[m + k] - curr[k]).collect();
            out.insert(dist);
        }
        return;
    }
    let (lo, hi) = bounds[dim];
    for v in lo..=hi {
        curr.push(v);
        enumerate_points(dim + 1, bounds, curr, sys, out);
        curr.pop();
    }
}

fn enumerate_dependence_vectors(query: &LoopNestQuery, sys: &[LinearIneq]) -> Vec<Vec<i64>> {
    let mut doubled_bounds = query.rect_bounds.clone();
    doubled_bounds.extend_from_slice(&query.rect_bounds);
    let mut curr = Vec::new();
    let mut out = BTreeSet::new();
    enumerate_points(0, &doubled_bounds, &mut curr, sys, &mut out);
    out.into_iter().collect()
}

fn rect_bounds_ineqs(rect: &[(i64, i64)]) -> Vec<LinearIneq> {
    let m = rect.len();
    let nvars = m * 2;
    let mut sys = Vec::new();
    for (k, &(lo, hi)) in rect.iter().enumerate() {
        for base in [0, m] {
            let mut ub = vec![0i64; nvars];
            ub[base + k] = 1;
            sys.push(LinearIneq {
                coeffs: ub,
                rhs: hi,
            });
            let mut lb = vec![0i64; nvars];
            lb[base + k] = -1;
            sys.push(LinearIneq {
                coeffs: lb,
                rhs: -lo,
            });
        }
    }
    sys
}

fn query_even_vs_odd() -> LoopNestQuery {
    // for i in 0..=10: A[2*i] = A[2*i + 1]
    let rect = vec![(0, 10)];
    LoopNestQuery {
        dims: 1,
        bounds: rect_bounds_ineqs(&rect),
        rect_bounds: rect,
        write_sub: vec![AffineSubscript {
            offset: 0,
            coeffs: vec![2],
        }],
        read_sub: vec![AffineSubscript {
            offset: 1,
            coeffs: vec![2],
        }],
    }
}

fn query_out_of_range_shift() -> LoopNestQuery {
    // for i in 0..=3: A[i] = A[i + 5]
    // GCD passes (1 | 5), Banerjee fails [0 - 3, 3 - 0] = [-3, 3] vs 5!
    let rect = vec![(0, 3)];
    LoopNestQuery {
        dims: 1,
        bounds: rect_bounds_ineqs(&rect),
        rect_bounds: rect,
        write_sub: vec![AffineSubscript {
            offset: 0,
            coeffs: vec![1],
        }],
        read_sub: vec![AffineSubscript {
            offset: 5,
            coeffs: vec![1],
        }],
    }
}

fn query_triangular_fm_infeasible() -> LoopNestQuery {
    // Triangular 2D nest: 0 <= i <= 4, i <= j <= 4.
    // A[i, j] written, A[j, i - 1] read => i == j' and j == i' - 1.
    //Wait: if i <= j, then j' <= i' - 1 contradicts i' <= j'!!
    // GCD passes (1 | 0, 1 | -1), rectangular Banerjee passes on [0,4]x[0,4],
    // ONLY Fourier-Motzkin sees coupled triangular constraint `i <= j` & `i' <= j'` is infeasible!
    let rect = vec![(0, 4), (0, 4)];
    let mut bounds = rect_bounds_ineqs(&rect);
    // i - j <= 0  and  i' - j' <= 0
    bounds.push(LinearIneq {
        coeffs: vec![1, -1, 0, 0],
        rhs: 0,
    });
    bounds.push(LinearIneq {
        coeffs: vec![0, 0, 1, -1],
        rhs: 0,
    });
    LoopNestQuery {
        dims: 2,
        bounds,
        rect_bounds: rect,
        write_sub: vec![
            AffineSubscript {
                offset: 0,
                coeffs: vec![1, 0],
            },
            AffineSubscript {
                offset: 0,
                coeffs: vec![0, 1],
            },
        ],
        read_sub: vec![
            AffineSubscript {
                offset: 0,
                coeffs: vec![0, 1],
            }, // j'
            AffineSubscript {
                offset: -1,
                coeffs: vec![1, 0],
            }, // i' - 1
        ],
    }
}

fn query_feasible_wavefront() -> LoopNestQuery {
    // 2D nest: 1 <= i <= 4, 1 <= j <= 4: A[i, j] written, A[i - 1, j + 1] read.
    // Solution satisfies i' - i = 1, j' - j = -1 => distance vector (1, -1).
    let rect = vec![(1, 4), (1, 4)];
    LoopNestQuery {
        dims: 2,
        bounds: rect_bounds_ineqs(&rect),
        rect_bounds: rect,
        write_sub: vec![
            AffineSubscript {
                offset: 0,
                coeffs: vec![1, 0],
            },
            AffineSubscript {
                offset: 0,
                coeffs: vec![0, 1],
            },
        ],
        read_sub: vec![
            AffineSubscript {
                offset: -1,
                coeffs: vec![1, 0],
            },
            AffineSubscript {
                offset: 1,
                coeffs: vec![0, 1],
            },
        ],
    }
}

fn main() {
    let cases = [
        ("A[2i] vs A[2i+1] on 0<=i<=10", query_even_vs_odd()),
        ("A[i] vs A[i+5] on 0<=i<4", query_out_of_range_shift()),
        (
            "Triangular A[i,j] vs A[j',i'-1]",
            query_triangular_fm_infeasible(),
        ),
        ("Wavefront A[i,j] vs A[i-1,j+1]", query_feasible_wavefront()),
    ];
    for (label, q) in cases {
        let (outcome, vecs) = analyze_dependence(&q);
        println!("{label}: {outcome:?}, dist_vectors={vecs:?}");
    }
}

#[test]
fn gcd_proves_even_odd_independent() {
    let (outcome, vecs) = analyze_dependence(&query_even_vs_odd());
    assert_eq!(outcome, DependenceOutcome::IndependentByGcd);
    assert!(vecs.is_empty());
}

#[test]
fn out_of_range_shift_passes_gcd_but_fails_bounds_and_fm() {
    let q = query_out_of_range_shift();
    assert!(gcd_test(&q.write_sub, &q.read_sub));
    assert!(!banerjee_test(&q));
    assert!(!fourier_motzkin_feasible(q.dims * 2, build_fm_system(&q)));
    let (outcome, _) = analyze_dependence(&q);
    assert_eq!(outcome, DependenceOutcome::IndependentByBanerjee);
}

#[test]
fn coupled_triangular_bounds_require_fourier_motzkin() {
    let q = query_triangular_fm_infeasible();
    assert!(gcd_test(&q.write_sub, &q.read_sub));
    assert!(banerjee_test(&q));
    let (outcome, vecs) = analyze_dependence(&q);
    assert_eq!(outcome, DependenceOutcome::IndependentByFourierMotzkin);
    assert!(vecs.is_empty());
}

#[test]
fn feasible_wavefront_reports_exact_distance_vector() {
    let (outcome, vecs) = analyze_dependence(&query_feasible_wavefront());
    assert_eq!(outcome, DependenceOutcome::Dependent);
    assert_eq!(vecs, vec![vec![1, -1]]);
}
