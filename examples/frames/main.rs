//! Model activation records and follow static links to nonlocal data.

#[derive(Debug)]
struct Frame {
    local: i64,
    // The caller reflects execution order. The static parent reflects lexical nesting.
    caller: Option<usize>,
    static_parent: Option<usize>,
}

fn read_nonlocal(stack: &[Frame], mut frame: usize, mut lexical_hops: usize) -> Option<i64> {
    // Each static link moves outward one lexical level.
    while lexical_hops > 0 {
        frame = stack.get(frame)?.static_parent?;
        lexical_hops -= 1;
    }
    // A real frame could have many slots. This model gives each frame one local.
    Some(stack.get(frame)?.local)
}

fn sample() -> Vec<Frame> {
    // Outer calls helper, then helper calls a sibling named inner.
    vec![
        Frame {
            local: 21,
            caller: None,
            static_parent: None,
        },
        Frame {
            local: 90,
            caller: Some(0),
            static_parent: Some(0),
        },
        Frame {
            local: 7,
            caller: Some(1),
            static_parent: Some(0),
        },
    ]
}

fn main() {
    let stack = sample();
    println!("Return to frame {:?}", stack[2].caller);
    println!("Read outer local {:?}", read_nonlocal(&stack, 2, 1));
}

#[test]
fn uses_lexical_parent_instead_of_immediate_caller() {
    let stack = sample();
    assert_eq!(stack[2].caller, Some(1));
    assert_eq!(read_nonlocal(&stack, 2, 0), Some(7));
    assert_eq!(read_nonlocal(&stack, 2, 1), Some(21));
    assert_eq!(read_nonlocal(&stack, 2, 2), None);
}
