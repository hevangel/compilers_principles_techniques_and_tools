//! Identify back edges using dominators and walk each natural loop backward.

use std::collections::BTreeSet;

/// Set of block indices (used for dominator sets and natural-loop body sets).
type Nodes = BTreeSet<usize>;

/// Invert outgoing adjacency lists into incoming predecessor lists.
fn predecessors(edges: &[&[usize]]) -> Vec<Vec<usize>> {
    let mut result = vec![Vec::new(); edges.len()];
    for (source, targets) in edges.iter().enumerate() {
        for &target in *targets {
            result[target].push(source);
        }
    }
    result
}

/// Compute `dom[node]` (all blocks that must be visited on every path from entry `0` to `node`).
///
/// Dominance is a forward must-analysis: non-entry nodes start at the universal set `{0..N-1}`
/// (top of the intersection lattice) so back edges converge to the greatest fixed point.
/// Note: assumes every node is reachable from entry node 0.
fn dominators(incoming: &[Vec<usize>]) -> Vec<Nodes> {
    let mut dom = vec![(0..incoming.len()).collect::<Nodes>(); incoming.len()];
    // Entry block 0 is only dominated by itself.
    dom[0] = Nodes::from([0]);
    loop {
        let previous = dom.clone();
        for (node, parents) in incoming.iter().enumerate().skip(1) {
            // Confluence (must / intersection): a strict dominator of `node` must dominate all preds.
            let mut common = dom[parents[0]].clone();
            for &parent in &parents[1..] {
                common.retain(|candidate| dom[parent].contains(candidate));
            }
            // Every reachable node trivially dominates itself.
            common.insert(node);
            dom[node] = common;
        }
        if dom == previous {
            return dom;
        }
    }
}

/// Collect all blocks belonging to the natural loop of back edge `tail -> header`.
///
/// By definition, the natural loop consists of `header` plus all nodes that can reach `tail`
/// without passing through `header`.
fn natural_loop(tail: usize, header: usize, incoming: &[Vec<usize>]) -> Nodes {
    // Seed `body` with `header` first so the reverse DFS never walks past `header` into preheaders.
    let mut body = Nodes::from([header]);
    let mut pending = vec![tail];
    while let Some(node) = pending.pop() {
        if body.insert(node) {
            pending.extend(&incoming[node]);
        }
    }
    body
}

/// Detect all back edges `tail -> header` (`header in dom[tail]`) and return `(tail, header, body)`.
fn find_loops(edges: &[&[usize]]) -> Vec<(usize, usize, Nodes)> {
    let incoming = predecessors(edges);
    let dom = dominators(&incoming);
    let mut result = Vec::new();
    for (tail, targets) in edges.iter().enumerate() {
        for &header in *targets {
            // Dominance check: only an edge targeting one of `tail`'s dominators is a back edge.
            if dom[tail].contains(&header) {
                result.push((tail, header, natural_loop(tail, header, &incoming)));
            }
        }
    }
    result
}

// B0 -> B1 -> {B2, B4}; B2 -> B3 -> B1 (back edge).
const GRAPH: &[&[usize]] = &[&[1], &[2, 4], &[3], &[1], &[]];

fn main() {
    for (tail, header, body) in find_loops(GRAPH) {
        println!("Back edge B{tail} -> B{header}, loop blocks {body:?}");
    }
}

#[test]
fn excludes_preheader_and_handles_self_loop() {
    // B0 (preheader) and B4 (exit) stay outside `{1, 2, 3}`.
    assert_eq!(find_loops(GRAPH), [(3, 1, Nodes::from([1, 2, 3]))]);
    // Self-loop `1 -> 1` has `tail == header == 1`, yielding `{1}`.
    assert_eq!(find_loops(&[&[1], &[1]]), [(1, 1, Nodes::from([1]))]);
    // Diamond DAG has no back edges.
    assert!(find_loops(&[&[1, 2], &[2], &[]]).is_empty());
}
