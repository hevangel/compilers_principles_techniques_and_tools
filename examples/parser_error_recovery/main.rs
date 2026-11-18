//! Chapter 4 (§4.4.5, §4.7–4.8): LL(1) Synchronization Sets and LR Error Recovery.
//!
//! Demonstrates:
//! 1. Computing `FIRST` and `FOLLOW` sets to populate an LL(1) table with explicit
//!    `Synch` entries alongside production entries,
//! 2. LL(1) panic + phrase-level recovery:
//!    - `Synch` pops the nonterminal on stack,
//!    - Empty cell skips an unexpected token,
//!    - Missing expected delimiter (e.g., `;` before `let`/`emit`/`$`) performs virtual
//!      single-token insertion,
//!    - Error cooldown prevents cascading secondary messages until real tokens advance,
//! 3. SLR(1) table-integrated `error` recovery (`S -> error ;`):
//!    - Unwinds the LR state stack to the nearest state with a valid shift on `error`,
//!    - Shifts the `error` pseudo-token, discards tokens until a valid follow token (`;`),
//!      and resumes parsing remaining statements in the same pass.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

#[derive(Clone, Debug, PartialEq, Eq)]
struct Token {
    kind: String, // let | emit | id | int | = | + | ( | ) | ; | $
    lexeme: String,
    pos: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ParseDiagnostic {
    pos: usize,
    phase: String,
    message: String,
}

fn tokenize(src: &str) -> Vec<Token> {
    let bytes = src.as_bytes();
    let mut i = 0;
    let mut out = Vec::new();
    while i < bytes.len() {
        if bytes[i].is_ascii_whitespace() {
            i += 1;
            continue;
        }
        let start = i;
        if bytes[i].is_ascii_alphabetic() || bytes[i] == b'_' {
            i += 1;
            while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                i += 1;
            }
            let lex = src[start..i].to_string();
            let kind = match lex.as_str() {
                "let" => "let",
                "emit" => "emit",
                _ => "id",
            };
            out.push(Token {
                kind: kind.into(),
                lexeme: lex,
                pos: start,
            });
            continue;
        }
        if bytes[i].is_ascii_digit() {
            i += 1;
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                i += 1;
            }
            out.push(Token {
                kind: "int".into(),
                lexeme: src[start..i].to_string(),
                pos: start,
            });
            continue;
        }
        let kind = match bytes[i] {
            b'=' => "=",
            b'+' => "+",
            b'(' => "(",
            b')' => ")",
            b';' => ";",
            other => {
                i += 1;
                out.push(Token {
                    kind: "bad".into(),
                    lexeme: (other as char).to_string(),
                    pos: start,
                });
                continue;
            }
        };
        i += 1;
        out.push(Token {
            kind: kind.into(),
            lexeme: kind.into(),
            pos: start,
        });
    }
    out.push(Token {
        kind: "$".into(),
        lexeme: "$".into(),
        pos: src.len(),
    });
    out
}

// ---------------------------------------------------------------------------
// Part 1: LL(1) Grammar, FIRST / FOLLOW, Synch Table & Table-Driven Recovery
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq)]
enum Sym {
    Nt(&'static str),
    T(&'static str),
}

#[derive(Clone, Debug)]
struct LlProd {
    lhs: &'static str,
    rhs: Vec<Sym>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum LlEntry {
    Prod(usize),
    Synch,
}

struct Ll1Table {
    prods: Vec<LlProd>,
    first: BTreeMap<&'static str, BTreeSet<&'static str>>,
    follow: BTreeMap<&'static str, BTreeSet<&'static str>>,
    table: BTreeMap<(&'static str, &'static str), LlEntry>,
}

fn build_ll1_table() -> Ll1Table {
    use Sym::{Nt, T};
    let prods = vec![
        LlProd {
            lhs: "Program",
            rhs: vec![Nt("StmtList")],
        },
        LlProd {
            lhs: "StmtList",
            rhs: vec![Nt("Stmt"), Nt("StmtList")],
        },
        LlProd {
            lhs: "StmtList",
            rhs: vec![],
        },
        LlProd {
            lhs: "Stmt",
            rhs: vec![T("let"), T("id"), T("="), Nt("Expr"), T(";")],
        },
        LlProd {
            lhs: "Stmt",
            rhs: vec![T("emit"), Nt("Expr"), T(";")],
        },
        LlProd {
            lhs: "Expr",
            rhs: vec![Nt("Term"), Nt("ExprP")],
        },
        LlProd {
            lhs: "ExprP",
            rhs: vec![T("+"), Nt("Term"), Nt("ExprP")],
        },
        LlProd {
            lhs: "ExprP",
            rhs: vec![],
        },
        LlProd {
            lhs: "Term",
            rhs: vec![T("id")],
        },
        LlProd {
            lhs: "Term",
            rhs: vec![T("int")],
        },
        LlProd {
            lhs: "Term",
            rhs: vec![T("("), Nt("Expr"), T(")")],
        },
    ];
    let nts = ["Program", "StmtList", "Stmt", "Expr", "ExprP", "Term"];
    let mut first: BTreeMap<&'static str, BTreeSet<&'static str>> =
        nts.iter().map(|&n| (n, BTreeSet::new())).collect();
    let mut nullable: BTreeMap<&'static str, bool> = nts.iter().map(|&n| (n, false)).collect();

    let mut changed = true;
    while changed {
        changed = false;
        for p in &prods {
            let mut all_null = true;
            for sym in &p.rhs {
                match sym {
                    T(t) => {
                        if first.get_mut(p.lhs).unwrap().insert(t) {
                            changed = true;
                        }
                        all_null = false;
                        break;
                    }
                    Nt(nt) => {
                        let copy = first[nt].clone();
                        for t in copy {
                            if first.get_mut(p.lhs).unwrap().insert(t) {
                                changed = true;
                            }
                        }
                        if !nullable[nt] {
                            all_null = false;
                            break;
                        }
                    }
                }
            }
            if all_null && !nullable[p.lhs] {
                nullable.insert(p.lhs, true);
                changed = true;
            }
        }
    }

    let mut follow: BTreeMap<&'static str, BTreeSet<&'static str>> =
        nts.iter().map(|&n| (n, BTreeSet::new())).collect();
    follow.get_mut("Program").unwrap().insert("$");
    changed = true;
    while changed {
        changed = false;
        for p in &prods {
            let mut trailer = follow[p.lhs].clone();
            for sym in p.rhs.iter().rev() {
                match sym {
                    T(t) => {
                        trailer.clear();
                        trailer.insert(t);
                    }
                    Nt(nt) => {
                        let fset = follow.get_mut(nt).unwrap();
                        for &t in &trailer {
                            if fset.insert(t) {
                                changed = true;
                            }
                        }
                        if nullable[nt] {
                            for &t in &first[nt] {
                                trailer.insert(t);
                            }
                        } else {
                            trailer = first[nt].clone();
                        }
                    }
                }
            }
        }
    }

    let first_of_rhs = |rhs: &[Sym]| -> (BTreeSet<&'static str>, bool) {
        let mut out = BTreeSet::new();
        let mut eps = true;
        for sym in rhs {
            match sym {
                T(t) => {
                    out.insert(*t);
                    eps = false;
                    break;
                }
                Nt(nt) => {
                    for &t in &first[nt] {
                        out.insert(t);
                    }
                    if !nullable[nt] {
                        eps = false;
                        break;
                    }
                }
            }
        }
        (out, eps)
    };

    let mut table = BTreeMap::new();
    for (idx, p) in prods.iter().enumerate() {
        let (fset, eps) = first_of_rhs(&p.rhs);
        for t in fset {
            table.insert((p.lhs, t), LlEntry::Prod(idx));
        }
        if eps {
            for &t in &follow[p.lhs] {
                table.insert((p.lhs, t), LlEntry::Prod(idx));
            }
        }
    }
    // Insert explicit `Synch` tokens from FOLLOW(A) ∪ {";", "$"} (plus enclosing
    // statement starters for inner expression nonterminals) into empty table cells.
    for &nt in &nts {
        let mut synchs = follow[nt].clone();
        synchs.insert(";");
        synchs.insert("$");
        if matches!(nt, "Expr" | "ExprP" | "Term") {
            synchs.insert("let");
            synchs.insert("emit");
        }
        for t in synchs {
            table.entry((nt, t)).or_insert(LlEntry::Synch);
        }
    }

    Ll1Table {
        prods,
        first,
        follow,
        table,
    }
}

fn ll1_parse_with_recovery(src: &str, ll: &Ll1Table) -> (usize, Vec<ParseDiagnostic>, Vec<String>) {
    let tokens = tokenize(src);
    let mut stack: Vec<Sym> = vec![Sym::T("$"), Sym::Nt("Program")];
    let mut ip = 0usize;
    let mut valid_stmts = 0usize;
    let mut diags = Vec::new();
    let mut actions = Vec::new();
    let mut recovering = false;
    let mut stmt_has_error = false;

    while let Some(top) = stack.pop() {
        let tok = &tokens[ip];
        match top {
            Sym::T(exp) => {
                if exp == "$" && tok.kind == "$" {
                    break;
                }
                if exp == tok.kind {
                    if exp == ";" && !stmt_has_error {
                        valid_stmts += 1;
                    }
                    if exp == ";" {
                        stmt_has_error = false;
                    }
                    ip += 1;
                    recovering = false;
                } else {
                    stmt_has_error = true;
                    // Phrase-level insertion for missing ';' or ')' when lookahead belongs to next stmt/EOF.
                    if !recovering {
                        diags.push(ParseDiagnostic {
                            pos: tok.pos,
                            phase: "LL(1)".into(),
                            message: format!("expected '{}', found '{}'", exp, tok.lexeme),
                        });
                        recovering = true;
                    }
                    if exp == ";" || exp == ")" {
                        actions.push(format!("insert missing '{exp}' before '{}'", tok.lexeme));
                    } else if exp != "$" && tok.kind != "$" {
                        actions.push(format!("pop mismatched terminal '{exp}'"));
                    }
                }
            }
            Sym::Nt(nt) => {
                if nt == "Stmt" {
                    stmt_has_error = false;
                }
                match ll.table.get(&(nt, tok.kind.as_str())) {
                    Some(LlEntry::Prod(idx)) => {
                        let rhs = &ll.prods[*idx].rhs;
                        for sym in rhs.iter().rev() {
                            stack.push(sym.clone());
                        }
                        recovering = false;
                    }
                    Some(LlEntry::Synch) => {
                        stmt_has_error = true;
                        if !recovering {
                            diags.push(ParseDiagnostic {
                                pos: tok.pos,
                                phase: "LL(1)".into(),
                                message: format!(
                                    "unexpected '{}' while expanding {nt}",
                                    tok.lexeme
                                ),
                            });
                            recovering = true;
                        }
                        actions.push(format!("synch: pop nonterminal {nt} on '{}'", tok.lexeme));
                    }
                    None => {
                        stmt_has_error = true;
                        if !recovering {
                            diags.push(ParseDiagnostic {
                                pos: tok.pos,
                                phase: "LL(1)".into(),
                                message: format!(
                                    "unexpected '{}' in {nt}; scanning to synch token",
                                    tok.lexeme
                                ),
                            });
                            recovering = true;
                        }
                        if tok.kind == "$" {
                            actions.push(format!("EOF reached; aborting {nt}"));
                        } else {
                            actions.push(format!(
                                "skip token '{}' while seeking synch for {nt}",
                                tok.lexeme
                            ));
                            ip += 1;
                            stack.push(Sym::Nt(nt));
                        }
                    }
                }
            }
        }
    }

    (valid_stmts, diags, actions)
}

// ---------------------------------------------------------------------------
// Part 2: SLR(1) Table with `error` Synchronization Production (`S -> error ;`)
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct LrProd {
    lhs: String,
    rhs: Vec<String>,
    is_error_prod: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum LrAction {
    Shift(usize),
    Reduce(usize),
    Accept,
}

impl fmt::Display for LrAction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LrAction::Shift(s) => write!(f, "s{s}"),
            LrAction::Reduce(r) => write!(f, "r{r}"),
            LrAction::Accept => f.write_str("acc"),
        }
    }
}

struct SlrParser {
    prods: Vec<LrProd>,
    action: BTreeMap<(usize, String), LrAction>,
    goto: BTreeMap<(usize, String), usize>,
}

fn build_slr_with_error_prod() -> SlrParser {
    let prods = vec![
        LrProd {
            lhs: "S'".into(),
            rhs: vec!["P".into()],
            is_error_prod: false,
        },
        LrProd {
            lhs: "P".into(),
            rhs: vec!["P".into(), "S".into()],
            is_error_prod: false,
        },
        LrProd {
            lhs: "P".into(),
            rhs: vec!["S".into()],
            is_error_prod: false,
        },
        LrProd {
            lhs: "S".into(),
            rhs: vec![
                "let".into(),
                "id".into(),
                "=".into(),
                "E".into(),
                ";".into(),
            ],
            is_error_prod: false,
        },
        LrProd {
            lhs: "S".into(),
            rhs: vec!["emit".into(), "E".into(), ";".into()],
            is_error_prod: false,
        },
        LrProd {
            lhs: "S".into(),
            rhs: vec!["error".into(), ";".into()],
            is_error_prod: true,
        },
        LrProd {
            lhs: "E".into(),
            rhs: vec!["E".into(), "+".into(), "T".into()],
            is_error_prod: false,
        },
        LrProd {
            lhs: "E".into(),
            rhs: vec!["T".into()],
            is_error_prod: false,
        },
        LrProd {
            lhs: "T".into(),
            rhs: vec!["id".into()],
            is_error_prod: false,
        },
        LrProd {
            lhs: "T".into(),
            rhs: vec!["int".into()],
            is_error_prod: false,
        },
        LrProd {
            lhs: "T".into(),
            rhs: vec!["(".into(), "E".into(), ")".into()],
            is_error_prod: false,
        },
    ];

    let nts: BTreeSet<String> = prods.iter().map(|p| p.lhs.clone()).collect();
    let mut all_syms: BTreeSet<String> = nts.clone();
    for p in &prods {
        for s in &p.rhs {
            all_syms.insert(s.clone());
        }
    }
    let terminals: Vec<String> = all_syms
        .iter()
        .filter(|s| !nts.contains(*s))
        .cloned()
        .collect();

    // Compute FOLLOW sets for SLR reductions
    let mut first: BTreeMap<String, BTreeSet<String>> =
        nts.iter().map(|n| (n.clone(), BTreeSet::new())).collect();
    let mut changed = true;
    while changed {
        changed = false;
        for p in &prods {
            if let Some(s) = p.rhs.first() {
                if !nts.contains(s) {
                    if first.get_mut(&p.lhs).unwrap().insert(s.clone()) {
                        changed = true;
                    }
                } else {
                    let c = first[s].clone();
                    for t in c {
                        if first.get_mut(&p.lhs).unwrap().insert(t) {
                            changed = true;
                        }
                    }
                }
            }
        }
    }
    let mut follow: BTreeMap<String, BTreeSet<String>> =
        nts.iter().map(|n| (n.clone(), BTreeSet::new())).collect();
    follow.get_mut("S'").unwrap().insert("$".into());
    changed = true;
    while changed {
        changed = false;
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
                            changed = true;
                        }
                    }
                    trailer = first[s].clone();
                }
            }
        }
    }

    type Item = (usize, usize); // (prod_id, dot)
    let closure = |seed: BTreeSet<Item>| -> BTreeSet<Item> {
        let mut set = seed;
        let mut ch = true;
        while ch {
            ch = false;
            let cur: Vec<Item> = set.iter().copied().collect();
            for (pid, dot) in cur {
                if let Some(sym) = prods[pid].rhs.get(dot) {
                    if nts.contains(sym) {
                        for (npid, np) in prods.iter().enumerate() {
                            if &np.lhs == sym && set.insert((npid, 0)) {
                                ch = true;
                            }
                        }
                    }
                }
            }
        }
        set
    };

    let goto_items = |items: &BTreeSet<Item>, sym: &str| -> BTreeSet<Item> {
        let mut seed = BTreeSet::new();
        for &(pid, dot) in items {
            if prods[pid].rhs.get(dot).map(String::as_str) == Some(sym) {
                seed.insert((pid, dot + 1));
            }
        }
        if seed.is_empty() {
            seed
        } else {
            closure(seed)
        }
    };

    let mut states: Vec<BTreeSet<Item>> = Vec::new();
    let mut state_id: BTreeMap<BTreeSet<Item>, usize> = BTreeMap::new();
    let mut transitions: BTreeMap<(usize, String), usize> = BTreeMap::new();

    let start = closure(BTreeSet::from([(0, 0)]));
    state_id.insert(start.clone(), 0);
    states.push(start);

    let mut idx = 0usize;
    while idx < states.len() {
        let cur_items = states[idx].clone();
        for sym in &all_syms {
            let next_set = goto_items(&cur_items, sym);
            if next_set.is_empty() {
                continue;
            }
            let target = match state_id.get(&next_set) {
                Some(&id) => id,
                None => {
                    let id = states.len();
                    state_id.insert(next_set.clone(), id);
                    states.push(next_set);
                    id
                }
            };
            transitions.insert((idx, sym.clone()), target);
        }
        idx += 1;
    }

    let mut action = BTreeMap::new();
    let mut goto = BTreeMap::new();
    for (sid, items) in states.iter().enumerate() {
        for sym in &terminals {
            if let Some(&target) = transitions.get(&(sid, sym.clone())) {
                action.insert((sid, sym.clone()), LrAction::Shift(target));
            }
        }
        for nt in &nts {
            if let Some(&target) = transitions.get(&(sid, nt.clone())) {
                goto.insert((sid, nt.clone()), target);
            }
        }
        for &(pid, dot) in items {
            if dot == prods[pid].rhs.len() {
                if pid == 0 {
                    action.insert((sid, "$".into()), LrAction::Accept);
                } else {
                    for f in &follow[&prods[pid].lhs] {
                        action
                            .entry((sid, f.clone()))
                            .or_insert(LrAction::Reduce(pid));
                    }
                }
            }
        }
    }

    SlrParser {
        prods,
        action,
        goto,
    }
}

fn slr_parse_with_recovery(
    src: &str,
    lr: &SlrParser,
) -> (usize, Vec<ParseDiagnostic>, Vec<String>) {
    let tokens = tokenize(src);
    let mut stack: Vec<usize> = vec![0];
    let mut ip = 0usize;
    let mut valid_stmts = 0usize;
    let mut diags = Vec::new();
    let mut actions = Vec::new();

    loop {
        let state = *stack.last().unwrap();
        let tok = &tokens[ip];
        match lr.action.get(&(state, tok.kind.clone())) {
            Some(LrAction::Shift(next)) => {
                stack.push(*next);
                ip += 1;
            }
            Some(LrAction::Reduce(pid)) => {
                let prod = &lr.prods[*pid];
                for _ in 0..prod.rhs.len() {
                    stack.pop();
                }
                if prod.lhs == "S" && !prod.is_error_prod {
                    valid_stmts += 1;
                }
                let top = *stack.last().unwrap();
                let next_state = lr.goto[&(top, prod.lhs.clone())];
                stack.push(next_state);
            }
            Some(LrAction::Accept) => {
                break;
            }
            None => {
                diags.push(ParseDiagnostic {
                    pos: tok.pos,
                    phase: "SLR".into(),
                    message: format!("syntax error near '{}'", tok.lexeme),
                });

                // Unwind LR stack to the nearest state that shifts the `error` pseudo-token.
                let mut popped = 0usize;
                while let Some(&top) = stack.last() {
                    if let Some(&LrAction::Shift(err_state)) =
                        lr.action.get(&(top, "error".to_string()))
                    {
                        stack.push(err_state);
                        actions.push(format!(
                            "unwound {popped} state(s) to state {top}, shifted `error` -> state {err_state}"
                        ));
                        // Discard tokens until `;` (valid after `error` in `S -> error ;`) or `$`.
                        let mut skipped = 0usize;
                        while ip < tokens.len()
                            && tokens[ip].kind != "$"
                            && !lr
                                .action
                                .contains_key(&(err_state, tokens[ip].kind.clone()))
                        {
                            ip += 1;
                            skipped += 1;
                        }
                        actions.push(format!(
                            "discarded {skipped} token(s), resumed at '{}'",
                            tokens[ip].lexeme
                        ));
                        break;
                    }
                    stack.pop();
                    popped += 1;
                }
                if stack.is_empty() || tokens[ip].kind == "$" {
                    break;
                }
            }
        }
    }

    (valid_stmts, diags, actions)
}

fn main() {
    let ll = build_ll1_table();
    let lr = build_slr_with_error_prod();

    let broken = "let x = + 5; let = 9; emit (1 + ); emit 42;";
    println!("=== LL(1) Synchronization Sets and LR Error Recovery (Ch. 4) ===");
    println!(
        "FIRST(Expr) = {{{}}}, FOLLOW(Expr) = {{{}}}, FOLLOW(Stmt) = {{{}}}",
        ll.first["Expr"]
            .iter()
            .copied()
            .collect::<Vec<_>>()
            .join(", "),
        ll.follow["Expr"]
            .iter()
            .copied()
            .collect::<Vec<_>>()
            .join(", "),
        ll.follow["Stmt"]
            .iter()
            .copied()
            .collect::<Vec<_>>()
            .join(", "),
    );
    println!("Sample input:\n  {broken}\n");

    let (ll_valid, ll_diags, ll_acts) = ll1_parse_with_recovery(broken, &ll);
    println!(
        "1. LL(1) Table-Driven Recovery (FOLLOW synch + virtual ';'):\n   valid statements recovered = {ll_valid}, diagnostics = {}",
        ll_diags.len()
    );
    for d in &ll_diags {
        println!("   [diag @{}] {}", d.pos, d.message);
    }
    for a in &ll_acts {
        println!("   [action] {a}");
    }

    let (lr_valid, lr_diags, lr_acts) = slr_parse_with_recovery(broken, &lr);
    println!(
        "\n2. SLR(1) Error-Production Recovery (`S -> error ;`):\n   valid statements recovered = {lr_valid}, diagnostics = {}",
        lr_diags.len()
    );
    for d in &lr_diags {
        println!("   [diag @{}] {}", d.pos, d.message);
    }
    for a in &lr_acts {
        println!("   [action] {a}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ll1_recovers_missing_semicolon_and_broken_statements() {
        let ll = build_ll1_table();
        let src = "let a = 1 emit 2; let = 3; emit 42;";
        let (valid, diags, acts) = ll1_parse_with_recovery(src, &ll);
        assert!(valid >= 2, "recovered valid statements: {valid}");
        assert!(!diags.is_empty());
        assert!(acts.iter().any(|a| a.contains("insert missing ';'")));
    }

    #[test]
    fn ll1_and_slr_both_recover_valid_tail_statement_without_error_cascade() {
        let ll = build_ll1_table();
        let lr = build_slr_with_error_prod();
        let src = "let x = + 5; let = 9; emit 42;";

        let (ll_valid, ll_diags, _) = ll1_parse_with_recovery(src, &ll);
        let (lr_valid, lr_diags, _) = slr_parse_with_recovery(src, &lr);

        assert!(ll_valid >= 1);
        assert_eq!(lr_valid, 1);
        assert!(ll_diags.len() <= 3);
        assert_eq!(lr_diags.len(), 2);
    }

    #[test]
    fn valid_input_produces_zero_diagnostics_in_both_parsers() {
        let ll = build_ll1_table();
        let lr = build_slr_with_error_prod();
        let src = "let x = 1 + 2; emit (x + 3);";
        let (ll_v, ll_d, _) = ll1_parse_with_recovery(src, &ll);
        let (lr_v, lr_d, _) = slr_parse_with_recovery(src, &lr);
        assert_eq!(ll_v, 2);
        assert_eq!(lr_v, 2);
        assert!(ll_d.is_empty());
        assert!(lr_d.is_empty());
    }
}
