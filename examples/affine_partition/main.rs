//! Solve affine space partitions (Appendix B nullspace) and verify unimodular skewing + tiling.

use std::collections::BTreeSet;
use std::ops::{Add, Div, Mul, Sub};

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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Rational {
    num: i64,
    den: i64,
}

impl Rational {
    fn new(mut num: i64, mut den: i64) -> Self {
        assert!(den != 0, "division by zero");
        if den < 0 {
            num = -num;
            den = -den;
        }
        if num == 0 {
            return Self { num: 0, den: 1 };
        }
        let g = gcd(num, den);
        Self {
            num: num / g,
            den: den / g,
        }
    }

    fn from_int(n: i64) -> Self {
        Self { num: n, den: 1 }
    }

    fn is_zero(self) -> bool {
        self.num == 0
    }
}

impl Add for Rational {
    type Output = Self;
    fn add(self, rhs: Self) -> Self {
        Self::new(self.num * rhs.den + rhs.num * self.den, self.den * rhs.den)
    }
}

impl Sub for Rational {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self {
        Self::new(self.num * rhs.den - rhs.num * self.den, self.den * rhs.den)
    }
}

impl Mul for Rational {
    type Output = Self;
    fn mul(self, rhs: Self) -> Self {
        Self::new(self.num * rhs.num, self.den * rhs.den)
    }
}

impl Div for Rational {
    type Output = Self;
    fn div(self, rhs: Self) -> Self {
        Self::new(self.num * rhs.den, self.den * rhs.num)
    }
}

fn lcm(a: i64, b: i64) -> i64 {
    if a == 0 || b == 0 {
        0
    } else {
        (a.abs() / gcd(a, b)) * b.abs()
    }
}

// Appendix B: Reduce matrix M to reduced row-echelon form and extract integer nullspace basis `M v = 0`.
fn nullspace_basis(matrix: &[Vec<i64>], cols: usize) -> Vec<Vec<i64>> {
    let rows = matrix.len();
    let mut rref: Vec<Vec<Rational>> = matrix
        .iter()
        .map(|row| row.iter().map(|&x| Rational::from_int(x)).collect())
        .collect();

    let mut pivot_cols = Vec::new();
    let mut r = 0;
    for c in 0..cols {
        if r >= rows {
            break;
        }
        let Some(piv) = (r..rows).find(|&i| !rref[i][c].is_zero()) else {
            continue;
        };
        rref.swap(r, piv);
        let scale = rref[r][c];
        for cell in &mut rref[r][c..cols] {
            *cell = *cell / scale;
        }
        let pivot_row = rref[r].clone();
        for (i, row) in rref.iter_mut().enumerate() {
            if i != r && !row[c].is_zero() {
                let factor = row[c];
                for (cell, &piv_val) in row[c..cols].iter_mut().zip(&pivot_row[c..cols]) {
                    *cell = *cell - factor * piv_val;
                }
            }
        }
        pivot_cols.push(c);
        r += 1;
    }

    let pivot_set: BTreeSet<usize> = pivot_cols.iter().copied().collect();
    let mut basis = Vec::new();
    for free_col in 0..cols {
        if pivot_set.contains(&free_col) {
            continue;
        }
        let mut vec_r = vec![Rational::from_int(0); cols];
        vec_r[free_col] = Rational::from_int(1);
        for (row_idx, &piv_col) in pivot_cols.iter().enumerate() {
            vec_r[piv_col] = Rational::from_int(0) - rref[row_idx][free_col];
        }
        let mut mult = 1i64;
        for val in &vec_r {
            mult = lcm(mult, val.den);
        }
        let mut int_vec: Vec<i64> = vec_r
            .into_iter()
            .map(|val| val.num * (mult / val.den))
            .collect();
        let mut g = 0i64;
        for &x in &int_vec {
            g = gcd(g, x);
        }
        if g > 1 {
            for x in &mut int_vec {
                *x /= g;
            }
        }
        basis.push(int_vec);
    }
    basis
}

type Mat2 = [[i64; 2]; 2];

fn det2(m: Mat2) -> i64 {
    m[0][0] * m[1][1] - m[0][1] * m[1][0]
}

fn mat2_mul(a: Mat2, b: Mat2) -> Mat2 {
    [
        [
            a[0][0] * b[0][0] + a[0][1] * b[1][0],
            a[0][0] * b[0][1] + a[0][1] * b[1][1],
        ],
        [
            a[1][0] * b[0][0] + a[1][1] * b[1][0],
            a[1][0] * b[0][1] + a[1][1] * b[1][1],
        ],
    ]
}

fn mat2_vec(m: Mat2, v: [i64; 2]) -> [i64; 2] {
    [
        m[0][0] * v[0] + m[0][1] * v[1],
        m[1][0] * v[0] + m[1][1] * v[1],
    ]
}

fn is_lex_positive(v: [i64; 2]) -> bool {
    v[0] > 0 || (v[0] == 0 && v[1] > 0)
}

fn is_legal_unimodular(u: Mat2, deps: &[[i64; 2]]) -> bool {
    det2(u).abs() == 1 && deps.iter().all(|&d| is_lex_positive(mat2_vec(u, d)))
}

fn can_tile_2d(deps: &[[i64; 2]]) -> bool {
    // Rectangular tiling is legal iff all components of dependence vectors are >= 0.
    deps.iter().all(|&d| d[0] >= 0 && d[1] >= 0)
}

// Execute `for i in 1..=rows { for j in 1..=cols { A[i][j] = update(A[i-1][j+1], i, j) } }`.
fn run_original_wavefront(rows: usize, cols: usize) -> Vec<Vec<i64>> {
    let mut a = vec![vec![0i64; cols + 2]; rows + 2];
    for (r, row) in a.iter_mut().enumerate() {
        for (c, cell) in row.iter_mut().enumerate() {
            *cell = (r * 7 + c * 3) as i64;
        }
    }
    for i in 1..=rows {
        for j in 1..=cols {
            a[i][j] = (a[i - 1][j + 1] * 3 + (i as i64) + 2 * (j as i64)) % 1009;
        }
    }
    a
}

// Skewed coordinates: u = i, v = i + j  <=>  i = u, j = v - u.
// Since dependence `(1, -1)` maps under `S = [[1, 0], [1, 1]]` to `(1, 0)`,
// rectangular tiling `(tu, tv)` over `(u, v)` space preserves every dependence!
fn run_skewed_tiled_wavefront(
    rows: usize,
    cols: usize,
    tile_u: usize,
    tile_v: usize,
) -> Vec<Vec<i64>> {
    let mut a = vec![vec![0i64; cols + 2]; rows + 2];
    for (r, row) in a.iter_mut().enumerate() {
        for (c, cell) in row.iter_mut().enumerate() {
            *cell = (r * 7 + c * 3) as i64;
        }
    }
    let u_min = 1usize;
    let u_max = rows;
    let v_min = 2usize;
    let v_max = rows + cols;

    let mut tu = u_min;
    while tu <= u_max {
        let tu_end = (tu + tile_u - 1).min(u_max);
        let mut tv = v_min;
        while tv <= v_max {
            let tv_end = (tv + tile_v - 1).min(v_max);
            for u in tu..=tu_end {
                let j_lo = 1usize.max(tv.saturating_sub(u));
                let j_hi = cols.min(tv_end.saturating_sub(u));
                for v in (u + j_lo)..=(u + j_hi) {
                    let i = u;
                    let j = v - u;
                    a[i][j] = (a[i - 1][j + 1] * 3 + (i as i64) + 2 * (j as i64)) % 1009;
                }
            }
            tv += tile_v;
        }
        tu += tile_u;
    }
    a
}

fn verify_nullspace(matrix: &[Vec<i64>], basis: &[Vec<i64>]) -> bool {
    for v in basis {
        for row in matrix {
            let dot: i64 = row.iter().zip(v).map(|(&a, &b)| a * b).sum();
            if dot != 0 {
                return false;
            }
        }
    }
    true
}

fn main() {
    // Dependence differences for synchronization-free affine partitioning: c . (i_1 - i_2) = 0.
    // Suppose two coupled dependences in 3D space are (1, -1, 0) and (0, 1, -1).
    let deps_3d = vec![vec![1, -1, 0], vec![0, 1, -1]];
    let basis = nullspace_basis(&deps_3d, 3);
    println!(
        "Appendix B sync-free affine partition basis for {deps_3d:?}: {basis:?} (valid={})",
        verify_nullspace(&deps_3d, &basis)
    );

    let deps = [[1, -1]];
    let interchange: Mat2 = [[0, 1], [1, 0]];
    let skew: Mat2 = [[1, 0], [1, 1]];
    let skewed_interchange = mat2_mul(interchange, skew);
    let skewed_dep = mat2_vec(skew, deps[0]);
    println!(
        "Original dep={:?}: tileable={}, interchange_legal={}",
        deps[0],
        can_tile_2d(&deps),
        is_legal_unimodular(interchange, &deps)
    );
    println!(
        "After skew S=[[1,0],[1,1]]: det={}, dep'={:?}, tileable={}, skew+interchange_legal={}",
        det2(skew),
        skewed_dep,
        can_tile_2d(&[skewed_dep]),
        is_legal_unimodular(skewed_interchange, &deps)
    );

    let seq = run_original_wavefront(8, 8);
    let tiled = run_skewed_tiled_wavefront(8, 8, 3, 4);
    println!("8x8 wavefront skewed+tiled exact match: {}", seq == tiled);
}

#[test]
fn appendix_b_nullspace_finds_sync_free_partitions() {
    let mat = vec![vec![1, -1, 0], vec![0, 1, -1]];
    let basis = nullspace_basis(&mat, 3);
    assert_eq!(basis, vec![vec![1, 1, 1]]);
    assert!(verify_nullspace(&mat, &basis));

    let mat2 = vec![vec![2, 1, -1, 0], vec![4, 2, 0, -2]];
    let basis2 = nullspace_basis(&mat2, 4);
    assert_eq!(basis2.len(), 2);
    assert!(verify_nullspace(&mat2, &basis2));
}

#[test]
fn skewing_enables_tiling_and_interchange_and_preserves_output() {
    let deps = [[1, -1]];
    let interchange: Mat2 = [[0, 1], [1, 0]];
    let skew: Mat2 = [[1, 0], [1, 1]];

    assert!(!can_tile_2d(&deps));
    assert!(!is_legal_unimodular(interchange, &deps));

    assert!(is_legal_unimodular(skew, &deps));
    let skewed_dep = mat2_vec(skew, deps[0]);
    assert_eq!(skewed_dep, [1, 0]);
    assert!(can_tile_2d(&[skewed_dep]));

    let skew_then_interchange = mat2_mul(interchange, skew);
    assert_eq!(det2(skew_then_interchange).abs(), 1);
    assert!(is_legal_unimodular(skew_then_interchange, &deps));

    let seq = run_original_wavefront(10, 12);
    for (tu, tv) in [(2, 2), (3, 5), (4, 4)] {
        let tiled = run_skewed_tiled_wavefront(10, 12, tu, tv);
        assert_eq!(tiled, seq);
    }
}
