//! Construct minimal SSA form via dominance frontiers, rename variables, fold constants, and de-SSA.

use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BinOp {
    Add,
    Sub,
    Lt,
}

// Pre-SSA CFG types: variables are mutable names that may be assigned in multiple blocks.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Rhs {
    Const(i64),
    Var(String),
    Bin(BinOp, String, String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Stmt {
    dst: String,
    rhs: Rhs,
}

#[derive(Clone, Debug, PartialEq, Eq)]
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

// SSA variable: `(base_name, version)`. Version 0 represents an initial/unassigned parameter.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct SsaVar {
    name: String,
    ver: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum SsaRhs {
    Const(i64),
    Var(SsaVar),
    Bin(BinOp, SsaVar, SsaVar),
}

// Merge instruction placed at block entry: selects `incoming[pred]` depending on which
// predecessor block transferred control here. All phis in a block execute simultaneously.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Phi {
    dst: SsaVar,
    incoming: BTreeMap<usize, SsaVar>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct SsaStmt {
    dst: SsaVar,
    rhs: SsaRhs,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum SsaTerm {
    Jump(usize),
    Branch {
        cond: SsaVar,
        then_bb: usize,
        else_bb: usize,
    },
    Return(SsaVar),
}

#[derive(Clone, Debug)]
struct SsaBlock {
    phis: Vec<Phi>,
    stmts: Vec<SsaStmt>,
    term: SsaTerm,
}

// Complete SSA CFG along with the dominance analysis artifacts used to construct and verify it.
struct SsaProgram {
    blocks: Vec<SsaBlock>,
    preds: Vec<Vec<usize>>,
    dom: Vec<BTreeSet<usize>>,
    idom: Vec<Option<usize>>,
    dom_frontiers: Vec<BTreeSet<usize>>,
}

fn successors_of(term: &Term) -> Vec<usize> {
    match term {
        Term::Jump(t) => vec![*t],
        Term::Branch {
            then_bb, else_bb, ..
        } => vec![*then_bb, *else_bb],
        Term::Return(_) => vec![],
    }
}

fn ssa_successors_of(term: &SsaTerm) -> Vec<usize> {
    match term {
        SsaTerm::Jump(t) => vec![*t],
        SsaTerm::Branch {
            then_bb, else_bb, ..
        } => vec![*then_bb, *else_bb],
        SsaTerm::Return(_) => vec![],
    }
}

struct DomTree {
    dom: Vec<BTreeSet<usize>>,
    idom: Vec<Option<usize>>,
    children: Vec<Vec<usize>>,
}

// Iterative dataflow computation of dominators over reachable blocks:
//   Dom(0) = {0}
//   Dom(b) = {b} U (intersection of Dom(p) over reachable predecessors p)
// Restricting to `reachable` guarantees unreachable blocks do not distort intersections.
fn compute_dominators(
    num_blocks: usize,
    preds: &[Vec<usize>],
    reachable: &BTreeSet<usize>,
) -> DomTree {
    let mut dom = vec![BTreeSet::new(); num_blocks];
    for &b in reachable {
        if b == 0 {
            dom[b].insert(0);
        } else {
            dom[b] = reachable.clone();
        }
    }
    loop {
        let mut changed = false;
        for &b in reachable {
            if b == 0 {
                continue;
            }
            let rpreds: Vec<usize> = preds[b]
                .iter()
                .copied()
                .filter(|p| reachable.contains(p))
                .collect();
            if rpreds.is_empty() {
                continue;
            }
            let mut inter = dom[rpreds[0]].clone();
            for &p in &rpreds[1..] {
                inter.retain(|x| dom[p].contains(x));
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

    let mut idom = vec![None; num_blocks];
    let mut dom_children = vec![Vec::new(); num_blocks];
    for &b in reachable {
        if b == 0 {
            continue;
        }
        let strict: Vec<usize> = dom[b].iter().copied().filter(|&d| d != b).collect();
        // Immediate dominator is the strict dominator dominated by all other strict dominators.
        let found = strict
            .iter()
            .copied()
            .find(|&cand| strict.iter().all(|&other| dom[cand].contains(&other)));
        idom[b] = found;
        if let Some(parent) = found {
            dom_children[parent].push(b);
        }
    }
    DomTree {
        dom,
        idom,
        children: dom_children,
    }
}

// Compute dominance frontiers using the Cooper-Harvey-Kennedy algorithm:
// At every join block `b` (>= 2 reachable predecessors), walk up the dominator tree from each
// predecessor `p` until reaching `idom[b]`. Every visited node `r` dominates `p` (a predecessor
// of `b`) without strictly dominating `b`, so `b` belongs to `DF[r]`.
fn compute_dominance_frontiers(
    preds: &[Vec<usize>],
    idom: &[Option<usize>],
    reachable: &BTreeSet<usize>,
) -> Vec<BTreeSet<usize>> {
    let mut df = vec![BTreeSet::new(); preds.len()];
    for &b in reachable {
        let rpreds: Vec<usize> = preds[b]
            .iter()
            .copied()
            .filter(|p| reachable.contains(p))
            .collect();
        if rpreds.len() >= 2 {
            for p in rpreds {
                let mut runner = Some(p);
                while let Some(r) = runner {
                    if Some(r) == idom[b] {
                        break;
                    }
                    df[r].insert(b);
                    runner = idom[r];
                }
            }
        }
    }
    df
}

// Convert a CFG into minimal pruned SSA form in 4 phases:
// 1. Reachability + dominator tree + dominance frontiers.
// 2. Backward liveness analysis (`live_in`) to prune dead phis.
// 3. Iterated dominance frontier worklist to place phis.
// 4. Dominator-tree DFS renaming with per-variable version stacks.
fn to_ssa(cfg: &[BasicBlock]) -> SsaProgram {
    let n = cfg.len();
    let mut preds = vec![Vec::new(); n];
    for (b, blk) in cfg.iter().enumerate() {
        for s in successors_of(&blk.term) {
            preds[s].push(b);
        }
    }

    // Reachable blocks from entry 0 (unreachable blocks are stripped from SSA).
    let mut reachable = BTreeSet::from([0usize]);
    let mut stack = vec![0usize];
    while let Some(b) = stack.pop() {
        for s in successors_of(&cfg[b].term) {
            if reachable.insert(s) {
                stack.push(s);
            }
        }
    }

    let DomTree {
        dom,
        idom,
        children: dom_children,
    } = compute_dominators(n, &preds, &reachable);
    let dom_frontiers = compute_dominance_frontiers(&preds, &idom, &reachable);

    // Collect variable definitions per reachable block.
    let mut defs: BTreeMap<String, BTreeSet<usize>> = BTreeMap::new();
    for &b in &reachable {
        for stmt in &cfg[b].stmts {
            defs.entry(stmt.dst.clone()).or_default().insert(b);
        }
    }

    // Liveness analysis for pruned SSA (avoid placing dead phis for block-local temps).
    let mut use_blk: Vec<BTreeSet<String>> = vec![BTreeSet::new(); n];
    let mut def_blk: Vec<BTreeSet<String>> = vec![BTreeSet::new(); n];
    for &b in &reachable {
        for stmt in &cfg[b].stmts {
            match &stmt.rhs {
                Rhs::Const(_) => {}
                Rhs::Var(v) => {
                    if !def_blk[b].contains(v) {
                        use_blk[b].insert(v.clone());
                    }
                }
                Rhs::Bin(_, l, r) => {
                    if !def_blk[b].contains(l) {
                        use_blk[b].insert(l.clone());
                    }
                    if !def_blk[b].contains(r) {
                        use_blk[b].insert(r.clone());
                    }
                }
            }
            def_blk[b].insert(stmt.dst.clone());
        }
        match &cfg[b].term {
            Term::Jump(_) => {}
            Term::Branch { cond, .. } => {
                if !def_blk[b].contains(cond) {
                    use_blk[b].insert(cond.clone());
                }
            }
            Term::Return(v) => {
                if !def_blk[b].contains(v) {
                    use_blk[b].insert(v.clone());
                }
            }
        }
    }
    let mut live_in: Vec<BTreeSet<String>> = vec![BTreeSet::new(); n];
    let mut live_out: Vec<BTreeSet<String>> = vec![BTreeSet::new(); n];
    loop {
        let mut changed = false;
        for &b in reachable.iter().rev() {
            let mut new_out = BTreeSet::new();
            for s in successors_of(&cfg[b].term) {
                new_out.extend(live_in[s].iter().cloned());
            }
            let mut new_in = use_blk[b].clone();
            for v in &new_out {
                if !def_blk[b].contains(v) {
                    new_in.insert(v.clone());
                }
            }
            if new_out != live_out[b] || new_in != live_in[b] {
                live_out[b] = new_out;
                live_in[b] = new_in;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }

    // Minimal (pruned) phi placement via iterated dominance frontier worklist:
    // only insert a phi for `var` at merge block `y` if `var` is live-in at `y`,
    // while continuing to propagate through `DF[y]` for downstream merges.
    let mut phi_vars: Vec<BTreeSet<String>> = vec![BTreeSet::new(); n];
    for (var, def_blocks) in &defs {
        let mut has_phi = BTreeSet::new();
        let mut worklist: Vec<usize> = def_blocks.iter().copied().collect();
        let mut queued = def_blocks.clone();
        while let Some(b) = worklist.pop() {
            for &y in &dom_frontiers[b] {
                if has_phi.insert(y) {
                    if live_in[y].contains(var) {
                        phi_vars[y].insert(var.clone());
                    }
                    if queued.insert(y) {
                        worklist.push(y);
                    }
                }
            }
        }
    }

    let mut ssa_blocks: Vec<SsaBlock> = cfg
        .iter()
        .enumerate()
        .map(|(b, blk)| SsaBlock {
            phis: phi_vars[b]
                .iter()
                .map(|v| Phi {
                    dst: SsaVar {
                        name: v.clone(),
                        ver: 0,
                    },
                    incoming: BTreeMap::new(),
                })
                .collect(),
            stmts: if reachable.contains(&b) {
                blk.stmts
                    .iter()
                    .map(|s| SsaStmt {
                        dst: SsaVar {
                            name: s.dst.clone(),
                            ver: 0,
                        },
                        rhs: match &s.rhs {
                            Rhs::Const(c) => SsaRhs::Const(*c),
                            Rhs::Var(v) => SsaRhs::Var(SsaVar {
                                name: v.clone(),
                                ver: 0,
                            }),
                            Rhs::Bin(op, l, r) => SsaRhs::Bin(
                                *op,
                                SsaVar {
                                    name: l.clone(),
                                    ver: 0,
                                },
                                SsaVar {
                                    name: r.clone(),
                                    ver: 0,
                                },
                            ),
                        },
                    })
                    .collect()
            } else {
                Vec::new()
            },
            term: match &blk.term {
                Term::Jump(t) => SsaTerm::Jump(*t),
                Term::Branch {
                    cond,
                    then_bb,
                    else_bb,
                } => SsaTerm::Branch {
                    cond: SsaVar {
                        name: cond.clone(),
                        ver: 0,
                    },
                    then_bb: *then_bb,
                    else_bb: *else_bb,
                },
                Term::Return(v) => SsaTerm::Return(SsaVar {
                    name: v.clone(),
                    ver: 0,
                }),
            },
        })
        .collect();

    // Step 3: Dominator-tree DFS variable renaming.
    // Invariant: `stacks[name].last()` always holds the reaching definition of `name`
    // at the current point in the dominator tree. Any version pushed in block `b` is
    // popped when leaving `b`'s dominator subtree so sibling branches remain isolated.
    let mut counters: BTreeMap<String, usize> = BTreeMap::new();
    let mut stacks: BTreeMap<String, Vec<usize>> = BTreeMap::new();

    #[allow(clippy::too_many_arguments)]
    fn rename_block(
        b: usize,
        cfg: &[BasicBlock],
        dom_children: &[Vec<usize>],
        ssa_blocks: &mut [SsaBlock],
        counters: &mut BTreeMap<String, usize>,
        stacks: &mut BTreeMap<String, Vec<usize>>,
    ) {
        let mut pushed = Vec::new();
        let new_version = |name: &str,
                           counters: &mut BTreeMap<String, usize>,
                           stacks: &mut BTreeMap<String, Vec<usize>>,
                           pushed: &mut Vec<String>|
         -> usize {
            let c = counters.entry(name.to_string()).or_insert(0);
            *c += 1;
            let ver = *c;
            stacks.entry(name.to_string()).or_default().push(ver);
            pushed.push(name.to_string());
            ver
        };
        let current_ver = |name: &str, stacks: &BTreeMap<String, Vec<usize>>| -> usize {
            stacks
                .get(name)
                .and_then(|s| s.last().copied())
                .unwrap_or(0)
        };

        for phi in &mut ssa_blocks[b].phis {
            phi.dst.ver = new_version(&phi.dst.name, counters, stacks, &mut pushed);
        }

        for stmt in &mut ssa_blocks[b].stmts {
            match &mut stmt.rhs {
                SsaRhs::Const(_) => {}
                SsaRhs::Var(v) => v.ver = current_ver(&v.name, stacks),
                SsaRhs::Bin(_, l, r) => {
                    l.ver = current_ver(&l.name, stacks);
                    r.ver = current_ver(&r.name, stacks);
                }
            }
            stmt.dst.ver = new_version(&stmt.dst.name, counters, stacks, &mut pushed);
        }

        match &mut ssa_blocks[b].term {
            SsaTerm::Jump(_) => {}
            SsaTerm::Branch { cond, .. } => cond.ver = current_ver(&cond.name, stacks),
            SsaTerm::Return(v) => v.ver = current_ver(&v.name, stacks),
        }

        // Fill phi incoming arguments on outgoing CFG edges using the active versions at the end of `b`.
        for succ in successors_of(&cfg[b].term) {
            for phi in &mut ssa_blocks[succ].phis {
                let ver = current_ver(&phi.dst.name, stacks);
                phi.incoming.insert(
                    b,
                    SsaVar {
                        name: phi.dst.name.clone(),
                        ver,
                    },
                );
            }
        }

        for &child in &dom_children[b] {
            rename_block(child, cfg, dom_children, ssa_blocks, counters, stacks);
        }

        for name in pushed {
            stacks.get_mut(&name).unwrap().pop();
        }
    }

    rename_block(
        0,
        cfg,
        &dom_children,
        &mut ssa_blocks,
        &mut counters,
        &mut stacks,
    );

    SsaProgram {
        blocks: ssa_blocks,
        preds,
        dom,
        idom,
        dom_frontiers,
    }
}

// Verify the two core SSA invariants:
// 1. Single definition: every SsaVar (with ver > 0) is defined at most once globally.
// 2. Strict dominance: every use in block B is dominated by its definition block; for a phi
//    incoming value from predecessor P, the "use" conceptually occurs on edge P -> B, so it must
//    be dominated by P (not B).
fn verify_ssa(ssa: &SsaProgram) -> Result<(), String> {
    let mut def_block: BTreeMap<SsaVar, usize> = BTreeMap::new();
    for (b, blk) in ssa.blocks.iter().enumerate() {
        for phi in &blk.phis {
            if def_block.insert(phi.dst.clone(), b).is_some() {
                return Err(format!("duplicate SSA definition {:?}", phi.dst));
            }
        }
        for stmt in &blk.stmts {
            if def_block.insert(stmt.dst.clone(), b).is_some() {
                return Err(format!("duplicate SSA definition {:?}", stmt.dst));
            }
        }
    }

    let check_use = |var: &SsaVar, use_bb: usize| -> Result<(), String> {
        if var.ver == 0 {
            return Ok(()); // undefined/initial parameter version 0
        }
        let Some(&d_bb) = def_block.get(var) else {
            return Err(format!("use of undefined SSA var {var:?}"));
        };
        if !ssa.dom[use_bb].contains(&d_bb) {
            return Err(format!(
                "SSA def {:?} in B{} does not dominate use in B{}",
                var, d_bb, use_bb
            ));
        }
        Ok(())
    };

    for (b, blk) in ssa.blocks.iter().enumerate() {
        if ssa.dom[b].is_empty() {
            continue;
        }
        for phi in &blk.phis {
            for (&pred, var) in &phi.incoming {
                check_use(var, pred)?;
            }
        }
        for stmt in &blk.stmts {
            match &stmt.rhs {
                SsaRhs::Const(_) => {}
                SsaRhs::Var(v) => check_use(v, b)?,
                SsaRhs::Bin(_, l, r) => {
                    check_use(l, b)?;
                    check_use(r, b)?;
                }
            }
        }
        match &blk.term {
            SsaTerm::Jump(_) => {}
            SsaTerm::Branch { cond, .. } => check_use(cond, b)?,
            SsaTerm::Return(v) => check_use(v, b)?,
        }
    }
    Ok(())
}

// Sparse-style constant propagation over SSA: because each SsaVar has a single definition,
// known constants go straight into a global map without per-block reaching-definition sets.
// A phi is constant if all its incoming versions resolve to the same constant.
fn fold_ssa_constants(ssa: &mut SsaProgram) {
    loop {
        let mut consts: BTreeMap<SsaVar, i64> = BTreeMap::new();
        for blk in &ssa.blocks {
            for stmt in &blk.stmts {
                if let SsaRhs::Const(c) = stmt.rhs {
                    consts.insert(stmt.dst.clone(), c);
                }
            }
        }
        let mut changed = false;
        for blk in &mut ssa.blocks {
            for phi in &mut blk.phis {
                let mut vals = BTreeSet::new();
                let mut all_const = !phi.incoming.is_empty();
                for v in phi.incoming.values() {
                    if let Some(&c) = consts.get(v) {
                        vals.insert(c);
                    } else {
                        all_const = false;
                        break;
                    }
                }
                if all_const && vals.len() == 1 {
                    let c = *vals.iter().next().unwrap();
                    consts.insert(phi.dst.clone(), c);
                }
            }
            for stmt in &mut blk.stmts {
                let folded = match &stmt.rhs {
                    SsaRhs::Var(v) => consts.get(v).copied(),
                    SsaRhs::Bin(op, l, r) => match (consts.get(l), consts.get(r)) {
                        (Some(&a), Some(&b)) => Some(match op {
                            BinOp::Add => a.wrapping_add(b),
                            BinOp::Sub => a.wrapping_sub(b),
                            BinOp::Lt => i64::from(a < b),
                        }),
                        _ => None,
                    },
                    SsaRhs::Const(_) => None,
                };
                if let Some(c) = folded {
                    stmt.rhs = SsaRhs::Const(c);
                    changed = true;
                }
            }
        }
        if !changed {
            break;
        }
    }
}

fn ssa_lowered_name(v: &SsaVar) -> String {
    if v.ver == 0 {
        v.name.clone()
    } else {
        format!("{}_{}", v.name, v.ver)
    }
}

// Lower SSA back into an executable non-SSA CFG by replacing phi nodes with edge copies.
// Two subtle cases are handled:
// 1. Parallel-copy swap problem: all phis at a block entry read their inputs simultaneously, so
//    copies first stage values into `__phi_tmp_*` before writing destinations.
// 2. Critical edges: if predecessor `pred` ends in a Branch (2 successors), appending copies to
//    `pred` would run on the wrong branch too, so a trampoline block is spliced onto the edge.
fn de_ssa(ssa: &SsaProgram) -> Vec<BasicBlock> {
    let mut blocks: Vec<BasicBlock> = ssa
        .blocks
        .iter()
        .map(|blk| BasicBlock {
            stmts: blk
                .stmts
                .iter()
                .map(|s| Stmt {
                    dst: ssa_lowered_name(&s.dst),
                    rhs: match &s.rhs {
                        SsaRhs::Const(c) => Rhs::Const(*c),
                        SsaRhs::Var(v) => Rhs::Var(ssa_lowered_name(v)),
                        SsaRhs::Bin(op, l, r) => {
                            Rhs::Bin(*op, ssa_lowered_name(l), ssa_lowered_name(r))
                        }
                    },
                })
                .collect(),
            term: match &blk.term {
                SsaTerm::Jump(t) => Term::Jump(*t),
                SsaTerm::Branch {
                    cond,
                    then_bb,
                    else_bb,
                } => Term::Branch {
                    cond: ssa_lowered_name(cond),
                    then_bb: *then_bb,
                    else_bb: *else_bb,
                },
                SsaTerm::Return(v) => Term::Return(ssa_lowered_name(v)),
            },
        })
        .collect();

    // Lower phi incoming copies along edges (splitting critical edges when needed).
    for (succ_idx, blk) in ssa.blocks.iter().enumerate() {
        if blk.phis.is_empty() {
            continue;
        }
        let mut by_pred: BTreeMap<usize, Vec<(String, String)>> = BTreeMap::new();
        for phi in &blk.phis {
            let dst = ssa_lowered_name(&phi.dst);
            for (&pred, src) in &phi.incoming {
                let src_name = ssa_lowered_name(src);
                by_pred
                    .entry(pred)
                    .or_default()
                    .push((dst.clone(), src_name));
            }
        }
        for (pred, copies) in by_pred {
            // Use temporary names so simultaneous phi copies do not overwrite each other.
            let mut copy_stmts = Vec::new();
            for (idx, (_, src)) in copies.iter().enumerate() {
                copy_stmts.push(Stmt {
                    dst: format!("__phi_tmp_{succ_idx}_{idx}"),
                    rhs: Rhs::Var(src.clone()),
                });
            }
            for (idx, (dst, _)) in copies.iter().enumerate() {
                copy_stmts.push(Stmt {
                    dst: dst.clone(),
                    rhs: Rhs::Var(format!("__phi_tmp_{succ_idx}_{idx}")),
                });
            }
            let pred_succs = successors_of(&blocks[pred].term);
            if pred_succs.len() == 1 {
                blocks[pred].stmts.extend(copy_stmts);
            } else {
                // Split critical edge pred -> succ_idx with a trampoline block.
                let tramp = blocks.len();
                blocks.push(BasicBlock {
                    stmts: copy_stmts,
                    term: Term::Jump(succ_idx),
                });
                if let Term::Branch {
                    then_bb, else_bb, ..
                } = &mut blocks[pred].term
                {
                    if *then_bb == succ_idx {
                        *then_bb = tramp;
                    }
                    if *else_bb == succ_idx {
                        *else_bb = tramp;
                    }
                }
            }
        }
    }
    blocks
}

fn eval_cfg(blocks: &[BasicBlock], initial: &[(&str, i64)]) -> i64 {
    let mut env: BTreeMap<String, i64> = initial.iter().map(|&(k, v)| (k.to_string(), v)).collect();
    let mut bb = 0usize;
    let mut steps = 0usize;
    loop {
        assert!(steps < 10_000, "infinite loop in interpreter");
        steps += 1;
        for stmt in &blocks[bb].stmts {
            let val = match &stmt.rhs {
                Rhs::Const(c) => *c,
                Rhs::Var(v) => *env.get(v).unwrap_or(&0),
                Rhs::Bin(op, l, r) => {
                    let lv = *env.get(l).unwrap_or(&0);
                    let rv = *env.get(r).unwrap_or(&0);
                    match op {
                        BinOp::Add => lv.wrapping_add(rv),
                        BinOp::Sub => lv.wrapping_sub(rv),
                        BinOp::Lt => i64::from(lv < rv),
                    }
                }
            };
            env.insert(stmt.dst.clone(), val);
        }
        match &blocks[bb].term {
            Term::Jump(t) => bb = *t,
            Term::Branch {
                cond,
                then_bb,
                else_bb,
            } => {
                let c = *env.get(cond).unwrap_or(&0);
                bb = if c != 0 { *then_bb } else { *else_bb };
            }
            Term::Return(v) => return *env.get(v).unwrap_or(&0),
        }
    }
}

fn eval_ssa(ssa: &SsaProgram, initial: &[(&str, i64)]) -> i64 {
    let mut env: BTreeMap<SsaVar, i64> = initial
        .iter()
        .map(|&(k, v)| {
            (
                SsaVar {
                    name: k.to_string(),
                    ver: 0,
                },
                v,
            )
        })
        .collect();
    let mut bb = 0usize;
    let mut prev_bb = None;
    let mut steps = 0usize;
    loop {
        assert!(steps < 10_000, "infinite loop in SSA interpreter");
        steps += 1;
        if let Some(p) = prev_bb {
            let mut phi_updates = Vec::new();
            for phi in &ssa.blocks[bb].phis {
                let src = &phi.incoming[&p];
                phi_updates.push((phi.dst.clone(), *env.get(src).unwrap_or(&0)));
            }
            for (dst, val) in phi_updates {
                env.insert(dst, val);
            }
        }
        for stmt in &ssa.blocks[bb].stmts {
            let val = match &stmt.rhs {
                SsaRhs::Const(c) => *c,
                SsaRhs::Var(v) => *env.get(v).unwrap_or(&0),
                SsaRhs::Bin(op, l, r) => {
                    let lv = *env.get(l).unwrap_or(&0);
                    let rv = *env.get(r).unwrap_or(&0);
                    match op {
                        BinOp::Add => lv.wrapping_add(rv),
                        BinOp::Sub => lv.wrapping_sub(rv),
                        BinOp::Lt => i64::from(lv < rv),
                    }
                }
            };
            env.insert(stmt.dst.clone(), val);
        }
        prev_bb = Some(bb);
        match &ssa.blocks[bb].term {
            SsaTerm::Jump(t) => bb = *t,
            SsaTerm::Branch {
                cond,
                then_bb,
                else_bb,
            } => {
                let c = *env.get(cond).unwrap_or(&0);
                bb = if c != 0 { *then_bb } else { *else_bb };
            }
            SsaTerm::Return(v) => return *env.get(v).unwrap_or(&0),
        }
    }
}

// Loop with preheader (B0), header (B1), body/latch (B2), exit (B3).
fn loop_sample() -> Vec<BasicBlock> {
    vec![
        BasicBlock {
            // B0: i = 0; s = 0; jump B1
            stmts: vec![
                Stmt {
                    dst: "i".into(),
                    rhs: Rhs::Const(0),
                },
                Stmt {
                    dst: "s".into(),
                    rhs: Rhs::Const(0),
                },
            ],
            term: Term::Jump(1),
        },
        BasicBlock {
            // B1: cond = i < n; if cond goto B2 else B3
            stmts: vec![Stmt {
                dst: "cond".into(),
                rhs: Rhs::Bin(BinOp::Lt, "i".into(), "n".into()),
            }],
            term: Term::Branch {
                cond: "cond".into(),
                then_bb: 2,
                else_bb: 3,
            },
        },
        BasicBlock {
            // B2: s = s + i; i = i + 1; jump B1
            stmts: vec![
                Stmt {
                    dst: "s".into(),
                    rhs: Rhs::Bin(BinOp::Add, "s".into(), "i".into()),
                },
                Stmt {
                    dst: "one".into(),
                    rhs: Rhs::Const(1),
                },
                Stmt {
                    dst: "i".into(),
                    rhs: Rhs::Bin(BinOp::Add, "i".into(), "one".into()),
                },
            ],
            term: Term::Jump(1),
        },
        BasicBlock {
            // B3: return s
            stmts: vec![],
            term: Term::Return("s".into()),
        },
    ]
}

fn main() {
    let cfg = loop_sample();
    let mut ssa = to_ssa(&cfg);
    verify_ssa(&ssa).unwrap();
    println!("Dominance frontiers: {:?}", ssa.dom_frontiers);
    println!("Immediate dominators: {:?}", ssa.idom);
    println!("Predecessors: {:?}", ssa.preds);
    for (b, blk) in ssa.blocks.iter().enumerate() {
        println!("B{b}:");
        for phi in &blk.phis {
            println!("  phi {:?} <- {:?}", phi.dst, phi.incoming);
        }
        for stmt in &blk.stmts {
            println!("  {:?} = {:?}", stmt.dst, stmt.rhs);
        }
        println!(
            "  term {:?} (succs {:?})",
            blk.term,
            ssa_successors_of(&blk.term)
        );
    }
    fold_ssa_constants(&mut ssa);
    let lowered = de_ssa(&ssa);
    println!(
        "Sum 0..5: original={}, ssa={}, de-ssa={}",
        eval_cfg(&cfg, &[("n", 5)]),
        eval_ssa(&ssa, &[("n", 5)]),
        eval_cfg(&lowered, &[("n", 5)]),
    );

    let sub_cfg = vec![BasicBlock {
        stmts: vec![
            Stmt {
                dst: "a".into(),
                rhs: Rhs::Const(10),
            },
            Stmt {
                dst: "b".into(),
                rhs: Rhs::Const(3),
            },
            Stmt {
                dst: "d".into(),
                rhs: Rhs::Bin(BinOp::Sub, "a".into(), "b".into()),
            },
        ],
        term: Term::Return("d".into()),
    }];
    let mut sub_ssa = to_ssa(&sub_cfg);
    fold_ssa_constants(&mut sub_ssa);
    println!("Folded 10 - 3 => {}", eval_ssa(&sub_ssa, &[]));
}

#[test]
fn diamond_places_phi_only_at_join_and_preserves_semantics() {
    let diamond = vec![
        BasicBlock {
            stmts: vec![Stmt {
                dst: "x".into(),
                rhs: Rhs::Const(10),
            }],
            term: Term::Branch {
                cond: "flag".into(),
                then_bb: 1,
                else_bb: 2,
            },
        },
        BasicBlock {
            stmts: vec![Stmt {
                dst: "x".into(),
                rhs: Rhs::Const(20),
            }],
            term: Term::Jump(3),
        },
        BasicBlock {
            stmts: vec![Stmt {
                dst: "x".into(),
                rhs: Rhs::Const(20),
            }],
            term: Term::Jump(3),
        },
        BasicBlock {
            stmts: vec![Stmt {
                dst: "y".into(),
                rhs: Rhs::Bin(BinOp::Sub, "x".into(), "z".into()),
            }],
            term: Term::Return("y".into()),
        },
        // Unreachable block assigning dead variable.
        BasicBlock {
            stmts: vec![Stmt {
                dst: "dead".into(),
                rhs: Rhs::Const(99),
            }],
            term: Term::Jump(3),
        },
    ];
    let mut ssa = to_ssa(&diamond);
    verify_ssa(&ssa).unwrap();
    assert!(ssa.blocks[0].phis.is_empty());
    assert!(ssa.blocks[1].phis.is_empty());
    assert!(ssa.blocks[2].phis.is_empty());
    assert_eq!(ssa.blocks[3].phis.len(), 1);
    assert_eq!(ssa.blocks[3].phis[0].dst.name, "x");
    assert!(ssa.blocks[4].stmts.is_empty());

    fold_ssa_constants(&mut ssa);
    let lowered = de_ssa(&ssa);
    for flag in [0, 1] {
        let orig = eval_cfg(&diamond, &[("flag", flag), ("z", 3)]);
        let ssa_v = eval_ssa(&ssa, &[("flag", flag), ("z", 3)]);
        let low_v = eval_cfg(&lowered, &[("flag", flag), ("z", 3)]);
        assert_eq!(orig, 17);
        assert_eq!(ssa_v, orig);
        assert_eq!(low_v, orig);
    }
}

#[test]
fn loop_places_header_phi_with_distinct_preheader_and_latch_versions() {
    let cfg = loop_sample();
    let mut ssa = to_ssa(&cfg);
    verify_ssa(&ssa).unwrap();
    assert_eq!(ssa.blocks[1].phis.len(), 2); // i and s
    for phi in &ssa.blocks[1].phis {
        assert_ne!(phi.incoming[&0], phi.incoming[&2]);
    }
    fold_ssa_constants(&mut ssa);
    let lowered = de_ssa(&ssa);
    for n in [0, 1, 5, 10] {
        let expected = (0..n).sum::<i64>();
        assert_eq!(eval_cfg(&cfg, &[("n", n)]), expected);
        assert_eq!(eval_ssa(&ssa, &[("n", n)]), expected);
        assert_eq!(eval_cfg(&lowered, &[("n", n)]), expected);
    }
}
