//! Convert an epsilon-NFA into a table-driven deterministic recognizer.

use std::collections::BTreeSet;

/// Canonical ordered set of NFA state IDs; equality on `States` identifies a DFA state.
type States = BTreeSet<usize>;

/// Directed NFA transition (`symbol: None` represents an epsilon edge consuming no input).
struct Edge {
    from: usize,
    // None represents an epsilon edge, which consumes no input.
    symbol: Option<u8>,
    to: usize,
}

// Compute all states reachable without reading another symbol (fixed-point epsilon closure).
fn closure(mut states: States, edges: &[Edge]) -> States {
    loop {
        let previous = states.len();
        for edge in edges {
            if edge.symbol.is_none() && states.contains(&edge.from) {
                states.insert(edge.to);
            }
        }
        // Stopping when the set size stabilizes handles arbitrary epsilon chains and cycles.
        if states.len() == previous {
            return states;
        }
    }
}

/// Deterministic automaton whose states correspond 1-to-1 with `subsets[i]`.
struct Dfa {
    subsets: Vec<States>,
    /// `rows[state][col]` gives the next DFA state index for `ALPHABET[col]`.
    rows: Vec<Vec<usize>>,
}

const ALPHABET: [u8; 3] = *b"abc";
const FINAL: usize = 4;

/// Subset construction (powerset construction) starting from NFA state 0.
fn determinize(edges: &[Edge]) -> Dfa {
    let mut subsets = vec![closure(States::from([0]), edges)];
    let mut rows = Vec::new();
    // Invariant: `rows.len() <= subsets.len()`. Unfinished rows act as a BFS worklist
    // of discovered NFA subsets waiting to have their transitions computed.
    while rows.len() < subsets.len() {
        let current = rows.len();
        let mut row = Vec::new();
        for symbol in ALPHABET {
            // Step 1 (move): follow all `symbol` edges out of every state in `subsets[current]`.
            let moved = edges
                .iter()
                .filter(|edge| edge.symbol == Some(symbol) && subsets[current].contains(&edge.from))
                .map(|edge| edge.to)
                .collect();
            // Step 2 (eps-closure): include states reachable silently after moving.
            // Note: if `moved` is empty, `next` is `{}` (the explicit dead/sink state).
            let next = closure(moved, edges);
            // Use the set itself as the identity of a DFA state.
            let index = subsets
                .iter()
                .position(|set| *set == next)
                .unwrap_or(subsets.len());
            if index == subsets.len() {
                subsets.push(next);
            }
            row.push(index);
        }
        rows.push(row);
    }
    Dfa { subsets, rows }
}

impl Dfa {
    fn accepts(&self, input: &str) -> bool {
        let mut state = 0;
        for byte in input.bytes() {
            // Any byte outside ALPHABET (such as non-ASCII 'é') immediately rejects.
            let Some(column) = ALPHABET.iter().position(|&symbol| symbol == byte) else {
                return false;
            };
            state = self.rows[state][column];
        }
        // A single accepting NFA state in the subset is enough to accept the whole string.
        self.subsets[state].contains(&FINAL)
    }
}

// Two epsilon branches model ab* | ac, with a shared accepting state (FINAL = 4).
fn sample() -> Dfa {
    let edges = [
        (0, None, 1),
        (0, None, 2),
        (1, Some(b'a'), 3),
        (3, Some(b'b'), 3),
        (3, None, FINAL),
        (2, Some(b'a'), 5),
        (5, Some(b'c'), FINAL),
    ]
    .map(|(from, symbol, to)| Edge { from, symbol, to });
    determinize(&edges)
}

fn main() {
    let dfa = sample();
    for (index, (set, row)) in dfa.subsets.iter().zip(&dfa.rows).enumerate() {
        println!("{index}: {set:?} -> {row:?} on a, b, c");
    }
    for word in ["a", "abbb", "ac", "abc"] {
        println!("{word}: {}", dfa.accepts(word));
    }
}

#[test]
fn follows_epsilon_branches_and_handles_dead_state() {
    let dfa = sample();
    for word in ["a", "ab", "abbb", "ac"] {
        assert!(dfa.accepts(word), "{word}");
    }
    for word in ["", "b", "abc", "acc", "é"] {
        assert!(!dfa.accepts(word), "{word}");
    }
}
