//! Label expression trees with register needs and emit code for an abstract machine.

#[derive(Clone, Copy, Debug)]
enum Op {
    Add,
    Sub,
}

/// Source expression AST whose leaves index into an `inputs` slice.
enum Expr {
    Input(usize),
    Binary(Op, Box<Expr>, Box<Expr>),
}

/// Expression tree annotated bottom-up with Sethi–Ullman (Ershov) register numbers:
/// `need` is the minimum number of registers required to evaluate the subtree without spilling.
enum Plan {
    Load(usize),
    Binary {
        op: Op,
        left: Box<Plan>,
        right: Box<Plan>,
        need: usize,
    },
}

impl Plan {
    /// Bottom-up Ershov labeling:
    /// - Leaf needs 1 register.
    /// - Unequal children (a != b): evaluate the larger first, leaving max(a, b) total.
    /// - Equal children (a == b): holding the first child's result takes 1 extra register -> a + 1.
    fn for_expr(expr: &Expr) -> Self {
        match expr {
            Expr::Input(slot) => Self::Load(*slot),
            Expr::Binary(op, left, right) => {
                let left = Box::new(Self::for_expr(left));
                let right = Box::new(Self::for_expr(right));
                let (a, b) = (left.need(), right.need());
                // Equal child needs require one extra register to hold the first result.
                let need = a.max(b) + usize::from(a == b);
                Self::Binary {
                    op: *op,
                    left,
                    right,
                    need,
                }
            }
        }
    }

    fn need(&self) -> usize {
        match self {
            Self::Load(_) => 1,
            Self::Binary { need, .. } => *need,
        }
    }
}

/// Abstract target instructions operating on a bank of registers `0..registers`.
#[derive(Debug)]
enum Inst {
    Load {
        dst: usize,
        slot: usize,
    },
    Binary {
        dst: usize,
        op: Op,
        left: usize,
        right: usize,
    },
}

/// Emit code for `plan` using registers `base .. base + plan.need()`.
/// Invariant: upon completion, the subtree's value sits in `base` and registers `< base` are untouched.
fn generate(plan: &Plan, base: usize, code: &mut Vec<Inst>) {
    match plan {
        Plan::Load(slot) => code.push(Inst::Load {
            dst: base,
            slot: *slot,
        }),
        Plan::Binary {
            op, left, right, ..
        } => {
            // Evaluate the more demanding subtree first in `base` (using all available registers),
            // leaving its result in `base` and freeing `base + 1..` for the smaller subtree.
            let (left_reg, right_reg) = if left.need() >= right.need() {
                generate(left, base, code);
                generate(right, base + 1, code);
                (base, base + 1)
            } else {
                generate(right, base, code);
                generate(left, base + 1, code);
                // Evaluation order flipped, so for non-commutative `Sub`, the left operand is now
                // in `base + 1` and the right operand is in `base`.
                (base + 1, base)
            };
            // Target instruction reads both inputs before overwriting `dst = base`.
            code.push(Inst::Binary {
                dst: base,
                op: *op,
                left: left_reg,
                right: right_reg,
            });
        }
    }
}

/// Execute emitted instructions on an abstract machine with `registers` slots; result ends in `regs[0]`.
fn run(code: &[Inst], registers: usize, inputs: &[i64]) -> i64 {
    let mut regs = vec![0_i64; registers];
    for instruction in code {
        match *instruction {
            Inst::Load { dst, slot } => regs[dst] = inputs[slot],
            Inst::Binary {
                dst,
                op,
                left,
                right,
            } => {
                regs[dst] = match op {
                    Op::Add => regs[left].wrapping_add(regs[right]),
                    Op::Sub => regs[left].wrapping_sub(regs[right]),
                };
            }
        }
    }
    regs[0]
}

fn sample() -> Plan {
    use Expr::{Binary, Input};
    Plan::for_expr(&Binary(
        Op::Sub,
        Box::new(Input(0)),
        Box::new(Binary(Op::Add, Box::new(Input(1)), Box::new(Input(2)))),
    ))
}

fn main() {
    let plan = sample();
    let mut code = Vec::new();
    generate(&plan, 0, &mut code);
    println!("{} registers", plan.need());
    for inst in &code {
        println!("{inst:?}");
    }
    println!("Result: {}", run(&code, plan.need(), &[30, 4, 5]));
}

#[test]
fn evaluates_larger_subtree_first_but_preserves_operand_order() {
    let plan = sample();
    assert_eq!(plan.need(), 2);
    let mut code = Vec::new();
    generate(&plan, 0, &mut code);
    assert!(matches!(code[0], Inst::Load { slot: 1, .. }));
    assert_eq!(run(&code, plan.need(), &[30, 4, 5]), 21);
}
