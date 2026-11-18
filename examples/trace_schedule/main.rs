//! Chapter 10 (§10.4): Global Trace Scheduling & Speculative Code Motion.
//!
//! Demonstrates profile-guided hot-trace selection, backward liveness for
//! off-trace speculation safety, control-dependence barriers for trapping/side-
//! effect instructions (`DivChecked`, `Store`), global list scheduling across
//! branch splits and joins, compensation code insertion on off-trace edges,
//! and multi-path CFG simulation proving semantic equivalence across both
//! hot and cold paths.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

// ---------------------------------------------------------------------------
// 1. CFG Intermediate Representation & Speculative Safety Classification
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq)]
enum Op {
    Const {
        dst: String,
        val: i64,
    },
    LoadSafe {
        dst: String,
        slot: usize,
    },
    AddImm {
        dst: String,
        src: String,
        imm: i64,
    },
    MulImm {
        dst: String,
        src: String,
        imm: i64,
    },
    Add {
        dst: String,
        lhs: String,
        rhs: String,
    },
    DivChecked {
        dst: String,
        num: i64,
        den: String,
    }, // may trap if den == 0!
    Store {
        slot: usize,
        src: String,
    }, // visible memory side effect!
}

impl Op {
    fn dst(&self) -> Option<&str> {
        match self {
            Op::Const { dst, .. }
            | Op::LoadSafe { dst, .. }
            | Op::AddImm { dst, .. }
            | Op::MulImm { dst, .. }
            | Op::Add { dst, .. }
            | Op::DivChecked { dst, .. } => Some(dst),
            Op::Store { .. } => None,
        }
    }

    fn uses(&self) -> Vec<&str> {
        match self {
            Op::Const { .. } | Op::LoadSafe { .. } => Vec::new(),
            Op::AddImm { src, .. } | Op::MulImm { src, .. } => vec![src],
            Op::Add { lhs, rhs, .. } => vec![lhs, rhs],
            Op::DivChecked { den, .. } => vec![den],
            Op::Store { src, .. } => vec![src],
        }
    }

    /// §10.4 safety: only pure, non-trapping operations may execute speculatively
    /// above an earlier controlling branch.
    fn is_speculative_safe(&self) -> bool {
        !matches!(self, Op::DivChecked { .. } | Op::Store { .. })
    }

    fn writes_mem(&self) -> bool {
        matches!(self, Op::Store { .. })
    }

    fn reads_mem(&self) -> bool {
        matches!(self, Op::LoadSafe { .. })
    }
}

impl fmt::Display for Op {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Op::Const { dst, val } => write!(f, "{dst} = {val}"),
            Op::LoadSafe { dst, slot } => write!(f, "{dst} = load_safe(mem[{slot}])"),
            Op::AddImm { dst, src, imm } => write!(f, "{dst} = {src} + {imm}"),
            Op::MulImm { dst, src, imm } => write!(f, "{dst} = {src} * {imm}"),
            Op::Add { dst, lhs, rhs } => write!(f, "{dst} = {lhs} + {rhs}"),
            Op::DivChecked { dst, num, den } => write!(f, "{dst} = {num} / {den} [trap]"),
            Op::Store { slot, src } => write!(f, "mem[{slot}] = {src} [effect]"),
        }
    }
}

#[derive(Clone, Debug)]
struct Inst {
    op: Op,
    latency: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Term {
    BrZero {
        cond: String,
        if_zero: usize,
        if_nonzero: usize,
    },
    Jump(usize),
    Ret(String),
}

impl fmt::Display for Term {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Term::BrZero {
                cond,
                if_zero,
                if_nonzero,
            } => write!(f, "br_zero {cond} ? B{if_zero} : B{if_nonzero}"),
            Term::Jump(t) => write!(f, "jump B{t}"),
            Term::Ret(r) => write!(f, "ret {r}"),
        }
    }
}

#[derive(Clone, Debug)]
struct Block {
    freq: usize,
    insts: Vec<Inst>,
    term: Term,
}

impl Block {
    fn succs(&self) -> Vec<usize> {
        match &self.term {
            Term::BrZero {
                if_zero,
                if_nonzero,
                ..
            } => vec![*if_zero, *if_nonzero],
            Term::Jump(t) => vec![*t],
            Term::Ret(_) => Vec::new(),
        }
    }
}

#[derive(Clone, Debug)]
struct CfgProgram {
    blocks: BTreeMap<usize, Block>,
    entry: usize,
}

// ---------------------------------------------------------------------------
// 2. Profile-Guided Trace Selection & Backward Liveness Analysis
// ---------------------------------------------------------------------------

/// Selects the highest-frequency acyclic path starting from `entry` (Fisher's
/// mutual most-likely trace picking across an acyclic/loop-body region).
fn pick_hot_trace(cfg: &CfgProgram) -> Vec<usize> {
    let mut trace = vec![cfg.entry];
    let mut seen = BTreeSet::from([cfg.entry]);
    let mut cur = cfg.entry;

    while let Some(next) = cfg.blocks[&cur]
        .succs()
        .into_iter()
        .filter(|s| !seen.contains(s))
        .max_by_key(|s| (cfg.blocks[s].freq, usize::MAX - *s))
    {
        trace.push(next);
        seen.insert(next);
        cur = next;
    }
    trace
}

#[derive(Debug)]
struct Liveness {
    live_in: BTreeMap<usize, BTreeSet<String>>,
    live_out: BTreeMap<usize, BTreeSet<String>>,
}

/// Computes exact backward liveness across the CFG so the trace scheduler knows
/// whether speculative writes would clobber live values on off-trace paths.
fn compute_liveness(cfg: &CfgProgram) -> Liveness {
    let mut live_in: BTreeMap<usize, BTreeSet<String>> =
        cfg.blocks.keys().map(|&id| (id, BTreeSet::new())).collect();
    let mut live_out: BTreeMap<usize, BTreeSet<String>> =
        cfg.blocks.keys().map(|&id| (id, BTreeSet::new())).collect();

    loop {
        let mut changed = false;
        for (&id, blk) in cfg.blocks.iter().rev() {
            let mut out = BTreeSet::new();
            for s in blk.succs() {
                out.extend(live_in[&s].iter().cloned());
            }
            let mut cur_in = out.clone();
            match &blk.term {
                Term::BrZero { cond, .. } | Term::Ret(cond) => {
                    cur_in.insert(cond.clone());
                }
                Term::Jump(_) => {}
            }
            for inst in blk.insts.iter().rev() {
                if let Some(d) = inst.op.dst() {
                    cur_in.remove(d);
                }
                for u in inst.op.uses() {
                    cur_in.insert(u.to_string());
                }
            }
            if out != live_out[&id] || cur_in != live_in[&id] {
                live_out.insert(id, out);
                live_in.insert(id, cur_in);
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    Liveness { live_in, live_out }
}

// ---------------------------------------------------------------------------
// 3. Basic-Block-Only Baseline vs. Global Trace Scheduling + Compensation
// ---------------------------------------------------------------------------

fn list_schedule_single_block(insts: &[Inst], term_extra_cycle: usize, width: usize) -> usize {
    assert!(width > 0);
    let n = insts.len();
    // Build intra-block data + memory dependencies.
    let mut deps: Vec<Vec<(usize, usize)>> = vec![Vec::new(); n];
    let mut last_def: BTreeMap<String, usize> = BTreeMap::new();
    let mut last_reads: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    let mut last_store: Option<usize> = None;
    let mut loads_since_store: Vec<usize> = Vec::new();

    for (i, inst) in insts.iter().enumerate() {
        for u in inst.op.uses() {
            if let Some(&def_i) = last_def.get(u) {
                deps[i].push((def_i, insts[def_i].latency));
            }
            last_reads.entry(u.to_string()).or_default().push(i);
        }
        if inst.op.reads_mem() {
            if let Some(st) = last_store {
                deps[i].push((st, insts[st].latency));
            }
            loads_since_store.push(i);
        }
        if inst.op.writes_mem() {
            if let Some(st) = last_store {
                deps[i].push((st, 1));
            }
            for &ld in &loads_since_store {
                deps[i].push((ld, 1));
            }
            loads_since_store.clear();
            last_store = Some(i);
        }
        if let Some(d) = inst.op.dst() {
            if let Some(&prev_def) = last_def.get(d) {
                deps[i].push((prev_def, 1));
            }
            if let Some(reads) = last_reads.get(d) {
                for &r in reads {
                    if r != i {
                        deps[i].push((r, 0));
                    }
                }
            }
            last_def.insert(d.to_string(), i);
        }
    }

    let mut issued: Vec<Option<usize>> = vec![None; n];
    let mut remaining = n;
    let mut cycle = 0usize;
    let mut finish = 0usize;

    while remaining > 0 {
        let mut slots = width;
        let mut next_ready: Option<usize> = None;
        for i in 0..n {
            if issued[i].is_some() {
                continue;
            }
            let mut ready_at = 0usize;
            let mut blocked = false;
            for &(pred, lat) in &deps[i] {
                if let Some(ic) = issued[pred] {
                    ready_at = ready_at.max(ic + lat);
                } else {
                    blocked = true;
                    break;
                }
            }
            if blocked {
                continue;
            }
            if ready_at <= cycle && slots > 0 {
                issued[i] = Some(cycle);
                finish = finish.max(cycle + insts[i].latency);
                slots -= 1;
                remaining -= 1;
            } else if ready_at > cycle {
                next_ready = Some(next_ready.map_or(ready_at, |nr| nr.min(ready_at)));
            }
        }
        if remaining > 0 {
            if slots == width {
                cycle = next_ready.expect("acyclic block");
            } else {
                cycle += 1;
            }
        }
    }
    finish + term_extra_cycle
}

fn basic_block_trace_cycles(cfg: &CfgProgram, trace: &[usize], width: usize) -> usize {
    trace
        .iter()
        .map(|&id| {
            let blk = &cfg.blocks[&id];
            let term_cyc = usize::from(matches!(blk.term, Term::BrZero { .. }));
            list_schedule_single_block(&blk.insts, term_cyc, width)
        })
        .sum()
}

#[derive(Clone, Debug)]
enum TraceItemKind {
    Op(Inst),
    BranchSplit {
        cond: String,
        off_target: usize,
        on_target: usize,
    },
}

#[derive(Clone, Debug)]
struct TraceNode {
    orig_block: usize,
    orig_trace_pos: usize,
    kind: TraceItemKind,
    latency: usize,
}

#[derive(Clone, Debug)]
struct ScheduledTraceEntry {
    orig_block: usize,
    cycle: usize,
    desc: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum MotionKind {
    SpeculativeHoistAcrossSplit {
        from_block: usize,
        split_block: usize,
    },
    HoistAcrossJoinWithComp {
        from_block: usize,
        join_block: usize,
        comp_block: usize,
    },
    DownwardAcrossSplitWithComp {
        from_block: usize,
        split_block: usize,
        comp_block: usize,
    },
}

#[derive(Debug)]
struct TraceScheduleResult {
    trace: Vec<usize>,
    bb_cycles: usize,
    trace_cycles: usize,
    schedule: Vec<ScheduledTraceEntry>,
    motions: Vec<(String, MotionKind)>,
    transformed_cfg: CfgProgram,
}

/// Performs §10.4 global trace list scheduling across `[B0, B1, ..., Bk]`:
/// 1. Builds the combined dependence DAG (RAW, WAW, WAR, memory, control-safety,
///    and off-trace `live_in` clobber prevention).
/// 2. List-schedules all trace nodes (including `BranchSplit` nodes) globally.
/// 3. Partitions scheduled operations back into trace blocks and inserts
///    compensation copies on off-trace edges whenever an instruction moves
///    upward past a join or downward past a branch split.
fn trace_schedule_cfg(cfg: &CfgProgram, width: usize) -> Result<TraceScheduleResult, String> {
    assert!(width > 0);
    let trace = pick_hot_trace(cfg);
    let trace_pos: BTreeMap<usize, usize> =
        trace.iter().enumerate().map(|(idx, &b)| (b, idx)).collect();
    let liveness = compute_liveness(cfg);
    let bb_cycles = basic_block_trace_cycles(cfg, &trace, width);

    // Precompute incoming predecessors for every block in the original CFG.
    let mut preds: BTreeMap<usize, Vec<usize>> =
        cfg.blocks.keys().map(|&id| (id, Vec::new())).collect();
    for (&b_id, blk) in &cfg.blocks {
        for s in blk.succs() {
            preds.get_mut(&s).unwrap().push(b_id);
        }
    }

    // Flatten instructions + branch splits along the trace into trace DAG nodes.
    let mut nodes: Vec<TraceNode> = Vec::new();
    let mut block_join_pred_ops: BTreeMap<usize, Vec<usize>> = BTreeMap::new();

    for (pos, &b_id) in trace.iter().enumerate() {
        let blk = &cfg.blocks[&b_id];
        for inst in &blk.insts {
            nodes.push(TraceNode {
                orig_block: b_id,
                orig_trace_pos: pos,
                kind: TraceItemKind::Op(inst.clone()),
                latency: inst.latency,
            });
        }
        if pos + 1 < trace.len() {
            if let Term::BrZero {
                cond,
                if_zero,
                if_nonzero,
            } = &blk.term
            {
                let next_on_trace = trace[pos + 1];
                let off_target = if *if_zero == next_on_trace {
                    *if_nonzero
                } else {
                    *if_zero
                };
                nodes.push(TraceNode {
                    orig_block: b_id,
                    orig_trace_pos: pos,
                    kind: TraceItemKind::BranchSplit {
                        cond: cond.clone(),
                        off_target,
                        on_target: next_on_trace,
                    },
                    latency: 1,
                });
            }
        }
        // Track joins entering this trace block from off-trace predecessors.
        let off_preds: Vec<usize> = preds[&b_id]
            .iter()
            .copied()
            .filter(|p| pos == 0 || *p != trace[pos - 1])
            .collect();
        if !off_preds.is_empty() {
            block_join_pred_ops.insert(b_id, off_preds);
        }
    }

    let n = nodes.len();
    let mut deps: Vec<Vec<(usize, usize)>> = vec![Vec::new(); n];
    let mut last_def: BTreeMap<String, usize> = BTreeMap::new();
    let mut last_reads: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    let mut last_store: Option<usize> = None;
    let mut loads_since_store: Vec<usize> = Vec::new();
    let mut branch_nodes: Vec<(usize, usize)> = Vec::new(); // (node_id, off_target)

    for i in 0..n {
        match &nodes[i].kind {
            TraceItemKind::Op(inst) => {
                for u in inst.op.uses() {
                    if let Some(&d) = last_def.get(u) {
                        deps[i].push((d, nodes[d].latency));
                    }
                    last_reads.entry(u.to_string()).or_default().push(i);
                }
                if inst.op.reads_mem() {
                    if let Some(st) = last_store {
                        deps[i].push((st, nodes[st].latency));
                    }
                    loads_since_store.push(i);
                }
                if inst.op.writes_mem() {
                    if let Some(st) = last_store {
                        deps[i].push((st, 1));
                    }
                    for &ld in &loads_since_store {
                        deps[i].push((ld, 1));
                    }
                    loads_since_store.clear();
                    last_store = Some(i);
                }

                // §10.4 Speculative safety barriers across earlier BranchSplit nodes:
                for &(br_id, off_target) in &branch_nodes {
                    if !inst.op.is_speculative_safe() {
                        // Traps (DivChecked) and side effects (Store) MUST wait for branch!
                        deps[i].push((br_id, nodes[br_id].latency));
                    } else if let Some(d) = inst.op.dst() {
                        // Do not speculatively clobber a register live on the cold branch!
                        if liveness.live_in[&off_target].contains(d) {
                            deps[i].push((br_id, nodes[br_id].latency));
                        }
                    }
                }

                if let Some(d) = inst.op.dst() {
                    if let Some(&prev_def) = last_def.get(d) {
                        deps[i].push((prev_def, 1));
                    }
                    if let Some(reads) = last_reads.get(d) {
                        for &r in reads {
                            if r != i {
                                deps[i].push((r, 0));
                            }
                        }
                    }
                    last_def.insert(d.to_string(), i);
                }
            }
            TraceItemKind::BranchSplit {
                cond, off_target, ..
            } => {
                // Branch depends on its condition plus every register live into off_target.
                if let Some(&def_cond) = last_def.get(cond) {
                    deps[i].push((def_cond, nodes[def_cond].latency));
                }
                last_reads.entry(cond.clone()).or_default().push(i);
                for live_reg in &liveness.live_in[off_target] {
                    if let Some(&d) = last_def.get(live_reg) {
                        deps[i].push((d, nodes[d].latency));
                    }
                    last_reads.entry(live_reg.clone()).or_default().push(i);
                }
                // Stores prior to the branch must complete before splitting.
                if let Some(st) = last_store {
                    deps[i].push((st, nodes[st].latency));
                }
                branch_nodes.push((i, *off_target));
            }
        }
    }

    // Global list scheduling across the trace DAG.
    let mut issued: Vec<Option<usize>> = vec![None; n];
    let mut order: Vec<usize> = Vec::with_capacity(n);
    let mut remaining = n;
    let mut cycle = 0usize;
    let mut total_finish = 0usize;

    while remaining > 0 {
        let mut slots = width;
        let mut next_ready: Option<usize> = None;
        for i in 0..n {
            if issued[i].is_some() {
                continue;
            }
            let mut ready_at = 0usize;
            let mut blocked = false;
            for &(pred, lat) in &deps[i] {
                if let Some(ic) = issued[pred] {
                    ready_at = ready_at.max(ic + lat);
                } else {
                    blocked = true;
                    break;
                }
            }
            if blocked {
                continue;
            }
            if ready_at <= cycle && slots > 0 {
                issued[i] = Some(cycle);
                order.push(i);
                total_finish = total_finish.max(cycle + nodes[i].latency);
                slots -= 1;
                remaining -= 1;
            } else if ready_at > cycle {
                next_ready = Some(next_ready.map_or(ready_at, |nr| nr.min(ready_at)));
            }
        }
        if remaining > 0 {
            if slots == width {
                cycle = next_ready.ok_or_else(|| "cycle detected in trace DAG".to_string())?;
            } else {
                cycle += 1;
            }
        }
    }

    // Map scheduled order back to trace blocks & compute compensation code.
    let mut new_blocks = cfg.blocks.clone();
    for &b_id in &trace {
        new_blocks.get_mut(&b_id).unwrap().insts.clear();
    }

    // Determine the scheduled trace segment (0..=trace.len()-1) each node lands in.
    let mut current_seg = 0usize;
    let mut branch_sched_seg: BTreeMap<usize, usize> = BTreeMap::new();
    let mut node_sched_seg: Vec<usize> = vec![0; n];
    let mut schedule_log: Vec<ScheduledTraceEntry> = Vec::with_capacity(n);

    for &nid in &order {
        let cyc = issued[nid].unwrap();
        let node = &nodes[nid];
        match &node.kind {
            TraceItemKind::Op(inst) => {
                // If an op originally in block >= a join block is placed at `current_seg`,
                // check if we crossed earlier into a pre-join segment.
                node_sched_seg[nid] = current_seg;
                let dest_block = trace[current_seg];
                new_blocks
                    .get_mut(&dest_block)
                    .unwrap()
                    .insts
                    .push(inst.clone());
                schedule_log.push(ScheduledTraceEntry {
                    orig_block: node.orig_block,
                    cycle: cyc,
                    desc: inst.op.to_string(),
                });
            }
            TraceItemKind::BranchSplit {
                cond,
                off_target,
                on_target,
            } => {
                branch_sched_seg.insert(node.orig_block, current_seg);
                node_sched_seg[nid] = current_seg;
                schedule_log.push(ScheduledTraceEntry {
                    orig_block: node.orig_block,
                    cycle: cyc,
                    desc: format!("br_zero {cond} ? B{off_target} : B{on_target}"),
                });
                current_seg += 1;
            }
        }
    }

    // Record code motions and insert §10.4 compensation instructions!
    let mut motions: Vec<(String, MotionKind)> = Vec::new();
    for nid in &order {
        let node = &nodes[*nid];
        let TraceItemKind::Op(inst) = &node.kind else {
            continue;
        };
        let orig_seg = node.orig_trace_pos;
        let sched_seg = node_sched_seg[*nid];

        if sched_seg < orig_seg {
            // Upward motion across every intermediate block boundary in `(sched_seg ..= orig_seg)`
            for pos in sched_seg..orig_seg {
                let boundary_block = trace[pos];
                let next_block = trace[pos + 1];
                // 1) Crossed upward over a branch split at `boundary_block`
                if let Term::BrZero { .. } = &cfg.blocks[&boundary_block].term {
                    motions.push((
                        inst.op.to_string(),
                        MotionKind::SpeculativeHoistAcrossSplit {
                            from_block: node.orig_block,
                            split_block: boundary_block,
                        },
                    ));
                }
                // 2) Crossed upward over an off-trace join entering `next_block`
                if let Some(off_preds) = block_join_pred_ops.get(&next_block) {
                    for &comp_blk in off_preds {
                        if !trace_pos.contains_key(&comp_blk) {
                            new_blocks
                                .get_mut(&comp_blk)
                                .unwrap()
                                .insts
                                .push(inst.clone());
                            motions.push((
                                inst.op.to_string(),
                                MotionKind::HoistAcrossJoinWithComp {
                                    from_block: node.orig_block,
                                    join_block: next_block,
                                    comp_block: comp_blk,
                                },
                            ));
                        }
                    }
                }
            }
        } else if sched_seg > orig_seg {
            // Downward motion across branch splits `(orig_seg .. sched_seg)`
            for pos in orig_seg..sched_seg {
                let split_blk = trace[pos];
                if let Term::BrZero {
                    if_zero,
                    if_nonzero,
                    ..
                } = &cfg.blocks[&split_blk].term
                {
                    let off_target = if *if_zero == trace[pos + 1] {
                        *if_nonzero
                    } else {
                        *if_zero
                    };
                    let target_blk = new_blocks.get_mut(&off_target).unwrap();
                    target_blk.insts.insert(0, inst.clone());
                    motions.push((
                        inst.op.to_string(),
                        MotionKind::DownwardAcrossSplitWithComp {
                            from_block: node.orig_block,
                            split_block: split_blk,
                            comp_block: off_target,
                        },
                    ));
                }
            }
        }
    }

    // Re-stitch sequential Jump terminators for blocks merged/split along trace.
    // Original `B0` keeps its `BrZero(cond, B2, B1)`, `B1` jumps to `B3`, etc.
    // Note that if a trace contained non-split Jump(B3) between B1 and B3, `current_seg`
    // didn't split across B1->B3, which merges B3 straight onto the end of B1!
    for idx in 0..trace.len() {
        let b_id = trace[idx];
        let is_split = branch_sched_seg.contains_key(&b_id);
        if !is_split && idx + 1 < trace.len() {
            // Merged into the following trace block: copy its terminator onward!
            let next_id = trace[idx + 1];
            let next_term = cfg.blocks[&next_id].term.clone();
            new_blocks.get_mut(&b_id).unwrap().term = next_term;
        }
    }

    Ok(TraceScheduleResult {
        trace,
        bb_cycles,
        trace_cycles: total_finish,
        schedule: schedule_log,
        motions,
        transformed_cfg: CfgProgram {
            blocks: new_blocks,
            entry: cfg.entry,
        },
    })
}

// ---------------------------------------------------------------------------
// 4. Multi-Path CFG VM Simulator (Proves Hot & Cold Path Equivalence)
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq)]
struct SimState {
    ret: i64,
    regs: BTreeMap<String, i64>,
    mem: Vec<i64>,
    visited_blocks: Vec<usize>,
}

fn simulate_cfg(
    cfg: &CfgProgram,
    init_regs: &[(&str, i64)],
    init_mem: &[i64],
) -> Result<SimState, String> {
    let mut regs: BTreeMap<String, i64> = init_regs
        .iter()
        .map(|(k, v)| ((*k).to_string(), *v))
        .collect();
    let mut mem = init_mem.to_vec();
    let mut pc_block = cfg.entry;
    let mut visited = Vec::new();

    for _step in 0..256 {
        visited.push(pc_block);
        let blk = &cfg.blocks[&pc_block];
        for inst in &blk.insts {
            match &inst.op {
                Op::Const { dst, val } => {
                    regs.insert(dst.clone(), *val);
                }
                Op::LoadSafe { dst, slot } => {
                    let v = *mem
                        .get(*slot)
                        .ok_or_else(|| format!("out-of-bounds slot {slot}"))?;
                    regs.insert(dst.clone(), v);
                }
                Op::AddImm { dst, src, imm } => {
                    let v = *regs
                        .get(src)
                        .ok_or_else(|| format!("undefined register '{src}'"))?;
                    regs.insert(dst.clone(), v + imm);
                }
                Op::MulImm { dst, src, imm } => {
                    let v = *regs
                        .get(src)
                        .ok_or_else(|| format!("undefined register '{src}'"))?;
                    regs.insert(dst.clone(), v * imm);
                }
                Op::Add { dst, lhs, rhs } => {
                    let l = *regs
                        .get(lhs)
                        .ok_or_else(|| format!("undefined register '{lhs}'"))?;
                    let r = *regs
                        .get(rhs)
                        .ok_or_else(|| format!("undefined register '{rhs}'"))?;
                    regs.insert(dst.clone(), l + r);
                }
                Op::DivChecked { dst, num, den } => {
                    let d = *regs
                        .get(den)
                        .ok_or_else(|| format!("undefined register '{den}'"))?;
                    if d == 0 {
                        return Err("TRAP: ZeroDivisionError during DivChecked".into());
                    }
                    regs.insert(dst.clone(), num / d);
                }
                Op::Store { slot, src } => {
                    let v = *regs
                        .get(src)
                        .ok_or_else(|| format!("undefined register '{src}'"))?;
                    if *slot >= mem.len() {
                        return Err(format!("out-of-bounds store slot {slot}"));
                    }
                    mem[*slot] = v;
                }
            }
        }
        match &blk.term {
            Term::BrZero {
                cond,
                if_zero,
                if_nonzero,
            } => {
                let c = *regs
                    .get(cond)
                    .ok_or_else(|| format!("undefined branch cond '{cond}'"))?;
                pc_block = if c == 0 { *if_zero } else { *if_nonzero };
            }
            Term::Jump(t) => pc_block = *t,
            Term::Ret(r) => {
                let ret = *regs
                    .get(r)
                    .ok_or_else(|| format!("undefined return reg '{r}'"))?;
                return Ok(SimState {
                    ret,
                    regs,
                    mem,
                    visited_blocks: visited,
                });
            }
        }
    }
    Err("exceeded CFG step budget".into())
}

fn sample_diamond_cfg() -> CfgProgram {
    // B0 (freq 100): loads mem[0] (lat 3), computes r2 = r1 + 5, branches on r_guard == 0
    //   - Hot edge  (90%): B1 computes r3 = r_in * 3, r4 = 100 / r_guard [trap!], r5 = r2 + r3
    //   - Cold edge (10%): B2 computes r5 = r2 + 15, r4 = 0
    // B3 (join, freq 100): computes r6 = r_in + 10, r7 = r5 + r6, r8 = r7 + r4, mem[1] = r8, ret r8
    let b0 = Block {
        freq: 100,
        insts: vec![
            Inst {
                op: Op::LoadSafe {
                    dst: "r1".into(),
                    slot: 0,
                },
                latency: 3,
            },
            Inst {
                op: Op::AddImm {
                    dst: "r2".into(),
                    src: "r1".into(),
                    imm: 5,
                },
                latency: 1,
            },
        ],
        term: Term::BrZero {
            cond: "r_guard".into(),
            if_zero: 2,
            if_nonzero: 1,
        },
    };
    let b1 = Block {
        freq: 90,
        insts: vec![
            Inst {
                op: Op::MulImm {
                    dst: "r3".into(),
                    src: "r_in".into(),
                    imm: 3,
                },
                latency: 2,
            },
            Inst {
                op: Op::DivChecked {
                    dst: "r4".into(),
                    num: 100,
                    den: "r_guard".into(),
                },
                latency: 2,
            },
            Inst {
                op: Op::Add {
                    dst: "r5".into(),
                    lhs: "r2".into(),
                    rhs: "r3".into(),
                },
                latency: 1,
            },
        ],
        term: Term::Jump(3),
    };
    let b2 = Block {
        freq: 10,
        insts: vec![
            Inst {
                op: Op::AddImm {
                    dst: "r5".into(),
                    src: "r2".into(),
                    imm: 15,
                },
                latency: 1,
            },
            Inst {
                op: Op::Const {
                    dst: "r4".into(),
                    val: 0,
                },
                latency: 1,
            },
        ],
        term: Term::Jump(3),
    };
    let b3 = Block {
        freq: 100,
        insts: vec![
            Inst {
                op: Op::AddImm {
                    dst: "r6".into(),
                    src: "r_in".into(),
                    imm: 10,
                },
                latency: 2,
            },
            Inst {
                op: Op::Add {
                    dst: "r7".into(),
                    lhs: "r5".into(),
                    rhs: "r6".into(),
                },
                latency: 1,
            },
            Inst {
                op: Op::Add {
                    dst: "r8".into(),
                    lhs: "r7".into(),
                    rhs: "r4".into(),
                },
                latency: 1,
            },
            Inst {
                op: Op::Store {
                    slot: 1,
                    src: "r8".into(),
                },
                latency: 1,
            },
        ],
        term: Term::Ret("r8".into()),
    };

    CfgProgram {
        blocks: BTreeMap::from([(0, b0), (1, b1), (2, b2), (3, b3)]),
        entry: 0,
    }
}

fn main() {
    let cfg = sample_diamond_cfg();
    let res = trace_schedule_cfg(&cfg, 2).expect("valid trace schedule");
    let live = compute_liveness(&cfg);

    println!(
        "Selected hot trace: {:?}",
        res.trace
            .iter()
            .map(|b| format!("B{b}"))
            .collect::<Vec<_>>()
    );
    println!(
        "Liveness: B0 live_out={:?}, cold B2 live_in={:?}",
        live.live_out[&0].iter().collect::<Vec<_>>(),
        live.live_in[&2].iter().collect::<Vec<_>>()
    );
    println!(
        "Hot-path cycles (width=2): basic-block-only = {} vs global-trace = {}",
        res.bb_cycles, res.trace_cycles
    );

    println!("\nGlobal trace schedule:");
    for entry in &res.schedule {
        println!(
            "  cycle {:2} | from B{} | {}",
            entry.cycle, entry.orig_block, entry.desc
        );
    }

    println!("\nCode motions & §10.4 compensation:");
    for (op, m) in &res.motions {
        println!("  {op:<30} -> {m:?}");
    }

    let hot_orig = simulate_cfg(&cfg, &[("r_in", 4), ("r_guard", 2)], &[20, 0]).unwrap();
    let hot_opt = simulate_cfg(
        &res.transformed_cfg,
        &[("r_in", 4), ("r_guard", 2)],
        &[20, 0],
    )
    .unwrap();
    let cold_orig = simulate_cfg(&cfg, &[("r_in", 4), ("r_guard", 0)], &[20, 0]).unwrap();
    let cold_opt = simulate_cfg(
        &res.transformed_cfg,
        &[("r_in", 4), ("r_guard", 0)],
        &[20, 0],
    )
    .unwrap();

    println!(
        "\nVerification (hot r_guard=2): orig={} opt={}, mem={:?}",
        hot_orig.ret, hot_opt.ret, hot_opt.mem
    );
    println!(
        "Verification (cold r_guard=0): orig={} opt={}, mem={:?} (comp ops in B2: {})",
        cold_orig.ret,
        cold_opt.ret,
        cold_opt.mem,
        res.transformed_cfg.blocks[&2].insts.len()
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trace_scheduling_hoists_speculative_ops_adds_compensation_and_saves_cycles() {
        let cfg = sample_diamond_cfg();
        let res = trace_schedule_cfg(&cfg, 2).expect("schedules");
        assert_eq!(res.trace, vec![0, 1, 3]);
        assert_eq!(res.bb_cycles, 13);
        assert_eq!(res.trace_cycles, 9);
        assert!(res.trace_cycles < res.bb_cycles);

        // `r6 = r_in + 10` hoisted above B1->B3 join must insert compensation into B2.
        assert!(res.motions.iter().any(|(op, m)| {
            op.contains("r6 = r_in + 10")
                && matches!(m, MotionKind::HoistAcrossJoinWithComp { comp_block: 2, .. })
        }));

        // `DivChecked` must NEVER move speculatively above B0's `BrZero` branch.
        let br_cyc = res
            .schedule
            .iter()
            .find(|e| e.desc.starts_with("br_zero"))
            .unwrap()
            .cycle;
        let div_cyc = res
            .schedule
            .iter()
            .find(|e| e.desc.contains("[trap]"))
            .unwrap()
            .cycle;
        assert!(div_cyc > br_cyc);

        // Both hot (`r_guard = 2`) and cold (`r_guard = 0`) paths yield identical results.
        let hot_orig = simulate_cfg(&cfg, &[("r_in", 4), ("r_guard", 2)], &[20, 0]).unwrap();
        let hot_opt = simulate_cfg(
            &res.transformed_cfg,
            &[("r_in", 4), ("r_guard", 2)],
            &[20, 0],
        )
        .unwrap();
        assert_eq!(hot_orig.ret, 101);
        assert_eq!(hot_opt.ret, 101);
        assert_eq!(hot_opt.mem, vec![20, 101]);

        let cold_orig = simulate_cfg(&cfg, &[("r_in", 4), ("r_guard", 0)], &[20, 0]).unwrap();
        let cold_opt = simulate_cfg(
            &res.transformed_cfg,
            &[("r_in", 4), ("r_guard", 0)],
            &[20, 0],
        )
        .unwrap();
        assert_eq!(cold_orig.ret, 54);
        assert_eq!(cold_opt.ret, 54);
        assert_eq!(cold_opt.mem, vec![20, 54]);
    }

    #[test]
    fn blocks_speculative_clobber_when_dst_is_live_in_cold_branch() {
        // B0: load slow r1, branch on r_guard; B1 writes `r_live = r_in + 99`;
        // B2 (cold) reads `r_live`! Moving `r_live = ...` into B0 would corrupt cold output!
        let b0 = Block {
            freq: 100,
            insts: vec![Inst {
                op: Op::LoadSafe {
                    dst: "r1".into(),
                    slot: 0,
                },
                latency: 3,
            }],
            term: Term::BrZero {
                cond: "r_guard".into(),
                if_zero: 2,
                if_nonzero: 1,
            },
        };
        let b1 = Block {
            freq: 90,
            insts: vec![
                Inst {
                    op: Op::AddImm {
                        dst: "r_live".into(),
                        src: "r_in".into(),
                        imm: 99,
                    },
                    latency: 1,
                },
                Inst {
                    op: Op::Add {
                        dst: "r_out".into(),
                        lhs: "r1".into(),
                        rhs: "r_live".into(),
                    },
                    latency: 1,
                },
            ],
            term: Term::Ret("r_out".into()),
        };
        let b2 = Block {
            freq: 10,
            insts: vec![Inst {
                op: Op::Add {
                    dst: "r_out".into(),
                    lhs: "r1".into(),
                    rhs: "r_live".into(),
                },
                latency: 1,
            }],
            term: Term::Ret("r_out".into()),
        };
        let cfg = CfgProgram {
            blocks: BTreeMap::from([(0, b0), (1, b1), (2, b2)]),
            entry: 0,
        };
        let res = trace_schedule_cfg(&cfg, 2).expect("schedules safely");
        let cold_orig =
            simulate_cfg(&cfg, &[("r_in", 5), ("r_live", 7), ("r_guard", 0)], &[20]).unwrap();
        let cold_opt = simulate_cfg(
            &res.transformed_cfg,
            &[("r_in", 5), ("r_live", 7), ("r_guard", 0)],
            &[20],
        )
        .unwrap();
        assert_eq!(cold_orig.ret, 27);
        assert_eq!(cold_opt.ret, 27);
    }

    #[test]
    fn blocks_speculative_memory_store_above_controlling_branch() {
        let b0 = Block {
            freq: 100,
            insts: vec![Inst {
                op: Op::LoadSafe {
                    dst: "r1".into(),
                    slot: 0,
                },
                latency: 3,
            }],
            term: Term::BrZero {
                cond: "r_guard".into(),
                if_zero: 2,
                if_nonzero: 1,
            },
        };
        let b1 = Block {
            freq: 90,
            insts: vec![
                Inst {
                    op: Op::Store {
                        slot: 1,
                        src: "r_in".into(),
                    },
                    latency: 1,
                },
                Inst {
                    op: Op::AddImm {
                        dst: "r_out".into(),
                        src: "r1".into(),
                        imm: 1,
                    },
                    latency: 1,
                },
            ],
            term: Term::Ret("r_out".into()),
        };
        let b2 = Block {
            freq: 10,
            insts: vec![Inst {
                op: Op::LoadSafe {
                    dst: "r_out".into(),
                    slot: 1,
                },
                latency: 1,
            }],
            term: Term::Ret("r_out".into()),
        };
        let cfg = CfgProgram {
            blocks: BTreeMap::from([(0, b0), (1, b1), (2, b2)]),
            entry: 0,
        };
        let res = trace_schedule_cfg(&cfg, 2).expect("schedules safely");
        let cold_orig = simulate_cfg(&cfg, &[("r_in", 999), ("r_guard", 0)], &[20, 42]).unwrap();
        let cold_opt = simulate_cfg(
            &res.transformed_cfg,
            &[("r_in", 999), ("r_guard", 0)],
            &[20, 42],
        )
        .unwrap();
        assert_eq!(cold_orig.ret, 42);
        assert_eq!(cold_opt.ret, 42);
        assert_eq!(cold_opt.mem, vec![20, 42]);
    }
}
