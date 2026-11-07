#[derive(Default)]
struct Trace {
    previous: Option<usize>,
    adjacent: usize,
    total: usize,
}

impl Trace {
    fn read(&mut self, index: usize) {
        self.adjacent +=
            usize::from(self.previous.and_then(|last| last.checked_add(1)) == Some(index));
        self.previous = Some(index);
        self.total += 1;
    }
}

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
