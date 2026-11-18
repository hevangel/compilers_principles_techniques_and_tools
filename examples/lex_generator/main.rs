//! Lex-style `.l` scanner-generator DSL compiler: expands macro definitions, builds a unified
//! multi-pattern Thompson NFA -> DFA with longest-match + rule-priority tie-breaking and `/`
//! trailing-context lookahead, and executes the generated scanner on sample source text.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fmt;

// ---------------------------------------------------------------------------
// 1. Finite Byte Domain & Regex AST
// ---------------------------------------------------------------------------

/// Finite deterministic byte alphabet supported in generated DFA rows:
/// tab (`\t`), newline (`\n`), and printable ASCII `0x20..=0x7E`.
fn all_alphabet_bytes() -> BTreeSet<u8> {
    let mut set = BTreeSet::new();
    set.insert(b'\t');
    set.insert(b'\n');
    for b in 0x20u8..=0x7Eu8 {
        set.insert(b);
    }
    set
}

#[derive(Clone, Debug)]
enum RegexAst {
    Empty,
    CharSet(BTreeSet<u8>),
    Cat(Box<RegexAst>, Box<RegexAst>),
    Alt(Box<RegexAst>, Box<RegexAst>),
    Star(Box<RegexAst>),
}

// ---------------------------------------------------------------------------
// 2. `.l` Specification Parser (`defs %% rules %% subroutines`)
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
struct LexRuleSpec {
    rule_id: usize,
    raw_pattern: String,
    action: String,
    head_ast: RegexAst,
    tail_ast: Option<RegexAst>,
}

#[derive(Clone, Debug)]
struct LexSpec {
    defs: BTreeMap<String, String>,
    rules: Vec<LexRuleSpec>,
    user_code: String,
}

/// Expands `{name}` macro references inside a Lex pattern using the `definitions` table,
/// wrapping each expansion in parentheses so operator precedence never leaks out.
fn expand_macros(
    pattern: &str,
    defs: &BTreeMap<String, String>,
    visiting: &mut BTreeSet<String>,
) -> Result<String, String> {
    let bytes = pattern.as_bytes();
    let mut out = String::new();
    let mut i = 0usize;
    let mut in_quotes = false;
    let mut in_class = false;

    while i < bytes.len() {
        let ch = bytes[i];
        if ch == b'\\' && i + 1 < bytes.len() {
            out.push(ch as char);
            out.push(bytes[i + 1] as char);
            i += 2;
            continue;
        }
        if !in_class && ch == b'"' {
            in_quotes = !in_quotes;
            out.push('"');
            i += 1;
            continue;
        }
        if !in_quotes && ch == b'[' {
            in_class = true;
            out.push('[');
            i += 1;
            continue;
        }
        if in_class && ch == b']' {
            in_class = false;
            out.push(']');
            i += 1;
            continue;
        }
        if !in_quotes && !in_class && ch == b'{' {
            let start = i + 1;
            let mut end = start;
            while end < bytes.len() && bytes[end] != b'}' {
                end += 1;
            }
            if end >= bytes.len() {
                return Err(format!("unclosed '{{macro}}' in pattern: {pattern}"));
            }
            let name = std::str::from_utf8(&bytes[start..end])
                .map_err(|_| "non-utf8 macro name".to_string())?;
            if let Some(body) = defs.get(name) {
                if !visiting.insert(name.to_string()) {
                    return Err(format!("cyclic macro definition: {{{name}}}"));
                }
                let expanded = expand_macros(body, defs, visiting)?;
                visiting.remove(name);
                out.push('(');
                out.push_str(&expanded);
                out.push(')');
                i = end + 1;
                continue;
            }
            return Err(format!("undefined Lex macro: {{{name}}}"));
        }
        out.push(ch as char);
        i += 1;
    }
    Ok(out)
}

/// Splits an expanded rule pattern on an unescaped, unquoted, top-level `/` trailing-context
/// operator. Rejects patterns containing more than one top-level `/`.
fn split_trailing_context(pattern: &str) -> Result<(&str, Option<&str>), String> {
    let bytes = pattern.as_bytes();
    let mut i = 0usize;
    let mut in_quotes = false;
    let mut in_class = false;
    let mut paren_depth = 0usize;
    let mut split_at: Option<usize> = None;

    while i < bytes.len() {
        let ch = bytes[i];
        if ch == b'\\' && i + 1 < bytes.len() {
            i += 2;
            continue;
        }
        if !in_class && ch == b'"' {
            in_quotes = !in_quotes;
            i += 1;
            continue;
        }
        if !in_quotes && ch == b'[' {
            in_class = true;
            i += 1;
            continue;
        }
        if in_class && ch == b']' {
            in_class = false;
            i += 1;
            continue;
        }
        if !in_quotes && !in_class {
            match ch {
                b'(' => paren_depth += 1,
                b')' => {
                    if paren_depth == 0 {
                        return Err(format!("unmatched ')' in pattern: {pattern}"));
                    }
                    paren_depth -= 1;
                }
                b'/' => {
                    if paren_depth != 0 {
                        return Err(format!(
                            "trailing context '/' not allowed inside parentheses: {pattern}"
                        ));
                    }
                    if split_at.is_some() {
                        return Err(format!(
                            "multiple trailing-context '/' operators in rule: {pattern}"
                        ));
                    }
                    split_at = Some(i);
                }
                _ => {}
            }
        }
        i += 1;
    }

    if let Some(pos) = split_at {
        Ok((&pattern[..pos], Some(&pattern[pos + 1..])))
    } else {
        Ok((pattern, None))
    }
}

/// Parses a single expanded Lex regex (without top-level `/`) into a `RegexAst`.
struct LexRegexParser<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> LexRegexParser<'a> {
    fn new(src: &'a str) -> Self {
        Self {
            bytes: src.as_bytes(),
            pos: 0,
        }
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.pos).copied()
    }

    fn decode_escape(&mut self) -> Result<u8, String> {
        if self.pos >= self.bytes.len() {
            return Err("dangling '\\' escape at end of regex".into());
        }
        let ch = self.bytes[self.pos];
        self.pos += 1;
        Ok(match ch {
            b'n' => b'\n',
            b't' => b'\t',
            other => other,
        })
    }

    fn parse(&mut self) -> Result<RegexAst, String> {
        if self.bytes.is_empty() {
            return Err("empty regex pattern".into());
        }
        let ast = self.parse_alt()?;
        if self.pos < self.bytes.len() {
            return Err(format!(
                "unexpected trailing regex token '{}'",
                self.bytes[self.pos] as char
            ));
        }
        Ok(ast)
    }

    fn parse_alt(&mut self) -> Result<RegexAst, String> {
        let mut node = self.parse_cat()?;
        while self.peek() == Some(b'|') {
            self.pos += 1;
            let rhs = self.parse_cat()?;
            node = RegexAst::Alt(Box::new(node), Box::new(rhs));
        }
        Ok(node)
    }

    fn parse_cat(&mut self) -> Result<RegexAst, String> {
        let mut items = Vec::new();
        while let Some(ch) = self.peek() {
            if ch == b'|' || ch == b')' {
                break;
            }
            items.push(self.parse_postfix()?);
        }
        if items.is_empty() {
            return Ok(RegexAst::Empty);
        }
        let mut iter = items.into_iter();
        let first = iter.next().unwrap();
        Ok(iter.fold(first, |acc, next| {
            RegexAst::Cat(Box::new(acc), Box::new(next))
        }))
    }

    fn parse_postfix(&mut self) -> Result<RegexAst, String> {
        let mut node = self.parse_atom()?;
        while let Some(op) = self.peek() {
            match op {
                b'*' => {
                    self.pos += 1;
                    node = RegexAst::Star(Box::new(node));
                }
                b'+' => {
                    // Desugar `r+` into `r r*`.
                    self.pos += 1;
                    let star = RegexAst::Star(Box::new(node.clone()));
                    node = RegexAst::Cat(Box::new(node), Box::new(star));
                }
                b'?' => {
                    // Desugar `r?` into `(r | eps)`.
                    self.pos += 1;
                    node = RegexAst::Alt(Box::new(node), Box::new(RegexAst::Empty));
                }
                _ => break,
            }
        }
        Ok(node)
    }

    fn parse_atom(&mut self) -> Result<RegexAst, String> {
        let Some(ch) = self.peek() else {
            return Err("unexpected end of regex atom".into());
        };
        match ch {
            b'*' | b'+' | b'?' | b'|' | b')' => {
                Err(format!("unexpected operator '{}' in regex", ch as char))
            }
            b'(' => {
                self.pos += 1;
                let inner = self.parse_alt()?;
                if self.peek() != Some(b')') {
                    return Err("unclosed '(' in regex".into());
                }
                self.pos += 1;
                Ok(inner)
            }
            b'"' => {
                // Quoted literal string: metacharacters inside `"..."` match verbatim bytes.
                self.pos += 1;
                let mut parts = Vec::new();
                while let Some(q) = self.peek() {
                    if q == b'"' {
                        self.pos += 1;
                        if parts.is_empty() {
                            return Ok(RegexAst::Empty);
                        }
                        let mut it = parts.into_iter();
                        let first = it.next().unwrap();
                        return Ok(it.fold(first, |a, b| RegexAst::Cat(Box::new(a), Box::new(b))));
                    }
                    let byte = if q == b'\\' {
                        self.pos += 1;
                        self.decode_escape()?
                    } else {
                        self.pos += 1;
                        q
                    };
                    let mut set = BTreeSet::new();
                    set.insert(byte);
                    parts.push(RegexAst::CharSet(set));
                }
                Err("unterminated quoted literal \"...\" in Lex rule".into())
            }
            b'[' => self.parse_char_class(),
            b'.' => {
                // In Lex, `.` matches any character except newline `\n`.
                self.pos += 1;
                let mut set = all_alphabet_bytes();
                set.remove(&b'\n');
                Ok(RegexAst::CharSet(set))
            }
            b'\\' => {
                self.pos += 1;
                let byte = self.decode_escape()?;
                let mut set = BTreeSet::new();
                set.insert(byte);
                Ok(RegexAst::CharSet(set))
            }
            literal => {
                self.pos += 1;
                let mut set = BTreeSet::new();
                set.insert(literal);
                Ok(RegexAst::CharSet(set))
            }
        }
    }

    fn parse_char_class(&mut self) -> Result<RegexAst, String> {
        self.pos += 1; // consume '['
        let negated = self.peek() == Some(b'^');
        if negated {
            self.pos += 1;
        }
        let mut members = BTreeSet::new();
        let mut prev: Option<u8> = None;
        let mut closed = false;

        while self.pos < self.bytes.len() {
            let ch = self.bytes[self.pos];
            if ch == b']' && (prev.is_some() || !members.is_empty()) {
                self.pos += 1;
                closed = true;
                break;
            }
            if ch == b'-'
                && prev.is_some()
                && self.pos + 1 < self.bytes.len()
                && self.bytes[self.pos + 1] != b']'
            {
                self.pos += 1; // consume '-'
                let end_byte = if self.bytes[self.pos] == b'\\' {
                    self.pos += 1;
                    self.decode_escape()?
                } else {
                    let b = self.bytes[self.pos];
                    self.pos += 1;
                    b
                };
                let start_byte = prev.unwrap();
                if start_byte > end_byte {
                    return Err(format!(
                        "inverted character range {}-{}",
                        start_byte as char, end_byte as char
                    ));
                }
                for b in start_byte..=end_byte {
                    members.insert(b);
                }
                prev = None;
                continue;
            }
            let byte = if ch == b'\\' {
                self.pos += 1;
                self.decode_escape()?
            } else {
                self.pos += 1;
                ch
            };
            members.insert(byte);
            prev = Some(byte);
        }

        if !closed {
            return Err("unclosed character class [...]".into());
        }
        if negated {
            let universe = all_alphabet_bytes();
            let comp: BTreeSet<u8> = universe.difference(&members).copied().collect();
            Ok(RegexAst::CharSet(comp))
        } else {
            Ok(RegexAst::CharSet(members))
        }
    }
}

/// Splits a `.l` rule line into `(pattern, action)` respecting quotes, brackets, and escapes.
fn split_rule_line(line: &str) -> Result<(&str, &str), String> {
    let bytes = line.as_bytes();
    let mut i = 0usize;
    let mut in_quotes = false;
    let mut in_class = false;

    while i < bytes.len() {
        let ch = bytes[i];
        if ch == b'\\' && i + 1 < bytes.len() {
            i += 2;
            continue;
        }
        if !in_class && ch == b'"' {
            in_quotes = !in_quotes;
            i += 1;
            continue;
        }
        if !in_quotes && ch == b'[' {
            in_class = true;
            i += 1;
            continue;
        }
        if in_class && ch == b']' {
            in_class = false;
            i += 1;
            continue;
        }
        if !in_quotes && !in_class && (ch == b' ' || ch == b'\t') {
            let pattern = &line[..i];
            let mut action = line[i..].trim();
            if action.starts_with('{') && action.ends_with('}') && action.len() >= 2 {
                action = action[1..action.len() - 1].trim();
            }
            if pattern.is_empty() || action.is_empty() {
                return Err(format!("invalid rule line: {line}"));
            }
            return Ok((pattern, action));
        }
        i += 1;
    }
    Err(format!("rule missing action: {line}"))
}

fn parse_lex_spec(src: &str) -> Result<LexSpec, String> {
    let mut sections: Vec<Vec<String>> = vec![Vec::new()];
    for raw_line in src.lines() {
        if raw_line.trim() == "%%" {
            sections.push(Vec::new());
            continue;
        }
        sections.last_mut().unwrap().push(raw_line.to_string());
    }
    if sections.len() < 2 {
        return Err("Lex specification requires at least one '%%' section divider".into());
    }

    let mut defs = BTreeMap::new();
    for line in &sections[0] {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with("//") || trimmed.starts_with('#') {
            continue;
        }
        let mut parts = trimmed.splitn(2, char::is_whitespace);
        let name = parts.next().unwrap_or("").trim();
        let rhs = parts.next().unwrap_or("").trim();
        if name.is_empty() || rhs.is_empty() {
            return Err(format!("invalid definition line: {trimmed}"));
        }
        defs.insert(name.to_string(), rhs.to_string());
    }

    let mut rules = Vec::new();
    for line in &sections[1] {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with("//") {
            continue;
        }
        let (raw_pat, action) = split_rule_line(trimmed)?;
        let mut visiting = BTreeSet::new();
        let expanded = expand_macros(raw_pat, &defs, &mut visiting)?;
        let (head_str, tail_opt) = split_trailing_context(&expanded)?;
        let head_ast = LexRegexParser::new(head_str).parse()?;
        let tail_ast = match tail_opt {
            Some(t) => Some(LexRegexParser::new(t).parse()?),
            None => None,
        };
        rules.push(LexRuleSpec {
            rule_id: rules.len(),
            raw_pattern: raw_pat.to_string(),
            action: action.to_string(),
            head_ast,
            tail_ast,
        });
    }
    if rules.is_empty() {
        return Err("Lex rules section contains zero rules".into());
    }

    let user_code = if sections.len() >= 3 {
        sections[2].join("\n")
    } else {
        String::new()
    };

    Ok(LexSpec {
        defs,
        rules,
        user_code,
    })
}

// ---------------------------------------------------------------------------
// 3. Combined NFA & Subset-Construction Multi-Pattern DFA
// ---------------------------------------------------------------------------

#[derive(Default)]
struct Nfa {
    eps: BTreeMap<usize, BTreeSet<usize>>,
    trans: BTreeMap<usize, Vec<(BTreeSet<u8>, usize)>>,
    accept_rule: BTreeMap<usize, usize>, // nfa_state -> rule_id
    next_state: usize,
}

impl Nfa {
    fn new_state(&mut self) -> usize {
        let s = self.next_state;
        self.next_state += 1;
        s
    }

    fn add_eps(&mut self, from: usize, to: usize) {
        self.eps.entry(from).or_default().insert(to);
    }

    fn build(&mut self, ast: &RegexAst) -> (usize, usize) {
        match ast {
            RegexAst::Empty => {
                let s = self.new_state();
                let e = self.new_state();
                self.add_eps(s, e);
                (s, e)
            }
            RegexAst::CharSet(set) => {
                let s = self.new_state();
                let e = self.new_state();
                self.trans.entry(s).or_default().push((set.clone(), e));
                (s, e)
            }
            RegexAst::Cat(a, b) => {
                let (s1, e1) = self.build(a);
                let (s2, e2) = self.build(b);
                self.add_eps(e1, s2);
                (s1, e2)
            }
            RegexAst::Alt(a, b) => {
                let s = self.new_state();
                let e = self.new_state();
                let (s1, e1) = self.build(a);
                let (s2, e2) = self.build(b);
                self.add_eps(s, s1);
                self.add_eps(s, s2);
                self.add_eps(e1, e);
                self.add_eps(e2, e);
                (s, e)
            }
            RegexAst::Star(a) => {
                let s = self.new_state();
                let e = self.new_state();
                let (s1, e1) = self.build(a);
                self.add_eps(s, s1);
                self.add_eps(s, e);
                self.add_eps(e1, s1);
                self.add_eps(e1, e);
                (s, e)
            }
        }
    }

    fn eps_closure(&self, seeds: &BTreeSet<usize>) -> BTreeSet<usize> {
        let mut closure = seeds.clone();
        let mut work: Vec<usize> = seeds.iter().copied().collect();
        while let Some(st) = work.pop() {
            if let Some(nexts) = self.eps.get(&st) {
                for &nx in nexts {
                    if closure.insert(nx) {
                        work.push(nx);
                    }
                }
            }
        }
        closure
    }

    fn move_byte(&self, states: &BTreeSet<usize>, byte: u8) -> BTreeSet<usize> {
        let mut out = BTreeSet::new();
        for &st in states {
            if let Some(edges) = self.trans.get(&st) {
                for (chars, target) in edges {
                    if chars.contains(&byte) {
                        out.insert(*target);
                    }
                }
            }
        }
        out
    }
}

#[derive(Clone, Debug)]
struct Dfa {
    start: usize,
    trans: Vec<BTreeMap<u8, usize>>,
    /// Earliest-listed accepting rule index (`0`-based) for each DFA state, if accepting.
    accept_rule: Vec<Option<usize>>,
}

impl Dfa {
    fn from_nfa(nfa: &Nfa, root: usize, alphabet: &BTreeSet<u8>) -> Self {
        let mut state_ids: BTreeMap<BTreeSet<usize>, usize> = BTreeMap::new();
        let mut queue = VecDeque::new();
        let mut trans = Vec::new();
        let mut accept_rule = Vec::new();

        let mut init_seed = BTreeSet::new();
        init_seed.insert(root);
        let init_set = nfa.eps_closure(&init_seed);
        state_ids.insert(init_set.clone(), 0);
        queue.push_back((0usize, init_set));
        trans.push(BTreeMap::new());
        accept_rule.push(None);

        while let Some((sid, cur_set)) = queue.pop_front() {
            // Lex tie-breaking: pick the lowest-numbered rule among accepting NFA states in this set.
            let mut winner: Option<usize> = None;
            for &ns in &cur_set {
                if let Some(&rid) = nfa.accept_rule.get(&ns) {
                    winner = Some(match winner {
                        Some(prev) => prev.min(rid),
                        None => rid,
                    });
                }
            }
            accept_rule[sid] = winner;

            for &byte in alphabet {
                let moved = nfa.move_byte(&cur_set, byte);
                if moved.is_empty() {
                    continue;
                }
                let next_set = nfa.eps_closure(&moved);
                let next_id = if let Some(&existing) = state_ids.get(&next_set) {
                    existing
                } else {
                    let nid = trans.len();
                    state_ids.insert(next_set.clone(), nid);
                    queue.push_back((nid, next_set));
                    trans.push(BTreeMap::new());
                    accept_rule.push(None);
                    nid
                };
                trans[sid].insert(byte, next_id);
            }
        }

        Self {
            start: 0,
            trans,
            accept_rule,
        }
    }

    fn matches_exact(&self, slice: &[u8]) -> bool {
        let mut st = self.start;
        for &b in slice {
            let Some(&next) = self.trans[st].get(&b) else {
                return false;
            };
            st = next;
        }
        self.accept_rule[st].is_some()
    }
}

#[derive(Debug)]
struct CompiledScanner {
    spec: LexSpec,
    nfa_state_count: usize,
    combined_dfa: Dfa,
    lookahead_dfas: BTreeMap<usize, (Dfa, Dfa)>, // rule_id -> (head_dfa, tail_dfa)
}

fn compile_single_regex_dfa(ast: &RegexAst, alphabet: &BTreeSet<u8>) -> Dfa {
    let mut nfa = Nfa::default();
    let root = nfa.new_state();
    let (s, e) = nfa.build(ast);
    nfa.add_eps(root, s);
    nfa.accept_rule.insert(e, 0);
    Dfa::from_nfa(&nfa, root, alphabet)
}

fn compile_lex_spec(spec_src: &str) -> Result<CompiledScanner, String> {
    let spec = parse_lex_spec(spec_src)?;
    let alphabet = all_alphabet_bytes();

    // Combine all rules under one common NFA start state `0`.
    let mut nfa = Nfa::default();
    let root = nfa.new_state();
    let mut lookahead_dfas = BTreeMap::new();

    for rule in &spec.rules {
        let full_ast = match &rule.tail_ast {
            Some(tail) => RegexAst::Cat(Box::new(rule.head_ast.clone()), Box::new(tail.clone())),
            None => rule.head_ast.clone(),
        };
        let (rs, re) = nfa.build(&full_ast);
        nfa.add_eps(root, rs);
        nfa.accept_rule.insert(re, rule.rule_id);

        if let Some(tail) = &rule.tail_ast {
            let head_dfa = compile_single_regex_dfa(&rule.head_ast, &alphabet);
            let tail_dfa = compile_single_regex_dfa(tail, &alphabet);
            lookahead_dfas.insert(rule.rule_id, (head_dfa, tail_dfa));
        }
    }

    let nfa_state_count = nfa.next_state;
    let combined_dfa = Dfa::from_nfa(&nfa, root, &alphabet);
    Ok(CompiledScanner {
        spec,
        nfa_state_count,
        combined_dfa,
        lookahead_dfas,
    })
}

// ---------------------------------------------------------------------------
// 4. Generated Scanner Driver (Maximal Munch + Priority + `/` Retract)
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq)]
struct EmittedToken {
    rule_id: usize,
    action: String,
    lexeme: String,
    line: usize,
}

impl fmt::Display for EmittedToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "line {}: {} ({:?}) [rule #{}]",
            self.line, self.action, self.lexeme, self.rule_id
        )
    }
}

fn scan_with_compiled(
    compiled: &CompiledScanner,
    input: &str,
) -> Result<Vec<EmittedToken>, String> {
    let bytes = input.as_bytes();
    let mut pos = 0usize;
    let mut line = 1usize;
    let mut tokens = Vec::new();

    while pos < bytes.len() {
        let mut st = compiled.combined_dfa.start;
        let mut cursor = pos;
        let mut best: Option<(usize, usize)> = None; // (end_pos, rule_id)

        while cursor < bytes.len() {
            let Some(&next) = compiled.combined_dfa.trans[st].get(&bytes[cursor]) else {
                break;
            };
            st = next;
            cursor += 1;
            if let Some(rid) = compiled.combined_dfa.accept_rule[st] {
                best = Some((cursor, rid));
            }
        }

        let Some((mut match_end, rule_id)) = best else {
            return Err(format!(
                "line {line}: lexical error at byte '{}'",
                bytes[pos] as char
            ));
        };

        // If the matched rule uses trailing context `r1/r2`, retract to the longest
        // prefix accepted by `r1` whose suffix to `match_end` is accepted by `r2`.
        if let Some((head_dfa, tail_dfa)) = compiled.lookahead_dfas.get(&rule_id) {
            let mut found_split = None;
            for split in (pos..=match_end).rev() {
                if head_dfa.matches_exact(&bytes[pos..split])
                    && tail_dfa.matches_exact(&bytes[split..match_end])
                {
                    found_split = Some(split);
                    break;
                }
            }
            match_end = found_split.ok_or_else(|| {
                format!("internal error: failed trailing-context split for rule #{rule_id}")
            })?;
            if match_end == pos {
                return Err(format!(
                    "line {line}: zero-length head match in trailing-context rule #{rule_id}"
                ));
            }
        }

        let lexeme = std::str::from_utf8(&bytes[pos..match_end])
            .map_err(|_| "invalid UTF-8 lexeme".to_string())?
            .to_string();
        let action = compiled.spec.rules[rule_id].action.clone();
        if action != "SKIP" {
            tokens.push(EmittedToken {
                rule_id,
                action,
                lexeme: lexeme.clone(),
                line,
            });
        }
        line += lexeme.bytes().filter(|&b| b == b'\n').count();
        pos = match_end;
    }

    Ok(tokens)
}

// ---------------------------------------------------------------------------
// 5. Worked Demo & Tests
// ---------------------------------------------------------------------------

const SAMPLE_LEX_SPEC: &str = r#"
digit   [0-9]
letter  [A-Za-z_]
id      {letter}({letter}|{digit})*
ws      [ \t\n]+

%%
{ws}                     { SKIP }
"while"                  { WHILE }
{id}                     { IDENT }
{digit}+/".."            { INT }
{digit}+"."{digit}*      { REAL }
{digit}+                 { INT }
".."                     { RANGE }
"+"                      { PLUS }
"="                      { ASSIGN }
";"                      { SEMI }
.                        { UNKNOWN }
%%
// User subroutine section preserved verbatim by the generator.
"#;

const SAMPLE_INPUT: &str = "while while1 = 1..4 + 12.5 + 3.;\n";

fn main() {
    let compiled = compile_lex_spec(SAMPLE_LEX_SPEC).expect("valid .l spec");
    println!(
        "Lex specification compiled: {} definitions, {} rules, {} NFA states -> {} combined DFA states",
        compiled.spec.defs.len(),
        compiled.spec.rules.len(),
        compiled.nfa_state_count,
        compiled.combined_dfa.trans.len()
    );
    println!("Rule table (priority = lowest rule index):");
    for r in &compiled.spec.rules {
        let la = if r.tail_ast.is_some() {
            " [trailing context /]"
        } else {
            ""
        };
        println!(
            "  #{} {:<22} -> {}{}",
            r.rule_id, r.raw_pattern, r.action, la
        );
    }
    println!("\nUser code footer: {}", compiled.spec.user_code.trim());

    let tokens = scan_with_compiled(&compiled, SAMPLE_INPUT).expect("valid input");
    println!("\nScanned tokens for {:?}:", SAMPLE_INPUT);
    for tok in &tokens {
        println!("  {tok}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn longest_match_rule_priority_and_trailing_context() {
        let compiled = compile_lex_spec(SAMPLE_LEX_SPEC).expect("spec compiles");
        let tokens = scan_with_compiled(&compiled, SAMPLE_INPUT).expect("input scans");
        let summary: Vec<(&str, &str)> = tokens
            .iter()
            .map(|t| (t.action.as_str(), t.lexeme.as_str()))
            .collect();
        assert_eq!(
            summary,
            vec![
                ("WHILE", "while"),  // `"while"` beats `{id}` on length-5 tie
                ("IDENT", "while1"), // `{id}` beats `"while"` via longest match (len 6 > 5)
                ("ASSIGN", "="),
                ("INT", "1"), // `{digit}+/".."` beats `{digit}+"."{digit}*` on `1..4` and retracts!
                ("RANGE", ".."),
                ("INT", "4"),
                ("PLUS", "+"),
                ("REAL", "12.5"),
                ("PLUS", "+"),
                ("REAL", "3."),
                ("SEMI", ";"),
            ]
        );
    }

    #[test]
    fn supports_negated_char_classes_and_rejects_bad_specs() {
        let str_spec = r#"
%%
\"[^"\n]*\"  { STRING }
[ \t]+       { SKIP }
%%
"#;
        let compiled = compile_lex_spec(str_spec).expect("compiles");
        let toks = scan_with_compiled(&compiled, "  \"hello world\"  ").expect("scans");
        assert_eq!(toks.len(), 1);
        assert_eq!(toks[0].action, "STRING");
        assert_eq!(toks[0].lexeme, "\"hello world\"");

        assert!(compile_lex_spec("no divider here")
            .unwrap_err()
            .contains("section divider"));
        assert!(compile_lex_spec("a {a}\n%%\n{a} { TOK }\n%%")
            .unwrap_err()
            .contains("cyclic macro"));
        assert!(compile_lex_spec("%%\na/b/c { TOK }\n%%")
            .unwrap_err()
            .contains("multiple trailing-context"));
    }
}
