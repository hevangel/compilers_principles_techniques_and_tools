//! Propagate assignment identities forward through branches and loops.

use std::collections::BTreeSet;

// Each ID identifies one assignment site. Several IDs can define the same variable.
type Definitions = BTreeSet<usize>;

struct Block {
    predecessors: Vec<usize>,
    assignments: Vec<usize>,
}

fn reaching(blocks: &[Block], variable_of: &[&str]) -> (Vec<Definitions>, Vec<Definitions>) {
    let mut entry = vec![Definitions::new(); blocks.len()];
    let mut exit = entry.clone();
    loop {
        let mut changed = false;
        for (index, block) in blocks.iter().enumerate() {
            // A definition may reach a block along any one of its predecessor paths.
            entry[index] = block
                .predecessors
                .iter()
                .flat_map(|&parent| exit[parent].iter().copied())
                .collect();
            let mut outgoing = entry[index].clone();
            // Process assignments in order. A new definition kills earlier ones for the same variable.
            for &definition in &block.assignments {
                outgoing.retain(|&old| variable_of[old] != variable_of[definition]);
                outgoing.insert(definition);
            }
            changed |= outgoing != exit[index];
            exit[index] = outgoing;
        }
        // Stable outgoing facts give stable incoming unions on the next pass.
        if !changed {
            return (entry, exit);
        }
    }
}

fn sample() -> [Block; 4] {
    // Entry sets x and y. Each arm overwrites x before the join.
    [
        Block {
            predecessors: vec![],
            assignments: vec![0, 1],
        },
        Block {
            predecessors: vec![0],
            assignments: vec![2],
        },
        Block {
            predecessors: vec![0],
            assignments: vec![3],
        },
        Block {
            predecessors: vec![1, 2],
            assignments: vec![],
        },
    ]
}

fn main() {
    let (entry, exit) = reaching(&sample(), &["x", "y", "x", "x"]);
    for (index, (entry, exit)) in entry.iter().zip(exit).enumerate() {
        println!("B{index}: reaching in {entry:?}, reaching out {exit:?}");
    }
}

#[test]
fn joins_branch_definitions_and_kills_overwritten_assignments() {
    let (entry, exit) = reaching(&sample(), &["x", "y", "x", "x"]);
    assert_eq!(exit[0], Definitions::from([0, 1]));
    assert_eq!(exit[1], Definitions::from([1, 2]));
    assert_eq!(entry[3], Definitions::from([1, 2, 3]));
    let loop_blocks = [
        Block {
            predecessors: vec![],
            assignments: vec![0],
        },
        Block {
            predecessors: vec![0, 1],
            assignments: vec![1],
        },
    ];
    assert_eq!(
        reaching(&loop_blocks, &["x", "x"]).0[1],
        Definitions::from([0, 1])
    );
}
