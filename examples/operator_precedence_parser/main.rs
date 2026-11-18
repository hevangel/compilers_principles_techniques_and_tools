//! 1977 Green §5.3 / 1986 Red §4.6: Classical Operator-Precedence Parsing.
//!
//! Explicitly contrasts with `precedence_parse` (LR shift/reduce precedence conflict
//! resolution) by implementing Robert Floyd's classical operator-precedence algorithm:
//! 1. Verifying operator-grammar invariants (no epsilon rules, no adjacent nonterminals),
//! 2. Computing `LEADING(A)` and `TRAILING(A)` sets via fixpoint iteration,
//! 3. Constructing the `<.`, `=.`, `.>` precedence-relation matrix directly from productions,
//! 4. Building integer precedence functions `f(a)` and `g(b)` via longest paths in the
//!    constraint DAG (and detecting cycles when no precedence functions exist),
//! 5. Running shift-reduce handle recognition using top-most stack terminal relations to
//!    build and evaluate an expression AST.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Sym {
    Nt(String),
    T(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Prod {
    lhs: String,
    rhs: Vec<Sym>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum PrecRel {
    Yields, // <.
    Equal,  // =.
    Takes,  // .>
}

impl fmt::Display for PrecRel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            PrecRel::Yields => "<.",
            PrecRel::Equal => "=.",
            PrecRel::Takes => ".>",
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Expr {
    Ident(String),
    Add(Box<Expr>, Box<Expr>),
    Mul(Box<Expr>, Box<Expr>),
}

impl fmt::Display for Expr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Expr::Ident(name) => f.write_str(name),
            Expr::Add(l, r) => write!(f, "({l} + {r})"),
            Expr::Mul(l, r) => write!(f, "({l} * {r})"),
        }
    }
}

fn nt(s: &str) -> Sym {
    Sym::Nt(s.to_string())
}

fn t(s: &str) -> Sym {
    Sym::T(s.to_string())
}

fn expression_operator_grammar() -> (&'static str, Vec<Prod>) {
    (
        "E",
        vec![
            Prod {
                lhs: "E".into(),
                rhs: vec![nt("E"), t("+"), nt("T")],
            },
            Prod {
                lhs: "E".into(),
                rhs: vec![nt("T")],
            },
            Prod {
                lhs: "T".into(),
                rhs: vec![nt("T"), t("*"), nt("F")],
            },
            Prod {
                lhs: "T".into(),
                rhs: vec![nt("F")],
            },
            Prod {
                lhs: "F".into(),
                rhs: vec![t("("), nt("E"), t(")")],
            },
            Prod {
                lhs: "F".into(),
                rhs: vec![t("id")],
            },
        ],
    )
}

fn verify_operator_grammar(prods: &[Prod]) -> Result<(), String> {
    for p in prods {
        if p.rhs.is_empty() {
            return Err(format!(
                "{} -> ε violates operator-grammar no-epsilon rule",
                p.lhs
            ));
        }
        for pair in p.rhs.windows(2) {
            if let [Sym::Nt(a), Sym::Nt(b)] = pair {
                return Err(format!(
                    "{} -> ... {a} {b} ... has adjacent nonterminals",
                    p.lhs
                ));
            }
        }
    }
    Ok(())
}

fn compute_leading_trailing(
    prods: &[Prod],
) -> (
    BTreeMap<String, BTreeSet<String>>,
    BTreeMap<String, BTreeSet<String>>,
) {
    let mut leading: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut trailing: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for p in prods {
        leading.entry(p.lhs.clone()).or_default();
        trailing.entry(p.lhs.clone()).or_default();
    }

    let mut changed = true;
    while changed {
        changed = false;
        for p in prods {
            let rhs = &p.rhs;
            // LEADING(A): first terminal or terminal immediately after leading nonterminal,
            // plus transitive closure LEADING(B) when RHS starts with B.
            match rhs.first() {
                Some(Sym::T(a)) => {
                    if leading.get_mut(&p.lhs).unwrap().insert(a.clone()) {
                        changed = true;
                    }
                }
                Some(Sym::Nt(b)) => {
                    let b_lead = leading.get(b).cloned().unwrap_or_default();
                    let a_lead = leading.get_mut(&p.lhs).unwrap();
                    for term in b_lead {
                        if a_lead.insert(term) {
                            changed = true;
                        }
                    }
                    if let Some(Sym::T(a)) = rhs.get(1) {
                        if a_lead.insert(a.clone()) {
                            changed = true;
                        }
                    }
                }
                None => {}
            }

            // TRAILING(A): symmetric rightmost scan.
            match rhs.last() {
                Some(Sym::T(a)) => {
                    if trailing.get_mut(&p.lhs).unwrap().insert(a.clone()) {
                        changed = true;
                    }
                }
                Some(Sym::Nt(b)) => {
                    let b_trail = trailing.get(b).cloned().unwrap_or_default();
                    let a_trail = trailing.get_mut(&p.lhs).unwrap();
                    for term in b_trail {
                        if a_trail.insert(term) {
                            changed = true;
                        }
                    }
                    if rhs.len() >= 2 {
                        if let Sym::T(a) = &rhs[rhs.len() - 2] {
                            if a_trail.insert(a.clone()) {
                                changed = true;
                            }
                        }
                    }
                }
                None => {}
            }
        }
    }

    (leading, trailing)
}

type PrecMatrix = BTreeMap<(String, String), PrecRel>;

fn build_precedence_matrix(start: &str, prods: &[Prod]) -> Result<PrecMatrix, String> {
    verify_operator_grammar(prods)?;
    let (leading, trailing) = compute_leading_trailing(prods);
    let mut matrix: PrecMatrix = BTreeMap::new();

    let mut insert_rel = |a: String, b: String, rel: PrecRel| -> Result<(), String> {
        if let Some(&prev) = matrix.get(&(a.clone(), b.clone())) {
            if prev != rel {
                return Err(format!(
                    "operator-precedence conflict at ({a}, {b}): {prev} vs {rel}"
                ));
            }
        } else {
            matrix.insert((a, b), rel);
        }
        Ok(())
    };

    for p in prods {
        let rhs = &p.rhs;
        for i in 0..rhs.len() {
            if let Sym::T(a) = &rhs[i] {
                // Case 1: ... a b ... => a =. b
                if let Some(Sym::T(b)) = rhs.get(i + 1) {
                    insert_rel(a.clone(), b.clone(), PrecRel::Equal)?;
                }
                // Case 2: ... a B b ... => a =. b
                if let (Some(Sym::Nt(_)), Some(Sym::T(b))) = (rhs.get(i + 1), rhs.get(i + 2)) {
                    insert_rel(a.clone(), b.clone(), PrecRel::Equal)?;
                }
                // Case 3: ... a B ... => for all b in LEADING(B), a <. b
                if let Some(Sym::Nt(b_nt)) = rhs.get(i + 1) {
                    for b in leading.get(b_nt).into_iter().flatten() {
                        insert_rel(a.clone(), b.clone(), PrecRel::Yields)?;
                    }
                }
            } else if let Sym::Nt(b_nt) = &rhs[i] {
                // Case 4: ... B a ... => for all c in TRAILING(B), c .> a
                if let Some(Sym::T(a)) = rhs.get(i + 1) {
                    for c in trailing.get(b_nt).into_iter().flatten() {
                        insert_rel(c.clone(), a.clone(), PrecRel::Takes)?;
                    }
                }
            }
        }
    }

    // Augmented `$ S $`: `$ <. LEADING(S)` and `TRAILING(S) .> $`
    for b in leading.get(start).into_iter().flatten() {
        insert_rel("$".into(), b.clone(), PrecRel::Yields)?;
    }
    for a in trailing.get(start).into_iter().flatten() {
        insert_rel(a.clone(), "$".into(), PrecRel::Takes)?;
    }

    Ok(matrix)
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PrecFunctions {
    f: BTreeMap<String, usize>,
    g: BTreeMap<String, usize>,
}

/// Dragon Book Algorithm 4.6: merge `=.` pairs into equivalence groups via union-find,
/// add directed edges (`a .> b` => `f_a -> g_b`; `a <. b` => `g_b -> f_a`), and compute
/// longest-path heights in the DAG (or report cycle if no precedence functions exist).
fn build_precedence_functions(
    terminals: &[&str],
    matrix: &PrecMatrix,
) -> Result<PrecFunctions, String> {
    // Node indices: 0..T are f(term_i), T..2T are g(term_i).
    let tcount = terminals.len();
    let idx_of: BTreeMap<&str, usize> =
        terminals.iter().enumerate().map(|(i, &s)| (s, i)).collect();
    let total = 2 * tcount;
    let mut parent: Vec<usize> = (0..total).collect();

    fn find(parent: &mut [usize], mut x: usize) -> usize {
        while parent[x] != x {
            parent[x] = parent[parent[x]];
            x = parent[x];
        }
        x
    }
    fn union(parent: &mut [usize], a: usize, b: usize) {
        let ra = find(parent, a);
        let rb = find(parent, b);
        if ra != rb {
            parent[rb] = ra;
        }
    }

    // Merge f(a) and g(b) whenever a =. b.
    for ((a, b), &rel) in matrix {
        if rel == PrecRel::Equal {
            let fa = idx_of[a.as_str()];
            let gb = tcount + idx_of[b.as_str()];
            union(&mut parent, fa, gb);
        }
    }

    let mut edges: BTreeMap<usize, BTreeSet<usize>> = BTreeMap::new();
    let mut reps: BTreeSet<usize> = BTreeSet::new();
    for u in 0..total {
        let r = find(&mut parent, u);
        reps.insert(r);
        edges.entry(r).or_default();
    }

    for ((a, b), &rel) in matrix {
        let fa = find(&mut parent, idx_of[a.as_str()]);
        let gb = find(&mut parent, tcount + idx_of[b.as_str()]);
        match rel {
            PrecRel::Takes => {
                if fa == gb {
                    return Err("precedence graph contains self-loop on .>".to_string());
                }
                edges.get_mut(&fa).unwrap().insert(gb);
            }
            PrecRel::Yields => {
                if fa == gb {
                    return Err("precedence graph contains self-loop on <.".to_string());
                }
                edges.get_mut(&gb).unwrap().insert(fa);
            }
            PrecRel::Equal => {}
        }
    }

    // Compute longest path from sinks via DFS with 3-color cycle detection.
    #[derive(Clone, Copy, PartialEq, Eq)]
    enum Color {
        Unvisited,
        Visiting,
        Done,
    }
    let mut color: BTreeMap<usize, Color> = reps.iter().map(|&r| (r, Color::Unvisited)).collect();
    let mut height: BTreeMap<usize, usize> = BTreeMap::new();

    fn dfs(
        u: usize,
        edges: &BTreeMap<usize, BTreeSet<usize>>,
        color: &mut BTreeMap<usize, Color>,
        height: &mut BTreeMap<usize, usize>,
    ) -> Result<usize, String> {
        match color[&u] {
            Color::Done => return Ok(height[&u]),
            Color::Visiting => {
                return Err("cycle in precedence graph; no precedence functions exist".into())
            }
            Color::Unvisited => {}
        }
        color.insert(u, Color::Visiting);
        let mut h = 1usize;
        for &v in &edges[&u] {
            h = h.max(1 + dfs(v, edges, color, height)?);
        }
        color.insert(u, Color::Done);
        height.insert(u, h);
        Ok(h)
    }

    for &r in &reps {
        dfs(r, &edges, &mut color, &mut height)?;
    }

    let mut f = BTreeMap::new();
    let mut g = BTreeMap::new();
    for (i, &term) in terminals.iter().enumerate() {
        f.insert(term.to_string(), height[&find(&mut parent, i)]);
        g.insert(term.to_string(), height[&find(&mut parent, tcount + i)]);
    }

    Ok(PrecFunctions { f, g })
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum StackItem {
    Term(String, Option<String>),
    NonTerm(Expr),
}

#[derive(Clone, Debug)]
struct TraceStep {
    stack: String,
    rel: String,
    lookahead: String,
    action: String,
}

fn tokenize_op_expr(src: &str) -> Result<Vec<(String, String)>, String> {
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
        match bytes[i] {
            b'+' => {
                i += 1;
                out.push(("+".into(), "+".into()));
            }
            b'*' => {
                i += 1;
                out.push(("*".into(), "*".into()));
            }
            b'(' => {
                i += 1;
                out.push(("(".into(), "(".into()));
            }
            b')' => {
                i += 1;
                out.push((")".into(), ")".into()));
            }
            other => return Err(format!("unexpected character '{}'", other as char)),
        }
    }
    out.push(("$".into(), "$".into()));
    Ok(out)
}

fn parse_operator_precedence(
    src: &str,
    matrix: &PrecMatrix,
    funcs: Option<&PrecFunctions>,
) -> Result<(Expr, Vec<TraceStep>), String> {
    let tokens = tokenize_op_expr(src)?;
    let mut stack: Vec<StackItem> = vec![StackItem::Term("$".into(), None)];
    let mut ip = 0usize;
    let mut trace = Vec::new();

    let format_stack = |st: &[StackItem]| -> String {
        st.iter()
            .map(|it| match it {
                StackItem::Term(t, Some(lex)) if t == "id" => format!("id({lex})"),
                StackItem::Term(t, _) => t.clone(),
                StackItem::NonTerm(_) => "E".into(),
            })
            .collect::<Vec<_>>()
            .join(" ")
    };

    loop {
        let top_t_idx = stack
            .iter()
            .rposition(|it| matches!(it, StackItem::Term(_, _)))
            .ok_or("missing bottom marker '$'")?;
        let a = match &stack[top_t_idx] {
            StackItem::Term(t, _) => t.clone(),
            _ => unreachable!(),
        };
        let (b, b_lex) = tokens[ip].clone();

        if a == "$" && b == "$" {
            if stack.len() == 2 {
                if let StackItem::NonTerm(expr) = &stack[1] {
                    trace.push(TraceStep {
                        stack: format_stack(&stack),
                        rel: "acc".into(),
                        lookahead: "$".into(),
                        action: "accept".into(),
                    });
                    return Ok((expr.clone(), trace));
                }
            }
            return Err("unexpected end of input".to_string());
        }

        let rel = matrix
            .get(&(a.clone(), b.clone()))
            .copied()
            .ok_or_else(|| {
                format!("syntax error: no precedence relation between '{a}' and '{b}'")
            })?;

        if let Some(pf) = funcs {
            let fa = pf.f[&a];
            let gb = pf.g[&b];
            match rel {
                PrecRel::Yields => assert!(fa < gb),
                PrecRel::Equal => assert_eq!(fa, gb),
                PrecRel::Takes => assert!(fa > gb),
            }
        }

        let stack_snap = format_stack(&stack);
        match rel {
            PrecRel::Yields | PrecRel::Equal => {
                trace.push(TraceStep {
                    stack: stack_snap,
                    rel: format!("{a} {rel} {b}"),
                    lookahead: b_lex.clone(),
                    action: format!("shift {b_lex}"),
                });
                stack.push(StackItem::Term(b, Some(b_lex)));
                ip += 1;
            }
            PrecRel::Takes => {
                // Pop handle up to and including the leftmost terminal `last_popped`
                // until `next_term_on_stack <. last_popped`.
                let mut handle: Vec<StackItem> = Vec::new();
                loop {
                    while let Some(StackItem::NonTerm(_)) = stack.last() {
                        handle.push(stack.pop().unwrap());
                    }
                    let popped_t = match stack.pop() {
                        Some(item @ StackItem::Term(_, _)) => {
                            let name = match &item {
                                StackItem::Term(n, _) => n.clone(),
                                _ => unreachable!(),
                            };
                            handle.push(item);
                            name
                        }
                        _ => return Err("empty handle during reduction".into()),
                    };
                    while let Some(StackItem::NonTerm(_)) = stack.last() {
                        handle.push(stack.pop().unwrap());
                    }
                    let prev_t = stack
                        .iter()
                        .rev()
                        .find_map(|it| match it {
                            StackItem::Term(n, _) => Some(n.clone()),
                            _ => None,
                        })
                        .ok_or("missing stack sentinel")?;
                    if matrix.get(&(prev_t, popped_t)) == Some(&PrecRel::Yields) {
                        break;
                    }
                }
                handle.reverse();
                let reduced = reduce_prime_phrase(handle)?;
                trace.push(TraceStep {
                    stack: stack_snap,
                    rel: format!("{a} .> {b}"),
                    lookahead: b_lex,
                    action: format!("reduce -> {reduced}"),
                });
                stack.push(StackItem::NonTerm(reduced));
            }
        }
    }
}

fn reduce_prime_phrase(handle: Vec<StackItem>) -> Result<Expr, String> {
    match handle.as_slice() {
        [StackItem::Term(t, Some(lex))] if t == "id" => Ok(Expr::Ident(lex.clone())),
        [StackItem::NonTerm(l), StackItem::Term(op, _), StackItem::NonTerm(r)] if op == "+" => {
            Ok(Expr::Add(Box::new(l.clone()), Box::new(r.clone())))
        }
        [StackItem::NonTerm(l), StackItem::Term(op, _), StackItem::NonTerm(r)] if op == "*" => {
            Ok(Expr::Mul(Box::new(l.clone()), Box::new(r.clone())))
        }
        [StackItem::Term(lp, _), StackItem::NonTerm(e), StackItem::Term(rp, _)]
            if lp == "(" && rp == ")" =>
        {
            Ok(e.clone())
        }
        _ => Err(format!("invalid prime phrase handle: {handle:?}")),
    }
}

fn main() {
    let (start, prods) = expression_operator_grammar();
    let (leading, trailing) = compute_leading_trailing(&prods);
    let matrix = build_precedence_matrix(start, &prods).expect("valid operator-precedence grammar");
    let terminals = ["+", "*", "(", ")", "id", "$"];
    let funcs = build_precedence_functions(&terminals, &matrix).expect("acyclic precedence DAG");

    println!("=== Classical Operator-Precedence Parsing (Green §5.3 / Red §4.6) ===");
    println!("LEADING sets:");
    for (nt_name, set) in &leading {
        println!(
            "  LEADING({nt_name})  = {{{}}}",
            set.iter().cloned().collect::<Vec<_>>().join(", ")
        );
    }
    println!("TRAILING sets:");
    for (nt_name, set) in &trailing {
        println!(
            "  TRAILING({nt_name}) = {{{}}}",
            set.iter().cloned().collect::<Vec<_>>().join(", ")
        );
    }

    println!("\nPrecedence Relation Matrix (<., =., .>):");
    print!("     ");
    for col in &terminals {
        print!("{col:>4}");
    }
    println!();
    for row in &terminals {
        print!("  {row:<3}");
        for col in &terminals {
            let cell = matrix
                .get(&(row.to_string(), col.to_string()))
                .map(|r| r.to_string())
                .unwrap_or_else(|| "·".into());
            print!("{cell:>4}");
        }
        println!();
    }

    println!("\nCompacted Precedence Functions f(a), g(a):");
    println!(
        "  f: {}",
        terminals
            .iter()
            .map(|t| format!("{t}={}", funcs.f[*t]))
            .collect::<Vec<_>>()
            .join("  ")
    );
    println!(
        "  g: {}",
        terminals
            .iter()
            .map(|t| format!("{t}={}", funcs.g[*t]))
            .collect::<Vec<_>>()
            .join("  ")
    );

    let sample = "a + b * (c + d)";
    let (ast, trace) =
        parse_operator_precedence(sample, &matrix, Some(&funcs)).expect("parse succeeds");
    println!("\nShift-reduce trace on {sample:?} -> {ast}:");
    for step in &trace {
        println!(
            "  {:<24} | {:<10} | {:<4} | {}",
            step.stack, step.rel, step.lookahead, step.action
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_non_operator_grammars_with_epsilon_or_adjacent_nonterminals() {
        let eps_grammar = vec![Prod {
            lhs: "E".into(),
            rhs: vec![],
        }];
        assert!(verify_operator_grammar(&eps_grammar).is_err());

        let adj_grammar = vec![Prod {
            lhs: "S".into(),
            rhs: vec![nt("A"), nt("B")],
        }];
        assert!(verify_operator_grammar(&adj_grammar).is_err());
    }

    #[test]
    fn matrix_and_precedence_functions_agree_and_parse_precedence() {
        let (start, prods) = expression_operator_grammar();
        let matrix = build_precedence_matrix(start, &prods).unwrap();
        let terminals = ["+", "*", "(", ")", "id", "$"];
        let funcs = build_precedence_functions(&terminals, &matrix).unwrap();

        let (ast, _) = parse_operator_precedence("a + b * c", &matrix, Some(&funcs)).unwrap();
        assert_eq!(ast.to_string(), "(a + (b * c))");

        let (ast2, _) =
            parse_operator_precedence("(a + b) * c + d", &matrix, Some(&funcs)).unwrap();
        assert_eq!(ast2.to_string(), "(((a + b) * c) + d)");
    }

    #[test]
    fn detects_cyclic_precedence_relations_without_functions() {
        let mut cyclic = BTreeMap::new();
        cyclic.insert(("a".into(), "b".into()), PrecRel::Yields);
        cyclic.insert(("a".into(), "a".into()), PrecRel::Takes);
        cyclic.insert(("b".into(), "b".into()), PrecRel::Takes);
        cyclic.insert(("b".into(), "a".into()), PrecRel::Yields);
        assert!(build_precedence_functions(&["a", "b"], &cyclic).is_err());
    }
}
