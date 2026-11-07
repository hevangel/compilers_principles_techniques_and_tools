//! Construct an LR(0) automaton and drive a small SLR shift/reduce parser.

use std::collections::{BTreeMap, BTreeSet};

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
// An item stores a production index and the position of its dot.
type ItemSet = BTreeSet<(usize, usize)>;
type Transition = BTreeMap<(usize, Symbol), usize>;

// Start -> List, List -> List ',' Item | Item, Item -> name.
const RULES: &[(Symbol, &[Symbol])] = &[
    (Start, &[List]),
    (List, &[List, Comma, Item]),
    (List, &[Item]),
    (Item, &[Name]),
];

fn after_dot((rule, dot): (usize, usize)) -> Option<Symbol> {
    RULES[rule].1.get(dot).copied()
}

// A nonterminal after a dot brings in each of its productions with the dot at zero.
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

fn automaton() -> (Vec<ItemSet>, Transition) {
    let mut sets = vec![closure(ItemSet::from([(0, 0)]))];
    let mut transitions = Transition::new();
    let mut state = 0;
    while state < sets.len() {
        let symbols: BTreeSet<_> = sets[state]
            .iter()
            .filter_map(|&item| after_dot(item))
            .collect();
        for symbol in symbols {
            // A transition advances only items waiting for this symbol.
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Action {
    Shift(usize),
    Reduce(usize),
    Accept,
}

type Actions = BTreeMap<(usize, Symbol), Action>;

fn actions(sets: &[ItemSet], transitions: &Transition) -> Result<Actions, (usize, Symbol)> {
    let mut actions = Actions::new();
    // Terminal transitions consume input. Nonterminal transitions are used after reductions.
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
            // FOLLOW(List) and FOLLOW(Item) are both { comma, end }.
            let (lookaheads, action): (&[Symbol], _) = if rule == 0 {
                (&[End], Action::Accept)
            } else {
                (&[Comma, End], Action::Reduce(rule))
            };
            for &token in lookaheads {
                // Two actions in one state/lookahead cell signal a grammar conflict.
                if actions.insert((state, token), action).is_some() {
                    return Err((state, token));
                }
            }
        }
    }
    Ok(actions)
}

fn parse(tokens: &[Symbol], actions: &Actions, goto: &Transition) -> Option<Vec<usize>> {
    if tokens.iter().any(|token| !matches!(token, Name | Comma)) {
        return None;
    }
    let mut input = tokens.iter().copied().chain([End]).peekable();
    let mut stack = vec![0];
    let mut reductions = Vec::new();
    loop {
        match actions.get(&(*stack.last()?, *input.peek()?))? {
            Action::Shift(state) => {
                stack.push(*state);
                input.next();
            }
            // Replace the recognized right-hand side with its left-hand nonterminal via goto.
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
