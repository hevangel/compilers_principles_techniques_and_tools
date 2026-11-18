//! Construct SLR(1), LALR(1), and canonical LR(1) tables and compact rows.

use std::collections::{BTreeMap, BTreeSet};

/// Grammar symbol indexed into `Grammar` tables; terminal `0` is always `$` (EOF).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Symbol {
    Nt(usize),
    Term(usize), // 0 is reserved for End ($)
}

/// Canonical LR(1) item `[prod: A -> alpha . beta, lookahead]`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Lr1Item {
    prod: usize,
    dot: usize,
    lookahead: usize, // terminal ID
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Action {
    Shift(usize),
    Reduce(usize),
    Accept,
}

/// Integer-indexed CFG with augmented start nonterminal `0` and production `0`.
struct Grammar {
    num_nt: usize,
    num_terms: usize, // includes $ at 0
    term_names: Vec<&'static str>,
    prods: Vec<(usize, Vec<Symbol>)>,
}

impl Grammar {
    /// Fixed-point computation of `(FIRST(nt), nullable(nt))` for all nonterminals.
    fn first_sets(&self) -> Vec<(BTreeSet<usize>, bool)> {
        let mut first = vec![(BTreeSet::new(), false); self.num_nt];
        loop {
            let mut changed = false;
            for &(lhs, ref rhs) in &self.prods {
                let (terms, nullable) = self.first_of_seq(rhs, &first);
                let prev_len = first[lhs].0.len();
                let prev_null = first[lhs].1;
                first[lhs].0.extend(terms);
                first[lhs].1 |= nullable;
                if first[lhs].0.len() != prev_len || first[lhs].1 != prev_null {
                    changed = true;
                }
            }
            if !changed {
                return first;
            }
        }
    }

    fn first_of_seq(
        &self,
        seq: &[Symbol],
        first: &[(BTreeSet<usize>, bool)],
    ) -> (BTreeSet<usize>, bool) {
        let mut out = BTreeSet::new();
        for &sym in seq {
            match sym {
                Symbol::Term(t) => {
                    out.insert(t);
                    return (out, false);
                }
                Symbol::Nt(nt) => {
                    out.extend(&first[nt].0);
                    if !first[nt].1 {
                        return (out, false);
                    }
                }
            }
        }
        (out, true)
    }

    /// Fixed-point computation of `FOLLOW(nt)` (used only by SLR(1)).
    fn follow_sets(&self, first: &[(BTreeSet<usize>, bool)]) -> Vec<BTreeSet<usize>> {
        let mut follow = vec![BTreeSet::new(); self.num_nt];
        follow[0].insert(0); // $ follows augmented start 0
        loop {
            let mut changed = false;
            for &(lhs, ref rhs) in &self.prods {
                for (i, &sym) in rhs.iter().enumerate() {
                    if let Symbol::Nt(nt) = sym {
                        let (mut terms, nullable) = self.first_of_seq(&rhs[i + 1..], first);
                        if nullable {
                            terms.extend(&follow[lhs]);
                        }
                        let prev = follow[nt].len();
                        follow[nt].extend(terms);
                        if follow[nt].len() != prev {
                            changed = true;
                        }
                    }
                }
            }
            if !changed {
                return follow;
            }
        }
    }

    /// LR(1) closure: for `[A -> alpha . B beta, a]`, add `[B -> . gamma, b]`
    /// for every `b` in `FIRST(beta a)`. Notice `a` is included at the end of `suffix`,
    /// so if `beta` is nullable, `a` propagates directly as a lookahead.
    fn closure_lr1(
        &self,
        mut items: BTreeSet<Lr1Item>,
        first: &[(BTreeSet<usize>, bool)],
    ) -> BTreeSet<Lr1Item> {
        loop {
            let prev = items.len();
            let mut additions = Vec::new();
            for &item in &items {
                let rhs = &self.prods[item.prod].1;
                if let Some(&Symbol::Nt(b)) = rhs.get(item.dot) {
                    let mut suffix = rhs[item.dot + 1..].to_vec();
                    suffix.push(Symbol::Term(item.lookahead));
                    let (lookaheads, _) = self.first_of_seq(&suffix, first);
                    for (pid, &(lhs, _)) in self.prods.iter().enumerate() {
                        if lhs == b {
                            for &la in &lookaheads {
                                additions.push(Lr1Item {
                                    prod: pid,
                                    dot: 0,
                                    lookahead: la,
                                });
                            }
                        }
                    }
                }
            }
            items.extend(additions);
            if items.len() == prev {
                return items;
            }
        }
    }

    /// Advance LR(1) items expecting `sym` (preserving their lookaheads) and take `closure_lr1`.
    fn goto_lr1(
        &self,
        items: &BTreeSet<Lr1Item>,
        sym: Symbol,
        first: &[(BTreeSet<usize>, bool)],
    ) -> BTreeSet<Lr1Item> {
        let kernel: BTreeSet<Lr1Item> = items
            .iter()
            .filter_map(|&item| {
                if self.prods[item.prod].1.get(item.dot) == Some(&sym) {
                    Some(Lr1Item {
                        prod: item.prod,
                        dot: item.dot + 1,
                        lookahead: item.lookahead,
                    })
                } else {
                    None
                }
            })
            .collect();
        if kernel.is_empty() {
            kernel
        } else {
            self.closure_lr1(kernel, first)
        }
    }
}

#[derive(Debug)]
struct ParseTable {
    num_states: usize,
    actions: BTreeMap<(usize, usize), Action>,
    goto: BTreeMap<(usize, usize), usize>,
    conflicts: Vec<(usize, usize, Action, Action)>,
}

/// Compacted action table replacing reduce-only rows with a single default reduction.
#[derive(Debug)]
struct CompactTable {
    dense_entries: usize,
    compact_entries: usize,
    default_reduce: Vec<Option<usize>>,
    explicit_actions: BTreeMap<(usize, usize), Action>,
}

/// Compress sparse action rows by hoisting the most frequent `Reduce(p)` on states
/// that contain no `Shift` or `Accept` actions (deferring syntax-error detection until
/// the parent state cannot shift/reduce further, which still rejects every invalid input).
fn compact_table(table: &ParseTable, num_terms: usize) -> CompactTable {
    let mut default_reduce = vec![None; table.num_states];
    let mut explicit_actions = BTreeMap::new();
    for (state, default_entry) in default_reduce.iter_mut().enumerate() {
        let mut counts: BTreeMap<usize, usize> = BTreeMap::new();
        let mut has_shift_or_accept = false;
        for t in 0..num_terms {
            match table.actions.get(&(state, t)) {
                Some(&Action::Reduce(p)) => *counts.entry(p).or_default() += 1,
                Some(_) => has_shift_or_accept = true,
                None => {}
            }
        }
        // Only hoist a default reduction when the row has no Shift/Accept entries
        // so explicit Shift/Accept cells can never be shadowed.
        let best_reduce = if !has_shift_or_accept {
            counts.into_iter().max_by_key(|&(_, c)| c).map(|(p, _)| p)
        } else {
            None
        };
        *default_entry = best_reduce;
        for t in 0..num_terms {
            if let Some(&act) = table.actions.get(&(state, t)) {
                if best_reduce.map(Action::Reduce) != Some(act) {
                    explicit_actions.insert((state, t), act);
                }
            }
        }
    }
    CompactTable {
        dense_entries: table.num_states * num_terms,
        compact_entries: explicit_actions.len() + default_reduce.iter().flatten().count(),
        default_reduce,
        explicit_actions,
    }
}

type Lr1Collection = (Vec<BTreeSet<Lr1Item>>, BTreeMap<(usize, Symbol), usize>);

/// Build the canonical collection of LR(1) item sets starting from `[S' -> . S, $]`.
fn build_lr1_collection(g: &Grammar) -> Lr1Collection {
    let first = g.first_sets();
    let start_set = g.closure_lr1(
        BTreeSet::from([Lr1Item {
            prod: 0,
            dot: 0,
            lookahead: 0,
        }]),
        &first,
    );
    let mut states = vec![start_set];
    let mut trans = BTreeMap::new();
    let mut i = 0;
    while i < states.len() {
        let syms: BTreeSet<Symbol> = states[i]
            .iter()
            .filter_map(|item| g.prods[item.prod].1.get(item.dot).copied())
            .collect();
        for sym in syms {
            let nxt = g.goto_lr1(&states[i], sym, &first);
            let idx = states
                .iter()
                .position(|s| *s == nxt)
                .unwrap_or(states.len());
            if idx == states.len() {
                states.push(nxt);
            }
            trans.insert((i, sym), idx);
        }
        i += 1;
    }
    (states, trans)
}

fn build_lr1_table(g: &Grammar) -> ParseTable {
    let (states, trans) = build_lr1_collection(g);
    table_from_lr1_states(g, &states, &trans)
}

/// Construct LALR(1) by merging canonical LR(1) states that share identical LR(0) cores
/// `(prod, dot)`, uniting their lookaheads. Merging cores preserves shift transitions
/// identically (so it never creates a new shift/reduce conflict), though uniting lookaheads
/// of two completed items can create a reduce/reduce conflict.
fn build_lalr1_table(g: &Grammar) -> ParseTable {
    let (lr1_states, lr1_trans) = build_lr1_collection(g);
    let mut cores: Vec<BTreeSet<(usize, usize)>> = Vec::new();
    let mut core_of_lr1 = vec![0usize; lr1_states.len()];
    let mut merged: Vec<BTreeSet<Lr1Item>> = Vec::new();

    for (idx, st) in lr1_states.iter().enumerate() {
        let core: BTreeSet<(usize, usize)> = st.iter().map(|it| (it.prod, it.dot)).collect();
        let cid = cores.iter().position(|c| *c == core).unwrap_or_else(|| {
            cores.push(core);
            merged.push(BTreeSet::new());
            cores.len() - 1
        });
        core_of_lr1[idx] = cid;
        merged[cid].extend(st.iter().copied());
    }

    let mut lalr_trans = BTreeMap::new();
    for (&(src, sym), &dst) in &lr1_trans {
        lalr_trans.insert((core_of_lr1[src], sym), core_of_lr1[dst]);
    }
    table_from_lr1_states(g, &merged, &lalr_trans)
}

/// Construct SLR(1) over the LR(0) cores using `FOLLOW(LHS)` for all reductions.
fn build_slr1_table(g: &Grammar) -> ParseTable {
    let first = g.first_sets();
    let follow = g.follow_sets(&first);
    let (lr1_states, lr1_trans) = build_lr1_collection(g);
    // Project LR(1) to LR(0) states.
    let mut cores: Vec<BTreeSet<(usize, usize)>> = Vec::new();
    let mut core_of = vec![0usize; lr1_states.len()];
    for (idx, st) in lr1_states.iter().enumerate() {
        let core: BTreeSet<(usize, usize)> = st.iter().map(|it| (it.prod, it.dot)).collect();
        let cid = cores.iter().position(|c| *c == core).unwrap_or_else(|| {
            cores.push(core);
            cores.len() - 1
        });
        core_of[idx] = cid;
    }
    let mut actions = BTreeMap::new();
    let mut goto = BTreeMap::new();
    let mut conflicts = Vec::new();
    for (&(src, sym), &dst) in &lr1_trans {
        let csrc = core_of[src];
        let cdst = core_of[dst];
        match sym {
            Symbol::Term(t) => {
                actions.insert((csrc, t), Action::Shift(cdst));
            }
            Symbol::Nt(nt) => {
                goto.insert((csrc, nt), cdst);
            }
        }
    }
    for (state, core) in cores.iter().enumerate() {
        for &(prod, dot) in core {
            if dot == g.prods[prod].1.len() {
                if prod == 0 {
                    record_action(&mut actions, &mut conflicts, state, 0, Action::Accept);
                } else {
                    let lhs = g.prods[prod].0;
                    for &la in &follow[lhs] {
                        record_action(
                            &mut actions,
                            &mut conflicts,
                            state,
                            la,
                            Action::Reduce(prod),
                        );
                    }
                }
            }
        }
    }
    ParseTable {
        num_states: cores.len(),
        actions,
        goto,
        conflicts,
    }
}

/// Insert an action or record a conflict if a different action already occupies `(state, term)`.
fn record_action(
    actions: &mut BTreeMap<(usize, usize), Action>,
    conflicts: &mut Vec<(usize, usize, Action, Action)>,
    state: usize,
    term: usize,
    act: Action,
) {
    if let Some(&prev) = actions.get(&(state, term)) {
        if prev != act {
            conflicts.push((state, term, prev, act));
        }
    } else {
        actions.insert((state, term), act);
    }
}

fn table_from_lr1_states(
    g: &Grammar,
    states: &[BTreeSet<Lr1Item>],
    trans: &BTreeMap<(usize, Symbol), usize>,
) -> ParseTable {
    let mut actions = BTreeMap::new();
    let mut goto = BTreeMap::new();
    let mut conflicts = Vec::new();

    for (&(src, sym), &dst) in trans {
        match sym {
            Symbol::Term(t) => {
                record_action(&mut actions, &mut conflicts, src, t, Action::Shift(dst));
            }
            Symbol::Nt(nt) => {
                goto.insert((src, nt), dst);
            }
        }
    }
    for (state, items) in states.iter().enumerate() {
        for &item in items {
            if item.dot == g.prods[item.prod].1.len() {
                let act = if item.prod == 0 {
                    Action::Accept
                } else {
                    Action::Reduce(item.prod)
                };
                record_action(&mut actions, &mut conflicts, state, item.lookahead, act);
            }
        }
    }

    ParseTable {
        num_states: states.len(),
        actions,
        goto,
        conflicts,
    }
}

/// Run the LR shift-reduce driver on `tokens` using a conflict-free `ParseTable`.
fn parse(g: &Grammar, table: &ParseTable, tokens: &[usize]) -> Result<Vec<usize>, String> {
    if !table.conflicts.is_empty() {
        return Err(format!("grammar has {} conflicts", table.conflicts.len()));
    }
    let mut input = tokens.to_vec();
    input.push(0); // $
    let mut pos = 0;
    let mut stack = vec![0usize];
    let mut reductions = Vec::new();
    loop {
        let state = *stack.last().unwrap();
        let tok = input[pos];
        match table.actions.get(&(state, tok)) {
            Some(&Action::Shift(next)) => {
                stack.push(next);
                pos += 1;
            }
            Some(&Action::Reduce(prod)) => {
                let (lhs, ref rhs) = g.prods[prod];
                stack.truncate(stack.len() - rhs.len());
                let prev = *stack.last().unwrap();
                let next = table.goto[&(prev, lhs)];
                stack.push(next);
                reductions.push(prod);
            }
            Some(&Action::Accept) => return Ok(reductions),
            None => {
                return Err(format!(
                    "syntax error at token {} ({})",
                    pos, g.term_names[tok]
                ))
            }
        }
    }
}

// S' -> S, S -> L = R | R, L -> * R | id, R -> L
fn lvalue_grammar() -> Grammar {
    use Symbol::{Nt, Term};
    Grammar {
        num_nt: 4, // 0:S', 1:S, 2:L, 3:R
        num_terms: 4,
        term_names: vec!["$", "=", "*", "id"],
        prods: vec![
            (0, vec![Nt(1)]),
            (1, vec![Nt(2), Term(1), Nt(3)]),
            (1, vec![Nt(3)]),
            (2, vec![Term(2), Nt(3)]),
            (2, vec![Term(3)]),
            (3, vec![Nt(2)]),
        ],
    }
}

// LR(1) but NOT LALR(1):
// S' -> S
// S -> a E c | a F d | b F c | b E d
// E -> e
// F -> e
fn lr1_only_grammar() -> Grammar {
    use Symbol::{Nt, Term};
    Grammar {
        num_nt: 4, // 0:S', 1:S, 2:E, 3:F
        num_terms: 6,
        term_names: vec!["$", "a", "b", "c", "d", "e"],
        prods: vec![
            (0, vec![Nt(1)]),
            (1, vec![Term(1), Nt(2), Term(3)]),
            (1, vec![Term(1), Nt(3), Term(4)]),
            (1, vec![Term(2), Nt(3), Term(3)]),
            (1, vec![Term(2), Nt(2), Term(4)]),
            (2, vec![Term(5)]),
            (3, vec![Term(5)]),
        ],
    }
}

fn main() {
    let lv = lvalue_grammar();
    let slr = build_slr1_table(&lv);
    let lalr = build_lalr1_table(&lv);
    let lr1 = build_lr1_table(&lv);
    let compact = compact_table(&lalr, lv.num_terms);
    println!(
        "Lvalue grammar: SLR conflicts={}, LALR states={} (conflicts={}), LR(1) states={} (conflicts={})",
        slr.conflicts.len(),
        lalr.num_states,
        lalr.conflicts.len(),
        lr1.num_states,
        lr1.conflicts.len()
    );
    println!(
        "LALR compaction: dense {} entries -> {} compact entries (explicit={}, default_reduce={:?})",
        compact.dense_entries,
        compact.compact_entries,
        compact.explicit_actions.len(),
        compact.default_reduce
    );
    // *id = id
    let reductions = parse(&lv, &lalr, &[2, 3, 1, 3]).unwrap();
    println!("Parsed '*id = id' via LALR(1), reductions: {reductions:?}");
    let lr1_only = lr1_only_grammar();
    println!(
        "LR(1)-only grammar: LALR conflicts={}, LR(1) conflicts={}",
        build_lalr1_table(&lr1_only).conflicts.len(),
        build_lr1_table(&lr1_only).conflicts.len()
    );
}

#[test]
fn lvalue_grammar_fails_slr_but_passes_lalr_and_lr1() {
    let lv = lvalue_grammar();
    let slr = build_slr1_table(&lv);
    let lalr = build_lalr1_table(&lv);
    let lr1 = build_lr1_table(&lv);

    assert!(!slr.conflicts.is_empty());
    assert!(lalr.conflicts.is_empty());
    assert!(lr1.conflicts.is_empty());
    assert!(lalr.num_states < lr1.num_states);

    assert!(parse(&lv, &lalr, &[3]).is_ok()); // id
    assert!(parse(&lv, &lalr, &[2, 3, 1, 3]).is_ok()); // *id = id
    assert!(parse(&lv, &lalr, &[2, 2, 3, 1, 2, 3]).is_ok()); // **id = *id
    assert!(parse(&lv, &lalr, &[3, 1]).is_err());

    let compact = compact_table(&lalr, lv.num_terms);
    assert!(compact.compact_entries < compact.dense_entries);
}

#[test]
fn demonstrates_strict_lr1_over_lalr1_hierarchy() {
    let g = lr1_only_grammar();
    let lalr = build_lalr1_table(&g);
    let lr1 = build_lr1_table(&g);
    assert!(!lalr.conflicts.is_empty());
    assert!(lr1.conflicts.is_empty());
    assert!(parse(&g, &lr1, &[1, 5, 3]).is_ok()); // a e c
    assert!(parse(&g, &lr1, &[1, 5, 4]).is_ok()); // a e d
    assert!(parse(&g, &lr1, &[2, 5, 3]).is_ok()); // b e c
    assert!(parse(&g, &lr1, &[2, 5, 4]).is_ok()); // b e d
    assert!(parse(&g, &lr1, &[1, 5, 5]).is_err());
}
