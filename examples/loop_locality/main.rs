//! Compare matrix-multiply loop orders using a trace of row-major accesses.

/// Lightweight proxy for spatial locality: counts consecutive memory reads that step
/// forward by exactly 1 element (`previous + 1 == index`), modeling cache-line hits on stride-1 traversal.
#[derive(Default)]
struct Trace {
    previous: Option<usize>,
    adjacent: usize,
    total: usize,
}

impl Trace {
    fn read(&mut self, index: usize) {
        // Count a pair only when the next read advances by one element (stride-1 access).
        self.adjacent +=
            usize::from(self.previous.and_then(|last| last.checked_add(1)) == Some(index));
        self.previous = Some(index);
        self.total += 1;
    }
}

/// Original `i, j, k` order: computes one `C[i, j]` dot product at a time.
/// Inner loop increments `k`, so `B[k * n + j]` jumps by stride `n` down a column (poor spatial locality).
fn original(a: &[i64], b: &[i64], n: usize) -> (Vec<i64>, Trace) {
    let mut c = vec![0_i64; n * n];
    let mut trace = Trace::default();
    for i in 0..n {
        for j in 0..n {
            for k in 0..n {
                let rhs = k * n + j;
                trace.read(rhs);
                c[i * n + j] = c[i * n + j].wrapping_add(a[i * n + k].wrapping_mul(b[rhs]));
            }
        }
    }
    (c, trace)
}

/// Interchanged `i, k, j` order: legal because `C` does not alias `A` or `B` and each `C[i, j]`
/// still accumulates its `k` terms in ascending `k` order (no carried dependence violated).
/// Inner loop increments `j`, so both `B[k * n + j]` and `C[i * n + j]` move with stride 1
/// across contiguous rows while `A[i * n + k]` stays loop-invariant in the `j` loop.
fn interchanged(a: &[i64], b: &[i64], n: usize) -> (Vec<i64>, Trace) {
    let mut c = vec![0_i64; n * n];
    let mut trace = Trace::default();
    for i in 0..n {
        for k in 0..n {
            for j in 0..n {
                let rhs = k * n + j;
                trace.read(rhs);
                c[i * n + j] = c[i * n + j].wrapping_add(a[i * n + k].wrapping_mul(b[rhs]));
            }
        }
    }
    (c, trace)
}

fn main() {
    let a: Vec<i64> = (0..16).collect();
    let b: Vec<i64> = (0..16).rev().collect();
    let (reference, before) = original(&a, &b, 4);
    let (result, after) = interchanged(&a, &b, 4);
    // Verify the numeric result before comparing memory access patterns.
    assert_eq!(reference, result);
    println!("Product: {result:?}");
    println!(
        "Adjacent B reads: {} -> {} across {} reads",
        before.adjacent, after.adjacent, after.total
    );
}

#[test]
fn keeps_the_product_and_improves_contiguous_access() {
    let a = [1, 2, 3, 4];
    let b = [5, 6, 7, 8];
    let (expected, before) = original(&a, &b, 2);
    let (actual, after) = interchanged(&a, &b, 2);
    assert_eq!(expected, [19, 22, 43, 50]);
    assert_eq!(actual, expected);
    assert_eq!(before.total, after.total);
    assert!(after.adjacent > before.adjacent);
    assert!(interchanged(&[], &[], 0).0.is_empty());
}
