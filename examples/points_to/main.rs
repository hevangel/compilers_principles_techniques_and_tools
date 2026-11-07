//! Solve inclusion constraints for a context-insensitive points-to analysis.

use std::collections::BTreeSet;

type Sites = BTreeSet<&'static str>;

fn solve(mut points_to: Vec<Sites>, inclusions: &[(usize, usize)]) -> Vec<Sites> {
    // Each pass adds facts to finite sets until no constraint adds anything new.
    loop {
        let mut changed = false;
        // An assignment or call edge copies possible targets from source to destination.
        for &(source, destination) in inclusions {
            let sites = points_to[source].clone();
            let previous = points_to[destination].len();
            points_to[destination].extend(sites);
            changed |= points_to[destination].len() != previous;
        }
        if !changed {
            return points_to;
        }
    }
}

fn example() -> Vec<Sites> {
    // a = new A; b = new B; x = relay(a); y = relay(b)
    // relay(p) { return p }
    const A: usize = 0;
    const B: usize = 1;
    // Both call sites use these same callee nodes, which merges their facts.
    const PARAMETER: usize = 2;
    const RETURN: usize = 3;
    const X: usize = 4;
    const Y: usize = 5;
    let mut initial = vec![Sites::new(); 6];
    // Allocation sites seed the initial facts before any propagation.
    initial[A].insert("alloc_A");
    initial[B].insert("alloc_B");
    solve(
        initial,
        &[
            (RETURN, Y),
            (RETURN, X),
            (PARAMETER, RETURN),
            (B, PARAMETER),
            (A, PARAMETER),
        ],
    )
}

fn main() {
    for (variable, sites) in ["a", "b", "relay.p", "relay.return", "x", "y"]
        .into_iter()
        .zip(example())
    {
        println!("{variable} may point to {sites:?}");
    }
}

#[test]
fn merges_call_sites_and_propagates_around_a_cycle() {
    let solution = example();
    assert_eq!(solution[0], Sites::from(["alloc_A"]));
    for sites in &solution[2..] {
        assert_eq!(*sites, Sites::from(["alloc_A", "alloc_B"]));
    }
    let initial = vec![Sites::from(["seed"]), Sites::new()];
    assert_eq!(
        solve(initial, &[(1, 0), (0, 1)]),
        vec![Sites::from(["seed"]); 2]
    );
}
