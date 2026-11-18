//! Compute FIRST and FOLLOW, then use the resulting LL(1) prediction table.

use std::collections::{BTreeMap, BTreeSet};

/// Grammar symbol: a concrete input character or a nonterminal rule index.
#[derive(Clone, Copy)]
enum Symbol {
    Terminal(char),
    Rule(usize),
}

use Symbol::{Rule, Terminal};

type Grammar = &'static [(usize, &'static [Symbol])];
// Store possible starting terminals and whether the sequence can derive empty input (`nullable`).
// Keeping `nullable` as a boolean avoids mixing an epsilon sentinel into character sets.
type First = (BTreeSet<char>, bool);
/// LL(1) predictive parsing table mapping `(nonterminal_id, lookahead_char) -> production_index`.
type Table = BTreeMap<(usize, char), usize>;

// S -> [ S ] S | epsilon
const GRAMMAR: Grammar = &[
    (0, &[Terminal('['), Rule(0), Terminal(']'), Rule(0)]),
    (0, &[]),
];

/// Compute `(FIRST(sequence), nullable(sequence))` using the current `first` estimates.
fn first_of(sequence: &[Symbol], first: &[First]) -> First {
    let mut result = BTreeSet::new();
    for symbol in sequence {
        match *symbol {
            Terminal(c) => {
                // A terminal cannot derive epsilon, so later symbols cannot start the sequence.
                result.insert(c);
                return (result, false);
            }
            Rule(rule) => {
                result.extend(&first[rule].0);
                // Only a nullable symbol lets a later symbol contribute to FIRST.
                if !first[rule].1 {
                    return (result, false);
                }
            }
        }
    }
    // Reached end of sequence with every symbol nullable -> whole sequence is nullable.
    (result, true)
}

/// Build an LL(1) prediction table, or return `Err((lhs, token))` on a table cell collision.
fn predict(grammar: Grammar, rules: usize) -> Result<Table, (usize, char)> {
    // Pass 1: iterate FIRST/nullable to a fixed point because rules can reference each other.
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

    // Pass 2: iterate FOLLOW sets to a fixed point.
    let mut follow = vec![BTreeSet::new(); rules];
    // The start symbol (rule 0) may be followed by the synthetic end-of-input marker `$`.
    follow[0].insert('$');
    loop {
        let previous = follow.clone();
        for &(lhs, rhs) in grammar {
            for (index, symbol) in rhs.iter().enumerate() {
                if let Rule(rule) = *symbol {
                    let (mut tokens, nullable) = first_of(&rhs[index + 1..], &first);
                    // If the suffix disappears (`nullable`), whatever follows `lhs` can also follow `rule`.
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

    // Pass 3: populate M[lhs, token]. A nullable production is selected by FOLLOW(lhs).
    let mut table = Table::new();
    for (index, &(lhs, rhs)) in grammar.iter().enumerate() {
        let (mut tokens, nullable) = first_of(rhs, &first);
        if nullable {
            tokens.extend(&follow[lhs]);
        }
        for token in tokens {
            // A collision means two distinct productions compete for the same (rule, lookahead).
            if table.insert((lhs, token), index).is_some() {
                return Err((lhs, token));
            }
        }
    }
    Ok(table)
}

/// Run a non-recursive LL(1) predictive parser using an explicit symbol stack.
fn parse(input: &str, grammar: Grammar, table: &Table) -> bool {
    let mut tokens = input.chars().chain(std::iter::once('$')).peekable();
    // Pre-seed `$` beneath the start symbol so matching `$` requires balanced nesting first.
    let mut stack = vec![Terminal('$'), Rule(0)];
    while let Some(symbol) = stack.pop() {
        match symbol {
            Terminal(expected) if tokens.next() == Some(expected) => {}
            Terminal(_) => return false,
            Rule(rule) => {
                let Some(index) = tokens.peek().and_then(|&token| table.get(&(rule, token))) else {
                    return false;
                };
                // Push backward so the first right-hand symbol is processed next.
                stack.extend(grammar[*index].1.iter().rev().copied());
            }
        }
    }
    // Reject inputs like `[]$` where an input `$` precedes the appended end marker.
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
