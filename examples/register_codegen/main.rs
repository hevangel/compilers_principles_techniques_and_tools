//! Label expression trees with register needs and emit code for an abstract machine.

#[derive(Clone, Copy, Debug)]
enum Op {
    Add,
    Sub,
}

enum Expr {
    Input(usize),
    Binary(Op, Box<Expr>, Box<Expr>),
}

// Annotate the tree once with the minimum number of registers per subtree.
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
    fn for_expr(expr: &Expr) -> Self {
        match expr {
            Expr::Input(slot) => Self::Load(*slot),
            Expr::Binary(op, left, right) => {
                let left = Box::new(Self::for_expr(left));
                let right = Box::new(Self::for_expr(right));
                let (a, b) = (left.need(), right.need());
                // Equal child needs require one extra register to save the first result.
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

fn generate(plan: &Plan, base: usize, code: &mut Vec<Inst>) {
    match plan {
        Plan::Load(slot) => code.push(Inst::Load {
            dst: base,
            slot: *slot,
        }),
        Plan::Binary {
            op, left, right, ..
        } => {
            // Evaluate the larger subtree first. Each subtree leaves its result in its base register.
            let (left_reg, right_reg) = if left.need() >= right.need() {
                generate(left, base, code);
                generate(right, base + 1, code);
                (base, base + 1)
            } else {
                generate(right, base, code);
                generate(left, base + 1, code);
                // Evaluation order changed, but left and right must still follow the source operation.
                (base + 1, base)
            };
            // Both inputs are read before the destination register is overwritten.
            code.push(Inst::Binary {
                dst: base,
                op: *op,
                left: left_reg,
                right: right_reg,
            });
        }
    }
}

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
