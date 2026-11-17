//! Forward must-analysis for Available Expressions, Global CSE, and Copy Propagation.

use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Op {
    Add,
    Sub,
    Mul,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct ExprKey {
    op: Op,
    lhs: String,
    rhs: String,
}

impl ExprKey {
    fn canonical(op: Op, a: &str, b: &str) -> Self {
        match op {
            Op::Add | Op::Mul if b < a => Self {
                op,
                lhs: b.to_string(),
                rhs: a.to_string(),
            },
            _ => Self {
                op,
                lhs: a.to_string(),
                rhs: b.to_string(),
            },
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Stmt {
    Const {
        dst: String,
        val: i64,
    },
    Copy {
        dst: String,
        src: String,
    },
    Bin {
        dst: String,
        op: Op,
        lhs: String,
        rhs: String,
    },
    Return(String),
}

#[derive(Clone, Debug)]
struct BasicBlock {
    stmts: Vec<Stmt>,
    succs: Vec<usize>,
}

struct AvailResult {
    in_avail: Vec<BTreeSet<ExprKey>>,
    out_avail: Vec<BTreeSet<ExprKey>>,
}

// Forward must-analysis initialized to Universe for all blocks except ENTRY.
fn available_expressions(blocks: &[BasicBlock]) -> AvailResult {
    let n = blocks.len();
    let mut preds = vec![Vec::new(); n];
    let mut universe: BTreeSet<ExprKey> = BTreeSet::new();
    for (b, blk) in blocks.iter().enumerate() {
        for &s in &blk.succs {
            preds[s].push(b);
        }
        for stmt in &blk.stmts {
            if let Stmt::Bin { op, lhs, rhs, .. } = stmt {
                universe.insert(ExprKey::canonical(*op, lhs, rhs));
            }
        }
    }

    let mut e_gen: Vec<BTreeSet<ExprKey>> = vec![BTreeSet::new(); n];
    let mut e_kill: Vec<BTreeSet<ExprKey>> = vec![BTreeSet::new(); n];
    for (b, blk) in blocks.iter().enumerate() {
        for stmt in &blk.stmts {
            match stmt {
                Stmt::Bin { dst, op, lhs, rhs } => {
                    let key = ExprKey::canonical(*op, lhs, rhs);
                    e_gen[b].insert(key);
                    for expr in &universe {
                        if expr.lhs == *dst || expr.rhs == *dst {
                            e_gen[b].remove(expr);
                            e_kill[b].insert(expr.clone());
                        }
                    }
                    // If dst != lhs && dst != rhs, key remains generated.
                    if dst != lhs && dst != rhs {
                        e_gen[b].insert(ExprKey::canonical(*op, lhs, rhs));
                    }
                }
                Stmt::Const { dst, .. } | Stmt::Copy { dst, .. } => {
                    for expr in &universe {
                        if expr.lhs == *dst || expr.rhs == *dst {
                            e_gen[b].remove(expr);
                            e_kill[b].insert(expr.clone());
                        }
                    }
                }
                Stmt::Return(_) => {}
            }
        }
    }

    let mut in_avail = vec![BTreeSet::new(); n];
    let mut out_avail: Vec<BTreeSet<ExprKey>> = (0..n)
        .map(|b| {
            if b == 0 {
                BTreeSet::new()
            } else {
                universe.clone()
            }
        })
        .collect();

    loop {
        let mut changed = false;
        for b in 0..n {
            if b == 0 {
                // ENTRY has no incoming paths except outer; keep in[0] empty.
                let mut new_out = e_gen[0].clone();
                for e in &in_avail[0] {
                    if !e_kill[0].contains(e) {
                        new_out.insert(e.clone());
                    }
                }
                if new_out != out_avail[0] {
                    out_avail[0] = new_out;
                    changed = true;
                }
                continue;
            }
            let new_in = if preds[b].is_empty() {
                BTreeSet::new()
            } else {
                let mut iter = preds[b].iter();
                let first = iter.next().unwrap();
                let mut acc = out_avail[*first].clone();
                for p in iter {
                    acc.retain(|e| out_avail[*p].contains(e));
                }
                acc
            };
            let mut new_out = e_gen[b].clone();
            for e in &new_in {
                if !e_kill[b].contains(e) {
                    new_out.insert(e.clone());
                }
            }
            if new_in != in_avail[b] || new_out != out_avail[b] {
                in_avail[b] = new_in;
                out_avail[b] = new_out;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    AvailResult {
        in_avail,
        out_avail,
    }
}

fn global_cse(blocks: &[BasicBlock]) -> (Vec<BasicBlock>, usize) {
    let avail = available_expressions(blocks);
    let mut cse_temps: BTreeMap<ExprKey, String> = BTreeMap::new();
    let mut all_avail_uses: BTreeSet<ExprKey> = BTreeSet::new();

    for (b, blk) in blocks.iter().enumerate() {
        let mut cur = avail.in_avail[b].clone();
        for stmt in &blk.stmts {
            match stmt {
                Stmt::Bin { dst, op, lhs, rhs } => {
                    let key = ExprKey::canonical(*op, lhs, rhs);
                    if cur.contains(&key) {
                        all_avail_uses.insert(key.clone());
                    }
                    cur.retain(|e| e.lhs != *dst && e.rhs != *dst);
                    if dst != lhs && dst != rhs {
                        cur.insert(key);
                    }
                }
                Stmt::Const { dst, .. } | Stmt::Copy { dst, .. } => {
                    cur.retain(|e| e.lhs != *dst && e.rhs != *dst);
                }
                Stmt::Return(_) => {}
            }
        }
    }

    for (idx, k) in all_avail_uses.iter().enumerate() {
        cse_temps.insert(k.clone(), format!("_cse_{idx}"));
    }

    let mut out_blocks = blocks.to_vec();
    let mut replaced = 0usize;
    for (b, blk) in out_blocks.iter_mut().enumerate() {
        let mut cur = avail.in_avail[b].clone();
        let mut new_stmts = Vec::new();
        for stmt in &blk.stmts {
            match stmt {
                Stmt::Bin { dst, op, lhs, rhs } => {
                    let key = ExprKey::canonical(*op, lhs, rhs);
                    if let Some(tmp) = cse_temps.get(&key) {
                        if cur.contains(&key) {
                            new_stmts.push(Stmt::Copy {
                                dst: dst.clone(),
                                src: tmp.clone(),
                            });
                            replaced += 1;
                        } else {
                            new_stmts.push(stmt.clone());
                            new_stmts.push(Stmt::Copy {
                                dst: tmp.clone(),
                                src: dst.clone(),
                            });
                        }
                    } else {
                        new_stmts.push(stmt.clone());
                    }
                    cur.retain(|e| e.lhs != *dst && e.rhs != *dst);
                    if dst != lhs && dst != rhs {
                        cur.insert(key);
                    }
                }
                Stmt::Const { dst, .. } | Stmt::Copy { dst, .. } => {
                    new_stmts.push(stmt.clone());
                    cur.retain(|e| e.lhs != *dst && e.rhs != *dst);
                }
                Stmt::Return(_) => new_stmts.push(stmt.clone()),
            }
        }
        blk.stmts = new_stmts;
    }
    (out_blocks, replaced)
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct CopyPair {
    dst: String,
    src: String,
}

fn copy_propagation(blocks: &[BasicBlock]) -> (Vec<BasicBlock>, usize) {
    let n = blocks.len();
    let mut preds = vec![Vec::new(); n];
    let mut all_copies: BTreeSet<CopyPair> = BTreeSet::new();
    for (b, blk) in blocks.iter().enumerate() {
        for &s in &blk.succs {
            preds[s].push(b);
        }
        for stmt in &blk.stmts {
            if let Stmt::Copy { dst, src } = stmt {
                if dst != src {
                    all_copies.insert(CopyPair {
                        dst: dst.clone(),
                        src: src.clone(),
                    });
                }
            }
        }
    }
    let mut c_gen = vec![BTreeSet::new(); n];
    let mut c_kill = vec![BTreeSet::new(); n];
    for (b, blk) in blocks.iter().enumerate() {
        for stmt in &blk.stmts {
            let defined = match stmt {
                Stmt::Const { dst, .. } | Stmt::Copy { dst, .. } | Stmt::Bin { dst, .. } => {
                    Some(dst)
                }
                Stmt::Return(_) => None,
            };
            if let Some(d) = defined {
                for cp in &all_copies {
                    if cp.dst == *d || cp.src == *d {
                        c_gen[b].remove(cp);
                        c_kill[b].insert(cp.clone());
                    }
                }
            }
            if let Stmt::Copy { dst, src } = stmt {
                if dst != src {
                    c_gen[b].insert(CopyPair {
                        dst: dst.clone(),
                        src: src.clone(),
                    });
                }
            }
        }
    }

    let mut in_copies = vec![BTreeSet::new(); n];
    let mut out_copies: Vec<BTreeSet<CopyPair>> = (0..n)
        .map(|b| {
            if b == 0 {
                BTreeSet::new()
            } else {
                all_copies.clone()
            }
        })
        .collect();
    loop {
        let mut changed = false;
        for b in 0..n {
            let new_in = if b == 0 || preds[b].is_empty() {
                BTreeSet::new()
            } else {
                let mut it = preds[b].iter();
                let first = it.next().unwrap();
                let mut acc = out_copies[*first].clone();
                for p in it {
                    acc.retain(|c| out_copies[*p].contains(c));
                }
                acc
            };
            let mut new_out = c_gen[b].clone();
            for c in &new_in {
                if !c_kill[b].contains(c) {
                    new_out.insert(c.clone());
                }
            }
            if new_in != in_copies[b] || new_out != out_copies[b] {
                in_copies[b] = new_in;
                out_copies[b] = new_out;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }

    let subst = |name: &str, active: &BTreeSet<CopyPair>| -> Option<String> {
        let matches: Vec<&str> = active
            .iter()
            .filter(|c| c.dst == name)
            .map(|c| c.src.as_str())
            .collect();
        if matches.len() == 1 {
            Some(matches[0].to_string())
        } else {
            None
        }
    };

    let mut out_blocks = blocks.to_vec();
    let mut propagated = 0usize;
    for (b, blk) in out_blocks.iter_mut().enumerate() {
        let mut active = in_copies[b].clone();
        for stmt in &mut blk.stmts {
            match stmt {
                Stmt::Const { dst, .. } => {
                    active.retain(|c| c.dst != *dst && c.src != *dst);
                }
                Stmt::Copy { dst, src } => {
                    if let Some(r) = subst(src, &active) {
                        *src = r;
                        propagated += 1;
                    }
                    let d_clone = dst.clone();
                    let s_clone = src.clone();
                    active.retain(|c| c.dst != d_clone && c.src != d_clone);
                    if d_clone != s_clone {
                        active.insert(CopyPair {
                            dst: d_clone,
                            src: s_clone,
                        });
                    }
                }
                Stmt::Bin { dst, lhs, rhs, .. } => {
                    if let Some(r) = subst(lhs, &active) {
                        *lhs = r;
                        propagated += 1;
                    }
                    if let Some(r) = subst(rhs, &active) {
                        *rhs = r;
                        propagated += 1;
                    }
                    let d_clone = dst.clone();
                    active.retain(|c| c.dst != d_clone && c.src != d_clone);
                }
                Stmt::Return(v) => {
                    if let Some(r) = subst(v, &active) {
                        *v = r;
                        propagated += 1;
                    }
                }
            }
        }
    }
    (out_blocks, propagated)
}

fn eval_cfg(blocks: &[BasicBlock], branch_choices: &[usize], initial: &[(&str, i64)]) -> i64 {
    let mut env: BTreeMap<String, i64> = initial.iter().map(|&(k, v)| (k.to_string(), v)).collect();
    let mut bb = 0usize;
    let mut choice_idx = 0usize;
    let mut steps = 0usize;
    loop {
        assert!(steps < 10_000, "infinite loop");
        steps += 1;
        for stmt in &blocks[bb].stmts {
            match stmt {
                Stmt::Const { dst, val } => {
                    env.insert(dst.clone(), *val);
                }
                Stmt::Copy { dst, src } => {
                    let v = *env.get(src).unwrap_or(&0);
                    env.insert(dst.clone(), v);
                }
                Stmt::Bin { dst, op, lhs, rhs } => {
                    let l = *env.get(lhs).unwrap_or(&0);
                    let r = *env.get(rhs).unwrap_or(&0);
                    let res = match op {
                        Op::Add => l.wrapping_add(r),
                        Op::Sub => l.wrapping_sub(r),
                        Op::Mul => l.wrapping_mul(r),
                    };
                    env.insert(dst.clone(), res);
                }
                Stmt::Return(v) => return *env.get(v).unwrap_or(&0),
            }
        }
        let succs = &blocks[bb].succs;
        bb = if succs.len() == 1 {
            succs[0]
        } else {
            let c = branch_choices[choice_idx];
            choice_idx += 1;
            succs[c]
        };
    }
}

fn sample_diamond() -> Vec<BasicBlock> {
    // B0 -> {B1, B2}; B1 computes x+y (and a*b); B2 computes y+x only; B3 recomputes x+y and a*b, uses t.
    vec![
        BasicBlock {
            stmts: vec![],
            succs: vec![1, 2],
        },
        BasicBlock {
            stmts: vec![
                Stmt::Bin {
                    dst: "s1".into(),
                    op: Op::Add,
                    lhs: "x".into(),
                    rhs: "y".into(),
                },
                Stmt::Bin {
                    dst: "m1".into(),
                    op: Op::Mul,
                    lhs: "a".into(),
                    rhs: "b".into(),
                },
            ],
            succs: vec![3],
        },
        BasicBlock {
            stmts: vec![Stmt::Bin {
                dst: "s2".into(),
                op: Op::Add,
                lhs: "y".into(),
                rhs: "x".into(),
            }],
            succs: vec![3],
        },
        BasicBlock {
            stmts: vec![
                Stmt::Bin {
                    dst: "s3".into(),
                    op: Op::Add,
                    lhs: "x".into(),
                    rhs: "y".into(),
                },
                Stmt::Bin {
                    dst: "m2".into(),
                    op: Op::Mul,
                    lhs: "a".into(),
                    rhs: "b".into(),
                },
                Stmt::Bin {
                    dst: "res".into(),
                    op: Op::Sub,
                    lhs: "s3".into(),
                    rhs: "m2".into(),
                },
                Stmt::Const {
                    dst: "c".into(),
                    val: 1,
                },
                Stmt::Bin {
                    dst: "out".into(),
                    op: Op::Add,
                    lhs: "res".into(),
                    rhs: "c".into(),
                },
                Stmt::Return("out".into()),
            ],
            succs: vec![],
        },
    ]
}

fn main() {
    let cfg = sample_diamond();
    let avail = available_expressions(&cfg);
    println!("Available out of B1: {:?}", avail.out_avail[1]);
    println!(
        "Available in B3 (after diamond merge): {:?}",
        avail.in_avail[3]
    );
    let (after_cse, cse_hits) = global_cse(&cfg);
    let (after_cp, cp_hits) = copy_propagation(&after_cse);
    let init = [("x", 10), ("y", 5), ("a", 2), ("b", 3)];
    println!(
        "CSE replacements = {cse_hits}, Copy propagations = {cp_hits}, eval orig={}, opt={}",
        eval_cfg(&cfg, &[0], &init),
        eval_cfg(&after_cp, &[0], &init)
    );
}

#[test]
fn must_intersection_requires_both_branches_and_propagates_copies() {
    let cfg = sample_diamond();
    let avail = available_expressions(&cfg);
    let xy = ExprKey::canonical(Op::Add, "x", "y");
    let ab = ExprKey::canonical(Op::Mul, "a", "b");
    assert!(avail.in_avail[3].contains(&xy));
    assert!(!avail.in_avail[3].contains(&ab));
    assert!(avail.out_avail[1].contains(&ab));

    let (after_cse, cse_hits) = global_cse(&cfg);
    assert_eq!(cse_hits, 1);
    let (after_cp, cp_hits) = copy_propagation(&after_cse);
    assert!(cp_hits >= 1);

    let init = [("x", 10), ("y", 5), ("a", 2), ("b", 3)];
    for path in [0usize, 1] {
        assert_eq!(
            eval_cfg(&cfg, &[path], &init),
            eval_cfg(&after_cp, &[path], &init)
        );
    }
}

#[test]
fn loop_header_converges_from_universal_set() {
    // B0 defines a+b before loop B1; B1 uses a+b; B2 kills a on back-edge OR preserves b+c.
    let cfg = vec![
        BasicBlock {
            stmts: vec![
                Stmt::Bin {
                    dst: "t1".into(),
                    op: Op::Add,
                    lhs: "a".into(),
                    rhs: "b".into(),
                },
                Stmt::Bin {
                    dst: "t2".into(),
                    op: Op::Add,
                    lhs: "b".into(),
                    rhs: "c".into(),
                },
            ],
            succs: vec![1],
        },
        BasicBlock {
            stmts: vec![Stmt::Return("t2".into())],
            succs: vec![2, 3],
        },
        BasicBlock {
            stmts: vec![Stmt::Const {
                dst: "a".into(),
                val: 99,
            }],
            succs: vec![1],
        },
        BasicBlock {
            stmts: vec![Stmt::Return("t2".into())],
            succs: vec![],
        },
    ];
    let avail = available_expressions(&cfg);
    let ab = ExprKey::canonical(Op::Add, "a", "b");
    let bc = ExprKey::canonical(Op::Add, "b", "c");
    // `a + b` was killed in B2, so Universal init at loop header must converge to NOT having `a + b`.
    assert!(!avail.in_avail[1].contains(&ab));
    // `b + c` is never killed inside B1 or B2, so Universal init keeps `b + c` available at header B1.
    assert!(avail.in_avail[1].contains(&bc));
}
