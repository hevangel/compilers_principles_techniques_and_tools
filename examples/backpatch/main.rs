//! Generate short-circuit Boolean branches by filling forward targets later.

/// Structured Boolean expression tree whose operators short-circuit left-to-right.
enum Condition {
    /// Leaf predicate reading `inputs[index]`.
    Input(usize),
    /// Short-circuit conjunction: evaluates `right` only if `left` is true.
    And(Box<Condition>, Box<Condition>),
    /// Short-circuit disjunction: evaluates `right` only if `left` is false.
    Or(Box<Condition>, Box<Condition>),
}

/// Low-level control-flow instruction; `None` represents a forward jump target
/// that has not yet been backpatched.
#[derive(Debug)]
enum Instruction {
    /// Reads `inputs[index]`; jumps to `target` if true, otherwise falls through to `pc + 1`.
    IfTrue(usize, Option<usize>),
    /// Unconditional jump to `target` (used for the false branch of `Input` leaves).
    Jump(Option<usize>),
    /// Terminates evaluation with the Boolean result.
    Return(bool),
}

// Each list contains instruction indices whose targets are still unknown (`None`).
// Invariant: no instruction index may appear in both `on_true` and `on_false`,
// and every index must still hold `None` until backpatched exactly once.
struct Exits {
    on_true: Vec<usize>,
    on_false: Vec<usize>,
}

/// Fills every unresolved jump in `sites` with `target`.
fn patch(code: &mut [Instruction], sites: &[usize], target: usize) {
    for &site in sites {
        match &mut code[site] {
            Instruction::IfTrue(_, destination) | Instruction::Jump(destination) => {
                // Every recorded jump site must still be `None` so it is filled at most once.
                assert!(destination.replace(target).is_none());
            }
            Instruction::Return(_) => unreachable!(),
        }
    }
}

/// Emits branch code for `condition` in a single post-order walk and returns its
/// still-unresolved true/false exit instruction indices.
fn emit(condition: &Condition, code: &mut Vec<Instruction>) -> Exits {
    match condition {
        Condition::Input(index) => {
            // Reserve one exit for a true test (`IfTrue`) and a separate fall-through
            // unconditional jump (`Jump`) for a false test.
            let site = code.len();
            code.extend([Instruction::IfTrue(*index, None), Instruction::Jump(None)]);
            Exits {
                on_true: vec![site],
                on_false: vec![site + 1],
            }
        }
        Condition::And(left, right) => {
            let mut left = emit(left, code);
            // `next` is the exact entry instruction index of `right`'s fragment.
            let next = code.len();
            // AND evaluates its right side only when the left side is true.
            patch(code, &left.on_true, next);
            let right = emit(right, code);
            // Both false paths short-circuit/exit the whole AND with false.
            left.on_false.extend(right.on_false);
            Exits {
                on_true: right.on_true,
                on_false: left.on_false,
            }
        }
        Condition::Or(left, right) => {
            let mut left = emit(left, code);
            // `next` is the exact entry instruction index of `right`'s fragment.
            let next = code.len();
            // OR evaluates its right side only when the left side is false.
            patch(code, &left.on_false, next);
            let right = emit(right, code);
            // Both true paths short-circuit/exit the whole OR with true.
            left.on_true.extend(right.on_true);
            Exits {
                on_true: left.on_true,
                on_false: right.on_false,
            }
        }
    }
}

/// Compiles `condition` into a self-contained instruction list ending in `Return(true)` / `Return(false)`.
fn compile(condition: &Condition) -> Vec<Instruction> {
    let mut code = Vec::new();
    let exits = emit(condition, &mut code);
    let end = code.len();
    code.extend([Instruction::Return(true), Instruction::Return(false)]);
    // Resolve the outermost exits once both return addresses (`end` and `end + 1`) are available.
    patch(&mut code, &exits.on_true, end);
    patch(&mut code, &exits.on_false, end + 1);
    code
}

/// Executes compiled branch instructions on `inputs` and returns `(result, read_indices)`.
fn run(code: &[Instruction], inputs: &[bool]) -> (bool, Vec<usize>) {
    let mut pc = 0;
    let mut reads = Vec::new();
    loop {
        pc = match code[pc] {
            Instruction::IfTrue(index, target) => {
                // Record reads to make short-circuit behavior observable in the tests.
                reads.push(index);
                if inputs[index] {
                    target.unwrap()
                } else {
                    // False falls through to the immediately following `Jump` at `pc + 1`.
                    pc + 1
                }
            }
            Instruction::Jump(target) => target.unwrap(),
            Instruction::Return(result) => return (result, reads),
        };
    }
}

/// Builds `(Input(0) && Input(1)) || Input(2)` for demonstration and testing.
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
