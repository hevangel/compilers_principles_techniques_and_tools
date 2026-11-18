//! Model activation records and follow static links to nonlocal data.

/// Activation record (stack frame) for a single procedure invocation.
#[derive(Debug)]
struct Frame {
    /// Local variable storage for this activation (simplified to one slot).
    local: i64,
    /// Dynamic (control) link: frame index of the procedure that called this one (`None` at entry).
    caller: Option<usize>,
    /// Static (access) link: frame index of the lexically enclosing procedure's active frame.
    static_parent: Option<usize>,
}

/// Read a variable located `lexical_hops` enclosing scopes outward from `frame`.
fn read_nonlocal(stack: &[Frame], mut frame: usize, mut lexical_hops: usize) -> Option<i64> {
    // Walk static_parent (NOT caller) because nonlocal visibility follows lexical nesting,
    // not runtime call order. The compiler knows `lexical_hops` statically from symbol tables.
    while lexical_hops > 0 {
        frame = stack.get(frame)?.static_parent?;
        lexical_hops -= 1;
    }
    // At 0 remaining hops we have reached the target enclosing frame; read its local slot.
    Some(stack.get(frame)?.local)
}

/// Construct a call-stack snapshot where dynamic caller and static parent diverge:
///   Lexical nesting: outer { helper, inner }
///   Dynamic calls:   outer (0) -> helper (1) -> inner (2)
fn sample() -> Vec<Frame> {
    vec![
        // Frame 0: `outer` at lexical depth 0 (no caller, no enclosing scope).
        Frame {
            local: 21,
            caller: None,
            static_parent: None,
        },
        // Frame 1: `helper` called by `outer` (0) and lexically nested in `outer` (0).
        Frame {
            local: 90,
            caller: Some(0),
            static_parent: Some(0),
        },
        // Frame 2: `inner` called by `helper` (1), yet lexically nested in `outer` (0).
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
