//! Loop-Invariant Code Motion (with dominance/exit-safety checks) and Induction-Variable Strength Reduction.

use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq, Eq)]
enum Operand {
    Const(i64),
    Var(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Op {
    Add,
    Mul,
    Lt,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Stmt {
    Assign {
        dst: String,
        op: Op,
        lhs: Operand,
        rhs: Operand,
    },
    Copy {
        dst: String,
        src: Operand,
    },
}

#[derive(Clone, Debug)]
enum Term {
    Jump(usize),
    Branch {
        cond: String,
        then_bb: usize,
        else_bb: usize,
    },
    Return(String),
}

#[derive(Clone, Debug)]
struct BasicBlock {
    stmts: Vec<Stmt>,
    term: Term,
}

fn succs(t: &Term) -> Vec<usize> {
    match t {
        Term::Jump(x) => vec![*x],
        Term::Branch {
            then_bb, else_bb, ..
        } => vec![*then_bb, *else_bb],
        Term::Return(_) => vec![],
    }
}

fn compute_doms(blocks: &[BasicBlock]) -> Vec<BTreeSet<usize>> {
    let n = blocks.len();
    let mut preds = vec![Vec::new(); n];
    for (b, blk) in blocks.iter().enumerate() {
        for s in succs(&blk.term) {
            preds[s].push(b);
        }
    }
    let all: BTreeSet<usize> = (0..n).collect();
    let mut dom = vec![all; n];
    dom[0] = BTreeSet::from([0]);
    loop {
        let mut changed = false;
        for b in 1..n {
            if preds[b].is_empty() {
                continue;
            }
            let mut inter = dom[preds[b][0]].clone();
            for &p in &preds[b][1..] {
                inter.retain(|d| dom[p].contains(d));
            }
            inter.insert(b);
            if inter != dom[b] {
                dom[b] = inter;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    dom
}

fn natural_loop(blocks: &[BasicBlock], header: usize, latch: usize) -> BTreeSet<usize> {
    let mut preds = vec![Vec::new(); blocks.len()];
    for (b, blk) in blocks.iter().enumerate() {
        for s in succs(&blk.term) {
            preds[s].push(b);
        }
    }
    let mut loop_set = BTreeSet::from([header, latch]);
    let mut stack = if latch == header { vec![] } else { vec![latch] };
    while let Some(m) = stack.pop() {
        for &p in &preds[m] {
            if loop_set.insert(p) {
                stack.push(p);
            }
        }
    }
    loop_set
}

fn stmt_dst(s: &Stmt) -> &str {
    match s {
        Stmt::Assign { dst, .. } | Stmt::Copy { dst, .. } => dst,
    }
}

fn stmt_uses(s: &Stmt) -> Vec<String> {
    let mut u = Vec::new();
    match s {
        Stmt::Assign { lhs, rhs, .. } => {
            if let Operand::Var(v) = lhs {
                u.push(v.clone());
            }
            if let Operand::Var(v) = rhs {
                u.push(v.clone());
            }
        }
        Stmt::Copy {
            src: Operand::Var(v),
            ..
        } => u.push(v.clone()),
        Stmt::Copy {
            src: Operand::Const(_),
            ..
        } => {}
    }
    u
}

// Compute live_in for blocks after/outside the loop so LICM avoids clobbering exit-live vars.
fn compute_live_in(blocks: &[BasicBlock]) -> Vec<BTreeSet<String>> {
    let n = blocks.len();
    let mut use_b = vec![BTreeSet::new(); n];
    let mut def_b = vec![BTreeSet::new(); n];
    for (b, blk) in blocks.iter().enumerate() {
        for s in &blk.stmts {
            for u in stmt_uses(s) {
                if !def_b[b].contains(&u) {
                    use_b[b].insert(u);
                }
            }
            def_b[b].insert(stmt_dst(s).to_string());
        }
        match &blk.term {
            Term::Jump(_) => {}
            Term::Branch { cond, .. } => {
                if !def_b[b].contains(cond) {
                    use_b[b].insert(cond.clone());
                }
            }
            Term::Return(v) => {
                if !def_b[b].contains(v) {
                    use_b[b].insert(v.clone());
                }
            }
        }
    }
    let mut live_in = vec![BTreeSet::new(); n];
    loop {
        let mut changed = false;
        for b in (0..n).rev() {
            let mut out = BTreeSet::new();
            for s in succs(&blocks[b].term) {
                out.extend(live_in[s].iter().cloned());
            }
            let mut inn = use_b[b].clone();
            for v in out {
                if !def_b[b].contains(&v) {
                    inn.insert(v);
                }
            }
            if inn != live_in[b] {
                live_in[b] = inn;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    live_in
}

fn licm_hoist(
    blocks: &[BasicBlock],
    preheader: usize,
    header: usize,
    latch: usize,
) -> (Vec<BasicBlock>, Vec<String>, Vec<String>) {
    let dom = compute_doms(blocks);
    let loop_nodes = natural_loop(blocks, header, latch);
    let live_in = compute_live_in(blocks);

    // Count how many times each variable is defined inside the loop.
    let mut def_count: BTreeMap<String, usize> = BTreeMap::new();
    for &b in &loop_nodes {
        for s in &blocks[b].stmts {
            *def_count.entry(stmt_dst(s).to_string()).or_insert(0) += 1;
        }
    }

    // Loop exit blocks: successors outside `loop_nodes`.
    let mut exits: Vec<usize> = Vec::new();
    let mut exit_targets: BTreeSet<usize> = BTreeSet::new();
    for &b in &loop_nodes {
        for s in succs(&blocks[b].term) {
            if !loop_nodes.contains(&s) {
                exits.push(b);
                exit_targets.insert(s);
            }
        }
    }
    let mut live_after_loop = BTreeSet::new();
    for &et in &exit_targets {
        live_after_loop.extend(live_in[et].iter().cloned());
    }

    // Iteratively mark invariant statements `(block, stmt_idx)`.
    let mut invariant: BTreeSet<(usize, usize)> = BTreeSet::new();
    let mut inv_vars: BTreeSet<String> = BTreeSet::new();
    loop {
        let mut changed = false;
        for &b in &loop_nodes {
            for (si, s) in blocks[b].stmts.iter().enumerate() {
                if invariant.contains(&(b, si)) {
                    continue;
                }
                let ok_ops = stmt_uses(s)
                    .iter()
                    .all(|v| def_count.get(v).copied().unwrap_or(0) == 0 || inv_vars.contains(v));
                let unique_def = def_count.get(stmt_dst(s)).copied().unwrap_or(0) == 1;
                if ok_ops && unique_def {
                    invariant.insert((b, si));
                    inv_vars.insert(stmt_dst(s).to_string());
                    changed = true;
                }
            }
        }
        if !changed {
            break;
        }
    }

    let mut out = blocks.to_vec();
    let mut hoisted_vars = Vec::new();
    let mut skipped_unsafe = Vec::new();

    // Check safety before hoisting to preheader:
    // 1. Block `b` dominates all loop exits OR variable is dead after the loop.
    // 2. All loop-internal uses are dominated by `b` (guaranteed when unique def is invariant & dominates uses).
    let mut to_hoist = Vec::new();
    for &b in &loop_nodes {
        let mut kept = Vec::new();
        for (si, s) in out[b].stmts.iter().enumerate() {
            let dst = stmt_dst(s);
            if invariant.contains(&(b, si)) {
                let dom_all_exits = exits.iter().all(|&ex| dom[ex].contains(&b));
                let dead_after = !live_after_loop.contains(dst);
                if dom_all_exits || dead_after {
                    to_hoist.push(s.clone());
                    hoisted_vars.push(dst.to_string());
                } else {
                    skipped_unsafe.push(dst.to_string());
                    kept.push(s.clone());
                }
            } else {
                kept.push(s.clone());
            }
        }
        out[b].stmts = kept;
    }
    out[preheader].stmts.extend(to_hoist);

    (out, hoisted_vars, skipped_unsafe)
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct DerivedIv {
    var: String,
    base_iv: String,
    scale: i64,
    offset: i64,
}

// Detect `i = i + step`, derived `t = scale * i + offset`, replace multiplication with incremental `t' = t' + scale * step`,
// and replace loop exit test `cond = i < limit` with `cond = t' < scale * limit + offset` (dead-IV cleanup).
fn strength_reduce_iv(
    blocks: &[BasicBlock],
    preheader: usize,
    header: usize,
    latch: usize,
) -> (Vec<BasicBlock>, Vec<DerivedIv>, bool) {
    let loop_nodes = natural_loop(blocks, header, latch);
    // Find basic IV: `i = i + c` inside loop.
    let mut basic_ivs: BTreeMap<String, i64> = BTreeMap::new();
    for &b in &loop_nodes {
        for s in &blocks[b].stmts {
            if let Stmt::Assign {
                dst,
                op: Op::Add,
                lhs: Operand::Var(v),
                rhs: Operand::Const(c),
            } = s
            {
                if dst == v {
                    basic_ivs.insert(dst.clone(), *c);
                }
            }
        }
    }

    let mut derived: Vec<DerivedIv> = Vec::new();
    for &b in &loop_nodes {
        for s in &blocks[b].stmts {
            if let Stmt::Assign {
                dst,
                op: Op::Mul,
                lhs: Operand::Var(iv),
                rhs: Operand::Const(scale),
            } = s
            {
                if basic_ivs.contains_key(iv) {
                    derived.push(DerivedIv {
                        var: dst.clone(),
                        base_iv: iv.clone(),
                        scale: *scale,
                        offset: 0,
                    });
                }
            }
        }
    }

    let mut out = blocks.to_vec();
    if derived.is_empty() {
        return (out, derived, false);
    }

    for dv in &derived {
        let step = basic_ivs[&dv.base_iv];
        let sr_var = format!("{}_sr", dv.var);
        let init_scale_var = format!("{}_init_mul", dv.var);
        // Preheader: init_scale_var = base_iv * scale; sr_var = init_scale_var + offset.
        out[preheader].stmts.push(Stmt::Assign {
            dst: init_scale_var.clone(),
            op: Op::Mul,
            lhs: Operand::Var(dv.base_iv.clone()),
            rhs: Operand::Const(dv.scale),
        });
        out[preheader].stmts.push(Stmt::Assign {
            dst: sr_var.clone(),
            op: Op::Add,
            lhs: Operand::Var(init_scale_var),
            rhs: Operand::Const(dv.offset),
        });
        // Replace `t = scale * i` in loop with `t = sr_var`, and update `sr_var = sr_var + scale * step` right after `i = i + step`.
        for &b in &loop_nodes {
            let mut new_stmts = Vec::new();
            for s in &out[b].stmts {
                match s {
                    Stmt::Assign {
                        dst,
                        op: Op::Mul,
                        lhs: Operand::Var(iv),
                        rhs: Operand::Const(sc),
                    } if *dst == dv.var && *iv == dv.base_iv && *sc == dv.scale => {
                        new_stmts.push(Stmt::Copy {
                            dst: dst.clone(),
                            src: Operand::Var(sr_var.clone()),
                        });
                    }
                    Stmt::Assign {
                        dst,
                        op: Op::Add,
                        lhs: Operand::Var(v),
                        rhs: Operand::Const(c),
                    } if *dst == dv.base_iv && v == dst && *c == step => {
                        new_stmts.push(s.clone());
                        new_stmts.push(Stmt::Assign {
                            dst: sr_var.clone(),
                            op: Op::Add,
                            lhs: Operand::Var(sr_var.clone()),
                            rhs: Operand::Const(dv.scale * step),
                        });
                    }
                    _ => new_stmts.push(s.clone()),
                }
            }
            out[b].stmts = new_stmts;
        }
    }

    // Rewrite exit test `cond = i < limit` -> `cond = sr_var < limit * scale`, and prune `i = i + step` if `i` becomes dead!
    let dv0 = derived[0].clone();
    let sr_var = format!("{}_sr", dv0.var);
    let mut exit_rewritten = false;
    let mut preheader_adds = Vec::new();
    for s in &mut out[header].stmts {
        if let Stmt::Assign {
            dst: _,
            op: Op::Lt,
            lhs: Operand::Var(v),
            rhs: Operand::Var(limit),
        } = s
        {
            if *v == dv0.base_iv {
                let lim_scaled = format!("{limit}_scaled");
                preheader_adds.push(Stmt::Assign {
                    dst: lim_scaled.clone(),
                    op: Op::Mul,
                    lhs: Operand::Var(limit.clone()),
                    rhs: Operand::Const(dv0.scale),
                });
                *v = sr_var.clone();
                *limit = lim_scaled;
                exit_rewritten = true;
            }
        }
    }
    out[preheader].stmts.extend(preheader_adds);

    if exit_rewritten {
        // Check if `base_iv` has any remaining uses inside or live-out of the loop.
        let mut live_in = compute_live_in(&out);
        let used_in_loop = loop_nodes.iter().any(|&b| {
            out[b]
                .stmts
                .iter()
                .filter(|s| {
                    !matches!(s, Stmt::Assign { dst, op: Op::Add, lhs: Operand::Var(v), .. } if *dst == dv0.base_iv && v == dst)
                })
                .any(|s| stmt_uses(s).contains(&dv0.base_iv))
        });
        let live_outside = loop_nodes.iter().any(|&b| {
            succs(&out[b].term)
                .into_iter()
                .any(|succ| !loop_nodes.contains(&succ) && live_in[succ].contains(&dv0.base_iv))
        });
        if !used_in_loop && !live_outside {
            for &b in &loop_nodes {
                out[b].stmts.retain(|s| {
                    !matches!(s, Stmt::Assign { dst, op: Op::Add, lhs: Operand::Var(v), .. } if *dst == dv0.base_iv && v == dst)
                });
            }
        }
        let _ = &mut live_in;
    }

    (out, derived, exit_rewritten)
}

fn eval_cfg(blocks: &[BasicBlock], initial: &[(&str, i64)]) -> (i64, usize) {
    let mut env: BTreeMap<String, i64> = initial.iter().map(|&(k, v)| (k.to_string(), v)).collect();
    let mut bb = 0usize;
    let mut muls = 0usize;
    let mut steps = 0usize;
    let eval_op = |op: &Operand, env: &BTreeMap<String, i64>| match op {
        Operand::Const(c) => *c,
        Operand::Var(v) => *env.get(v).unwrap_or(&0),
    };
    loop {
        assert!(steps < 50_000, "infinite loop");
        steps += 1;
        for s in &blocks[bb].stmts {
            match s {
                Stmt::Assign { dst, op, lhs, rhs } => {
                    let l = eval_op(lhs, &env);
                    let r = eval_op(rhs, &env);
                    if *op == Op::Mul {
                        muls += 1;
                    }
                    let v = match op {
                        Op::Add => l.wrapping_add(r),
                        Op::Mul => l.wrapping_mul(r),
                        Op::Lt => i64::from(l < r),
                    };
                    env.insert(dst.clone(), v);
                }
                Stmt::Copy { dst, src } => {
                    let v = eval_op(src, &env);
                    env.insert(dst.clone(), v);
                }
            }
        }
        match &blocks[bb].term {
            Term::Jump(t) => bb = *t,
            Term::Branch {
                cond,
                then_bb,
                else_bb,
            } => {
                bb = if *env.get(cond).unwrap_or(&0) != 0 {
                    *then_bb
                } else {
                    *else_bb
                }
            }
            Term::Return(v) => return (*env.get(v).unwrap_or(&0), muls),
        }
    }
}

fn sample_loop_with_guarded_invariant() -> Vec<BasicBlock> {
    // B0 preheader -> B1 header (i < n ? B2 : B5)
    // B2 body: safe_inv = a * b (dominates exit B1! well, B2 does NOT dominate exit B1 unless B1 is checked, wait:
    // Make B1 a do-while? No, put safe_inv inside B1 (dominates exit) or let `safe_inv` be dead after loop,
    // while B3 (if-branch inside body) sets `unsafe_inv = x * 2` which IS live at exit B5 (if branch skipped, should stay initial `unsafe_inv`!).
    // B4 latch merges B2/B3, computes `t = i * 4; sum = sum + t + safe_inv; i = i + 1` -> B1.
    // B5 exit: `res = sum + unsafe_inv; return res`.
    vec![
        BasicBlock {
            stmts: vec![
                Stmt::Copy {
                    dst: "i".into(),
                    src: Operand::Const(0),
                },
                Stmt::Copy {
                    dst: "sum".into(),
                    src: Operand::Const(0),
                },
            ],
            term: Term::Jump(1),
        },
        BasicBlock {
            stmts: vec![Stmt::Assign {
                dst: "cond".into(),
                op: Op::Lt,
                lhs: Operand::Var("i".into()),
                rhs: Operand::Var("n".into()),
            }],
            term: Term::Branch {
                cond: "cond".into(),
                then_bb: 2,
                else_bb: 5,
            },
        },
        BasicBlock {
            stmts: vec![Stmt::Assign {
                dst: "safe_inv".into(), // dead after loop, unique def in loop -> safe to hoist!
                op: Op::Mul,
                lhs: Operand::Var("a".into()),
                rhs: Operand::Var("b".into()),
            }],
            term: Term::Branch {
                cond: "flag".into(),
                then_bb: 3,
                else_bb: 4,
            },
        },
        BasicBlock {
            stmts: vec![Stmt::Assign {
                dst: "unsafe_inv".into(), // live after loop AND B3 doesn't dominate exit B1 -> MUST NOT hoist!
                op: Op::Mul,
                lhs: Operand::Var("a".into()),
                rhs: Operand::Const(10),
            }],
            term: Term::Jump(4),
        },
        BasicBlock {
            stmts: vec![
                Stmt::Assign {
                    dst: "t".into(),
                    op: Op::Mul,
                    lhs: Operand::Var("i".into()),
                    rhs: Operand::Const(4),
                },
                Stmt::Assign {
                    dst: "sum1".into(),
                    op: Op::Add,
                    lhs: Operand::Var("sum".into()),
                    rhs: Operand::Var("t".into()),
                },
                Stmt::Assign {
                    dst: "sum".into(),
                    op: Op::Add,
                    lhs: Operand::Var("sum1".into()),
                    rhs: Operand::Var("safe_inv".into()),
                },
                Stmt::Assign {
                    dst: "i".into(),
                    op: Op::Add,
                    lhs: Operand::Var("i".into()),
                    rhs: Operand::Const(1),
                },
            ],
            term: Term::Jump(1),
        },
        BasicBlock {
            stmts: vec![Stmt::Assign {
                dst: "res".into(),
                op: Op::Add,
                lhs: Operand::Var("sum".into()),
                rhs: Operand::Var("unsafe_inv".into()),
            }],
            term: Term::Return("res".into()),
        },
    ]
}

fn main() {
    let cfg = sample_loop_with_guarded_invariant();
    let (after_licm, hoisted, skipped) = licm_hoist(&cfg, 0, 1, 4);
    let (after_sr, derived, exit_rewritten) = strength_reduce_iv(&after_licm, 0, 1, 4);
    println!("LICM hoisted={hoisted:?}, skipped_unsafe={skipped:?}");
    println!("Strength reduced={derived:?}, exit_rewritten={exit_rewritten}");

    let init = [
        ("n", 6),
        ("a", 3),
        ("b", 5),
        ("flag", 0),
        ("unsafe_inv", 77),
    ];
    let (v_orig, mul_orig) = eval_cfg(&cfg, &init);
    let (v_opt, mul_opt) = eval_cfg(&after_sr, &init);
    println!("Eval (flag=0): orig={v_orig} ({mul_orig} muls), opt={v_opt} ({mul_opt} muls)");
}

#[test]
fn skips_unsafe_guarded_invariant_while_hoisting_safe_invariant() {
    let cfg = sample_loop_with_guarded_invariant();
    let (after_licm, hoisted, skipped) = licm_hoist(&cfg, 0, 1, 4);
    assert_eq!(hoisted, vec!["safe_inv".to_string()]);
    assert_eq!(skipped, vec!["unsafe_inv".to_string()]);

    // With flag=0 and n=5, `unsafe_inv` must retain its initial value 77 (NOT overwritten by hoisted `a * 10 = 30`).
    let init_skip = [
        ("n", 5),
        ("a", 3),
        ("b", 5),
        ("flag", 0),
        ("unsafe_inv", 77),
    ];
    assert_eq!(
        eval_cfg(&after_licm, &init_skip).0,
        eval_cfg(&cfg, &init_skip).0
    );
    let init_take = [
        ("n", 5),
        ("a", 3),
        ("b", 5),
        ("flag", 1),
        ("unsafe_inv", 77),
    ];
    assert_eq!(
        eval_cfg(&after_licm, &init_take).0,
        eval_cfg(&cfg, &init_take).0
    );
}

#[test]
fn strength_reduces_derived_iv_and_eliminates_loop_muls() {
    let cfg = sample_loop_with_guarded_invariant();
    let (after_licm, _, _) = licm_hoist(&cfg, 0, 1, 4);
    let (after_sr, derived, exit_rewritten) = strength_reduce_iv(&after_licm, 0, 1, 4);
    assert_eq!(derived.len(), 1);
    assert!(exit_rewritten);

    for flag in [0, 1] {
        for n in [0, 1, 8] {
            let init = [
                ("n", n),
                ("a", 3),
                ("b", 5),
                ("flag", flag),
                ("unsafe_inv", 77),
            ];
            let (v0, m0) = eval_cfg(&cfg, &init);
            let (v1, m1) = eval_cfg(&after_sr, &init);
            assert_eq!(v1, v0);
            if n >= 3 {
                assert!(m1 < m0, "expected fewer muls ({m1} vs {m0})");
            }
        }
    }
}
