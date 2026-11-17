//! Minimize a DFA by pruning unreachable states and refining equivalence partitions.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

#[derive(Clone, Debug, PartialEq, Eq)]
struct Dfa {
    start: usize,
    alphabet: Vec<u8>,
    accepting: BTreeSet<usize>,
    trans: Vec<Vec<usize>>,
}

impl Dfa {
    fn accepts(&self, input: &str) -> bool {
        let mut state = self.start;
        for &byte in input.as_bytes() {
            let Some(col) = self.alphabet.iter().position(|&b| b == byte) else {
                return false;
            };
            state = self.trans[state][col];
        }
        self.accepting.contains(&state)
    }
}

fn prune_unreachable(dfa: &Dfa) -> Dfa {
    let mut visited = BTreeSet::from([dfa.start]);
    let mut queue = VecDeque::from([dfa.start]);
    let mut order = Vec::new();
    while let Some(state) = queue.pop_front() {
        order.push(state);
        for &next in &dfa.trans[state] {
            if visited.insert(next) {
                queue.push_back(next);
            }
        }
    }
    let remap: BTreeMap<usize, usize> = order.iter().enumerate().map(|(i, &s)| (s, i)).collect();
    let trans = order
        .iter()
        .map(|&s| dfa.trans[s].iter().map(|t| remap[t]).collect())
        .collect();
    let accepting = dfa
        .accepting
        .iter()
        .filter_map(|s| remap.get(s).copied())
        .collect();
    Dfa {
        start: 0,
        alphabet: dfa.alphabet.clone(),
        accepting,
        trans,
    }
}

fn minimize(dfa: &Dfa) -> (Dfa, Vec<BTreeSet<usize>>) {
    let reachable = prune_unreachable(dfa);
    let num_states = reachable.trans.len();
    let accepting: BTreeSet<usize> = reachable.accepting.iter().copied().collect();
    let non_accepting: BTreeSet<usize> =
        (0..num_states).filter(|s| !accepting.contains(s)).collect();

    // Initial partition separates accepting from non-accepting states, omitting empty blocks.
    let mut partition: Vec<BTreeSet<usize>> = [accepting, non_accepting]
        .into_iter()
        .filter(|block| !block.is_empty())
        .collect();

    loop {
        let mut state_group = vec![0usize; num_states];
        for (gid, group) in partition.iter().enumerate() {
            for &s in group {
                state_group[s] = gid;
            }
        }

        let mut next_partition: Vec<BTreeSet<usize>> = Vec::new();
        for group in &partition {
            let mut subgroups: BTreeMap<Vec<usize>, BTreeSet<usize>> = BTreeMap::new();
            for &state in group {
                let signature: Vec<usize> = (0..reachable.alphabet.len())
                    .map(|col| state_group[reachable.trans[state][col]])
                    .collect();
                subgroups.entry(signature).or_default().insert(state);
            }
            next_partition.extend(subgroups.into_values());
        }

        if next_partition == partition {
            break;
        }
        partition = next_partition;
    }

    // Order blocks deterministically with the start state's group at index 0.
    partition.sort_by_key(|group| {
        (
            usize::from(!group.contains(&reachable.start)),
            *group.iter().next().unwrap(),
        )
    });

    let mut group_of = vec![0usize; num_states];
    for (gid, group) in partition.iter().enumerate() {
        for &s in group {
            group_of[s] = gid;
        }
    }

    let trans = partition
        .iter()
        .map(|group| {
            let rep = *group.iter().next().unwrap();
            (0..reachable.alphabet.len())
                .map(|col| group_of[reachable.trans[rep][col]])
                .collect()
        })
        .collect();

    let min_accepting = partition
        .iter()
        .enumerate()
        .filter_map(|(gid, group)| {
            let rep = *group.iter().next().unwrap();
            reachable.accepting.contains(&rep).then_some(gid)
        })
        .collect();

    (
        Dfa {
            start: group_of[reachable.start],
            alphabet: reachable.alphabet,
            accepting: min_accepting,
            trans,
        },
        partition,
    )
}

// Inflated 6-state DFA for (a|b)*abb (plus one unreachable sink state 5).
fn inflated_abb_dfa() -> Dfa {
    // States:
    // 0: start ("")
    // 1: saw a (equivalent to 3 via duplicate branch)
    // 2: saw b (equivalent to 0)
    // 3: saw a
    // 4: saw ab
    // 5: unreachable dead state
    // 6: saw abb (accepting)
    Dfa {
        start: 0,
        alphabet: b"ab".to_vec(),
        accepting: BTreeSet::from([6]),
        trans: vec![
            vec![1, 2], // 0
            vec![3, 4], // 1 -> equivalent to 3
            vec![1, 2], // 2 -> equivalent to 0
            vec![3, 4], // 3
            vec![1, 6], // 4
            vec![5, 5], // 5 (unreachable)
            vec![1, 2], // 6 (accepting)
        ],
    }
}

fn main() {
    let original = inflated_abb_dfa();
    let (minimized, groups) = minimize(&original);
    println!(
        "States reduced: {} -> {}",
        original.trans.len(),
        minimized.trans.len()
    );
    println!("Equivalence classes: {groups:?}");
    for (idx, row) in minimized.trans.iter().enumerate() {
        let mark = if minimized.accepting.contains(&idx) {
            " (accept)"
        } else {
            ""
        };
        println!("  q{idx}{mark}: a -> q{}, b -> q{}", row[0], row[1]);
    }
    for word in ["ababb", "aba"] {
        println!(
            "  {word:?}: original={}, minimized={}",
            original.accepts(word),
            minimized.accepts(word)
        );
    }
}

#[test]
fn minimizes_inflated_dfa_and_preserves_language() {
    let original = inflated_abb_dfa();
    let (minimized, groups) = minimize(&original);
    assert_eq!(minimized.trans.len(), 4);
    assert_eq!(groups.len(), 4);

    let mut words = vec![String::new()];
    for _ in 0..7 {
        for w in &words {
            assert_eq!(original.accepts(w), minimized.accepts(w), "word {w:?}");
        }
        words = words
            .iter()
            .flat_map(|w| [format!("{w}a"), format!("{w}b")])
            .collect();
    }
}

#[test]
fn handles_single_state_and_unreachable_dfas() {
    let all_accept = Dfa {
        start: 0,
        alphabet: b"ab".to_vec(),
        accepting: BTreeSet::from([0, 1]),
        trans: vec![vec![1, 0], vec![0, 1]],
    };
    let (min_accept, _) = minimize(&all_accept);
    assert_eq!(min_accept.trans.len(), 1);
    assert!(min_accept.accepts("abba"));

    let all_reject = Dfa {
        start: 0,
        alphabet: b"ab".to_vec(),
        accepting: BTreeSet::new(),
        trans: vec![vec![1, 1], vec![0, 0], vec![2, 2]],
    };
    let (min_reject, _) = minimize(&all_reject);
    assert_eq!(min_reject.trans.len(), 1);
    assert!(!min_reject.accepts("ab"));
}
