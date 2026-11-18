//! Propagate assignment identities forward through branches and loops.

use std::collections::BTreeSet;

/// Set of definition site IDs (lattice ordered by subset inclusion under union meet).
/// Several distinct IDs can define the same variable name.
type Definitions = BTreeSet<usize>;

/// CFG basic block storing incoming edges (`predecessors`) and ordered assignment IDs.
struct Block {
    predecessors: Vec<usize>,
    assignments: Vec<usize>,
}

/// Compute `(entry, exit)` reaching-definition sets via forward may-analysis.
///
/// A definition `d` reaches a point if *at least one* execution path carries `d` there
/// without an intervening redefinition of `variable_of[d]`. Because missing a possible
/// reaching definition would be unsound, confluence is set union and sets start empty.
fn reaching(blocks: &[Block], variable_of: &[&str]) -> (Vec<Definitions>, Vec<Definitions>) {
    // Initialize at the lattice bottom (empty sets) and grow monotonically until fixed point.
    let mut entry = vec![Definitions::new(); blocks.len()];
    let mut exit = entry.clone();
    loop {
        let mut changed = false;
        for (index, block) in blocks.iter().enumerate() {
            // Confluence (may / union): any definition escaping *any* predecessor reaches `IN[B]`.
            entry[index] = block
                .predecessors
                .iter()
                .flat_map(|&parent| exit[parent].iter().copied())
                .collect();
            let mut outgoing = entry[index].clone();
            // Sequential transfer: walk assignments in program order so each write kills all
            // earlier definitions of the same variable and generates its own definition ID.
            for &definition in &block.assignments {
                outgoing.retain(|&old| variable_of[old] != variable_of[definition]);
                outgoing.insert(definition);
            }
            changed |= outgoing != exit[index];
            exit[index] = outgoing;
        }
        // Stable outgoing facts guarantee stable incoming unions on the next pass.
        if !changed {
            return (entry, exit);
        }
    }
}

fn sample() -> [Block; 4] {
    // Diamond CFG: B0 defines x (0) and y (1); B1 redefines x (2); B2 redefines x (3); B3 joins.
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
    // B1's assignment 2 kills definition 0 (`x`), while preserving 1 (`y`).
    assert_eq!(exit[1], Definitions::from([1, 2]));
    // At join B3, both branch definitions of `x` (2 and 3) plus `y` (1) arrive.
    assert_eq!(entry[3], Definitions::from([1, 2, 3]));
    // Two-block loop: B1's own definition 1 flows back along edge B1 -> B1 on pass 2.
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
