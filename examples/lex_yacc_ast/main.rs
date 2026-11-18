//! Chapters 3–5 Integration: `.l` + `.y` Pipeline Constructing a Typed AST.
//!
//! Unlike `yacc_generator` (which evaluates integer expressions eagerly during reductions),
//! this lesson connects a compiled `.l` DFA scanner directly to a compiled `.y` LALR(1)
//! parser whose semantic actions construct a typed, span-annotated AST for downstream stages:
//!   1. Compile a `.l` lexical spec into a multi-rule DFA emitting `(kind, yylval, Span)`,
//!   2. Compile a `.y` grammar spec (with `%left`, `%prec`, and constructor actions like
//!      `Program($1, $2)`, `Let($2, $4)`, `Bin("+", $1, $3)`) into LALR(1) tables,
//!   3. Parse source tokens into an AST (`AstProgram`) preserving exact merged `Span`s,
//!   4. Hand the AST to Chapter 5 semantic analysis (undeclared variable check with caret
//!      diagnostics) and Chapter 6 three-address code (TAC) lowering + execution.

use std::collections::{BTreeMap, BTreeSet};

// ---------------------------------------------------------------------------
// 1. Token + Span Interface (`yylval` between .l Scanner & .y Parser)
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Span {
    start: usize,
    end: usize,
    line: usize,
    col: usize,
}

impl Span {
    fn merge(self, other: Span) -> Span {
        if self.start <= other.start {
            Span {
                start: self.start,
                end: self.end.max(other.end),
                line: self.line,
                col: self.col,
            }
        } else {
            Span {
                start: other.start,
                end: self.end.max(other.end),
                line: other.line,
                col: other.col,
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Token {
    kind: String, // "LET" | "EMIT" | "ID" | "NUM" | "+" | "-" | "*" | "/" | "=" | ";" | "(" | ")" | "$"
    lexeme: String,
    span: Span,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Diagnostic {
    span: Span,
    message: String,
}

fn format_diagnostic(src: &str, diag: &Diagnostic) -> String {
    let lines: Vec<&str> = src.lines().collect();
    let line_str = lines
        .get(diag.span.line.saturating_sub(1))
        .copied()
        .unwrap_or("");
    let pad = " ".repeat(diag.span.col.saturating_sub(1));
    let avail = line_str
        .len()
        .saturating_sub(diag.span.col.saturating_sub(1))
        .max(1);
    let width = diag
        .span
        .end
        .saturating_sub(diag.span.start)
        .max(1)
        .min(avail);
    let carets = "^".repeat(width);
    format!(
        "error at line {}, col {}: {}\n  | {}\n  | {}{}",
        diag.span.line, diag.span.col, diag.message, line_str, pad, carets
    )
}

// ---------------------------------------------------------------------------
// 2. Mini `.l` Specification Compiler -> Multi-Rule DFA Scanner
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
struct LexRule {
    pattern: String,
    token_kind: Option<String>, // None => skip (whitespace / comment)
}

struct CompiledLexer {
    rules: Vec<LexRule>,
}

fn compile_lex_spec(spec: &str) -> Result<CompiledLexer, String> {
    let mut rules = Vec::new();
    let mut in_rules = false;
    for raw in spec.lines() {
        let line = raw.trim();
        if line.is_empty() || (!in_rules && line.starts_with("//")) {
            continue;
        }
        if line == "%%" {
            in_rules = !in_rules;
            continue;
        }
        if !in_rules {
            continue;
        }
        let Some((pat_raw, act_raw)) = line.rsplit_once(char::is_whitespace) else {
            return Err("missing action in .l rule".into());
        };
        let pat = pat_raw.trim();
        let action = act_raw.trim();
        let token_kind = if action == "SKIP" {
            None
        } else {
            Some(action.to_string())
        };
        rules.push(LexRule {
            pattern: pat.to_string(),
            token_kind,
        });
    }
    if rules.is_empty() {
        return Err("empty .l rule section".into());
    }
    Ok(CompiledLexer { rules })
}

fn match_rule_len(pattern: &str, bytes: &[u8], pos: usize) -> usize {
    let n = bytes.len();
    match pattern {
        "[ \\t\\r\\n]+" => {
            let mut k = pos;
            while k < n && bytes[k].is_ascii_whitespace() {
                k += 1;
            }
            k - pos
        }
        "//[^\\n]*" => {
            if bytes.get(pos..pos + 2) == Some(b"//") {
                let mut k = pos + 2;
                while k < n && bytes[k] != b'\n' {
                    k += 1;
                }
                k - pos
            } else {
                0
            }
        }
        "[a-zA-Z_][a-zA-Z0-9_]*" => {
            if pos < n && (bytes[pos].is_ascii_alphabetic() || bytes[pos] == b'_') {
                let mut k = pos + 1;
                while k < n && (bytes[k].is_ascii_alphanumeric() || bytes[k] == b'_') {
                    k += 1;
                }
                k - pos
            } else {
                0
            }
        }
        "[0-9]+" => {
            let mut k = pos;
            while k < n && bytes[k].is_ascii_digit() {
                k += 1;
            }
            k - pos
        }
        lit => {
            let lit_b = lit.as_bytes();
            if pos + lit_b.len() <= n && &bytes[pos..pos + lit_b.len()] == lit_b {
                lit_b.len()
            } else {
                0
            }
        }
    }
}

fn lex_source(lexer: &CompiledLexer, src: &str) -> Result<Vec<Token>, Diagnostic> {
    let bytes = src.as_bytes();
    let n = bytes.len();
    let mut i = 0usize;
    let mut line = 1usize;
    let mut line_start = 0usize;
    let mut out = Vec::new();

    while i < n {
        let mut best: Option<(usize, usize)> = None; // (len, rule_idx)
        for (idx, rule) in lexer.rules.iter().enumerate() {
            let mlen = match_rule_len(&rule.pattern, bytes, i);
            if mlen > 0 && best.is_none_or(|(bl, _)| mlen > bl) {
                best = Some((mlen, idx));
            }
        }
        let col = i - line_start + 1;
        let Some((mlen, ridx)) = best else {
            return Err(Diagnostic {
                span: Span {
                    start: i,
                    end: i + 1,
                    line,
                    col,
                },
                message: format!("lexical error: unexpected character '{}'", bytes[i] as char),
            });
        };
        let end = i + mlen;
        if let Some(kind) = &lexer.rules[ridx].token_kind {
            out.push(Token {
                kind: kind.clone(),
                lexeme: src[i..end].to_string(),
                span: Span {
                    start: i,
                    end,
                    line,
                    col,
                },
            });
        }
        for &b in &bytes[i..end] {
            if b == b'\n' {
                line += 1;
                line_start = i + 1;
            }
            i += 1;
        }
    }
    out.push(Token {
        kind: "$".into(),
        lexeme: "$".into(),
        span: Span {
            start: n,
            end: n,
            line,
            col: n.saturating_sub(line_start) + 1,
        },
    });
    Ok(out)
}

// ---------------------------------------------------------------------------
// 3. Typed AST Nodes & .y AST Constructor Actions
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq)]
enum AstExpr {
    Int(i64, Span),
    Var(String, Span),
    Neg(Box<AstExpr>, Span),
    Bin {
        op: String,
        left: Box<AstExpr>,
        right: Box<AstExpr>,
        span: Span,
    },
}

impl AstExpr {
    fn span(&self) -> Span {
        match self {
            AstExpr::Int(_, s)
            | AstExpr::Var(_, s)
            | AstExpr::Neg(_, s)
            | AstExpr::Bin { span: s, .. } => *s,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum AstStmt {
    Let {
        name: String,
        expr: AstExpr,
        span: Span,
    },
    Emit {
        expr: AstExpr,
        span: Span,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct AstProgram {
    stmts: Vec<AstStmt>,
}

#[derive(Clone, Debug)]
enum YyVal {
    Tok(Token),
    Expr(AstExpr),
    Stmt(AstStmt),
    Prog(AstProgram),
}

impl YyVal {
    fn span(&self) -> Span {
        match self {
            YyVal::Tok(t) => t.span,
            YyVal::Expr(e) => e.span(),
            YyVal::Stmt(AstStmt::Let { span, .. } | AstStmt::Emit { span, .. }) => *span,
            YyVal::Prog(p) => p
                .stmts
                .first()
                .zip(p.stmts.last())
                .map(|(a, b)| {
                    let sa = match a {
                        AstStmt::Let { span, .. } | AstStmt::Emit { span, .. } => *span,
                    };
                    let sb = match b {
                        AstStmt::Let { span, .. } | AstStmt::Emit { span, .. } => *span,
                    };
                    sa.merge(sb)
                })
                .unwrap_or(Span {
                    start: 0,
                    end: 0,
                    line: 1,
                    col: 1,
                }),
        }
    }
}

// ---------------------------------------------------------------------------
// 4. Mini `.y` Specification Compiler -> LALR(1) Table + AST Builder
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Assoc {
    Left,
    Right,
}

#[derive(Clone, Debug)]
struct YaccProd {
    lhs: String,
    rhs: Vec<String>,
    prec_sym: Option<String>,
    action: String,
}

#[derive(Clone, Debug)]
enum LrAction {
    Shift(usize),
    Reduce(usize),
    Accept,
}

struct CompiledParser {
    prods: Vec<YaccProd>,
    action: BTreeMap<(usize, String), LrAction>,
    goto: BTreeMap<(usize, String), usize>,
}

fn compile_yacc_spec(spec: &str) -> Result<CompiledParser, String> {
    let mut prec: BTreeMap<String, (usize, Assoc)> = BTreeMap::new();
    let mut prods: Vec<YaccProd> = Vec::new();
    let mut level = 0usize;
    let mut in_rules = false;
    let mut start_nt: Option<String> = None;

    for raw in spec.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with("//") {
            continue;
        }
        if line == "%%" {
            in_rules = !in_rules;
            continue;
        }
        if !in_rules {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.first() == Some(&"%left") {
                level += 1;
                for &t in &parts[1..] {
                    prec.insert(t.into(), (level, Assoc::Left));
                }
            } else if parts.first() == Some(&"%right") {
                level += 1;
                for &t in &parts[1..] {
                    prec.insert(t.into(), (level, Assoc::Right));
                }
            } else if parts.first() == Some(&"%start") && parts.len() == 2 {
                start_nt = Some(parts[1].into());
            }
            continue;
        }
        // Format: `LHS : rhs... [ %prec SYM ] { ACTION }`
        let (head_rhs, act_part) = line
            .split_once('{')
            .ok_or("missing '{' action in .y rule")?;
        let action = act_part.trim().trim_end_matches('}').trim().to_string();
        let (lhs_raw, rhs_raw) = head_rhs.split_once(':').ok_or("missing ':' in .y rule")?;
        let lhs = lhs_raw.trim().to_string();
        if start_nt.is_none() {
            start_nt = Some(lhs.clone());
        }
        let tokens: Vec<&str> = rhs_raw.split_whitespace().collect();
        let mut rhs = Vec::new();
        let mut prec_sym = None;
        let mut idx = 0;
        while idx < tokens.len() {
            if tokens[idx] == "%prec" {
                prec_sym = Some(
                    tokens
                        .get(idx + 1)
                        .ok_or("missing %prec symbol")?
                        .to_string(),
                );
                idx += 2;
            } else {
                rhs.push(tokens[idx].to_string());
                idx += 1;
            }
        }
        prods.push(YaccProd {
            lhs,
            rhs,
            prec_sym,
            action,
        });
    }

    let start = start_nt.ok_or("no start symbol")?;
    let mut full_prods = vec![YaccProd {
        lhs: "S'".into(),
        rhs: vec![start],
        prec_sym: None,
        action: "$1".into(),
    }];
    full_prods.extend(prods);

    let nts: BTreeSet<String> = full_prods.iter().map(|p| p.lhs.clone()).collect();
    let mut all_syms: BTreeSet<String> = nts.clone();
    let mut terminals: BTreeSet<String> = BTreeSet::from(["$".into()]);
    for p in &full_prods {
        for s in &p.rhs {
            all_syms.insert(s.clone());
            if !nts.contains(s) {
                terminals.insert(s.clone());
            }
        }
    }

    // FIRST sets
    let mut first: BTreeMap<String, BTreeSet<String>> =
        nts.iter().map(|n| (n.clone(), BTreeSet::new())).collect();
    let mut changed = true;
    while changed {
        changed = false;
        for p in &full_prods {
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

    // LR(1) item set construction -> merged into LALR(1) by identical LR(0) core!
    type Lr1Item = (usize, usize, String); // (prod_id, dot, lookahead)
    type Lr0Core = BTreeSet<(usize, usize)>;

    let closure1 = |seed: BTreeSet<Lr1Item>| -> BTreeSet<Lr1Item> {
        let mut set = seed;
        let mut ch = true;
        while ch {
            ch = false;
            let cur: Vec<Lr1Item> = set.iter().cloned().collect();
            for (pid, dot, la) in cur {
                if let Some(sym) = full_prods[pid].rhs.get(dot) {
                    if nts.contains(sym) {
                        let follow_la: BTreeSet<String> = match full_prods[pid].rhs.get(dot + 1) {
                            Some(beta) if !nts.contains(beta) => BTreeSet::from([beta.clone()]),
                            Some(beta) => first[beta].clone(),
                            None => BTreeSet::from([la.clone()]),
                        };
                        for (npid, np) in full_prods.iter().enumerate() {
                            if &np.lhs == sym {
                                for b in &follow_la {
                                    if set.insert((npid, 0, b.clone())) {
                                        ch = true;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        set
    };

    let mut lr1_states: Vec<BTreeSet<Lr1Item>> = Vec::new();
    let mut lr1_id: BTreeMap<BTreeSet<Lr1Item>, usize> = BTreeMap::new();
    let mut lr1_trans: BTreeMap<(usize, String), usize> = BTreeMap::new();

    let s0 = closure1(BTreeSet::from([(0, 0, "$".into())]));
    lr1_id.insert(s0.clone(), 0);
    lr1_states.push(s0);

    let mut idx = 0usize;
    while idx < lr1_states.len() {
        let items = lr1_states[idx].clone();
        for sym in &all_syms {
            let seed: BTreeSet<Lr1Item> = items
                .iter()
                .filter(|(pid, dot, _)| full_prods[*pid].rhs.get(*dot) == Some(sym))
                .map(|(pid, dot, la)| (*pid, dot + 1, la.clone()))
                .collect();
            if seed.is_empty() {
                continue;
            }
            let target_set = closure1(seed);
            let tid = match lr1_id.get(&target_set) {
                Some(&id) => id,
                None => {
                    let id = lr1_states.len();
                    lr1_id.insert(target_set.clone(), id);
                    lr1_states.push(target_set);
                    id
                }
            };
            lr1_trans.insert((idx, sym.clone()), tid);
        }
        idx += 1;
    }

    // Merge canonical LR(1) states sharing identical LR(0) core -> LALR(1).
    let mut core_to_lalr: BTreeMap<Lr0Core, usize> = BTreeMap::new();
    let mut lalr_states: Vec<BTreeSet<Lr1Item>> = Vec::new();
    let mut map_state = vec![0usize; lr1_states.len()];
    for (sid, st) in lr1_states.iter().enumerate() {
        let core: Lr0Core = st.iter().map(|&(p, d, _)| (p, d)).collect();
        let lid = match core_to_lalr.get(&core) {
            Some(&id) => {
                lalr_states[id].extend(st.iter().cloned());
                id
            }
            None => {
                let id = lalr_states.len();
                core_to_lalr.insert(core, id);
                lalr_states.push(st.clone());
                id
            }
        };
        map_state[sid] = lid;
    }

    let mut action: BTreeMap<(usize, String), LrAction> = BTreeMap::new();
    let mut goto: BTreeMap<(usize, String), usize> = BTreeMap::new();

    for (old_sid, sym) in lr1_trans.keys() {
        let lid = map_state[*old_sid];
        let target = map_state[lr1_trans[&(*old_sid, sym.clone())]];
        if nts.contains(sym) {
            goto.insert((lid, sym.clone()), target);
        } else {
            action.insert((lid, sym.clone()), LrAction::Shift(target));
        }
    }

    let prod_prec = |pid: usize| -> Option<(usize, Assoc)> {
        let p = &full_prods[pid];
        if let Some(ps) = &p.prec_sym {
            return prec.get(ps).copied();
        }
        p.rhs.iter().rev().find_map(|s| prec.get(s).copied())
    };

    for (lid, items) in lalr_states.iter().enumerate() {
        for (pid, dot, la) in items {
            if *dot != full_prods[*pid].rhs.len() {
                continue;
            }
            if *pid == 0 && la == "$" {
                action.insert((lid, "$".into()), LrAction::Accept);
                continue;
            }
            let key = (lid, la.clone());
            match action.get(&key).cloned() {
                None => {
                    action.insert(key, LrAction::Reduce(*pid));
                }
                Some(LrAction::Shift(_)) => {
                    // Resolve S/R using `%left` / `%right` / `%prec`
                    let Some((p_lvl, p_assoc)) = prod_prec(*pid) else {
                        return Err(format!("unresolved shift/reduce conflict on '{la}'"));
                    };
                    let Some(&(t_lvl, _)) = prec.get(la) else {
                        return Err(format!("missing precedence for lookahead '{la}'"));
                    };
                    if p_lvl > t_lvl || (p_lvl == t_lvl && p_assoc == Assoc::Left) {
                        action.insert(key, LrAction::Reduce(*pid));
                    }
                }
                Some(LrAction::Reduce(prev)) if prev != *pid => {
                    return Err(format!("reduce/reduce conflict on '{la}'"));
                }
                _ => {}
            }
        }
    }

    Ok(CompiledParser {
        prods: full_prods,
        action,
        goto,
    })
}

fn eval_yacc_action(action: &str, args: &[YyVal]) -> Result<YyVal, String> {
    let span = args
        .first()
        .zip(args.last())
        .map(|(a, b)| a.span().merge(b.span()))
        .unwrap_or(Span {
            start: 0,
            end: 0,
            line: 1,
            col: 1,
        });
    match action {
        "$1" => Ok(args[0].clone()),
        "ProgSingle($1)" => match &args[0] {
            YyVal::Stmt(s) => Ok(YyVal::Prog(AstProgram {
                stmts: vec![s.clone()],
            })),
            _ => Err("expected Stmt for ProgSingle".into()),
        },
        "ProgAppend($1,$2)" => match (&args[0], &args[1]) {
            (YyVal::Prog(p), YyVal::Stmt(s)) => {
                let mut stmts = p.stmts.clone();
                stmts.push(s.clone());
                Ok(YyVal::Prog(AstProgram { stmts }))
            }
            _ => Err("expected (Prog, Stmt)".into()),
        },
        "LetStmt($2,$4)" => match (&args[1], &args[3]) {
            (YyVal::Tok(id), YyVal::Expr(e)) => Ok(YyVal::Stmt(AstStmt::Let {
                name: id.lexeme.clone(),
                expr: e.clone(),
                span,
            })),
            _ => Err("expected (Tok, Expr) for LetStmt".into()),
        },
        "EmitStmt($2)" => match &args[1] {
            YyVal::Expr(e) => Ok(YyVal::Stmt(AstStmt::Emit {
                expr: e.clone(),
                span,
            })),
            _ => Err("expected Expr for EmitStmt".into()),
        },
        "Bin($1,$2,$3)" => match (&args[0], &args[1], &args[2]) {
            (YyVal::Expr(l), YyVal::Tok(op), YyVal::Expr(r)) => Ok(YyVal::Expr(AstExpr::Bin {
                op: op.lexeme.clone(),
                left: Box::new(l.clone()),
                right: Box::new(r.clone()),
                span,
            })),
            _ => Err("expected (Expr, Tok, Expr)".into()),
        },
        "Neg($2)" => match &args[1] {
            YyVal::Expr(e) => Ok(YyVal::Expr(AstExpr::Neg(Box::new(e.clone()), span))),
            _ => Err("expected Expr for Neg".into()),
        },
        "Group($2)" => match &args[1] {
            YyVal::Expr(e) => {
                // Re-tag span to include parentheses `( ... )`
                let inner = match e {
                    AstExpr::Int(v, _) => AstExpr::Int(*v, span),
                    AstExpr::Var(n, _) => AstExpr::Var(n.clone(), span),
                    AstExpr::Neg(i, _) => AstExpr::Neg(i.clone(), span),
                    AstExpr::Bin {
                        op, left, right, ..
                    } => AstExpr::Bin {
                        op: op.clone(),
                        left: left.clone(),
                        right: right.clone(),
                        span,
                    },
                };
                Ok(YyVal::Expr(inner))
            }
            _ => Err("expected Expr for Group".into()),
        },
        "IdExpr($1)" => match &args[0] {
            YyVal::Tok(t) => Ok(YyVal::Expr(AstExpr::Var(t.lexeme.clone(), t.span))),
            _ => Err("expected Tok for IdExpr".into()),
        },
        "NumExpr($1)" => match &args[0] {
            YyVal::Tok(t) => {
                let v = t
                    .lexeme
                    .parse::<i64>()
                    .map_err(|_| format!("invalid int '{}'", t.lexeme))?;
                Ok(YyVal::Expr(AstExpr::Int(v, t.span)))
            }
            _ => Err("expected Tok for NumExpr".into()),
        },
        other => Err(format!("unknown .y action '{other}'")),
    }
}

fn parse_to_ast(parser: &CompiledParser, tokens: &[Token]) -> Result<AstProgram, Diagnostic> {
    let mut states: Vec<usize> = vec![0];
    let mut vals: Vec<YyVal> = Vec::new();
    let mut ip = 0usize;

    loop {
        let s = *states.last().unwrap();
        let tok = &tokens[ip];
        match parser.action.get(&(s, tok.kind.clone())) {
            Some(LrAction::Shift(next)) => {
                states.push(*next);
                vals.push(YyVal::Tok(tok.clone()));
                ip += 1;
            }
            Some(LrAction::Reduce(pid)) => {
                let prod = &parser.prods[*pid];
                let k = prod.rhs.len();
                let split = vals.len() - k;
                let rhs_vals: Vec<YyVal> = vals.drain(split..).collect();
                states.truncate(states.len() - k);
                let result = eval_yacc_action(&prod.action, &rhs_vals).map_err(|m| Diagnostic {
                    span: tok.span,
                    message: m,
                })?;
                let top = *states.last().unwrap();
                let next_s = parser.goto[&(top, prod.lhs.clone())];
                states.push(next_s);
                vals.push(result);
            }
            Some(LrAction::Accept) => {
                return match vals.pop() {
                    Some(YyVal::Prog(prog)) => Ok(prog),
                    _ => Err(Diagnostic {
                        span: tok.span,
                        message: "internal error: root value is not AstProgram".into(),
                    }),
                };
            }
            None => {
                return Err(Diagnostic {
                    span: tok.span,
                    message: format!("syntax error near '{}'", tok.lexeme),
                });
            }
        }
    }
}

// ---------------------------------------------------------------------------
// 5. Chapter 5 Semantic Check & Chapter 6 TAC Lowering
// ---------------------------------------------------------------------------

fn check_semantics(prog: &AstProgram) -> Result<(), Diagnostic> {
    let mut declared = BTreeSet::new();
    fn check_expr(e: &AstExpr, declared: &BTreeSet<String>) -> Result<(), Diagnostic> {
        match e {
            AstExpr::Int(_, _) => Ok(()),
            AstExpr::Var(name, span) => {
                if !declared.contains(name) {
                    return Err(Diagnostic {
                        span: *span,
                        message: format!("use of undeclared variable '{name}'"),
                    });
                }
                Ok(())
            }
            AstExpr::Neg(inner, _) => check_expr(inner, declared),
            AstExpr::Bin {
                op,
                left,
                right,
                span,
            } => {
                check_expr(left, declared)?;
                check_expr(right, declared)?;
                if op == "/" && matches!(right.as_ref(), AstExpr::Int(0, _)) {
                    return Err(Diagnostic {
                        span: *span,
                        message: "constant division by zero".into(),
                    });
                }
                Ok(())
            }
        }
    }

    for stmt in &prog.stmts {
        match stmt {
            AstStmt::Let { name, expr, .. } => {
                check_expr(expr, &declared)?;
                declared.insert(name.clone());
            }
            AstStmt::Emit { expr, .. } => check_expr(expr, &declared)?,
        }
    }
    Ok(())
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum TacInstr {
    Const(String, i64),
    Copy(String, String),
    Neg(String, String),
    Bin {
        dst: String,
        op: String,
        lhs: String,
        rhs: String,
    },
    Emit(String),
}

fn lower_to_tac(prog: &AstProgram) -> Vec<TacInstr> {
    let mut temp = 0usize;
    let mut code = Vec::new();

    fn lower_expr(e: &AstExpr, temp: &mut usize, code: &mut Vec<TacInstr>) -> String {
        let next_t = |tp: &mut usize| {
            let name = format!("t{}", *tp);
            *tp += 1;
            name
        };
        match e {
            AstExpr::Int(v, _) => {
                let d = next_t(temp);
                code.push(TacInstr::Const(d.clone(), *v));
                d
            }
            AstExpr::Var(n, _) => n.clone(),
            AstExpr::Neg(inner, _) => {
                let s = lower_expr(inner, temp, code);
                let d = next_t(temp);
                code.push(TacInstr::Neg(d.clone(), s));
                d
            }
            AstExpr::Bin {
                op, left, right, ..
            } => {
                let l = lower_expr(left, temp, code);
                let r = lower_expr(right, temp, code);
                let d = next_t(temp);
                code.push(TacInstr::Bin {
                    dst: d.clone(),
                    op: op.clone(),
                    lhs: l,
                    rhs: r,
                });
                d
            }
        }
    }

    for stmt in &prog.stmts {
        match stmt {
            AstStmt::Let { name, expr, .. } => {
                let src = lower_expr(expr, &mut temp, &mut code);
                code.push(TacInstr::Copy(name.clone(), src));
            }
            AstStmt::Emit { expr, .. } => {
                let src = lower_expr(expr, &mut temp, &mut code);
                code.push(TacInstr::Emit(src));
            }
        }
    }
    code
}

fn eval_tac(code: &[TacInstr]) -> Result<Vec<i64>, String> {
    let mut env = BTreeMap::new();
    let mut out = Vec::new();
    for instr in code {
        match instr {
            TacInstr::Const(d, v) => {
                env.insert(d.clone(), *v);
            }
            TacInstr::Copy(d, s) => {
                let val = *env.get(s).ok_or_else(|| format!("unbound '{s}'"))?;
                env.insert(d.clone(), val);
            }
            TacInstr::Neg(d, s) => {
                let val = *env.get(s).ok_or_else(|| format!("unbound '{s}'"))?;
                env.insert(d.clone(), -val);
            }
            TacInstr::Bin { dst, op, lhs, rhs } => {
                let a = env[lhs];
                let b = env[rhs];
                let r = match op.as_str() {
                    "+" => a + b,
                    "-" => a - b,
                    "*" => a * b,
                    "/" => {
                        if b == 0 {
                            return Err("runtime division by zero".into());
                        }
                        a / b
                    }
                    _ => return Err(format!("unknown op '{op}'")),
                };
                env.insert(dst.clone(), r);
            }
            TacInstr::Emit(s) => out.push(env[s]),
        }
    }
    Ok(out)
}

const LEX_SPEC: &str = r"
%%
[ \t\r\n]+                SKIP
//[^\n]*                  SKIP
let                       LET
emit                      EMIT
[a-zA-Z_][a-zA-Z0-9_]*    ID
[0-9]+                    NUM
=                         =
+                         +
-                         -
*                         *
/                         /
(                         (
)                         )
;                         ;
%%
";

const YACC_SPEC: &str = r"
%left + -
%left * /
%right UMINUS
%start Program
%%
Program : Stmt               { ProgSingle($1) }
Program : Program Stmt       { ProgAppend($1,$2) }
Stmt    : LET ID = Expr ;    { LetStmt($2,$4) }
Stmt    : EMIT Expr ;        { EmitStmt($2) }
Expr    : Expr + Expr        { Bin($1,$2,$3) }
Expr    : Expr - Expr        { Bin($1,$2,$3) }
Expr    : Expr * Expr        { Bin($1,$2,$3) }
Expr    : Expr / Expr        { Bin($1,$2,$3) }
Expr    : - Expr %prec UMINUS { Neg($2) }
Expr    : ( Expr )           { Group($2) }
Expr    : ID                 { IdExpr($1) }
Expr    : NUM                { NumExpr($1) }
%%
";

fn run_pipeline(src: &str) -> Result<(AstProgram, Vec<TacInstr>, Vec<i64>), Diagnostic> {
    let lexer = compile_lex_spec(LEX_SPEC).map_err(|m| Diagnostic {
        span: Span {
            start: 0,
            end: 1,
            line: 1,
            col: 1,
        },
        message: m,
    })?;
    let parser = compile_yacc_spec(YACC_SPEC).map_err(|m| Diagnostic {
        span: Span {
            start: 0,
            end: 1,
            line: 1,
            col: 1,
        },
        message: m,
    })?;
    let tokens = lex_source(&lexer, src)?;
    let ast = parse_to_ast(&parser, &tokens)?;
    check_semantics(&ast)?;
    let tac = lower_to_tac(&ast);
    let out = eval_tac(&tac).map_err(|m| Diagnostic {
        span: Span {
            start: 0,
            end: 1,
            line: 1,
            col: 1,
        },
        message: m,
    })?;
    Ok((ast, tac, out))
}

fn main() {
    let src = "\
// Integrated .l + .y -> AST -> SemCheck -> TAC
let price = 6;
let total = (price + 2) * 3 - -1;
emit total;
";
    let (ast, tac, out) = run_pipeline(src).expect("pipeline succeeds");
    println!("=== Integrated .l + .y -> AST -> Semantic Check -> TAC (Ch. 3–5) ===");
    println!("Constructed AST statements: {}", ast.stmts.len());
    for stmt in &ast.stmts {
        println!("  {stmt:?}");
    }
    println!(
        "\nLowered TAC ({} instructions) -> output {out:?}",
        tac.len()
    );

    let bad = "let x = 5;\nemit x + ghost;\n";
    let diag = run_pipeline(bad).expect_err("undeclared identifier must fail with span");
    println!(
        "\nSemantic diagnostic preserved across .l -> .y -> AST:\n{}",
        format_diagnostic(bad, &diag)
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lex_plus_yacc_builds_typed_ast_and_executes_tac() {
        let src = "let a = 2 + 3 * 4; emit -a + (10 / 2);";
        let (ast, _tac, out) = run_pipeline(src).expect("pipeline should succeed");
        assert_eq!(ast.stmts.len(), 2);
        assert_eq!(out, vec![-9]);
    }

    #[test]
    fn longest_match_keeps_letter_as_id_not_let() {
        let src = "let letter = 7; emit letter + 3;";
        let (_, _, out) = run_pipeline(src).expect("longest match beats keyword prefix");
        assert_eq!(out, vec![10]);
    }

    #[test]
    fn preserves_spans_from_lex_through_yacc_to_semantic_diagnostic() {
        let src = "let a = 1;\nemit a + missing;\n";
        let diag = run_pipeline(src).expect_err("undeclared 'missing' must fail");
        assert_eq!(diag.span.line, 2);
        assert_eq!(diag.span.col, 10);
        let rendered = format_diagnostic(src, &diag);
        assert!(rendered.contains("^^^^^^^"));
    }
}
