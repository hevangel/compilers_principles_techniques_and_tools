//! Yacc-style `.y` LALR(1) parser-generator DSL compiler: parses declarations (`%token`,
//! `%left`, `%right`, `%nonassoc`, `%start`) and productions (`%prec`, `%empty`, `{ actions }`),
//! builds canonical LR(1) -> merged LALR(1) tables with Yacc conflict resolution, and runs the
//! generated parser + semantic action evaluator.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

// ---------------------------------------------------------------------------
// 1. `.y` Grammar, Precedence & Semantic Action AST
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Assoc {
    Left,
    Right,
    Neither,
}

impl fmt::Display for Assoc {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Assoc::Left => write!(f, "%left"),
            Assoc::Right => write!(f, "%right"),
            Assoc::Neither => write!(f, "%nonassoc"),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PrecEntry {
    level: usize,
    assoc: Assoc,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Symbol {
    Term(String),
    NonTerm(String),
}

impl fmt::Display for Symbol {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Symbol::Term(t) => {
                if t.len() == 1 {
                    write!(f, "'{t}'")
                } else {
                    write!(f, "{t}")
                }
            }
            Symbol::NonTerm(nt) => write!(f, "{nt}"),
        }
    }
}

#[derive(Clone, Debug)]
enum ActionExpr {
    Dollar(usize),
    IntLit(i64),
    Call { name: String, args: Vec<ActionExpr> },
}

#[derive(Clone, Debug)]
struct Production {
    id: usize,
    lhs: String,
    rhs: Vec<Symbol>,
    prec_override: Option<String>,
    action: ActionExpr,
}

#[derive(Clone, Debug)]
struct YaccSpec {
    start_sym: String,
    terminals: BTreeSet<String>,
    nonterminals: BTreeSet<String>,
    prec_table: BTreeMap<String, PrecEntry>,
    prods: Vec<Production>,
}

/// Strips `//` and `/* ... */` comments from a `.y` specification while preserving
/// quoted character literals `'...'` and line breaks.
fn strip_yacc_comments(src: &str) -> Result<String, String> {
    let bytes = src.as_bytes();
    let mut out = String::with_capacity(src.len());
    let mut i = 0usize;
    let mut in_sq = false;

    while i < bytes.len() {
        let ch = bytes[i];
        if in_sq {
            out.push(ch as char);
            if ch == b'\\' && i + 1 < bytes.len() {
                i += 1;
                out.push(bytes[i] as char);
            } else if ch == b'\'' {
                in_sq = false;
            }
            i += 1;
            continue;
        }
        if ch == b'\'' {
            in_sq = true;
            out.push('\'');
            i += 1;
            continue;
        }
        if ch == b'/' && bytes.get(i + 1) == Some(&b'/') {
            i += 2;
            while i < bytes.len() && bytes[i] != b'\n' {
                i += 1;
            }
            continue;
        }
        if ch == b'/' && bytes.get(i + 1) == Some(&b'*') {
            i += 2;
            let mut closed = false;
            while i + 1 < bytes.len() {
                if bytes[i] == b'*' && bytes[i + 1] == b'/' {
                    i += 2;
                    closed = true;
                    break;
                }
                if bytes[i] == b'\n' {
                    out.push('\n');
                }
                i += 1;
            }
            if !closed {
                return Err("unterminated /* ... */ comment in .y spec".into());
            }
            continue;
        }
        out.push(ch as char);
        i += 1;
    }
    Ok(out)
}

/// Parses `{ add($1, $3) }`, `{ $2 }`, `{ 0 }` semantic action bodies into `ActionExpr`.
struct ActionParser<'a> {
    bytes: &'a [u8],
    pos: usize,
    rhs_len: usize,
}

impl<'a> ActionParser<'a> {
    fn new(src: &'a str, rhs_len: usize) -> Self {
        Self {
            bytes: src.as_bytes(),
            pos: 0,
            rhs_len,
        }
    }

    fn skip_ws(&mut self) {
        while self.pos < self.bytes.len() && self.bytes[self.pos].is_ascii_whitespace() {
            self.pos += 1;
        }
    }

    fn parse(&mut self) -> Result<ActionExpr, String> {
        self.skip_ws();
        let expr = self.parse_expr()?;
        self.skip_ws();
        if self.pos < self.bytes.len() {
            return Err(format!(
                "unexpected trailing token '{}' in semantic action",
                self.bytes[self.pos] as char
            ));
        }
        Ok(expr)
    }

    fn parse_expr(&mut self) -> Result<ActionExpr, String> {
        self.skip_ws();
        let Some(&ch) = self.bytes.get(self.pos) else {
            return Err("empty semantic action".into());
        };
        if ch == b'$' {
            self.pos += 1;
            let start = self.pos;
            while self.pos < self.bytes.len() && self.bytes[self.pos].is_ascii_digit() {
                self.pos += 1;
            }
            if self.pos == start {
                return Err("expected positional index after '$' in action".into());
            }
            let idx: usize = std::str::from_utf8(&self.bytes[start..self.pos])
                .unwrap()
                .parse()
                .map_err(|_| "invalid $N index".to_string())?;
            if idx == 0 || idx > self.rhs_len {
                return Err(format!(
                    "semantic action reference ${idx} out of bounds for rule with {} symbols",
                    self.rhs_len
                ));
            }
            return Ok(ActionExpr::Dollar(idx));
        }
        if ch == b'-' || ch.is_ascii_digit() {
            let start = self.pos;
            if ch == b'-' {
                self.pos += 1;
            }
            let dstart = self.pos;
            while self.pos < self.bytes.len() && self.bytes[self.pos].is_ascii_digit() {
                self.pos += 1;
            }
            if self.pos == dstart {
                return Err("invalid integer literal in semantic action".into());
            }
            let val: i64 = std::str::from_utf8(&self.bytes[start..self.pos])
                .unwrap()
                .parse()
                .map_err(|_| "integer literal overflow in action".to_string())?;
            return Ok(ActionExpr::IntLit(val));
        }
        if ch.is_ascii_alphabetic() || ch == b'_' {
            let start = self.pos;
            while self.pos < self.bytes.len()
                && (self.bytes[self.pos].is_ascii_alphanumeric() || self.bytes[self.pos] == b'_')
            {
                self.pos += 1;
            }
            let name = std::str::from_utf8(&self.bytes[start..self.pos])
                .unwrap()
                .to_string();
            self.skip_ws();
            if self.bytes.get(self.pos) != Some(&b'(') {
                return Err(format!("expected '(' after action helper '{name}'"));
            }
            self.pos += 1;
            let mut args = Vec::new();
            self.skip_ws();
            if self.bytes.get(self.pos) != Some(&b')') {
                loop {
                    args.push(self.parse_expr()?);
                    self.skip_ws();
                    match self.bytes.get(self.pos) {
                        Some(&b',') => self.pos += 1,
                        Some(&b')') => break,
                        _ => return Err(format!("expected ',' or ')' in call '{name}(...)'")),
                    }
                }
            }
            self.pos += 1;
            return Ok(ActionExpr::Call { name, args });
        }
        Err(format!("unexpected character '{}' in action", ch as char))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum RuleTok {
    Ident(String),
    CharLit(String),
    Colon,
    Pipe,
    Semi,
    Prec,
    EmptyKw,
    Action(String),
}

fn lex_rules_section(src: &str) -> Result<Vec<RuleTok>, String> {
    let bytes = src.as_bytes();
    let mut i = 0usize;
    let mut out = Vec::new();

    while i < bytes.len() {
        let ch = bytes[i];
        if ch.is_ascii_whitespace() {
            i += 1;
            continue;
        }
        match ch {
            b':' => {
                out.push(RuleTok::Colon);
                i += 1;
            }
            b'|' => {
                out.push(RuleTok::Pipe);
                i += 1;
            }
            b';' => {
                out.push(RuleTok::Semi);
                i += 1;
            }
            b'\'' => {
                i += 1;
                if i >= bytes.len() {
                    return Err("unterminated character token in rules".into());
                }
                let lit = if bytes[i] == b'\\' {
                    i += 1;
                    let esc = *bytes.get(i).ok_or("dangling escape in char token")?;
                    i += 1;
                    esc as char
                } else {
                    let c = bytes[i] as char;
                    i += 1;
                    c
                };
                if bytes.get(i) != Some(&b'\'') {
                    return Err("expected closing '\\'' in single-char token".into());
                }
                i += 1;
                out.push(RuleTok::CharLit(lit.to_string()));
            }
            b'{' => {
                i += 1;
                let start = i;
                let mut depth = 1usize;
                while i < bytes.len() && depth > 0 {
                    if bytes[i] == b'{' {
                        depth += 1;
                    } else if bytes[i] == b'}' {
                        depth -= 1;
                    }
                    if depth > 0 {
                        i += 1;
                    }
                }
                if depth != 0 {
                    return Err("unclosed '{ ... }' semantic action in rules".into());
                }
                let body = std::str::from_utf8(&bytes[start..i])
                    .unwrap()
                    .trim()
                    .to_string();
                i += 1; // consume '}'
                out.push(RuleTok::Action(body));
            }
            b'%' => {
                let start = i;
                i += 1;
                while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                    i += 1;
                }
                let dir = std::str::from_utf8(&bytes[start..i]).unwrap();
                match dir {
                    "%prec" => out.push(RuleTok::Prec),
                    "%empty" => out.push(RuleTok::EmptyKw),
                    other => return Err(format!("unexpected directive in rules section: {other}")),
                }
            }
            c if c.is_ascii_alphabetic() || c == b'_' => {
                let start = i;
                i += 1;
                while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                    i += 1;
                }
                let word = std::str::from_utf8(&bytes[start..i]).unwrap().to_string();
                out.push(RuleTok::Ident(word));
            }
            other => {
                return Err(format!(
                    "unexpected character '{}' in .y rules section",
                    other as char
                ));
            }
        }
    }
    Ok(out)
}

fn parse_decl_token(tok: &str) -> Result<(String, bool), String> {
    if tok.starts_with('\'') && tok.ends_with('\'') && tok.len() >= 3 {
        let inner = &tok[1..tok.len() - 1];
        if inner.chars().count() != 1 {
            return Err(format!("multi-char literal token unsupported: {tok}"));
        }
        return Ok((inner.to_string(), true));
    }
    Ok((tok.to_string(), false))
}

fn parse_yacc_spec(raw_src: &str) -> Result<YaccSpec, String> {
    let cleaned = strip_yacc_comments(raw_src)?;
    let mut sections: Vec<Vec<String>> = vec![Vec::new()];
    for line in cleaned.lines() {
        if line.trim() == "%%" {
            sections.push(Vec::new());
        } else {
            sections.last_mut().unwrap().push(line.to_string());
        }
    }
    if sections.len() < 2 {
        return Err("Yacc specification requires at least one '%%' section divider".into());
    }

    let mut declared_tokens = BTreeSet::new();
    let mut prec_table = BTreeMap::new();
    let mut start_opt: Option<String> = None;
    let mut prec_level = 0usize;

    let decl_text = sections[0].join("\n");
    let words: Vec<&str> = decl_text.split_whitespace().collect();
    let mut idx = 0usize;
    while idx < words.len() {
        let dir = words[idx];
        idx += 1;
        match dir {
            "%token" => {
                while idx < words.len() && !words[idx].starts_with('%') {
                    let (name, _) = parse_decl_token(words[idx])?;
                    declared_tokens.insert(name);
                    idx += 1;
                }
            }
            "%left" | "%right" | "%nonassoc" => {
                prec_level += 1;
                let assoc = match dir {
                    "%left" => Assoc::Left,
                    "%right" => Assoc::Right,
                    _ => Assoc::Neither,
                };
                while idx < words.len() && !words[idx].starts_with('%') {
                    let (name, _) = parse_decl_token(words[idx])?;
                    declared_tokens.insert(name.clone());
                    prec_table.insert(
                        name,
                        PrecEntry {
                            level: prec_level,
                            assoc,
                        },
                    );
                    idx += 1;
                }
            }
            "%start" => {
                if idx >= words.len() || words[idx].starts_with('%') {
                    return Err("expected symbol name after %start".into());
                }
                start_opt = Some(words[idx].to_string());
                idx += 1;
            }
            other => {
                return Err(format!("unknown Yacc declaration directive: {other}"));
            }
        }
    }

    struct RawProd {
        lhs: String,
        rhs_names: Vec<String>,
        prec_override: Option<String>,
        action_opt: Option<String>,
    }

    let rules_text = sections[1].join("\n");
    let rtoks = lex_rules_section(&rules_text)?;
    let mut raw_prods: Vec<RawProd> = Vec::new();
    let mut nonterminals = BTreeSet::new();
    let mut terminals = declared_tokens.clone();

    let mut p = 0usize;
    let mut first_lhs: Option<String> = None;
    while p < rtoks.len() {
        let RuleTok::Ident(lhs) = &rtoks[p] else {
            return Err("expected nonterminal LHS at start of rule group".into());
        };
        let lhs = lhs.clone();
        if first_lhs.is_none() {
            first_lhs = Some(lhs.clone());
        }
        nonterminals.insert(lhs.clone());
        p += 1;
        if rtoks.get(p) != Some(&RuleTok::Colon) {
            return Err(format!("expected ':' after LHS '{lhs}'"));
        }
        p += 1;

        loop {
            let mut rhs_names: Vec<String> = Vec::new();
            let mut prec_override: Option<String> = None;
            let mut action_opt: Option<String> = None;

            while p < rtoks.len() {
                match &rtoks[p] {
                    RuleTok::Pipe | RuleTok::Semi => break,
                    RuleTok::EmptyKw => {
                        p += 1;
                    }
                    RuleTok::Ident(sym) => {
                        rhs_names.push(sym.clone());
                        p += 1;
                    }
                    RuleTok::CharLit(ch) => {
                        terminals.insert(ch.clone());
                        rhs_names.push(ch.clone());
                        p += 1;
                    }
                    RuleTok::Prec => {
                        p += 1;
                        let tok_name = match rtoks.get(p) {
                            Some(RuleTok::Ident(s)) => s.clone(),
                            Some(RuleTok::CharLit(c)) => {
                                terminals.insert(c.clone());
                                c.clone()
                            }
                            _ => return Err("expected terminal after %prec".into()),
                        };
                        prec_override = Some(tok_name);
                        p += 1;
                    }
                    RuleTok::Action(body) => {
                        action_opt = Some(body.clone());
                        p += 1;
                    }
                    RuleTok::Colon => {
                        return Err(format!(
                            "unexpected ':' inside rule for '{lhs}' (missing ';')"
                        ))
                    }
                }
            }

            raw_prods.push(RawProd {
                lhs: lhs.clone(),
                rhs_names,
                prec_override,
                action_opt,
            });

            match rtoks.get(p) {
                Some(RuleTok::Pipe) => p += 1,
                Some(RuleTok::Semi) => {
                    p += 1;
                    break;
                }
                _ => return Err(format!("expected '|' or ';' closing rule '{lhs}'")),
            }
        }
    }

    if raw_prods.is_empty() {
        return Err("Yacc rules section has no productions".into());
    }

    let start_sym = start_opt.unwrap_or_else(|| first_lhs.unwrap());
    if !nonterminals.contains(&start_sym) {
        return Err(format!("start symbol '{start_sym}' has no defining rule"));
    }

    let mut prods = Vec::with_capacity(raw_prods.len());
    for (id, rp) in raw_prods.into_iter().enumerate() {
        let mut rhs = Vec::with_capacity(rp.rhs_names.len());
        for sym in rp.rhs_names {
            if nonterminals.contains(&sym) {
                rhs.push(Symbol::NonTerm(sym));
            } else {
                terminals.insert(sym.clone());
                rhs.push(Symbol::Term(sym));
            }
        }
        let raw_action = rp.action_opt.unwrap_or_else(|| {
            if rhs.is_empty() {
                "0".into()
            } else {
                "$1".into()
            }
        });
        let action = ActionParser::new(&raw_action, rhs.len()).parse()?;
        prods.push(Production {
            id,
            lhs: rp.lhs,
            rhs,
            prec_override: rp.prec_override,
            action,
        });
    }

    Ok(YaccSpec {
        start_sym,
        terminals,
        nonterminals,
        prec_table,
        prods,
    })
}

// ---------------------------------------------------------------------------
// 2. Canonical LR(1) -> Merged LALR(1) + Precedence Conflict Resolution
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Lr1Item {
    prod_id: usize,
    dot: usize,
    lookahead: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Action {
    Shift(usize),
    Reduce(usize),
    Accept,
    NonAssocError,
}

impl fmt::Display for Action {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Action::Shift(s) => write!(f, "shift {s}"),
            Action::Reduce(p) => write!(f, "reduce r{p}"),
            Action::Accept => write!(f, "accept"),
            Action::NonAssocError => write!(f, "error (nonassoc)"),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum ConflictKind {
    ResolvedSr,
    UnresolvedSr,
    UnresolvedRr,
}

#[derive(Clone, Debug)]
struct ConflictRecord {
    kind: ConflictKind,
    state: usize,
    term: String,
    detail: String,
}

struct CompiledParser {
    spec: YaccSpec,
    /// Production 0 is augmented `$accept -> start_sym`.
    prods: Vec<Production>,
    lr1_state_count: usize,
    lalr_state_count: usize,
    action_table: BTreeMap<(usize, String), Action>,
    goto_table: BTreeMap<(usize, String), usize>,
    conflicts: Vec<ConflictRecord>,
}

const EOF_SYM: &str = "$";
const AUG_START: &str = "$accept";

fn compute_first_sets(
    prods: &[Production],
    terminals: &BTreeSet<String>,
    nonterminals: &BTreeSet<String>,
) -> BTreeMap<String, (BTreeSet<String>, bool)> {
    let mut first: BTreeMap<String, (BTreeSet<String>, bool)> = BTreeMap::new();
    for t in terminals {
        let mut s = BTreeSet::new();
        s.insert(t.clone());
        first.insert(t.clone(), (s, false));
    }
    first.insert(
        EOF_SYM.to_string(),
        (BTreeSet::from([EOF_SYM.to_string()]), false),
    );
    for nt in nonterminals {
        first.insert(nt.clone(), (BTreeSet::new(), false));
    }
    first.insert(AUG_START.to_string(), (BTreeSet::new(), false));

    loop {
        let mut changed = false;
        for prod in prods {
            let mut all_null = true;
            let mut to_add = BTreeSet::new();
            for sym in &prod.rhs {
                let key = match sym {
                    Symbol::Term(t) | Symbol::NonTerm(t) => t,
                };
                let (fset, nullable) = first.get(key).cloned().unwrap_or_default();
                to_add.extend(fset);
                if !nullable {
                    all_null = false;
                    break;
                }
            }
            let entry = first.get_mut(&prod.lhs).unwrap();
            let before_len = entry.0.len();
            entry.0.extend(to_add);
            if entry.0.len() > before_len {
                changed = true;
            }
            if all_null && !entry.1 {
                entry.1 = true;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    first
}

fn first_of_seq(
    seq: &[Symbol],
    trail: &str,
    first: &BTreeMap<String, (BTreeSet<String>, bool)>,
) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let mut all_null = true;
    for sym in seq {
        let key = match sym {
            Symbol::Term(t) | Symbol::NonTerm(t) => t,
        };
        if let Some((fset, nullable)) = first.get(key) {
            out.extend(fset.iter().cloned());
            if !nullable {
                all_null = false;
                break;
            }
        }
    }
    if all_null {
        out.insert(trail.to_string());
    }
    out
}

fn closure_lr1(
    seed: BTreeSet<Lr1Item>,
    prods: &[Production],
    first: &BTreeMap<String, (BTreeSet<String>, bool)>,
) -> BTreeSet<Lr1Item> {
    let mut items = seed;
    let mut work: Vec<Lr1Item> = items.iter().cloned().collect();
    while let Some(it) = work.pop() {
        let prod = &prods[it.prod_id];
        if let Some(Symbol::NonTerm(nt)) = prod.rhs.get(it.dot) {
            let lookaheads = first_of_seq(&prod.rhs[it.dot + 1..], &it.lookahead, first);
            for p in prods {
                if &p.lhs == nt {
                    for la in &lookaheads {
                        let candidate = Lr1Item {
                            prod_id: p.id,
                            dot: 0,
                            lookahead: la.clone(),
                        };
                        if items.insert(candidate.clone()) {
                            work.push(candidate);
                        }
                    }
                }
            }
        }
    }
    items
}

fn goto_lr1(
    items: &BTreeSet<Lr1Item>,
    sym: &Symbol,
    prods: &[Production],
    first: &BTreeMap<String, (BTreeSet<String>, bool)>,
) -> BTreeSet<Lr1Item> {
    let mut seed = BTreeSet::new();
    for it in items {
        if prods[it.prod_id].rhs.get(it.dot) == Some(sym) {
            seed.insert(Lr1Item {
                prod_id: it.prod_id,
                dot: it.dot + 1,
                lookahead: it.lookahead.clone(),
            });
        }
    }
    if seed.is_empty() {
        seed
    } else {
        closure_lr1(seed, prods, first)
    }
}

/// Computes Yacc's effective precedence for a production: `%prec TOK` overrides,
/// otherwise the precedence of the rightmost terminal appearing in `prod.rhs`.
fn production_prec(
    prod: &Production,
    prec_table: &BTreeMap<String, PrecEntry>,
) -> Option<PrecEntry> {
    if let Some(ov) = &prod.prec_override {
        return prec_table.get(ov).copied();
    }
    let mut last_term = None;
    for sym in &prod.rhs {
        if let Symbol::Term(t) = sym {
            last_term = Some(t);
        }
    }
    last_term.and_then(|t| prec_table.get(t).copied())
}

fn compile_yacc_spec(raw_src: &str) -> Result<CompiledParser, String> {
    let spec = parse_yacc_spec(raw_src)?;

    // Build augmented production list: id 0 = `$accept -> start_sym`, user rules shifted by +1.
    let mut prods = Vec::with_capacity(spec.prods.len() + 1);
    prods.push(Production {
        id: 0,
        lhs: AUG_START.to_string(),
        rhs: vec![Symbol::NonTerm(spec.start_sym.clone())],
        prec_override: None,
        action: ActionExpr::Dollar(1),
    });
    for p in &spec.prods {
        let mut shifted = p.clone();
        shifted.id = p.id + 1;
        prods.push(shifted);
    }

    let first = compute_first_sets(&prods, &spec.terminals, &spec.nonterminals);
    let mut all_symbols: Vec<Symbol> = Vec::new();
    for t in &spec.terminals {
        all_symbols.push(Symbol::Term(t.clone()));
    }
    for nt in &spec.nonterminals {
        all_symbols.push(Symbol::NonTerm(nt.clone()));
    }

    // Step A: Build canonical LR(1) state collection.
    let init_items = closure_lr1(
        BTreeSet::from([Lr1Item {
            prod_id: 0,
            dot: 0,
            lookahead: EOF_SYM.to_string(),
        }]),
        &prods,
        &first,
    );
    let mut lr1_states: Vec<BTreeSet<Lr1Item>> = vec![init_items.clone()];
    let mut lr1_index: BTreeMap<BTreeSet<Lr1Item>, usize> = BTreeMap::from([(init_items, 0)]);
    let mut lr1_edges: BTreeMap<(usize, Symbol), usize> = BTreeMap::new();

    let mut q_idx = 0usize;
    while q_idx < lr1_states.len() {
        let cur = lr1_states[q_idx].clone();
        for sym in &all_symbols {
            let nxt = goto_lr1(&cur, sym, &prods, &first);
            if nxt.is_empty() {
                continue;
            }
            let target_id = if let Some(&eid) = lr1_index.get(&nxt) {
                eid
            } else {
                let nid = lr1_states.len();
                lr1_states.push(nxt.clone());
                lr1_index.insert(nxt, nid);
                nid
            };
            lr1_edges.insert((q_idx, sym.clone()), target_id);
        }
        q_idx += 1;
    }

    // Step B: Merge LR(1) states sharing the same LR(0) core -> LALR(1) states.
    let mut core_to_lalr: BTreeMap<BTreeSet<(usize, usize)>, usize> = BTreeMap::new();
    let mut lr1_to_lalr: Vec<usize> = vec![0; lr1_states.len()];
    let mut lalr_states: Vec<BTreeSet<Lr1Item>> = Vec::new();

    for (sid, st) in lr1_states.iter().enumerate() {
        let core: BTreeSet<(usize, usize)> = st.iter().map(|it| (it.prod_id, it.dot)).collect();
        let lid = if let Some(&existing) = core_to_lalr.get(&core) {
            existing
        } else {
            let nid = lalr_states.len();
            core_to_lalr.insert(core, nid);
            lalr_states.push(BTreeSet::new());
            nid
        };
        lr1_to_lalr[sid] = lid;
        lalr_states[lid].extend(st.iter().cloned());
    }

    let mut lalr_edges: BTreeMap<(usize, Symbol), usize> = BTreeMap::new();
    for ((src, sym), dst) in &lr1_edges {
        lalr_edges.insert((lr1_to_lalr[*src], sym.clone()), lr1_to_lalr[*dst]);
    }

    // Step C: Populate ACTION and GOTO tables with Yacc precedence + rule-order resolution.
    let mut action_table: BTreeMap<(usize, String), Action> = BTreeMap::new();
    let mut goto_table: BTreeMap<(usize, String), usize> = BTreeMap::new();
    let mut conflicts: Vec<ConflictRecord> = Vec::new();

    for ((src, sym), dst) in &lalr_edges {
        if let Symbol::NonTerm(nt) = sym {
            goto_table.insert((*src, nt.clone()), *dst);
        }
    }

    let mut all_terms: BTreeSet<String> = spec.terminals.clone();
    all_terms.insert(EOF_SYM.to_string());

    for (state_id, st) in lalr_states.iter().enumerate() {
        for term in &all_terms {
            let shift_target = if term != EOF_SYM {
                lalr_edges
                    .get(&(state_id, Symbol::Term(term.clone())))
                    .copied()
            } else {
                None
            };
            let mut reduces: Vec<usize> = Vec::new();
            let mut has_accept = false;
            for it in st {
                let prod = &prods[it.prod_id];
                if it.dot == prod.rhs.len() && &it.lookahead == term {
                    if it.prod_id == 0 {
                        has_accept = true;
                    } else {
                        reduces.push(it.prod_id);
                    }
                }
            }
            reduces.sort_unstable();
            reduces.dedup();

            let mut chosen: Option<Action> = None;
            if has_accept && reduces.is_empty() && shift_target.is_none() {
                chosen = Some(Action::Accept);
            } else if !reduces.is_empty() {
                let chosen_red = reduces[0];
                if reduces.len() > 1 {
                    for &other in &reduces[1..] {
                        conflicts.push(ConflictRecord {
                            kind: ConflictKind::UnresolvedRr,
                            state: state_id,
                            term: term.clone(),
                            detail: format!(
                                "reduce r{chosen_red} ({}) vs reduce r{other} ({}) -> picked earlier rule r{chosen_red}",
                                prods[chosen_red].lhs, prods[other].lhs
                            ),
                        });
                    }
                }
                if let Some(shift_s) = shift_target {
                    let r_prec = production_prec(&prods[chosen_red], &spec.prec_table);
                    let t_prec = spec.prec_table.get(term).copied();
                    match (r_prec, t_prec) {
                        (Some(rp), Some(tp)) => {
                            let (act, why) = if rp.level > tp.level {
                                (
                                    Action::Reduce(chosen_red),
                                    format!(
                                        "rule r{chosen_red} prec {} > '{}' prec {} -> reduce",
                                        rp.level, term, tp.level
                                    ),
                                )
                            } else if rp.level < tp.level {
                                (
                                    Action::Shift(shift_s),
                                    format!(
                                        "rule r{chosen_red} prec {} < '{}' prec {} -> shift {shift_s}",
                                        rp.level, term, tp.level
                                    ),
                                )
                            } else {
                                match tp.assoc {
                                    Assoc::Left => (
                                        Action::Reduce(chosen_red),
                                        format!("equal prec {} (%left) -> reduce", rp.level),
                                    ),
                                    Assoc::Right => (
                                        Action::Shift(shift_s),
                                        format!(
                                            "equal prec {} (%right) -> shift {shift_s}",
                                            rp.level
                                        ),
                                    ),
                                    Assoc::Neither => (
                                        Action::NonAssocError,
                                        format!("equal prec {} (%nonassoc) -> error", rp.level),
                                    ),
                                }
                            };
                            conflicts.push(ConflictRecord {
                                kind: ConflictKind::ResolvedSr,
                                state: state_id,
                                term: term.clone(),
                                detail: why,
                            });
                            chosen = Some(act);
                        }
                        _ => {
                            conflicts.push(ConflictRecord {
                                kind: ConflictKind::UnresolvedSr,
                                state: state_id,
                                term: term.clone(),
                                detail: format!(
                                    "shift {shift_s} vs reduce r{chosen_red} (no prec pair) -> default shift {shift_s}"
                                ),
                            });
                            chosen = Some(Action::Shift(shift_s));
                        }
                    }
                } else {
                    chosen = Some(Action::Reduce(chosen_red));
                }
            } else if let Some(shift_s) = shift_target {
                chosen = Some(Action::Shift(shift_s));
            }

            if let Some(act) = chosen {
                action_table.insert((state_id, term.clone()), act);
            }
        }
    }

    Ok(CompiledParser {
        spec,
        prods,
        lr1_state_count: lr1_states.len(),
        lalr_state_count: lalr_states.len(),
        action_table,
        goto_table,
        conflicts,
    })
}

// ---------------------------------------------------------------------------
// 3. Generated Parser Driver & Semantic Action Evaluator
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq)]
struct InToken {
    kind: String,
    lexeme: String,
}

fn tokenize_program(src: &str, declared_terms: &BTreeSet<String>) -> Result<Vec<InToken>, String> {
    let bytes = src.as_bytes();
    let mut i = 0usize;
    let mut out = Vec::new();
    while i < bytes.len() {
        let ch = bytes[i];
        if ch.is_ascii_whitespace() {
            i += 1;
            continue;
        }
        if ch.is_ascii_digit() {
            let start = i;
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                i += 1;
            }
            out.push(InToken {
                kind: "NUM".into(),
                lexeme: std::str::from_utf8(&bytes[start..i]).unwrap().to_string(),
            });
            continue;
        }
        if ch.is_ascii_alphabetic() || ch == b'_' {
            let start = i;
            while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                i += 1;
            }
            let word = std::str::from_utf8(&bytes[start..i]).unwrap().to_string();
            let kind = if declared_terms.contains(&word) {
                word.clone()
            } else {
                "ID".into()
            };
            out.push(InToken { kind, lexeme: word });
            continue;
        }
        let s = (ch as char).to_string();
        if declared_terms.contains(&s) {
            out.push(InToken {
                kind: s.clone(),
                lexeme: s,
            });
            i += 1;
            continue;
        }
        return Err(format!("unexpected input character '{}'", ch as char));
    }
    out.push(InToken {
        kind: EOF_SYM.into(),
        lexeme: EOF_SYM.into(),
    });
    Ok(out)
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum SemVal {
    Int(i64),
    Str(String),
}

impl SemVal {
    fn as_int(&self, env: &BTreeMap<String, i64>) -> Result<i64, String> {
        match self {
            SemVal::Int(v) => Ok(*v),
            SemVal::Str(name) => env
                .get(name)
                .copied()
                .ok_or_else(|| format!("undefined identifier '{name}'")),
        }
    }

    fn as_str(&self) -> Result<&str, String> {
        match self {
            SemVal::Str(s) => Ok(s),
            SemVal::Int(n) => Err(format!("expected identifier name, found integer {n}")),
        }
    }
}

fn eval_action(
    expr: &ActionExpr,
    args: &[SemVal],
    env: &mut BTreeMap<String, i64>,
) -> Result<SemVal, String> {
    match expr {
        ActionExpr::Dollar(idx) => Ok(args[*idx - 1].clone()),
        ActionExpr::IntLit(v) => Ok(SemVal::Int(*v)),
        ActionExpr::Call {
            name,
            args: call_args,
        } => {
            let mut evaluated = Vec::with_capacity(call_args.len());
            for a in call_args {
                evaluated.push(eval_action(a, args, env)?);
            }
            match (name.as_str(), evaluated.as_slice()) {
                ("num", [SemVal::Int(n)]) => Ok(SemVal::Int(*n)),
                ("var", [SemVal::Str(s)]) => {
                    let v = env
                        .get(s)
                        .copied()
                        .ok_or_else(|| format!("undefined variable '{s}'"))?;
                    Ok(SemVal::Int(v))
                }
                ("neg", [x]) => Ok(SemVal::Int(-x.as_int(env)?)),
                ("add", [a, b]) => Ok(SemVal::Int(a.as_int(env)? + b.as_int(env)?)),
                ("sub", [a, b]) => Ok(SemVal::Int(a.as_int(env)? - b.as_int(env)?)),
                ("mul", [a, b]) => Ok(SemVal::Int(a.as_int(env)? * b.as_int(env)?)),
                ("div", [a, b]) => {
                    let rhs = b.as_int(env)?;
                    if rhs == 0 {
                        return Err("division by zero in semantic action".into());
                    }
                    Ok(SemVal::Int(a.as_int(env)? / rhs))
                }
                ("lt", [a, b]) => Ok(SemVal::Int(if a.as_int(env)? < b.as_int(env)? {
                    1
                } else {
                    0
                })),
                ("assign", [id_v, rhs]) => {
                    let key = id_v.as_str()?.to_string();
                    let val = rhs.as_int(env)?;
                    env.insert(key, val);
                    Ok(SemVal::Int(val))
                }
                ("seq", [_, last]) => Ok(last.clone()),
                _ => Err(format!(
                    "unknown semantic action call '{name}' with {} args",
                    evaluated.len()
                )),
            }
        }
    }
}

#[derive(Debug)]
struct ParseOutcome {
    value: SemVal,
    env: BTreeMap<String, i64>,
    reductions: usize,
}

fn parse_with_yacc(compiled: &CompiledParser, input: &str) -> Result<ParseOutcome, String> {
    let tokens = tokenize_program(input, &compiled.spec.terminals)?;
    let mut state_stack: Vec<usize> = vec![0];
    let mut val_stack: Vec<SemVal> = Vec::new();
    let mut cursor = 0usize;
    let mut env = BTreeMap::new();
    let mut reductions = 0usize;

    loop {
        let state = *state_stack.last().unwrap();
        let tok = &tokens[cursor];
        let Some(act) = compiled.action_table.get(&(state, tok.kind.clone())) else {
            let expected: Vec<String> = compiled
                .action_table
                .keys()
                .filter(|(s, _)| *s == state)
                .map(|(_, t)| t.clone())
                .collect();
            return Err(format!(
                "syntax error at token '{}' ('{}') in state {state}; expected one of {:?}",
                tok.lexeme, tok.kind, expected
            ));
        };

        match act {
            Action::Shift(next_st) => {
                let sv = if tok.kind == "NUM" {
                    SemVal::Int(tok.lexeme.parse::<i64>().unwrap())
                } else {
                    SemVal::Str(tok.lexeme.clone())
                };
                val_stack.push(sv);
                state_stack.push(*next_st);
                cursor += 1;
            }
            Action::Reduce(prod_id) => {
                let prod = &compiled.prods[*prod_id];
                let k = prod.rhs.len();
                let split_at = val_stack.len() - k;
                let rhs_vals: Vec<SemVal> = val_stack.drain(split_at..).collect();
                state_stack.truncate(state_stack.len() - k);

                let res = eval_action(&prod.action, &rhs_vals, &mut env)?;
                let top_state = *state_stack.last().unwrap();
                let goto_st = compiled
                    .goto_table
                    .get(&(top_state, prod.lhs.clone()))
                    .copied()
                    .ok_or_else(|| {
                        format!(
                            "missing GOTO for nonterminal '{}' from state {top_state}",
                            prod.lhs
                        )
                    })?;
                val_stack.push(res);
                state_stack.push(goto_st);
                reductions += 1;
            }
            Action::Accept => {
                let final_val = val_stack.pop().unwrap_or(SemVal::Int(0));
                return Ok(ParseOutcome {
                    value: final_val,
                    env,
                    reductions,
                });
            }
            Action::NonAssocError => {
                return Err(format!(
                    "syntax error: non-associative operator '{}' cannot be chained",
                    tok.lexeme
                ));
            }
        }
    }
}

// ---------------------------------------------------------------------------
// 4. Worked Demo & Tests
// ---------------------------------------------------------------------------

const SAMPLE_YACC_SPEC: &str = r#"
%token ID NUM
%left ';'
%nonassoc '<'
%left '+' '-'
%left '*' '/'
%right UMINUS
%start stmts

%%
stmts : stmts ';' stmt        { seq($1, $3) }
      | stmt                  { $1 }
      ;

stmt  : ID '=' expr           { assign($1, $3) }
      | expr                  { $1 }
      ;

expr  : expr '+' expr         { add($1, $3) }
      | expr '-' expr         { sub($1, $3) }
      | expr '*' expr         { mul($1, $3) }
      | expr '/' expr         { div($1, $3) }
      | expr '<' expr         { lt($1, $3) }
      | '-' expr %prec UMINUS { neg($2) }
      | '(' expr ')'          { $2 }
      | ID                    { var($1) }
      | NUM                   { num($1) }
      ;
%%
"#;

const SAMPLE_PROGRAM: &str = "x = 2 + 3 * -4; x * 2 + 1";

fn main() {
    let compiled = compile_yacc_spec(SAMPLE_YACC_SPEC).expect("valid .y spec");
    let resolved_cnt = compiled
        .conflicts
        .iter()
        .filter(|c| c.kind == ConflictKind::ResolvedSr)
        .count();
    let unresolved_cnt = compiled.conflicts.len() - resolved_cnt;

    println!(
        "Yacc LALR(1) table built: {} rules, {} canonical LR(1) states -> {} merged LALR(1) states",
        compiled.spec.prods.len(),
        compiled.lr1_state_count,
        compiled.lalr_state_count
    );
    println!(
        "Conflicts: {resolved_cnt} S/R resolved by %left/%right/%nonassoc/%prec, {unresolved_cnt} unresolved"
    );
    for c in compiled.conflicts.iter().take(6) {
        println!("  [state {} on '{}'] {}", c.state, c.term, c.detail);
    }

    let out = parse_with_yacc(&compiled, SAMPLE_PROGRAM).expect("sample parses");
    println!("\nParsed & evaluated {:?}:", SAMPLE_PROGRAM);
    println!("  reductions = {}", out.reductions);
    println!("  env        = {:?}", out.env);
    println!("  result     = {:?}", out.value);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_operator_ambiguity_and_evaluates_actions() {
        let compiled = compile_yacc_spec(SAMPLE_YACC_SPEC).expect("compiles cleanly");
        let unresolved = compiled
            .conflicts
            .iter()
            .filter(|c| c.kind != ConflictKind::ResolvedSr)
            .count();
        assert_eq!(unresolved, 0);
        assert!(compiled
            .conflicts
            .iter()
            .any(|c| c.kind == ConflictKind::ResolvedSr));

        let res = parse_with_yacc(&compiled, SAMPLE_PROGRAM).expect("parses");
        assert_eq!(res.env.get("x"), Some(&-10));
        assert_eq!(res.value, SemVal::Int(-19));

        // Left-associativity of '-' and comparison semantics:
        let res2 = parse_with_yacc(&compiled, "10 - 3 - 2; 4 < 9").expect("parses");
        assert_eq!(res2.value, SemVal::Int(1));
        let res3 = parse_with_yacc(&compiled, "10 - 3 - 2").expect("parses");
        assert_eq!(res3.value, SemVal::Int(5));
    }

    #[test]
    fn nonassoc_rejects_chaining_and_unannotated_grammar_warns() {
        let compiled = compile_yacc_spec(SAMPLE_YACC_SPEC).expect("compiles");
        let err = parse_with_yacc(&compiled, "1 < 2 < 3").unwrap_err();
        assert!(err.contains("non-associative"));

        // Removing precedence declarations yields unresolved shift/reduce warnings.
        let unannotated = r#"
%token NUM
%%
expr : expr '+' expr { add($1, $3) }
     | expr '*' expr { mul($1, $3) }
     | NUM           { num($1) }
     ;
%%
"#;
        let weak = compile_yacc_spec(unannotated).expect("compiles with warnings");
        assert!(weak
            .conflicts
            .iter()
            .any(|c| c.kind == ConflictKind::UnresolvedSr));

        // Reduce/reduce conflict picks the earlier-listed rule and warns.
        let rr_spec = r#"
%token A
%%
s : x A { $1 }
  | y A { $1 }
  ;
x : %empty { 1 } ;
y : %empty { 2 } ;
%%
"#;
        let rr = compile_yacc_spec(rr_spec).expect("compiles with R/R warning");
        assert!(rr
            .conflicts
            .iter()
            .any(|c| c.kind == ConflictKind::UnresolvedRr));
        let out = parse_with_yacc(&rr, "A").expect("picks earlier rule x");
        assert_eq!(out.value, SemVal::Int(1));
    }
}
