//! Replace repeated pure calculations within one basic block with copies.

use std::collections::BTreeMap;

/// Pure binary arithmetic operators supported inside the basic block.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum Op {
    Add,
    Sub,
}

/// Three-address instruction whose destination index is implicit (`inputs + instruction_index`).
#[derive(Clone, Copy, Debug)]
enum Expr {
    Compute(Op, usize, usize),
    Copy(usize),
}

/// Perform Local Value Numbering (LVN) over a single basic block.
///
/// Conceptually, LVN partitions temporaries into equivalence classes that are guaranteed
/// to hold identical runtime values, naming each class by the earliest index that produced it.
fn number(inputs: usize, code: &[Expr]) -> Vec<Expr> {
    // `values[i]` holds the canonical representative (value number) for temporary `i`.
    // Initially each input argument `0..inputs` is its own distinct value.
    let mut values: Vec<_> = (0..inputs).collect();
    // `known` maps a canonical expression `(op, left_rep, right_rep)` to the first
    // destination index that computed that expression.
    let mut known = BTreeMap::new();
    let mut output = Vec::new();
    for &expr in code {
        let (result, representative) = match expr {
            Expr::Compute(op, left, right) => {
                // Always resolve operands to their representatives *before* key lookup so
                // aliases created by earlier copies (like `3 -> 2`) expose downstream matches.
                let (mut left, mut right) = (values[left], values[right]);
                // Soundness: wrapping two's-complement addition is commutative even at overflow,
                // so sorting operands makes `a + b` and `b + a` share one key. Subtraction is
                // non-commutative and must keep `(left, right)` order intact.
                if op == Op::Add && left > right {
                    std::mem::swap(&mut left, &mut right);
                }
                let fresh = values.len();
                // Insert `fresh` only if this expression key has never been seen in the block;
                // otherwise reuse the earlier temporary (`previous`) and emit a cheap `Copy`.
                let previous = *known.entry((op, left, right)).or_insert(fresh);
                let result = if previous == fresh {
                    Expr::Compute(op, left, right)
                } else {
                    Expr::Copy(previous)
                };
                (result, previous)
            }
            // A copy inherits the source's canonical representative directly (copy folding).
            Expr::Copy(source) => (Expr::Copy(values[source]), values[source]),
        };
        output.push(result);
        // Keep the original destination numbering (`values.len()`) while recording which
        // canonical value that temporary now holds.
        values.push(representative);
    }
    output
}

/// Reference interpreter using 64-bit wrapping arithmetic to verify semantic equivalence.
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

// Straight-line sample:
// t2 = t0 + t1; t3 = t1 + t0 (redundant); t4 = t3 - t0; t5 = t0 - t2 (distinct); t6 = t2 - t0 (redundant with t4).
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
    // `1 + 0` collapses to `Copy(2)` via commutative canonicalization.
    assert!(matches!(optimized[1], Expr::Copy(2)));
    // `0 - 2` must stay distinct from `2 - 0` (`4`).
    assert!(matches!(optimized[3], Expr::Compute(Op::Sub, 0, 2)));
    // `2 - 0` matches `3 - 0` because `values[3] == values[2] == 2`.
    assert!(matches!(optimized[4], Expr::Copy(4)));
    // Verify identical outputs for every temporary, including signed overflow `[i64::MAX, 1]`.
    for inputs in [[9, 4], [4, -9], [i64::MAX, 1]] {
        assert_eq!(execute(&inputs, &optimized), execute(&inputs, CODE));
    }
}
