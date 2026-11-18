//! Solve backward liveness equations on a control-flow graph with a loop.

use std::collections::BTreeSet;

/// Set of variable names live at a program point (the data-flow lattice ordered by subset inclusion).
type Vars = BTreeSet<&'static str>;

/// A CFG basic block summarized by its local use/def behavior and outgoing control-flow edges.
struct Block {
    /// Upward-exposed uses: variables read in this block *before* any local assignment to them.
    uses: Vars,
    /// Definitions: variables assigned anywhere in this block (killing values arriving from entry).
    defs: Vars,
    successors: Vec<usize>,
}

/// Compute `(live_in, live_out)` for every basic block via backward may-analysis.
///
/// Liveness asks whether a value *may* be read along at least one future path before
/// redefinition. Missing a live variable would be unsound (e.g., dead-code elimination
/// could delete a needed write), so the confluence operator is set union and sets start empty.
fn liveness(blocks: &[Block]) -> (Vec<Vars>, Vec<Vars>) {
    // Initialize at the lattice bottom (empty sets) and grow monotonically until fixed point.
    let mut live_in = vec![Vars::new(); blocks.len()];
    let mut live_out = live_in.clone();
    loop {
        let mut changed = false;
        // Traverse in reverse block order so backward information flows faster across sequential blocks.
        for (index, block) in blocks.iter().enumerate().rev() {
            // Confluence (may / union): a variable is live at exit if *any* successor needs it on entry.
            let outgoing: Vars = block
                .successors
                .iter()
                .flat_map(|&successor| live_in[successor].iter().copied())
                .collect();
            // Transfer function `IN[B] = USE[B] U (OUT[B] \ DEF[B])`:
            // local assignments kill outgoing needs, while upward-exposed reads remain live at entry.
            let mut incoming = block.uses.clone();
            incoming.extend(outgoing.difference(&block.defs));
            changed |= incoming != live_in[index];
            live_in[index] = incoming;
            live_out[index] = outgoing;
        }
        // Stable live-in sets imply stable live-out sets because OUT is computed solely from IN.
        if !changed {
            return (live_in, live_out);
        }
    }
}

fn sample() -> [Block; 4] {
    // Models: B0: x = ...; B1: while (again) { B2: x = x + step } B3: return x.
    [
        Block {
            uses: Vars::new(),
            defs: Vars::from(["x"]),
            successors: vec![1],
        },
        Block {
            uses: Vars::from(["again"]),
            defs: Vars::new(),
            successors: vec![2, 3],
        },
        Block {
            // `x` is in both `uses` (read on RHS) and `defs` (written on LHS) of `x = x + step`.
            uses: Vars::from(["x", "step"]),
            defs: Vars::from(["x"]),
            successors: vec![1],
        },
        Block {
            uses: Vars::from(["x"]),
            defs: Vars::new(),
            successors: vec![],
        },
    ]
}

fn main() {
    let (live_in, live_out) = liveness(&sample());
    for (index, (entry, exit)) in live_in.iter().zip(live_out).enumerate() {
        println!("B{index}: in {entry:?}, out {exit:?}");
    }
}

#[test]
fn propagates_around_loop_and_kills_entry_definition() {
    let (entry, exit) = liveness(&sample());
    // B0 defines `x` before any read, killing `x` from entry[0] while keeping loop-carried `again`, `step`.
    assert_eq!(entry[0], Vars::from(["again", "step"]));
    assert_eq!(exit[0], Vars::from(["again", "step", "x"]));
    // `again` reaches B2 via back-edge B2 -> B1 on the second pass of the fixed-point loop.
    assert_eq!(entry[2], Vars::from(["again", "step", "x"]));
    // Exit block B3 has no successors, so nothing is live on exit.
    assert_eq!(exit[3], Vars::new());
}
