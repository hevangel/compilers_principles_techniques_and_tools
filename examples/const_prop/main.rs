//! Semilattice Constant Propagation (MFP) and path-by-path MOP nondistributivity witness.

use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ConstVal {
    Top, // UNDEF
    Const(i64),
    Bottom, // NAC
}

fn meet(a: ConstVal, b: ConstVal) -> ConstVal {
    match (a, b) {
        (ConstVal::Top, x) | (x, ConstVal::Top) => x,
        (ConstVal::Bottom, _) | (_, ConstVal::Bottom) => ConstVal::Bottom,
        (ConstVal::Const(x), ConstVal::Const(y)) => {
            if x == y {
                ConstVal::Const(x)
            } else {
                ConstVal::Bottom
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Op {
    Add,
    Mul,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Rhs {
    Const(i64),
    Var(String),
    Bin(Op, String, String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Stmt {
    dst: String,
    rhs: Rhs,
}

#[derive(Clone, Debug)]
struct BasicBlock {
    stmts: Vec<Stmt>,
    succs: Vec<usize>,
}

type Env = BTreeMap<String, ConstVal>;

fn eval_rhs(rhs: &Rhs, env: &Env) -> ConstVal {
    let get = |v: &str| *env.get(v).unwrap_or(&ConstVal::Top);
    match rhs {
        Rhs::Const(c) => ConstVal::Const(*c),
        Rhs::Var(v) => get(v),
        Rhs::Bin(op, l, r) => match (op, get(l), get(r)) {
            (Op::Add, ConstVal::Const(a), ConstVal::Const(b)) => ConstVal::Const(a.wrapping_add(b)),
            (Op::Mul, ConstVal::Const(0), _) | (Op::Mul, _, ConstVal::Const(0)) => {
                ConstVal::Const(0)
            }
            (Op::Mul, ConstVal::Const(a), ConstVal::Const(b)) => ConstVal::Const(a.wrapping_mul(b)),
            (_, ConstVal::Bottom, _) | (_, _, ConstVal::Bottom) => ConstVal::Bottom,
            _ => ConstVal::Top,
        },
    }
}

fn transfer_block(stmts: &[Stmt], initial: &Env) -> Env {
    let mut env = initial.clone();
    for s in stmts {
        let val = eval_rhs(&s.rhs, &env);
        env.insert(s.dst.clone(), val);
    }
    env
}

fn solve_mfp(blocks: &[BasicBlock], initial: &Env) -> (Vec<Env>, Vec<Env>) {
    let n = blocks.len();
    let mut preds = vec![Vec::new(); n];
    let mut all_vars: BTreeSet<String> = initial.keys().cloned().collect();
    for (b, blk) in blocks.iter().enumerate() {
        for &s in &blk.succs {
            preds[s].push(b);
        }
        for stmt in &blk.stmts {
            all_vars.insert(stmt.dst.clone());
        }
    }

    let top_env: Env = all_vars
        .iter()
        .map(|v| (v.clone(), ConstVal::Top))
        .collect();
    let mut in_env = vec![top_env.clone(); n];
    in_env[0] = initial.clone();
    let mut out_env = vec![top_env; n];
    out_env[0] = transfer_block(&blocks[0].stmts, &in_env[0]);

    loop {
        let mut changed = false;
        for b in 0..n {
            let new_in = if b == 0 {
                initial.clone()
            } else if preds[b].is_empty() {
                in_env[b].clone()
            } else {
                let mut acc = out_env[preds[b][0]].clone();
                for &p in &preds[b][1..] {
                    for v in &all_vars {
                        let m = meet(
                            *acc.get(v).unwrap_or(&ConstVal::Top),
                            *out_env[p].get(v).unwrap_or(&ConstVal::Top),
                        );
                        acc.insert(v.clone(), m);
                    }
                }
                acc
            };
            let new_out = transfer_block(&blocks[b].stmts, &new_in);
            if new_in != in_env[b] || new_out != out_env[b] {
                in_env[b] = new_in;
                out_env[b] = new_out;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    (in_env, out_env)
}

// Compute exact Meet-Over-All-Paths (MOP) output for acyclic CFGs.
fn solve_mop_acyclic(blocks: &[BasicBlock], target_block: usize, initial: &Env) -> Env {
    fn dfs(b: usize, target: usize, blocks: &[BasicBlock], cur_env: &Env, results: &mut Vec<Env>) {
        let out = transfer_block(&blocks[b].stmts, cur_env);
        if b == target {
            results.push(out);
            return;
        }
        for &s in &blocks[b].succs {
            dfs(s, target, blocks, &out, results);
        }
    }

    let mut path_envs = Vec::new();
    dfs(0, target_block, blocks, initial, &mut path_envs);
    let mut keys: BTreeSet<String> = BTreeSet::new();
    for e in &path_envs {
        keys.extend(e.keys().cloned());
    }
    let mut joined = Env::new();
    for k in keys {
        let mut acc = ConstVal::Top;
        for e in &path_envs {
            acc = meet(acc, *e.get(&k).unwrap_or(&ConstVal::Top));
        }
        joined.insert(k, acc);
    }
    joined
}

fn fold_blocks(blocks: &[BasicBlock], in_env: &[Env]) -> Vec<BasicBlock> {
    blocks
        .iter()
        .enumerate()
        .map(|(b, blk)| {
            let mut env = in_env[b].clone();
            let mut new_stmts = Vec::with_capacity(blk.stmts.len());
            for s in &blk.stmts {
                let v = eval_rhs(&s.rhs, &env);
                let folded_rhs = match v {
                    ConstVal::Const(c) => Rhs::Const(c),
                    _ => s.rhs.clone(),
                };
                new_stmts.push(Stmt {
                    dst: s.dst.clone(),
                    rhs: folded_rhs,
                });
                env.insert(s.dst.clone(), v);
            }
            BasicBlock {
                stmts: new_stmts,
                succs: blk.succs.clone(),
            }
        })
        .collect()
}

fn nondistributivity_witness() -> Vec<BasicBlock> {
    // if c { x = 2; y = 3 } else { x = 3; y = 2 }; z = x + y
    vec![
        BasicBlock {
            stmts: vec![],
            succs: vec![1, 2],
        },
        BasicBlock {
            stmts: vec![
                Stmt {
                    dst: "x".into(),
                    rhs: Rhs::Const(2),
                },
                Stmt {
                    dst: "y".into(),
                    rhs: Rhs::Const(3),
                },
            ],
            succs: vec![3],
        },
        BasicBlock {
            stmts: vec![
                Stmt {
                    dst: "x".into(),
                    rhs: Rhs::Const(3),
                },
                Stmt {
                    dst: "y".into(),
                    rhs: Rhs::Const(2),
                },
            ],
            succs: vec![3],
        },
        BasicBlock {
            stmts: vec![Stmt {
                dst: "z".into(),
                rhs: Rhs::Bin(Op::Add, "x".into(), "y".into()),
            }],
            succs: vec![],
        },
    ]
}

fn main() {
    let cfg = nondistributivity_witness();
    let init = Env::from([("c".into(), ConstVal::Bottom)]);
    let (_, mfp_out) = solve_mfp(&cfg, &init);
    let mop_out = solve_mop_acyclic(&cfg, 3, &init);
    println!("Nondistributivity witness (`z = x + y`):");
    println!(
        "  MFP out[B3][z] = {:?} (strictly weaker than MOP!)",
        mfp_out[3]["z"]
    );
    println!("  MOP out[B3][z] = {:?}", mop_out["z"]);

    let zero_cfg = vec![BasicBlock {
        stmts: vec![
            Stmt {
                dst: "p".into(),
                rhs: Rhs::Const(0),
            },
            Stmt {
                dst: "q".into(),
                rhs: Rhs::Bin(Op::Mul, "unknown".into(), "p".into()),
            },
            Stmt {
                dst: "w".into(),
                rhs: Rhs::Var("q".into()),
            },
        ],
        succs: vec![],
    }];
    let (in_z, out_z) = solve_mfp(
        &zero_cfg,
        &Env::from([("unknown".into(), ConstVal::Bottom)]),
    );
    let folded = fold_blocks(&zero_cfg, &in_z);
    println!(
        "NAC * 0 => {:?}, folded: {:?}",
        out_z[0]["q"], folded[0].stmts
    );
}

#[test]
fn mfp_yields_nac_while_mop_yields_5_on_nondistributive_diamond() {
    let cfg = nondistributivity_witness();
    let init = Env::from([("c".into(), ConstVal::Bottom)]);
    let (_, mfp_out) = solve_mfp(&cfg, &init);
    let mop_out = solve_mop_acyclic(&cfg, 3, &init);

    assert_eq!(mfp_out[3]["x"], ConstVal::Bottom);
    assert_eq!(mfp_out[3]["y"], ConstVal::Bottom);
    assert_eq!(mfp_out[3]["z"], ConstVal::Bottom);
    assert_eq!(mop_out["z"], ConstVal::Const(5));
}

#[test]
fn folds_loop_constants_and_nac_times_zero() {
    let cfg = vec![
        BasicBlock {
            stmts: vec![
                Stmt {
                    dst: "a".into(),
                    rhs: Rhs::Const(4),
                },
                Stmt {
                    dst: "b".into(),
                    rhs: Rhs::Const(0),
                },
            ],
            succs: vec![1],
        },
        BasicBlock {
            stmts: vec![
                Stmt {
                    dst: "c".into(),
                    rhs: Rhs::Bin(Op::Add, "a".into(), "b".into()),
                },
                Stmt {
                    dst: "d".into(),
                    rhs: Rhs::Bin(Op::Mul, "c".into(), "b".into()),
                },
            ],
            succs: vec![1, 2],
        },
        BasicBlock {
            stmts: vec![Stmt {
                dst: "z".into(),
                rhs: Rhs::Bin(Op::Mul, "param".into(), "b".into()),
            }],
            succs: vec![],
        },
    ];
    let init = Env::from([("param".into(), ConstVal::Bottom)]);
    let (in_env, out_env) = solve_mfp(&cfg, &init);
    assert_eq!(out_env[1]["c"], ConstVal::Const(4));
    assert_eq!(out_env[2]["z"], ConstVal::Const(0));
    let folded = fold_blocks(&cfg, &in_env);
    assert_eq!(folded[2].stmts[0].rhs, Rhs::Const(0));
}
