//! Modern Extension: Generalized LR (GLR) Parsing, GSS & SPPF Forests.
//!
//! Explicitly labeled as a modern parsing extension building directly on Chapter 4 LR tables
//! (Masaru Tomita 1985; Bison `%glr-parser`, Tree-sitter):
//! 1. Keeps ALL shift/reduce and reduce/reduce conflicts inside `ACTION[state, sym] -> Vec<Action>`
//!    for ambiguous grammars like `E -> E + E | E * E | ( E ) | id`,
//! 2. Runs Tomita's breadth-first GLR algorithm over a Graph-Structured Stack (GSS) that
//!    forks on conflicts and merges heads reaching identical LR states at the same input offset,
//! 3. Records all derivations in a compact Shared Packed Parse Forest (SPPF) keyed by
//!    `(symbol, start, end)`, sharing common sub-derivations instead of copying trees,
//! 4. Enumerates all Catalan `C_n` distinct parse trees cleanly (`id + id + id + id` -> 5 trees).

use std::collections::{BTreeMap, BTreeSet, VecDeque};

#[derive(Clone, Debug, PartialEq, Eq)]
struct LrProd {
    lhs: String,
    rhs: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Action {
    Shift(usize),
    Reduce(usize),
    Accept,
}

struct GlrTable {
    prods: Vec<LrProd>,
    action: BTreeMap<(usize, String), Vec<Action>>,
    goto: BTreeMap<(usize, String), usize>,
}

fn build_ambiguous_glr_table() -> GlrTable {
    let prods = vec![
        LrProd {
            lhs: "S'".into(),
            rhs: vec!["E".into()],
        },
        LrProd {
            lhs: "E".into(),
            rhs: vec!["E".into(), "+".into(), "E".into()],
        },
        LrProd {
            lhs: "E".into(),
            rhs: vec!["E".into(), "*".into(), "E".into()],
        },
        LrProd {
            lhs: "E".into(),
            rhs: vec!["(".into(), "E".into(), ")".into()],
        },
        LrProd {
            lhs: "E".into(),
            rhs: vec!["id".into()],
        },
    ];
    let nts: BTreeSet<String> = prods.iter().map(|p| p.lhs.clone()).collect();
    let mut all_syms: BTreeSet<String> = nts.clone();
    for p in &prods {
        for s in &p.rhs {
            all_syms.insert(s.clone());
        }
    }
    let terminals: BTreeSet<String> = all_syms
        .difference(&nts)
        .cloned()
        .chain(std::iter::once("$".into()))
        .collect();

    // SLR(1) FOLLOW sets (still yields rich shift/reduce conflicts on `+` and `*`).
    let mut first: BTreeMap<String, BTreeSet<String>> =
        nts.iter().map(|n| (n.clone(), BTreeSet::new())).collect();
    let mut ch = true;
    while ch {
        ch = false;
        for p in &prods {
            if let Some(s) = p.rhs.first() {
                if !nts.contains(s) {
                    if first.get_mut(&p.lhs).unwrap().insert(s.clone()) {
                        ch = true;
                    }
                } else {
                    let c = first[s].clone();
                    for t in c {
                        if first.get_mut(&p.lhs).unwrap().insert(t) {
                            ch = true;
                        }
                    }
                }
            }
        }
    }
    let mut follow: BTreeMap<String, BTreeSet<String>> =
        nts.iter().map(|n| (n.clone(), BTreeSet::new())).collect();
    follow.get_mut("S'").unwrap().insert("$".into());
    ch = true;
    while ch {
        ch = false;
        for p in &prods {
            let mut trailer = follow[&p.lhs].clone();
            for s in p.rhs.iter().rev() {
                if !nts.contains(s) {
                    trailer.clear();
                    trailer.insert(s.clone());
                } else {
                    let f = follow.get_mut(s).unwrap();
                    for t in &trailer {
                        if f.insert(t.clone()) {
                            ch = true;
                        }
                    }
                    trailer = first[s].clone();
                }
            }
        }
    }

    type Item = (usize, usize);
    let closure = |seed: BTreeSet<Item>| -> BTreeSet<Item> {
        let mut set = seed;
        let mut changed = true;
        while changed {
            changed = false;
            let cur: Vec<Item> = set.iter().copied().collect();
            for (pid, dot) in cur {
                if let Some(sym) = prods[pid].rhs.get(dot) {
                    if nts.contains(sym) {
                        for (npid, np) in prods.iter().enumerate() {
                            if &np.lhs == sym && set.insert((npid, 0)) {
                                changed = true;
                            }
                        }
                    }
                }
            }
        }
        set
    };

    let mut states: Vec<BTreeSet<Item>> = Vec::new();
    let mut state_id: BTreeMap<BTreeSet<Item>, usize> = BTreeMap::new();
    let mut trans: BTreeMap<(usize, String), usize> = BTreeMap::new();
    let s0 = closure(BTreeSet::from([(0, 0)]));
    state_id.insert(s0.clone(), 0);
    states.push(s0);

    let mut idx = 0usize;
    while idx < states.len() {
        let cur = states[idx].clone();
        for sym in &all_syms {
            let seed: BTreeSet<Item> = cur
                .iter()
                .filter(|&&(pid, dot)| prods[pid].rhs.get(dot) == Some(sym))
                .map(|&(pid, dot)| (pid, dot + 1))
                .collect();
            if seed.is_empty() {
                continue;
            }
            let tgt_set = closure(seed);
            let tid = match state_id.get(&tgt_set) {
                Some(&id) => id,
                None => {
                    let id = states.len();
                    state_id.insert(tgt_set.clone(), id);
                    states.push(tgt_set);
                    id
                }
            };
            trans.insert((idx, sym.clone()), tid);
        }
        idx += 1;
    }

    // Keep ALL conflicting Shift and Reduce actions in `ACTION`!
    let mut action_sets: BTreeMap<(usize, String), BTreeSet<Action>> = BTreeMap::new();
    let mut goto = BTreeMap::new();
    for (sid, items) in states.iter().enumerate() {
        for sym in &terminals {
            if let Some(&tid) = trans.get(&(sid, sym.clone())) {
                action_sets
                    .entry((sid, sym.clone()))
                    .or_default()
                    .insert(Action::Shift(tid));
            }
        }
        for nt in &nts {
            if let Some(&tid) = trans.get(&(sid, nt.clone())) {
                goto.insert((sid, nt.clone()), tid);
            }
        }
        for &(pid, dot) in items {
            if dot == prods[pid].rhs.len() {
                if pid == 0 {
                    action_sets
                        .entry((sid, "$".into()))
                        .or_default()
                        .insert(Action::Accept);
                } else {
                    for f in &follow[&prods[pid].lhs] {
                        action_sets
                            .entry((sid, f.clone()))
                            .or_default()
                            .insert(Action::Reduce(pid));
                    }
                }
            }
        }
    }

    let action = action_sets
        .into_iter()
        .map(|(k, v)| (k, v.into_iter().collect()))
        .collect();
    GlrTable {
        prods,
        action,
        goto,
    }
}

// ---------------------------------------------------------------------------
// Shared Packed Parse Forest (SPPF) & Graph-Structured Stack (GSS)
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
enum SpfKind {
    Terminal(String),
    NonTerminal {
        label: String,
        packs: BTreeSet<Vec<usize>>, // each alternative holds child SPPF node IDs
    },
}

#[derive(Clone, Debug)]
struct SpfNode {
    start: usize,
    end: usize,
    kind: SpfKind,
}

#[derive(Clone, Debug)]
struct GssNode {
    state: usize,
    pos: usize,
    /// Incoming stack links `(parent_gss_node, edge_sppf_id)` toward the bottom of the stack.
    parents: Vec<(usize, usize)>,
}

struct GlrResult {
    root_sppf: Option<usize>,
    sppf_nodes: Vec<SpfNode>,
    gss_nodes_created: usize,
    gss_merges: usize,
    conflict_forks: usize,
}

fn tokenize_glr(src: &str) -> Result<Vec<(String, String)>, String> {
    let bytes = src.as_bytes();
    let mut i = 0;
    let mut out = Vec::new();
    while i < bytes.len() {
        if bytes[i].is_ascii_whitespace() {
            i += 1;
            continue;
        }
        if bytes[i].is_ascii_alphabetic() || bytes[i] == b'_' {
            let start = i;
            i += 1;
            while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                i += 1;
            }
            out.push(("id".into(), src[start..i].to_string()));
            continue;
        }
        let s = match bytes[i] {
            b'+' => "+",
            b'*' => "*",
            b'(' => "(",
            b')' => ")",
            other => return Err(format!("unexpected character '{}'", other as char)),
        };
        i += 1;
        out.push((s.into(), s.into()));
    }
    out.push(("$".into(), "$".into()));
    Ok(out)
}

fn glr_parse(src: &str, table: &GlrTable) -> Result<GlrResult, String> {
    let tokens = tokenize_glr(src)?;
    let mut sppf: Vec<SpfNode> = Vec::new();
    let mut nt_index: BTreeMap<(String, usize, usize), usize> = BTreeMap::new();
    let mut term_index: BTreeMap<(String, usize, usize), usize> = BTreeMap::new();

    let mut get_or_create_nt = |sppf: &mut Vec<SpfNode>,
                                label: String,
                                start: usize,
                                end: usize,
                                pack: Vec<usize>|
     -> usize {
        let key = (label.clone(), start, end);
        let id = match nt_index.get(&key) {
            Some(&existing) => existing,
            None => {
                let id = sppf.len();
                sppf.push(SpfNode {
                    start,
                    end,
                    kind: SpfKind::NonTerminal {
                        label,
                        packs: BTreeSet::new(),
                    },
                });
                nt_index.insert(key, id);
                id
            }
        };
        if let SpfKind::NonTerminal { packs, .. } = &mut sppf[id].kind {
            packs.insert(pack);
        }
        id
    };

    let mut get_or_create_term =
        |sppf: &mut Vec<SpfNode>, lex: String, start: usize, end: usize| -> usize {
            let key = (lex.clone(), start, end);
            if let Some(&id) = term_index.get(&key) {
                return id;
            }
            let id = sppf.len();
            sppf.push(SpfNode {
                start,
                end,
                kind: SpfKind::Terminal(lex),
            });
            term_index.insert(key, id);
            id
        };

    let mut gss: Vec<GssNode> = vec![GssNode {
        state: 0,
        pos: 0,
        parents: Vec::new(),
    }];
    let mut gss_merges = 0usize;
    let mut conflict_forks = 0usize;

    let mut frontier: Vec<usize> = vec![0];
    let mut root_sppf: Option<usize> = None;

    // Collect all GSS ancestor paths of length `k` ending at `node_id`.
    fn collect_paths(
        gss: &[GssNode],
        node_id: usize,
        remaining: usize,
    ) -> Vec<(usize, Vec<usize>)> {
        if remaining == 0 {
            return vec![(node_id, Vec::new())];
        }
        let mut out = Vec::new();
        for &(parent, edge_spf) in &gss[node_id].parents {
            for (anc, mut suffix) in collect_paths(gss, parent, remaining - 1) {
                suffix.push(edge_spf);
                out.push((anc, suffix));
            }
        }
        out
    }

    for (pos, (tok_kind, tok_lex)) in tokens.iter().enumerate() {
        if frontier.is_empty() {
            return Err(format!("syntax error near '{tok_lex}' at token {pos}"));
        }

        // Track `(state -> gss_id)` at current input position `pos` for GSS merging.
        let mut at_pos: BTreeMap<usize, usize> = BTreeMap::new();
        for &gid in &frontier {
            at_pos.insert(gss[gid].state, gid);
        }

        // Worklist of reductions `(gss_node, prod_id)` and deduplication set to handle
        // edges added into existing GSS nodes during breadth-first reduction!
        let mut reduce_q: VecDeque<usize> = frontier.iter().copied().collect();
        let mut queued_edges: BTreeSet<(usize, usize, usize, Vec<usize>)> = BTreeSet::new();

        // Count forks across current frontier.
        for &gid in &frontier {
            if let Some(acts) = table.action.get(&(gss[gid].state, tok_kind.clone())) {
                if acts.len() > 1 {
                    conflict_forks += acts.len() - 1;
                }
            }
        }

        while let Some(gid) = reduce_q.pop_front() {
            let st = gss[gid].state;
            let Some(acts) = table.action.get(&(st, tok_kind.clone())) else {
                continue;
            };
            for act in acts {
                match act {
                    Action::Accept => {
                        if let Some(&(_, spf_id)) = gss[gid].parents.first() {
                            root_sppf = Some(spf_id);
                        }
                    }
                    Action::Reduce(pid) => {
                        let prod = &table.prods[*pid];
                        let k = prod.rhs.len();
                        let paths = collect_paths(&gss, gid, k);
                        for (anc_id, children_spf) in paths {
                            if !queued_edges.insert((gid, *pid, anc_id, children_spf.clone())) {
                                continue;
                            }
                            let next_state = table.goto[&(gss[anc_id].state, prod.lhs.clone())];
                            let span_start = gss[anc_id].pos;
                            let spf_id = get_or_create_nt(
                                &mut sppf,
                                prod.lhs.clone(),
                                span_start,
                                pos,
                                children_spf,
                            );

                            if let Some(&existing_gid) = at_pos.get(&next_state) {
                                // Merge parent edge if not already present; re-check reductions!
                                if !gss[existing_gid].parents.contains(&(anc_id, spf_id)) {
                                    gss[existing_gid].parents.push((anc_id, spf_id));
                                    gss_merges += 1;
                                    reduce_q.push_back(existing_gid);
                                    // Any node downstream that reaches `existing_gid` may also have new paths;
                                    // enqueue all nodes in `at_pos` cleanly for full Tomita completeness.
                                    for &other in at_pos.values() {
                                        reduce_q.push_back(other);
                                    }
                                }
                            } else {
                                let new_gid = gss.len();
                                gss.push(GssNode {
                                    state: next_state,
                                    pos,
                                    parents: vec![(anc_id, spf_id)],
                                });
                                at_pos.insert(next_state, new_gid);
                                reduce_q.push_back(new_gid);
                            }
                        }
                    }
                    Action::Shift(_) => {}
                }
            }
        }

        if tok_kind == "$" {
            break;
        }

        // Shift step: advance every active head with Shift(next_state) to `pos + 1`.
        let mut next_at_pos: BTreeMap<usize, usize> = BTreeMap::new();
        let mut next_frontier = Vec::new();
        let leaf_spf = get_or_create_term(&mut sppf, tok_lex.clone(), pos, pos + 1);

        let all_cur_gids: Vec<usize> = at_pos.values().copied().collect();
        for gid in all_cur_gids {
            let st = gss[gid].state;
            let Some(acts) = table.action.get(&(st, tok_kind.clone())) else {
                continue;
            };
            for act in acts {
                if let Action::Shift(next_state) = *act {
                    if let Some(&existing_next) = next_at_pos.get(&next_state) {
                        if !gss[existing_next].parents.contains(&(gid, leaf_spf)) {
                            gss[existing_next].parents.push((gid, leaf_spf));
                            gss_merges += 1;
                        }
                    } else {
                        let new_gid = gss.len();
                        gss.push(GssNode {
                            state: next_state,
                            pos: pos + 1,
                            parents: vec![(gid, leaf_spf)],
                        });
                        next_at_pos.insert(next_state, new_gid);
                        next_frontier.push(new_gid);
                    }
                }
            }
        }
        frontier = next_frontier;
    }

    Ok(GlrResult {
        root_sppf,
        sppf_nodes: sppf,
        gss_nodes_created: gss.len(),
        gss_merges,
        conflict_forks,
    })
}

fn enumerate_sppf_exprs(sppf: &[SpfNode], node_id: usize) -> Vec<String> {
    match &sppf[node_id].kind {
        SpfKind::Terminal(lex) => vec![lex.clone()],
        SpfKind::NonTerminal { packs, .. } => {
            let mut out = BTreeSet::new();
            for children in packs {
                if children.len() == 1 {
                    for s in enumerate_sppf_exprs(sppf, children[0]) {
                        out.insert(s);
                    }
                } else if children.len() == 3 {
                    let mid_lex = match &sppf[children[1]].kind {
                        SpfKind::Terminal(s) => s.clone(),
                        _ => String::new(),
                    };
                    if mid_lex == "+" || mid_lex == "*" {
                        let lefts = enumerate_sppf_exprs(sppf, children[0]);
                        let rights = enumerate_sppf_exprs(sppf, children[2]);
                        for l in &lefts {
                            for r in &rights {
                                out.insert(format!("({l} {mid_lex} {r})"));
                            }
                        }
                    } else {
                        // `( E )`
                        for inner in enumerate_sppf_exprs(sppf, children[1]) {
                            out.insert(format!("({inner})"));
                        }
                    }
                }
            }
            out.into_iter().collect()
        }
    }
}

fn main() {
    let table = build_ambiguous_glr_table();
    let conflict_cells = table.action.values().filter(|v| v.len() > 1).count();
    println!("=== Advanced Parsing (Modern): Tomita GLR Parsing, GSS & SPPF Forests ===");
    println!("Ambiguous LR grammar `E -> E + E | E * E | ( E ) | id` has {conflict_cells} S/R conflict cells (kept intact!).");

    for sample in ["id + id * id", "id + id + id + id"] {
        let res = glr_parse(sample, &table).expect("GLR parse succeeds");
        let root = res.root_sppf.expect("accept root present");
        let trees = enumerate_sppf_exprs(&res.sppf_nodes, root);
        println!(
            "\nInput: {sample:?}\n  SPPF nodes = {}, GSS nodes = {}, merges = {}, conflict forks = {}, unpacked ASTs = {}",
            res.sppf_nodes.len(),
            res.gss_nodes_created,
            res.gss_merges,
            res.conflict_forks,
            trees.len()
        );
        for (i, t) in trees.iter().enumerate() {
            println!("    AST #{}: {t}", i + 1);
        }
        let shared: Vec<String> = res
            .sppf_nodes
            .iter()
            .filter_map(|n| match &n.kind {
                SpfKind::NonTerminal { label, packs } if packs.len() > 1 => {
                    Some(format!("{label}[{}..{}]({})", n.start, n.end, packs.len()))
                }
                _ => None,
            })
            .collect();
        println!("  Shared ambiguous SPPF nodes: {:?}", shared);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glr_parses_ambiguous_precedence_into_two_sppf_trees() {
        let table = build_ambiguous_glr_table();
        let res = glr_parse("a + b * c", &table).unwrap();
        let trees = enumerate_sppf_exprs(&res.sppf_nodes, res.root_sppf.unwrap());
        assert_eq!(trees, vec!["((a + b) * c)", "(a + (b * c))"]);
    }

    #[test]
    fn glr_produces_catalan_number_of_trees_with_sppf_sharing() {
        let table = build_ambiguous_glr_table();
        // 4 operands => Catalan C_3 = 5 distinct parenthesizations
        let res4 = glr_parse("a + b + c + d", &table).unwrap();
        let trees4 = enumerate_sppf_exprs(&res4.sppf_nodes, res4.root_sppf.unwrap());
        assert_eq!(trees4.len(), 5);
        assert!(res4.gss_merges > 0);

        // 5 operands => Catalan C_4 = 14 distinct parenthesizations packed cleanly in SPPF
        let res5 = glr_parse("a + b + c + d + e", &table).unwrap();
        let trees5 = enumerate_sppf_exprs(&res5.sppf_nodes, res5.root_sppf.unwrap());
        assert_eq!(trees5.len(), 14);
        assert!(res5.sppf_nodes.len() < 40);
    }

    #[test]
    fn glr_rejects_unmatched_tokens() {
        let table = build_ambiguous_glr_table();
        assert!(
            glr_parse("(a + b", &table).is_err()
                || glr_parse("(a + b", &table).unwrap().root_sppf.is_none()
        );
        assert!(glr_parse("a + * b", &table).is_err());
    }
}
