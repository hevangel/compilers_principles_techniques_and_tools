//! Convert an epsilon-NFA into a table-driven deterministic recognizer.

use std::collections::BTreeSet;

type States = BTreeSet<usize>;

struct Edge {
    from: usize,
    // None represents an epsilon edge, which consumes no input.
    symbol: Option<u8>,
    to: usize,
}

// Compute all states reachable without reading another symbol.
fn closure(mut states: States, edges: &[Edge]) -> States {
    loop {
        let previous = states.len();
        for edge in edges {
            if edge.symbol.is_none() && states.contains(&edge.from) {
                states.insert(edge.to);
            }
        }
        if states.len() == previous {
            return states;
        }
    }
}

struct Dfa {
    subsets: Vec<States>,
    rows: Vec<Vec<usize>>,
}

const ALPHABET: [u8; 3] = *b"abc";
const FINAL: usize = 4;

fn determinize(edges: &[Edge]) -> Dfa {
    let mut subsets = vec![closure(States::from([0]), edges)];
    let mut rows = Vec::new();
    // The unfinished rows act as a queue of discovered subsets.
    while rows.len() < subsets.len() {
        let current = rows.len();
        let mut row = Vec::new();
        for symbol in ALPHABET {
            // Move every possible NFA state on this symbol, then follow epsilon edges.
            let moved = edges
                .iter()
                .filter(|edge| edge.symbol == Some(symbol) && subsets[current].contains(&edge.from))
                .map(|edge| edge.to)
                .collect();
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
            let Some(column) = ALPHABET.iter().position(|&symbol| symbol == byte) else {
                return false;
            };
            state = self.rows[state][column];
        }
        // A single accepting NFA state is enough to accept the whole string.
        self.subsets[state].contains(&FINAL)
    }
}

// Two epsilon branches model ab* | ac, with a shared accepting state.
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
