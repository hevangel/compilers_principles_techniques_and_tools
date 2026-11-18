//! Eliminate left recursion and extract common prefixes via left factoring.

use std::collections::{BTreeMap, BTreeSet};

/// Grammar symbol: either a concrete input token (`Terminal`) or a rule head (`NonTerminal`).
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Symbol {
    Terminal(String),
    NonTerminal(String),
}

/// Context-free grammar with an explicit nonterminal ordering (`nonterminals`)
/// used by left-recursion elimination. An empty `Vec<Symbol>` represents epsilon (`eps`).
#[derive(Clone, Debug, PartialEq, Eq)]
struct Cfg {
    nonterminals: Vec<String>,
    start: String,
    prods: BTreeMap<String, Vec<Vec<Symbol>>>,
}

impl Cfg {
    /// Generate a collision-free nonterminal name such as `A'`, `A'1`, or `S_f`.
    fn fresh_name(&self, base: &str, used: &mut BTreeSet<String>, suffix: &str) -> String {
        let mut candidate = format!("{base}{suffix}");
        let mut counter = 1;
        while used.contains(&candidate) {
            candidate = format!("{base}{suffix}{counter}");
            counter += 1;
        }
        used.insert(candidate.clone());
        candidate
    }
}

/// Check structural preconditions before rewriting: start symbol exists, every
/// nonterminal has >= 1 production, and there are no pure unit cycles (`A =>+ A`).
fn validate_cfg(cfg: &Cfg) -> Result<(), String> {
    if !cfg.prods.contains_key(&cfg.start) {
        return Err(format!("missing start symbol {}", cfg.start));
    }
    for nt in &cfg.nonterminals {
        match cfg.prods.get(nt) {
            Some(alts) if !alts.is_empty() => {}
            _ => return Err(format!("nonterminal {nt} has no productions")),
        }
    }
    // Reject unit epsilon cycles A ->* A via nullable prefixes.
    let mut nullable = BTreeSet::new();
    loop {
        let prev = nullable.len();
        for (lhs, alts) in &cfg.prods {
            if alts.iter().any(|rhs| {
                rhs.iter().all(|sym| match sym {
                    Symbol::NonTerminal(n) => nullable.contains(n),
                    Symbol::Terminal(_) => false,
                })
            }) {
                nullable.insert(lhs.clone());
            }
        }
        if nullable.len() == prev {
            break;
        }
    }
    // Check direct/indirect unit cycles A =>+ A via nullable prefixes.
    for start in &cfg.nonterminals {
        let mut reachable = BTreeSet::new();
        let mut stack = vec![start.clone()];
        while let Some(cur) = stack.pop() {
            if let Some(alts) = cfg.prods.get(&cur) {
                for rhs in alts {
                    for sym in rhs {
                        match sym {
                            Symbol::Terminal(_) => break,
                            Symbol::NonTerminal(next) => {
                                if next == start && rhs.iter().all(|s| matches!(s, Symbol::NonTerminal(n) if n == start || nullable.contains(n))) {
                                    // Direct or indirect left recursion is valid unless pure unit cycle A =>+ A.
                                    if rhs.len() == 1 {
                                        return Err(format!("unit cycle involving {start}"));
                                    }
                                }
                                if reachable.insert(next.clone()) {
                                    stack.push(next.clone());
                                }
                                if !nullable.contains(next) {
                                    break;
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    Ok(())
}

/// Remove both indirect and direct left recursion using Paull's ordered algorithm.
/// Invariant after step `i`: for every `k <= i`, no production of `A_k` starts with
/// any `A_m` where `m <= k`.
fn eliminate_left_recursion(cfg: &Cfg) -> Result<Cfg, String> {
    validate_cfg(cfg)?;
    let mut prods = cfg.prods.clone();
    let mut order = cfg.nonterminals.clone();
    let mut used: BTreeSet<String> = order.iter().cloned().collect();

    for i in 0..order.len() {
        let ai = order[i].clone();
        // Substitute A_i -> A_j gamma for every j < i to convert indirect left recursion
        // (cycles through earlier nonterminals) into direct left recursion on A_i.
        let earlier = order[..i].to_vec();
        for aj in &earlier {
            let aj_alts = prods.get(aj).cloned().unwrap_or_default();
            let current_alts = prods.get(&ai).cloned().unwrap_or_default();
            let mut updated = Vec::new();
            for rhs in current_alts {
                if let Some(Symbol::NonTerminal(head)) = rhs.first() {
                    if head == aj {
                        let tail = &rhs[1..];
                        for aj_rhs in &aj_alts {
                            let mut combined = aj_rhs.clone();
                            combined.extend_from_slice(tail);
                            updated.push(combined);
                        }
                        continue;
                    }
                }
                updated.push(rhs);
            }
            prods.insert(ai.clone(), updated);
        }

        // Partition A_i's rules into left-recursive (`A_i alpha`) and non-left-recursive (`beta`).
        let alts = prods.get(&ai).cloned().unwrap_or_default();
        let mut alphas = Vec::new();
        let mut betas = Vec::new();
        for rhs in alts {
            if let Some(Symbol::NonTerminal(head)) = rhs.first() {
                if head == &ai {
                    alphas.push(rhs[1..].to_vec());
                    continue;
                }
            }
            betas.push(rhs);
        }

        if !alphas.is_empty() {
            // Without at least one base case `beta`, `A_i` generates no finite terminal strings.
            if betas.is_empty() {
                return Err(format!(
                    "nonterminal {ai} has no non-left-recursive base case"
                ));
            }
            // Rewrite `A_i -> A_i alpha | beta` into right-recursive `A_i -> beta A_i'`
            // and `A_i' -> alpha A_i' | eps`.
            let tail_nt = cfg.fresh_name(&ai, &mut used, "'");
            let new_ai: Vec<Vec<Symbol>> = betas
                .into_iter()
                .map(|mut beta| {
                    beta.push(Symbol::NonTerminal(tail_nt.clone()));
                    beta
                })
                .collect();
            let mut new_tail: Vec<Vec<Symbol>> = alphas
                .into_iter()
                .map(|mut alpha| {
                    alpha.push(Symbol::NonTerminal(tail_nt.clone()));
                    alpha
                })
                .collect();
            new_tail.push(Vec::new()); // epsilon
            prods.insert(ai.clone(), new_ai);
            prods.insert(tail_nt.clone(), new_tail);
            order.push(tail_nt);
        }
    }

    Ok(Cfg {
        nonterminals: order,
        start: cfg.start.clone(),
        prods,
    })
}

/// Iteratively extract the longest common non-empty prefix shared by >= 2 alternatives
/// of a nonterminal, moving their differing suffixes into a fresh tail rule.
fn left_factor(cfg: &Cfg) -> Cfg {
    let mut prods = cfg.prods.clone();
    let mut order = cfg.nonterminals.clone();
    let mut used: BTreeSet<String> = order.iter().cloned().collect();

    loop {
        let mut changed = false;
        let snapshot = order.clone();
        for nt in snapshot {
            let alts = prods.get(&nt).cloned().unwrap_or_default();
            if alts.len() < 2 {
                continue;
            }
            // Find longest prefix shared by at least two alternatives.
            let mut best_prefix: Vec<Symbol> = Vec::new();
            for i in 0..alts.len() {
                for j in (i + 1)..alts.len() {
                    let common_len = alts[i]
                        .iter()
                        .zip(&alts[j])
                        .take_while(|(a, b)| a == b)
                        .count();
                    if common_len > best_prefix.len() {
                        best_prefix = alts[i][..common_len].to_vec();
                    }
                }
            }
            if best_prefix.is_empty() {
                continue;
            }
            let fresh = cfg.fresh_name(&nt, &mut used, "_f");
            let prefix_len = best_prefix.len();
            let mut factored_tails = Vec::new();
            let mut remaining = Vec::new();
            // Note: if one alternative equals `best_prefix` exactly, its suffix is `[]` (epsilon).
            for rhs in alts {
                if rhs.starts_with(&best_prefix) {
                    factored_tails.push(rhs[prefix_len..].to_vec());
                } else {
                    remaining.push(rhs);
                }
            }
            let mut new_alt = best_prefix;
            new_alt.push(Symbol::NonTerminal(fresh.clone()));
            remaining.push(new_alt);
            prods.insert(nt.clone(), remaining);
            prods.insert(fresh.clone(), factored_tails);
            order.push(fresh);
            changed = true;
            break;
        }
        if !changed {
            break;
        }
    }

    Cfg {
        nonterminals: order,
        start: cfg.start.clone(),
        prods,
    }
}

/// Detect whether any nonterminal `A` can reach itself along leftmost production symbols (`A =>+ A ...`).
fn has_left_recursion(cfg: &Cfg) -> bool {
    for start in &cfg.nonterminals {
        let mut seen = BTreeSet::new();
        let mut stack = vec![start.clone()];
        while let Some(cur) = stack.pop() {
            if let Some(alts) = cfg.prods.get(&cur) {
                for rhs in alts {
                    if let Some(Symbol::NonTerminal(head)) = rhs.first() {
                        if head == start {
                            return true;
                        }
                        if seen.insert(head.clone()) {
                            stack.push(head.clone());
                        }
                    }
                }
            }
        }
    }
    false
}

fn derive_bounded(cfg: &Cfg, max_len: usize, max_steps: usize) -> BTreeSet<Vec<String>> {
    let mut forms = BTreeSet::from([vec![Symbol::NonTerminal(cfg.start.clone())]]);
    let mut strings = BTreeSet::new();
    for _ in 0..max_steps {
        let mut next_forms = BTreeSet::new();
        for form in &forms {
            let term_count = form
                .iter()
                .filter(|s| matches!(s, Symbol::Terminal(_)))
                .count();
            if term_count > max_len {
                continue;
            }
            if let Some(pos) = form
                .iter()
                .position(|s| matches!(s, Symbol::NonTerminal(_)))
            {
                if let Symbol::NonTerminal(nt) = &form[pos] {
                    for rhs in &cfg.prods[nt] {
                        let mut expanded = form[..pos].to_vec();
                        expanded.extend(rhs.iter().cloned());
                        expanded.extend(form[pos + 1..].iter().cloned());
                        let new_terms = expanded
                            .iter()
                            .filter(|s| matches!(s, Symbol::Terminal(_)))
                            .count();
                        if new_terms <= max_len {
                            next_forms.insert(expanded);
                        }
                    }
                }
            } else {
                let words: Vec<String> = form
                    .iter()
                    .map(|s| match s {
                        Symbol::Terminal(t) => t.clone(),
                        Symbol::NonTerminal(_) => unreachable!(),
                    })
                    .collect();
                strings.insert(words);
            }
        }
        if next_forms.is_empty() {
            break;
        }
        forms = next_forms;
    }
    strings
}

fn format_cfg(cfg: &Cfg) -> String {
    let mut lines = Vec::new();
    for nt in &cfg.nonterminals {
        let alts: Vec<String> = cfg.prods[nt]
            .iter()
            .map(|rhs| {
                if rhs.is_empty() {
                    "eps".to_string()
                } else {
                    rhs.iter()
                        .map(|s| match s {
                            Symbol::Terminal(t) | Symbol::NonTerminal(t) => t.clone(),
                        })
                        .collect::<Vec<_>>()
                        .join(" ")
                }
            })
            .collect();
        lines.push(format!("  {nt} -> {}", alts.join(" | ")));
    }
    lines.join("\n")
}

fn indirect_left_recursive_sample() -> Cfg {
    use Symbol::{NonTerminal as NT, Terminal as T};
    Cfg {
        nonterminals: vec!["S".into(), "A".into()],
        start: "S".into(),
        prods: BTreeMap::from([
            (
                "S".into(),
                vec![vec![NT("A".into()), T("a".into())], vec![T("b".into())]],
            ),
            (
                "A".into(),
                vec![vec![NT("S".into()), T("d".into())], vec![T("c".into())]],
            ),
        ]),
    }
}

fn dangling_else_sample() -> Cfg {
    use Symbol::Terminal as T;
    Cfg {
        nonterminals: vec!["S".into()],
        start: "S".into(),
        prods: BTreeMap::from([(
            "S".into(),
            vec![
                vec![T("i".into()), T("E".into()), T("t".into()), T("S".into())],
                vec![
                    T("i".into()),
                    T("E".into()),
                    T("t".into()),
                    T("S".into()),
                    T("e".into()),
                    T("S".into()),
                ],
                vec![T("a".into())],
            ],
        )]),
    }
}

fn main() {
    let lr = indirect_left_recursive_sample();
    let lr_free = eliminate_left_recursion(&lr).unwrap();
    println!(
        "Original indirect left-recursive grammar:\n{}",
        format_cfg(&lr)
    );
    println!(
        "After left-recursion elimination (left recursive={}):\n{}",
        has_left_recursion(&lr_free),
        format_cfg(&lr_free)
    );

    let df = dangling_else_sample();
    let factored = left_factor(&df);
    println!(
        "After left factoring dangling-else:\n{}",
        format_cfg(&factored)
    );
    println!(
        "Bounded strings (len <= 4): {}",
        derive_bounded(&lr_free, 4, 8).len()
    );
}

#[test]
fn eliminates_indirect_left_recursion_and_preserves_bounded_language() {
    let cfg = indirect_left_recursive_sample();
    assert!(has_left_recursion(&cfg));
    let rewritten = eliminate_left_recursion(&cfg).unwrap();
    assert!(!has_left_recursion(&rewritten));
    assert_eq!(
        derive_bounded(&cfg, 5, 10),
        derive_bounded(&rewritten, 5, 10)
    );
}

#[test]
fn factors_common_prefixes_and_rejects_invalid_grammars() {
    let df = dangling_else_sample();
    let factored = left_factor(&df);
    assert_eq!(factored.prods["S"].len(), 2);
    assert!(factored.nonterminals.len() >= 2);
    assert_eq!(derive_bounded(&df, 6, 4), derive_bounded(&factored, 6, 4));

    let broken = Cfg {
        nonterminals: vec!["S".into()],
        start: "S".into(),
        prods: BTreeMap::from([(
            "S".into(),
            vec![vec![
                Symbol::NonTerminal("S".into()),
                Symbol::Terminal("a".into()),
            ]],
        )]),
    };
    assert!(eliminate_left_recursion(&broken).is_err());

    let unit_cycle = Cfg {
        nonterminals: vec!["S".into()],
        start: "S".into(),
        prods: BTreeMap::from([("S".into(), vec![vec![Symbol::NonTerminal("S".into())]])]),
    };
    assert!(eliminate_left_recursion(&unit_cycle).is_err());
}
