enum Condition {
    Input(usize),
    And(Box<Condition>, Box<Condition>),
    Or(Box<Condition>, Box<Condition>),
}

#[derive(Debug)]
enum Instruction {
    IfTrue(usize, Option<usize>),
    Jump(Option<usize>),
    Return(bool),
}

struct Exits {
    on_true: Vec<usize>,
    on_false: Vec<usize>,
}

fn patch(code: &mut [Instruction], sites: &[usize], target: usize) {
    for &site in sites {
        match &mut code[site] {
            Instruction::IfTrue(_, destination) | Instruction::Jump(destination) => {
                assert!(destination.replace(target).is_none());
            }
            Instruction::Return(_) => unreachable!(),
        }
    }
}

fn emit(condition: &Condition, code: &mut Vec<Instruction>) -> Exits {
    match condition {
        Condition::Input(index) => {
            let site = code.len();
            code.extend([Instruction::IfTrue(*index, None), Instruction::Jump(None)]);
            Exits {
                on_true: vec![site],
                on_false: vec![site + 1],
            }
        }
        Condition::And(left, right) => {
            let mut left = emit(left, code);
            let next = code.len();
            patch(code, &left.on_true, next);
            let right = emit(right, code);
            left.on_false.extend(right.on_false);
            Exits {
                on_true: right.on_true,
                on_false: left.on_false,
            }
        }
        Condition::Or(left, right) => {
            let mut left = emit(left, code);
            let next = code.len();
            patch(code, &left.on_false, next);
            let right = emit(right, code);
            left.on_true.extend(right.on_true);
            Exits {
                on_true: left.on_true,
                on_false: right.on_false,
            }
        }
    }
}

fn compile(condition: &Condition) -> Vec<Instruction> {
    let mut code = Vec::new();
    let exits = emit(condition, &mut code);
    let end = code.len();
    code.extend([Instruction::Return(true), Instruction::Return(false)]);
    patch(&mut code, &exits.on_true, end);
    patch(&mut code, &exits.on_false, end + 1);
    code
}

fn run(code: &[Instruction], inputs: &[bool]) -> (bool, Vec<usize>) {
    let mut pc = 0;
    let mut reads = Vec::new();
    loop {
        pc = match code[pc] {
            Instruction::IfTrue(index, target) => {
                reads.push(index);
                if inputs[index] {
                    target.unwrap()
                } else {
                    pc + 1
                }
            }
            Instruction::Jump(target) => target.unwrap(),
            Instruction::Return(result) => return (result, reads),
        };
    }
}

fn sample() -> Condition {
    use Condition::{And, Input, Or};
    Or(
        Box::new(And(Box::new(Input(0)), Box::new(Input(1)))),
        Box::new(Input(2)),
    )
}

fn main() {
    let code = compile(&sample());
    for (pc, instruction) in code.iter().enumerate() {
        println!("{pc}: {instruction:?}");
    }
    println!(
        "Result and inputs read: {:?}",
        run(&code, &[false, false, true])
    );
}

#[test]
fn matches_truth_table_and_skips_unused_inputs() {
    let code = compile(&sample());
    for a in [false, true] {
        for b in [false, true] {
            for c in [false, true] {
                assert_eq!(run(&code, &[a, b, c]).0, (a && b) || c);
            }
        }
    }
    assert_eq!(run(&code, &[false, false, true]), (true, vec![0, 2]));
    assert_eq!(run(&code, &[true, true, false]), (true, vec![0, 1]));
}
