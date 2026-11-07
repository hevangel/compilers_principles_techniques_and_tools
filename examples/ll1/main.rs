use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy)]
enum Symbol {
    Terminal(char),
    Rule(usize),
}

use Symbol::{Rule, Terminal};

type Grammar = &'static [(usize, &'static [Symbol])];
type First = (BTreeSet<char>, bool);
type Table = BTreeMap<(usize, char), usize>;

// S -> [ S ] S | epsilon
const GRAMMAR: Grammar = &[
    (0, &[Terminal('['), Rule(0), Terminal(']'), Rule(0)]),
    (0, &[]),
];

fn first_of(sequence: &[Symbol], first: &[First]) -> First {
    let mut result = BTreeSet::new();
    for symbol in sequence {
        match *symbol {
            Terminal(c) => {
                result.insert(c);
                return (result, false);
            }
            Rule(rule) => {
                result.extend(&first[rule].0);
                if !first[rule].1 {
                    return (result, false);
                }
            }
        }
    }
    (result, true)
}

fn predict(grammar: Grammar, rules: usize) -> Result<Table, (usize, char)> {
    let mut first = vec![First::default(); rules];
    loop {
        let previous = first.clone();
        for &(lhs, rhs) in grammar {
            let (tokens, nullable) = first_of(rhs, &first);
            first[lhs].0.extend(tokens);
            first[lhs].1 |= nullable;
        }
        if first == previous {
            break;
        }
    }

    let mut follow = vec![BTreeSet::new(); rules];
    follow[0].insert('$');
    loop {
        let previous = follow.clone();
        for &(lhs, rhs) in grammar {
            for (index, symbol) in rhs.iter().enumerate() {
                if let Rule(rule) = *symbol {
                    let (mut tokens, nullable) = first_of(&rhs[index + 1..], &first);
                    if nullable {
                        tokens.extend(&follow[lhs]);
                    }
                    follow[rule].extend(tokens);
                }
            }
        }
        if follow == previous {
            break;
        }
    }

    let mut table = Table::new();
    for (index, &(lhs, rhs)) in grammar.iter().enumerate() {
        let (mut tokens, nullable) = first_of(rhs, &first);
        if nullable {
            tokens.extend(&follow[lhs]);
        }
        for token in tokens {
            if table.insert((lhs, token), index).is_some() {
                return Err((lhs, token));
            }
        }
    }
    Ok(table)
}

fn parse(input: &str, grammar: Grammar, table: &Table) -> bool {
    let mut tokens = input.chars().chain(std::iter::once('$')).peekable();
    let mut stack = vec![Terminal('$'), Rule(0)];
    while let Some(symbol) = stack.pop() {
        match symbol {
            Terminal(expected) if tokens.next() == Some(expected) => {}
            Terminal(_) => return false,
            Rule(rule) => {
                let Some(index) = tokens.peek().and_then(|&token| table.get(&(rule, token))) else {
                    return false;
                };
                stack.extend(grammar[*index].1.iter().rev().copied());
            }
        }
    }
    tokens.next().is_none()
}

fn main() {
    let table = predict(GRAMMAR, 1).unwrap();
    println!("Prediction table: {table:?}");
    for input in ["[[]][]", "[][", ""] {
        println!("{input:?}: {}", parse(input, GRAMMAR, &table));
    }
}

#[test]
fn predicts_nullable_rule_from_follow() {
    let table = predict(GRAMMAR, 1).unwrap();
    assert_eq!(table.get(&(0, ']')), Some(&1));
    assert!(parse("[[]][]", GRAMMAR, &table));
    assert!(!parse("[]]", GRAMMAR, &table));
    assert!(!parse("[]$", GRAMMAR, &table));
}

#[test]
fn reports_prediction_collision() {
    assert_eq!(
        predict(&[(0, &[Terminal('x')]), (0, &[Terminal('x'), Rule(0)])], 1),
        Err((0, 'x'))
    );
}
