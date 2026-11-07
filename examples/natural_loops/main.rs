use std::collections::BTreeSet;

type Nodes = BTreeSet<usize>;

fn predecessors(edges: &[&[usize]]) -> Vec<Vec<usize>> {
    let mut result = vec![Vec::new(); edges.len()];
    for (source, targets) in edges.iter().enumerate() {
        for &target in *targets {
            result[target].push(source);
        }
    }
    result
}

// This example assumes every node is reachable from entry node 0.
fn dominators(incoming: &[Vec<usize>]) -> Vec<Nodes> {
    let mut dom = vec![(0..incoming.len()).collect::<Nodes>(); incoming.len()];
    dom[0] = Nodes::from([0]);
    loop {
        let previous = dom.clone();
        for (node, parents) in incoming.iter().enumerate().skip(1) {
            let mut common = dom[parents[0]].clone();
            for &parent in &parents[1..] {
                common.retain(|candidate| dom[parent].contains(candidate));
            }
            common.insert(node);
            dom[node] = common;
        }
        if dom == previous {
            return dom;
        }
    }
}

fn natural_loop(tail: usize, header: usize, incoming: &[Vec<usize>]) -> Nodes {
    let mut body = Nodes::from([header]);
    let mut pending = vec![tail];
    while let Some(node) = pending.pop() {
        if body.insert(node) {
            pending.extend(&incoming[node]);
        }
    }
    body
}

fn find_loops(edges: &[&[usize]]) -> Vec<(usize, usize, Nodes)> {
    let incoming = predecessors(edges);
    let dom = dominators(&incoming);
    let mut result = Vec::new();
    for (tail, targets) in edges.iter().enumerate() {
        for &header in *targets {
            if dom[tail].contains(&header) {
                result.push((tail, header, natural_loop(tail, header, &incoming)));
            }
        }
    }
    result
}

const GRAPH: &[&[usize]] = &[&[1], &[2, 4], &[3], &[1], &[]];

fn main() {
    for (tail, header, body) in find_loops(GRAPH) {
        println!("Back edge B{tail} -> B{header}, loop blocks {body:?}");
    }
}

#[test]
fn excludes_preheader_and_handles_self_loop() {
    assert_eq!(find_loops(GRAPH), [(3, 1, Nodes::from([1, 2, 3]))]);
    assert_eq!(find_loops(&[&[1], &[1]]), [(1, 1, Nodes::from([1]))]);
    assert!(find_loops(&[&[1, 2], &[2], &[]]).is_empty());
}
