//! Solve backward liveness equations on a control-flow graph with a loop.

use std::collections::BTreeSet;

type Vars = BTreeSet<&'static str>;

struct Block {
    // USE records reads before a local definition. DEF records assignments in the block.
    uses: Vars,
    defs: Vars,
    successors: Vec<usize>,
}

fn liveness(blocks: &[Block]) -> (Vec<Vars>, Vec<Vars>) {
    // Start with no known live values and propagate demands until stable.
    let mut live_in = vec![Vars::new(); blocks.len()];
    let mut live_out = live_in.clone();
    loop {
        let mut changed = false;
        for (index, block) in blocks.iter().enumerate().rev() {
            // A value is live out if at least one successor needs it.
            let outgoing: Vars = block
                .successors
                .iter()
                .flat_map(|&successor| live_in[successor].iter().copied())
                .collect();
            // Definitions kill the old value, but an earlier read still needs it on entry.
            let mut incoming = block.uses.clone();
            incoming.extend(outgoing.difference(&block.defs));
            changed |= incoming != live_in[index];
            live_in[index] = incoming;
            live_out[index] = outgoing;
        }
        // Stable live-in sets imply stable live-out sets because OUT is computed from IN.
        if !changed {
            return (live_in, live_out);
        }
    }
}

fn sample() -> [Block; 4] {
    // x = ..., then while again { x = x + step }, then return x.
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
    assert_eq!(entry[0], Vars::from(["again", "step"]));
    assert_eq!(exit[0], Vars::from(["again", "step", "x"]));
    assert_eq!(entry[2], Vars::from(["again", "step", "x"]));
    assert_eq!(exit[3], Vars::new());
}
