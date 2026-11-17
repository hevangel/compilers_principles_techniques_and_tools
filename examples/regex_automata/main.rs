//! Compile regular expressions into Thompson NFAs and direct followpos DFAs.

use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq, Eq)]
enum RegexNode {
    Empty,
    Literal(u8, usize),
    Cat(Box<RegexNode>, Box<RegexNode>),
    Alt(Box<RegexNode>, Box<RegexNode>),
    Star(Box<RegexNode>),
    EndMarker(usize),
}

#[derive(Clone, Debug)]
enum RawAst {
    Empty,
    Literal(u8),
    Cat(Box<RawAst>, Box<RawAst>),
    Alt(Box<RawAst>, Box<RawAst>),
    Star(Box<RawAst>),
    Plus(Box<RawAst>),
    Opt(Box<RawAst>),
}

struct RegexParser<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> RegexParser<'a> {
    fn new(pattern: &'a str) -> Self {
        Self {
            bytes: pattern.as_bytes(),
            pos: 0,
        }
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.pos).copied()
    }

    fn parse_alt(&mut self) -> Result<RawAst, String> {
        let mut node = self.parse_cat()?;
        while self.peek() == Some(b'|') {
            self.pos += 1;
            let rhs = self.parse_cat()?;
            node = RawAst::Alt(Box::new(node), Box::new(rhs));
        }
        Ok(node)
    }

    fn parse_cat(&mut self) -> Result<RawAst, String> {
        let mut factors = Vec::new();
        while let Some(ch) = self.peek() {
            if ch == b'|' || ch == b')' {
                break;
            }
            factors.push(self.parse_postfix()?);
        }
        if factors.is_empty() {
            return Err("empty alternative in regular expression".into());
        }
        let mut it = factors.into_iter();
        let first = it.next().unwrap();
        Ok(it.fold(first, |acc, next| {
            RawAst::Cat(Box::new(acc), Box::new(next))
        }))
    }

    fn parse_postfix(&mut self) -> Result<RawAst, String> {
        let mut node = self.parse_atom()?;
        while let Some(op) = self.peek() {
            match op {
                b'*' => {
                    self.pos += 1;
                    node = RawAst::Star(Box::new(node));
                }
                b'+' => {
                    self.pos += 1;
                    node = RawAst::Plus(Box::new(node));
                }
                b'?' => {
                    self.pos += 1;
                    node = RawAst::Opt(Box::new(node));
                }
                _ => break,
            }
        }
        Ok(node)
    }

    fn parse_atom(&mut self) -> Result<RawAst, String> {
        match self.peek() {
            Some(b'(') => {
                self.pos += 1;
                if self.peek() == Some(b')') {
                    self.pos += 1;
                    return Ok(RawAst::Empty);
                }
                let inner = self.parse_alt()?;
                if self.peek() != Some(b')') {
                    return Err("unclosed '(' in regular expression".into());
                }
                self.pos += 1;
                Ok(inner)
            }
            Some(b'*' | b'+' | b'?' | b'|' | b')') => {
                Err(format!("unexpected operator at byte {}", self.pos))
            }
            Some(b'\\') => {
                self.pos += 1;
                let escaped = self
                    .peek()
                    .ok_or_else(|| "trailing backslash in regular expression".to_string())?;
                self.pos += 1;
                Ok(RawAst::Literal(escaped))
            }
            Some(ch) => {
                self.pos += 1;
                Ok(RawAst::Literal(ch))
            }
            None => Err("unexpected end of regular expression".into()),
        }
    }
}

fn assign_positions(raw: &RawAst, next_pos: &mut usize) -> RegexNode {
    match raw {
        RawAst::Empty => RegexNode::Empty,
        RawAst::Literal(ch) => {
            let id = *next_pos;
            *next_pos += 1;
            RegexNode::Literal(*ch, id)
        }
        RawAst::Cat(a, b) => RegexNode::Cat(
            Box::new(assign_positions(a, next_pos)),
            Box::new(assign_positions(b, next_pos)),
        ),
        RawAst::Alt(a, b) => RegexNode::Alt(
            Box::new(assign_positions(a, next_pos)),
            Box::new(assign_positions(b, next_pos)),
        ),
        RawAst::Star(a) => RegexNode::Star(Box::new(assign_positions(a, next_pos))),
        RawAst::Plus(a) => RegexNode::Cat(
            Box::new(assign_positions(a, next_pos)),
            Box::new(RegexNode::Star(Box::new(assign_positions(a, next_pos)))),
        ),
        RawAst::Opt(a) => RegexNode::Alt(
            Box::new(assign_positions(a, next_pos)),
            Box::new(RegexNode::Empty),
        ),
    }
}

fn parse_regex(pattern: &str) -> Result<RegexNode, String> {
    if pattern.is_empty() {
        return Ok(RegexNode::Empty);
    }
    if !pattern.is_ascii() {
        return Err("only ASCII regular expressions are supported".into());
    }
    let mut parser = RegexParser::new(pattern);
    let raw = parser.parse_alt()?;
    if parser.pos != parser.bytes.len() {
        return Err(format!(
            "unexpected trailing character at byte {}",
            parser.pos
        ));
    }
    let mut next_pos = 1;
    Ok(assign_positions(&raw, &mut next_pos))
}

#[derive(Clone, Debug)]
struct Nfa {
    start: usize,
    accept: usize,
    transitions: Vec<Vec<(Option<u8>, usize)>>,
}

impl Nfa {
    fn new_state(&mut self) -> usize {
        let id = self.transitions.len();
        self.transitions.push(Vec::new());
        id
    }

    fn build(&mut self, node: &RegexNode) -> (usize, usize) {
        match node {
            RegexNode::Empty => {
                let s = self.new_state();
                let t = self.new_state();
                self.transitions[s].push((None, t));
                (s, t)
            }
            RegexNode::Literal(ch, _) => {
                let s = self.new_state();
                let t = self.new_state();
                self.transitions[s].push((Some(*ch), t));
                (s, t)
            }
            RegexNode::Cat(a, b) => {
                let (s1, t1) = self.build(a);
                let (s2, t2) = self.build(b);
                self.transitions[t1].push((None, s2));
                (s1, t2)
            }
            RegexNode::Alt(a, b) => {
                let s = self.new_state();
                let (s1, t1) = self.build(a);
                let (s2, t2) = self.build(b);
                let t = self.new_state();
                self.transitions[s].push((None, s1));
                self.transitions[s].push((None, s2));
                self.transitions[t1].push((None, t));
                self.transitions[t2].push((None, t));
                (s, t)
            }
            RegexNode::Star(a) => {
                let s = self.new_state();
                let (s1, t1) = self.build(a);
                let t = self.new_state();
                self.transitions[s].push((None, s1));
                self.transitions[s].push((None, t));
                self.transitions[t1].push((None, s1));
                self.transitions[t1].push((None, t));
                (s, t)
            }
            RegexNode::EndMarker(_) => unreachable!("EndMarker is only added for direct DFA"),
        }
    }

    fn eps_closure(&self, seeds: &BTreeSet<usize>) -> BTreeSet<usize> {
        let mut closure = seeds.clone();
        let mut stack: Vec<usize> = seeds.iter().copied().collect();
        while let Some(state) = stack.pop() {
            for &(sym, dst) in &self.transitions[state] {
                if sym.is_none() && closure.insert(dst) {
                    stack.push(dst);
                }
            }
        }
        closure
    }

    fn accepts(&self, input: &str) -> bool {
        let mut current = self.eps_closure(&BTreeSet::from([self.start]));
        for &byte in input.as_bytes() {
            let mut moved = BTreeSet::new();
            for &state in &current {
                for &(sym, dst) in &self.transitions[state] {
                    if sym == Some(byte) {
                        moved.insert(dst);
                    }
                }
            }
            current = self.eps_closure(&moved);
        }
        current.contains(&self.accept)
    }
}

fn to_thompson_nfa(node: &RegexNode) -> Nfa {
    let mut nfa = Nfa {
        start: 0,
        accept: 0,
        transitions: Vec::new(),
    };
    let (start, accept) = nfa.build(node);
    nfa.start = start;
    nfa.accept = accept;
    nfa
}

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

struct DirectInfo {
    nullable: bool,
    firstpos: BTreeSet<usize>,
    lastpos: BTreeSet<usize>,
}

fn max_position(node: &RegexNode) -> usize {
    match node {
        RegexNode::Empty => 0,
        RegexNode::Literal(_, pos) | RegexNode::EndMarker(pos) => *pos,
        RegexNode::Cat(a, b) | RegexNode::Alt(a, b) => max_position(a).max(max_position(b)),
        RegexNode::Star(a) => max_position(a),
    }
}

fn collect_positions(
    node: &RegexNode,
    pos_char: &mut BTreeMap<usize, u8>,
    followpos: &mut BTreeMap<usize, BTreeSet<usize>>,
) -> DirectInfo {
    match node {
        RegexNode::Empty => DirectInfo {
            nullable: true,
            firstpos: BTreeSet::new(),
            lastpos: BTreeSet::new(),
        },
        RegexNode::Literal(ch, pos) => {
            pos_char.insert(*pos, *ch);
            followpos.entry(*pos).or_default();
            DirectInfo {
                nullable: false,
                firstpos: BTreeSet::from([*pos]),
                lastpos: BTreeSet::from([*pos]),
            }
        }
        RegexNode::EndMarker(pos) => {
            followpos.entry(*pos).or_default();
            DirectInfo {
                nullable: false,
                firstpos: BTreeSet::from([*pos]),
                lastpos: BTreeSet::from([*pos]),
            }
        }
        RegexNode::Alt(a, b) => {
            let left = collect_positions(a, pos_char, followpos);
            let right = collect_positions(b, pos_char, followpos);
            let mut firstpos = left.firstpos;
            firstpos.extend(right.firstpos);
            let mut lastpos = left.lastpos;
            lastpos.extend(right.lastpos);
            DirectInfo {
                nullable: left.nullable || right.nullable,
                firstpos,
                lastpos,
            }
        }
        RegexNode::Cat(a, b) => {
            let left = collect_positions(a, pos_char, followpos);
            let right = collect_positions(b, pos_char, followpos);
            for &p in &left.lastpos {
                followpos.entry(p).or_default().extend(&right.firstpos);
            }
            let mut firstpos = left.firstpos;
            if left.nullable {
                firstpos.extend(right.firstpos);
            }
            let mut lastpos = right.lastpos.clone();
            if right.nullable {
                lastpos.extend(left.lastpos);
            }
            DirectInfo {
                nullable: left.nullable && right.nullable,
                firstpos,
                lastpos,
            }
        }
        RegexNode::Star(a) => {
            let inner = collect_positions(a, pos_char, followpos);
            for &p in &inner.lastpos {
                followpos.entry(p).or_default().extend(&inner.firstpos);
            }
            DirectInfo {
                nullable: true,
                firstpos: inner.firstpos,
                lastpos: inner.lastpos,
            }
        }
    }
}

fn compile_direct_dfa(node: &RegexNode) -> Dfa {
    let end_pos = max_position(node) + 1;
    let augmented = RegexNode::Cat(
        Box::new(node.clone()),
        Box::new(RegexNode::EndMarker(end_pos)),
    );
    let mut pos_char = BTreeMap::new();
    let mut followpos = BTreeMap::new();
    let root = collect_positions(&augmented, &mut pos_char, &mut followpos);

    let alphabet: Vec<u8> = pos_char
        .values()
        .copied()
        .collect::<BTreeSet<u8>>()
        .into_iter()
        .collect();

    let mut state_sets = vec![root.firstpos];
    let mut state_index = BTreeMap::from([(state_sets[0].clone(), 0usize)]);
    let mut accepting = BTreeSet::new();
    let mut trans = Vec::new();

    let mut cursor = 0;
    while cursor < state_sets.len() {
        let current = state_sets[cursor].clone();
        if current.contains(&end_pos) {
            accepting.insert(cursor);
        }
        let mut row = Vec::with_capacity(alphabet.len());
        for &sym in &alphabet {
            let mut next_set = BTreeSet::new();
            for &p in &current {
                if pos_char.get(&p) == Some(&sym) {
                    if let Some(fp) = followpos.get(&p) {
                        next_set.extend(fp);
                    }
                }
            }
            let target = match state_index.get(&next_set) {
                Some(&idx) => idx,
                None => {
                    let idx = state_sets.len();
                    state_sets.push(next_set.clone());
                    state_index.insert(next_set, idx);
                    idx
                }
            };
            row.push(target);
        }
        trans.push(row);
        cursor += 1;
    }

    Dfa {
        start: 0,
        alphabet,
        accepting,
        trans,
    }
}

fn main() {
    let pattern = "(a|b)*abb";
    let ast = parse_regex(pattern).unwrap();
    let nfa = to_thompson_nfa(&ast);
    let dfa = compile_direct_dfa(&ast);
    println!(
        "Pattern {pattern}: Thompson NFA has {} states, direct DFA has {} states",
        nfa.transitions.len(),
        dfa.trans.len()
    );
    for word in ["abb", "aabb", "babb", "ab", "abbc", ""] {
        println!(
            "  {word:?}: NFA={}, DFA={}",
            nfa.accepts(word),
            dfa.accepts(word)
        );
    }
}

#[test]
fn thompson_nfa_and_direct_dfa_agree_across_patterns() {
    let patterns = [
        ("(a|b)*abb", &["a", "b"][..]),
        ("a(b|c)*", &["a", "b", "c"][..]),
        ("a+b?", &["a", "b"][..]),
        ("()", &["a"][..]),
        ("", &["a"][..]),
    ];
    for &(pattern, alpha) in &patterns {
        let ast = parse_regex(pattern).unwrap();
        let nfa = to_thompson_nfa(&ast);
        let dfa = compile_direct_dfa(&ast);
        let mut words = vec![String::new()];
        for _ in 0..5 {
            let mut next = Vec::new();
            for w in &words {
                for &ch in alpha {
                    next.push(format!("{w}{ch}"));
                }
            }
            for w in &words {
                assert_eq!(
                    nfa.accepts(w),
                    dfa.accepts(w),
                    "mismatch on pattern {pattern:?} word {w:?}"
                );
            }
            words.extend(next);
        }
    }
    let dfa = compile_direct_dfa(&parse_regex("(a|b)*abb").unwrap());
    assert!(dfa.accepts("abb"));
    assert!(dfa.accepts("ababb"));
    assert!(!dfa.accepts("aba"));
    assert!(!dfa.accepts("abbc"));
}

#[test]
fn rejects_malformed_regexes() {
    for bad in ["(ab", "*a", "a||b", "a)", "a|", "|a", "a\\"] {
        assert!(parse_regex(bad).is_err(), "expected error for {bad:?}");
    }
}
