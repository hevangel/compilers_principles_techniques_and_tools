//! Construct an LR(0) automaton and drive a small SLR shift/reduce parser.

use std::collections::{BTreeMap, BTreeSet};

/// Nonterminals (`Start`, `List`, `Item`), terminals (`Name`, `Comma`), and EOF (`End`).
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum Symbol {
    Start,
    List,
    Item,
    Name,
    Comma,
    End,
}

use Symbol::*;
// An LR(0) item is `(rule_index, dot_position)`. A set of items forms one automaton state.
type ItemSet = BTreeSet<(usize, usize)>;
/// Maps `(state, symbol) -> next_state` for both terminals (shifts) and nonterminals (goto).
type Transition = BTreeMap<(usize, Symbol), usize>;

// Augmented grammar: Start -> List, List -> List ',' Item | Item, Item -> name.
// Rule 0 (`Start -> List`) exists solely to give acceptance a unique completed item.
const RULES: &[(Symbol, &[Symbol])] = &[
    (Start, &[List]),
    (List, &[List, Comma, Item]),
    (List, &[Item]),
    (Item, &[Name]),
];

/// Return the symbol immediately after the dot, or `None` if the item is complete.
fn after_dot((rule, dot): (usize, usize)) -> Option<Symbol> {
    RULES[rule].1.get(dot).copied()
}

// A nonterminal after a dot brings in each of its productions with the dot at zero.
// Fixed-point loop handles chains of nonterminals (e.g. `Start -> . List` -> `List -> . Item` -> `Item -> . Name`).
fn closure(mut items: ItemSet) -> ItemSet {
    loop {
        let previous = items.len();
        let expanded: ItemSet = items
            .iter()
            .filter_map(|&item| after_dot(item))
            .flat_map(|symbol| {
                RULES
                    .iter()
                    .enumerate()
                    .filter_map(move |(index, &(lhs, _))| (lhs == symbol).then_some((index, 0)))
            })
            .collect();
        items.extend(expanded);
        if items.len() == previous {
            return items;
        }
    }
}

/// Build the canonical LR(0) collection of item sets and its DFA transition graph.
fn automaton() -> (Vec<ItemSet>, Transition) {
    let mut sets = vec![closure(ItemSet::from([(0, 0)]))];
    let mut transitions = Transition::new();
    let mut state = 0;
    // Invariant: `sets[0..state]` have all outgoing transitions recorded; `sets[state..]` is the BFS queue.
    while state < sets.len() {
        let symbols: BTreeSet<_> = sets[state]
            .iter()
            .filter_map(|&item| after_dot(item))
            .collect();
        for symbol in symbols {
            // A transition advances only items whose dot sits directly before `symbol`.
            let advanced = sets[state]
                .iter()
                .filter(|&&item| after_dot(item) == Some(symbol))
                .map(|&(rule, dot)| (rule, dot + 1))
                .collect();
            let next = closure(advanced);
            let index = sets
                .iter()
                .position(|set| *set == next)
                .unwrap_or(sets.len());
            if index == sets.len() {
                sets.push(next);
            }
            transitions.insert((state, symbol), index);
        }
        state += 1;
    }
    (sets, transitions)
}

/// Parser table action indexed by `(current_state, lookahead_terminal)`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Action {
    Shift(usize),
    Reduce(usize),
    Accept,
}

type Actions = BTreeMap<(usize, Symbol), Action>;

/// Build the SLR(1) action table from the LR(0) automaton.
/// Completed rules only reduce when the lookahead belongs to `FOLLOW(LHS)`.
fn actions(sets: &[ItemSet], transitions: &Transition) -> Result<Actions, (usize, Symbol)> {
    let mut actions = Actions::new();
    // Terminal transitions consume input. Nonterminal transitions (`goto`) are used after reductions.
    for (&key @ (_, symbol), &target) in transitions {
        if matches!(symbol, Name | Comma) {
            actions.insert(key, Action::Shift(target));
        }
    }
    for (state, items) in sets.iter().enumerate() {
        for &(rule, dot) in items {
            if after_dot((rule, dot)).is_some() {
                continue;
            }
            // FOLLOW(List) and FOLLOW(Item) are both { Comma, End }.
            let (lookaheads, action): (&[Symbol], _) = if rule == 0 {
                (&[End], Action::Accept)
            } else {
                (&[Comma, End], Action::Reduce(rule))
            };
            for &token in lookaheads {
                // Two actions in one state/lookahead cell signal a shift/reduce or reduce/reduce conflict.
                if actions.insert((state, token), action).is_some() {
                    return Err((state, token));
                }
            }
        }
    }
    Ok(actions)
}

/// Run the SLR(1) shift-reduce driver; returns the sequence of reduced rule indices on success.
fn parse(tokens: &[Symbol], actions: &Actions, goto: &Transition) -> Option<Vec<usize>> {
    // Reject nonterminals or stray End markers injected inside the token slice.
    if tokens.iter().any(|token| !matches!(token, Name | Comma)) {
        return None;
    }
    let mut input = tokens.iter().copied().chain([End]).peekable();
    // State stack always starts at state 0 and never becomes empty.
    let mut stack = vec![0];
    let mut reductions = Vec::new();
    loop {
        match actions.get(&(*stack.last()?, *input.peek()?))? {
            Action::Shift(state) => {
                stack.push(*state);
                input.next();
            }
            // Pop one state per RHS symbol, then push `goto[uncovered_state, lhs]`.
            // Notice `input` is NOT advanced on Reduce: the same lookahead is re-examined next.
            Action::Reduce(rule) => {
                let (lhs, rhs) = RULES[*rule];
                stack.truncate(stack.len() - rhs.len());
                stack.push(*goto.get(&(*stack.last()?, lhs))?);
                reductions.push(*rule);
            }
            Action::Accept => return Some(reductions),
        }
    }
}

fn main() {
    let (sets, goto) = automaton();
    let actions = actions(&sets, &goto).unwrap();
    println!("LR(0) states: {sets:?}");
    println!("Actions: {actions:?}");
    println!(
        "Reductions: {:?}",
        parse(&[Name, Comma, Name], &actions, &goto).unwrap()
    );
}

#[test]
fn handles_left_recursion_and_rejects_trailing_separator() {
    let (sets, goto) = automaton();
    let actions = actions(&sets, &goto).unwrap();
    assert_eq!(
        parse(&[Name, Comma, Name], &actions, &goto),
        Some(vec![3, 2, 3, 1])
    );
    assert_eq!(parse(&[Name, Comma], &actions, &goto), None);
    assert_eq!(parse(&[Name, End, Name], &actions, &goto), None);
    assert_eq!(parse(&[], &actions, &goto), None);
}
