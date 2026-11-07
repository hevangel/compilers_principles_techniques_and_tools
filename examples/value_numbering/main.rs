use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum Op {
    Add,
    Sub,
}

#[derive(Clone, Copy, Debug)]
enum Expr {
    Compute(Op, usize, usize),
    Copy(usize),
}

fn number(inputs: usize, code: &[Expr]) -> Vec<Expr> {
    let mut values: Vec<_> = (0..inputs).collect();
    let mut known = BTreeMap::new();
    let mut output = Vec::new();
    for &expr in code {
        let (result, representative) = match expr {
            Expr::Compute(op, left, right) => {
                let (mut left, mut right) = (values[left], values[right]);
                if op == Op::Add && left > right {
                    std::mem::swap(&mut left, &mut right);
                }
                let fresh = values.len();
                let previous = *known.entry((op, left, right)).or_insert(fresh);
                let result = if previous == fresh {
                    Expr::Compute(op, left, right)
                } else {
                    Expr::Copy(previous)
                };
                (result, previous)
            }
            Expr::Copy(source) => (Expr::Copy(values[source]), values[source]),
        };
        output.push(result);
        values.push(representative);
    }
    output
}

fn execute(inputs: &[i64], code: &[Expr]) -> Vec<i64> {
    let mut values = inputs.to_vec();
    for &expr in code {
        let result = match expr {
            Expr::Compute(Op::Add, a, b) => values[a].wrapping_add(values[b]),
            Expr::Compute(Op::Sub, a, b) => values[a].wrapping_sub(values[b]),
            Expr::Copy(a) => values[a],
        };
        values.push(result);
    }
    values
}

const CODE: &[Expr] = &[
    Expr::Compute(Op::Add, 0, 1),
    Expr::Compute(Op::Add, 1, 0),
    Expr::Compute(Op::Sub, 3, 0),
    Expr::Compute(Op::Sub, 0, 2),
    Expr::Compute(Op::Sub, 2, 0),
];

fn main() {
    let optimized = number(2, CODE);
    println!("Original: {CODE:?}");
    println!("Optimized: {optimized:?}");
    println!("Values: {:?}", execute(&[9, 4], &optimized));
}

#[test]
fn combines_redundant_values_and_preserves_subtraction_order() {
    let optimized = number(2, CODE);
    assert!(matches!(optimized[1], Expr::Copy(2)));
    assert!(matches!(optimized[3], Expr::Compute(Op::Sub, 0, 2)));
    assert!(matches!(optimized[4], Expr::Copy(4)));
    for inputs in [[9, 4], [4, -9], [i64::MAX, 1]] {
        assert_eq!(execute(&inputs, &optimized), execute(&inputs, CODE));
    }
}
