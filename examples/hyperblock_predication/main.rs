//! Chapter 10 (§10.4): Hardware-Predicated Execution & Full If-Converted Hyperblocks.
//!
//! Demonstrates:
//! - Hyperblock region selection (merging balanced conditional arms while leaving
//!   ultra-cold error exits outside as side exits),
//! - Full control-dependence if-conversion using unconditional partition compares
//!   (`cmp.zero.un pT, pF = cond if p_guard`) and OR-type predicate merges
//!   (`PredClear` + `PredOrTrue`),
//! - Exact predicate disjointness proving (`PredTree::are_disjoint`) to eliminate
//!   false WAW/WAR register and memory dependences across mutually exclusive paths,
//! - Speculative predicate promotion (`p_child -> p0`) for pure non-trapping ops
//!   while proving trapping (`DivChecked`) and `Store` ops must keep their guards,
//! - Width-2 predicated hyperblock list scheduling vs. expected CFG cycles, and
//! - Cycle-accurate predicated VLIW hardware VM proving exact register, memory,
//!   and trap-squash equivalence across all hot, balanced, and cold-exit paths.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

// ---------------------------------------------------------------------------
// 1. CFG Intermediate Representation
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
    }, // memory side effect!
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

    /// Only pure, non-trapping, non-memory ops may be predicate-promoted above
    /// a branch or compare without hardware support for speculative traps.
    fn is_promotion_safe(&self) -> bool {
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
    params: Vec<String>,
}

// ---------------------------------------------------------------------------
// 2. CFG Liveness Analysis & Hyperblock Region Selection
// ---------------------------------------------------------------------------

fn compute_liveness(cfg: &CfgProgram) -> BTreeMap<usize, BTreeSet<String>> {
    let mut live_in: BTreeMap<usize, BTreeSet<String>> =
        cfg.blocks.keys().map(|&id| (id, BTreeSet::new())).collect();

    loop {
        let mut changed = false;
        for (&id, blk) in cfg.blocks.iter().rev() {
            let mut cur_in = BTreeSet::new();
            for s in blk.succs() {
                cur_in.extend(live_in[&s].iter().cloned());
            }
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
            if cur_in != live_in[&id] {
                live_in.insert(id, cur_in);
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    live_in
}

/// Computes topological order of the acyclic CFG starting from `cfg.entry`.
fn topo_sort_cfg(cfg: &CfgProgram) -> Vec<usize> {
    let mut indeg: BTreeMap<usize, usize> = cfg.blocks.keys().map(|&id| (id, 0)).collect();
    for blk in cfg.blocks.values() {
        for s in blk.succs() {
            *indeg.entry(s).or_insert(0) += 1;
        }
    }
    let mut ready: BTreeSet<usize> = indeg
        .iter()
        .filter(|(_, &d)| d == 0)
        .map(|(&id, _)| id)
        .collect();
    let mut order = Vec::new();
    while let Some(u) = ready.pop_first() {
        order.push(u);
        for s in cfg.blocks[&u].succs() {
            let e = indeg.get_mut(&s).unwrap();
            *e -= 1;
            if *e == 0 {
                ready.insert(s);
            }
        }
    }
    order
}

/// Hyperblock selection (§10.4): unlike trace selection (one linear path),
/// hyperblocks include BOTH arms of frequent or balanced branches while
/// excluding low-frequency trap/error handlers (< `min_ratio_pct`% of entry).
fn select_hyperblock(cfg: &CfgProgram, min_ratio_pct: usize) -> BTreeSet<usize> {
    let entry_freq = cfg.blocks[&cfg.entry].freq;
    let mut selected = BTreeSet::new();
    for id in topo_sort_cfg(cfg) {
        let blk = &cfg.blocks[&id];
        let reachable_from_hb = id == cfg.entry
            || selected
                .iter()
                .any(|&p| cfg.blocks[&p].succs().contains(&id));
        if reachable_from_hb && blk.freq * 100 >= entry_freq * min_ratio_pct {
            selected.insert(id);
        }
    }
    selected
}

// ---------------------------------------------------------------------------
// 3. Predicate Hierarchy Tree & Exact Disjointness Solver
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
enum PredDef {
    /// Root architectural predicate `p0 = true`.
    TrueRoot,
    /// Complementary pair created by `cmp.zero.un p_true, p_false = cond if parent`.
    PartitionChild { parent: usize, sibling: usize },
    /// Synthesized join predicate `p_dst = OR_i p_inputs[i]`.
    JoinOr { inputs: Vec<usize> },
}

#[derive(Clone, Debug)]
struct PredTree {
    defs: Vec<PredDef>,
}

impl PredTree {
    fn new() -> Self {
        Self {
            defs: vec![PredDef::TrueRoot],
        }
    }

    fn alloc_partition(&mut self, parent: usize) -> (usize, usize) {
        let p_left = self.defs.len();
        let p_right = p_left + 1;
        self.defs.push(PredDef::PartitionChild {
            parent,
            sibling: p_right,
        });
        self.defs.push(PredDef::PartitionChild {
            parent,
            sibling: p_left,
        });
        (p_left, p_right)
    }

    fn alloc_join(&mut self, inputs: Vec<usize>) -> usize {
        let p = self.defs.len();
        self.defs.push(PredDef::JoinOr { inputs });
        p
    }

    /// Walks up `PartitionChild` parents returning `[p, parent, ..., p0]`.
    fn atomic_ancestors(&self, p: usize) -> BTreeSet<usize> {
        let mut set = BTreeSet::new();
        let mut cur = p;
        loop {
            set.insert(cur);
            match &self.defs[cur] {
                PredDef::PartitionChild { parent, .. } => cur = *parent,
                PredDef::TrueRoot | PredDef::JoinOr { .. } => break,
            }
        }
        set
    }

    /// True iff `a` and `b` can NEVER both be true in the same execution.
    fn are_disjoint(&self, a: usize, b: usize) -> bool {
        if a == b {
            return false;
        }
        if let PredDef::JoinOr { inputs } = &self.defs[a] {
            return inputs.iter().all(|&in_p| self.are_disjoint(in_p, b));
        }
        if let PredDef::JoinOr { inputs } = &self.defs[b] {
            return inputs.iter().all(|&in_p| self.are_disjoint(a, in_p));
        }
        let anc_a = self.atomic_ancestors(a);
        let anc_b = self.atomic_ancestors(b);
        for &node in &anc_a {
            if let PredDef::PartitionChild { sibling, .. } = &self.defs[node] {
                if anc_b.contains(sibling) {
                    return true;
                }
            }
        }
        false
    }

    /// Nearest common ancestor partition predicate if both are atomic, else `0`.
    fn nearest_atomic_parent(&self, p: usize) -> Option<usize> {
        match &self.defs[p] {
            PredDef::PartitionChild { parent, .. } => Some(*parent),
            PredDef::TrueRoot | PredDef::JoinOr { .. } => None,
        }
    }
}

// ---------------------------------------------------------------------------
// 4. Predicated Hyperblock IR & Control-Dependence If-Conversion (§10.4)
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq)]
enum HOp {
    Core(Op),
    /// Unconditional partition compare:
    /// `p_true = (guard && src == 0); p_false = (guard && src != 0)`
    CmpZeroUn {
        p_true: usize,
        p_false: usize,
        src: String,
    },
    /// Clears join predicate `p_dst = false` unconditionally (under `p0`).
    PredClear {
        p_dst: usize,
    },
    /// OR-type predicate merge: `if guard { p_dst = true }`.
    PredOrTrue {
        p_dst: usize,
    },
    /// Hyperblock side exit to an excluded cold CFG block.
    SideExit {
        target: usize,
    },
    Ret(String),
}

impl HOp {
    fn dst_reg(&self) -> Option<&str> {
        match self {
            HOp::Core(op) => op.dst(),
            _ => None,
        }
    }

    fn uses_regs(&self) -> Vec<&str> {
        match self {
            HOp::Core(op) => op.uses(),
            HOp::CmpZeroUn { src, .. } | HOp::Ret(src) => vec![src],
            HOp::PredClear { .. } | HOp::PredOrTrue { .. } | HOp::SideExit { .. } => Vec::new(),
        }
    }

    fn defines_preds(&self) -> Vec<usize> {
        match self {
            HOp::CmpZeroUn {
                p_true, p_false, ..
            } => vec![*p_true, *p_false],
            HOp::PredClear { p_dst } | HOp::PredOrTrue { p_dst } => vec![*p_dst],
            HOp::Core(_) | HOp::SideExit { .. } | HOp::Ret(_) => Vec::new(),
        }
    }
}

impl fmt::Display for HOp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HOp::Core(op) => write!(f, "{op}"),
            HOp::CmpZeroUn {
                p_true,
                p_false,
                src,
            } => write!(f, "cmp.zero.un p{p_true}, p{p_false} = {src}"),
            HOp::PredClear { p_dst } => write!(f, "p{p_dst} = false"),
            HOp::PredOrTrue { p_dst } => write!(f, "p{p_dst} |= true"),
            HOp::SideExit { target } => write!(f, "exit B{target}"),
            HOp::Ret(r) => write!(f, "ret {r}"),
        }
    }
}

#[derive(Clone, Debug)]
struct HInst {
    pred: usize,
    orig_pred: usize,
    op: HOp,
    latency: usize,
    promoted: bool,
}

impl fmt::Display for HInst {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.promoted {
            write!(f, "(p{}) {} [from p{}]", self.pred, self.op, self.orig_pred)
        } else {
            write!(f, "(p{}) {}", self.pred, self.op)
        }
    }
}

#[derive(Clone, Debug)]
struct Hyperblock {
    blocks: BTreeSet<usize>,
    pred_tree: PredTree,
    insts: Vec<HInst>,
    block_pred: BTreeMap<usize, usize>,
    /// Partition edges `p_child -> sibling_target_block` to check live_in on promotion.
    split_sibling_target: BTreeMap<usize, usize>,
    side_exits: BTreeSet<usize>,
}

/// Converts all hyperblock CFG blocks into a straight-line sequence of predicated
/// instructions, synthesizing partition predicates and `PredClear` + `PredOrTrue`
/// join predicates when cold side exits shrink a join block's path condition.
fn if_convert(cfg: &CfgProgram, hb_blocks: &BTreeSet<usize>) -> Hyperblock {
    let mut pred_tree = PredTree::new();
    let mut incoming: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    let mut block_pred: BTreeMap<usize, usize> = BTreeMap::new();
    let mut split_sibling_target: BTreeMap<usize, usize> = BTreeMap::new();
    let mut side_exits: BTreeSet<usize> = BTreeSet::new();
    let mut insts = Vec::new();

    for b in topo_sort_cfg(cfg) {
        if !hb_blocks.contains(&b) {
            continue;
        }
        let p_b = if b == cfg.entry {
            0
        } else {
            let mut inc = incoming.get(&b).cloned().unwrap_or_default();
            inc.sort_unstable();
            inc.dedup();
            assert!(
                !inc.is_empty(),
                "Hyperblock block B{b} has no incoming pred"
            );
            if inc.len() == 1 {
                inc[0]
            } else {
                // Check if two incoming atomic edges completely reunite a parent partition.
                let merged_parent = if inc.len() == 2 {
                    match (
                        pred_tree.nearest_atomic_parent(inc[0]),
                        pred_tree.nearest_atomic_parent(inc[1]),
                    ) {
                        (Some(par0), Some(par1))
                            if par0 == par1
                                && matches!(
                                    pred_tree.defs[inc[0]],
                                    PredDef::PartitionChild { sibling, .. } if sibling == inc[1]
                                ) =>
                        {
                            Some(par0)
                        }
                        _ => None,
                    }
                } else {
                    None
                };

                if let Some(par) = merged_parent {
                    par
                } else {
                    let p_join = pred_tree.alloc_join(inc.clone());
                    insts.push(HInst {
                        pred: 0,
                        orig_pred: 0,
                        op: HOp::PredClear { p_dst: p_join },
                        latency: 1,
                        promoted: false,
                    });
                    for p_in in inc {
                        insts.push(HInst {
                            pred: p_in,
                            orig_pred: p_in,
                            op: HOp::PredOrTrue { p_dst: p_join },
                            latency: 1,
                            promoted: false,
                        });
                    }
                    p_join
                }
            }
        };
        block_pred.insert(b, p_b);

        let blk = &cfg.blocks[&b];
        for inst in &blk.insts {
            insts.push(HInst {
                pred: p_b,
                orig_pred: p_b,
                op: HOp::Core(inst.op.clone()),
                latency: inst.latency,
                promoted: false,
            });
        }

        match &blk.term {
            Term::BrZero {
                cond,
                if_zero,
                if_nonzero,
            } => {
                let (p_zero, p_nz) = pred_tree.alloc_partition(p_b);
                // Sibling target map: if we promote past p_zero, check live_in[if_nonzero], and vice versa.
                split_sibling_target.insert(p_zero, *if_nonzero);
                split_sibling_target.insert(p_nz, *if_zero);

                insts.push(HInst {
                    pred: p_b,
                    orig_pred: p_b,
                    op: HOp::CmpZeroUn {
                        p_true: p_zero,
                        p_false: p_nz,
                        src: cond.clone(),
                    },
                    latency: 1,
                    promoted: false,
                });

                for (p_edge, target) in [(p_zero, *if_zero), (p_nz, *if_nonzero)] {
                    if hb_blocks.contains(&target) {
                        incoming.entry(target).or_default().push(p_edge);
                    } else {
                        side_exits.insert(target);
                        insts.push(HInst {
                            pred: p_edge,
                            orig_pred: p_edge,
                            op: HOp::SideExit { target },
                            latency: 1,
                            promoted: false,
                        });
                    }
                }
            }
            Term::Jump(t) => {
                if hb_blocks.contains(t) {
                    incoming.entry(*t).or_default().push(p_b);
                } else {
                    side_exits.insert(*t);
                    insts.push(HInst {
                        pred: p_b,
                        orig_pred: p_b,
                        op: HOp::SideExit { target: *t },
                        latency: 1,
                        promoted: false,
                    });
                }
            }
            Term::Ret(r) => {
                insts.push(HInst {
                    pred: p_b,
                    orig_pred: p_b,
                    op: HOp::Ret(r.clone()),
                    latency: 1,
                    promoted: false,
                });
            }
        }
    }

    Hyperblock {
        blocks: hb_blocks.clone(),
        pred_tree,
        insts,
        block_pred,
        split_sibling_target,
        side_exits,
    }
}

// ---------------------------------------------------------------------------
// 5. Speculative Predicate Promotion (§10.4)
// ---------------------------------------------------------------------------

/// Promotes pure, non-trapping operations from narrow branch predicates up to
/// an ancestor predicate (`p0`) when:
/// 1. `op.is_promotion_safe()` (blocks `DivChecked` traps & `Store` side effects),
/// 2. The destination register `d` is NOT `live_in` on any crossed sibling target
///    (and no disjoint instruction in the hyperblock also writes `d`),
/// 3. All source registers of `op` are available at the target ancestor predicate.
fn promote_predicates(
    cfg: &CfgProgram,
    hb: &mut Hyperblock,
    live_in: &BTreeMap<usize, BTreeSet<String>>,
) {
    // Track which predicates define each register so we only promote when all uses are valid at `p0`
    // and no disjoint sibling defines the same destination register.
    let mut defs_by_reg: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    for param in &cfg.params {
        defs_by_reg.entry(param.clone()).or_default().push(0);
    }
    for inst in &hb.insts {
        if let Some(d) = inst.op.dst_reg() {
            defs_by_reg
                .entry(d.to_string())
                .or_default()
                .push(inst.pred);
        }
    }

    for inst in &mut hb.insts {
        if inst.pred == 0 {
            continue;
        }
        let HOp::Core(core_op) = &inst.op else {
            continue;
        };
        if !core_op.is_promotion_safe() {
            continue;
        }
        let Some(dst) = core_op.dst() else {
            continue;
        };

        // Can only promote along an atomic `PartitionChild` chain up toward `p0`.
        let mut target_pred = inst.pred;
        let mut cur = inst.pred;
        while let PredDef::PartitionChild { parent, .. } = &hb.pred_tree.defs[cur] {
            // Check 1: is `dst` live entering the sibling path of `cur`?
            if let Some(&sib_blk) = hb.split_sibling_target.get(&cur) {
                if live_in[&sib_blk].contains(dst) {
                    break;
                }
            }
            // Check 2: does any instruction under a predicate disjoint from `inst.pred`
            // also define `dst` (meaning promoting above the split would corrupt sibling's def)?
            let conflict_def = defs_by_reg[dst]
                .iter()
                .any(|&p_def| hb.pred_tree.are_disjoint(p_def, inst.pred));
            if conflict_def {
                break;
            }
            // Check 3: are all source registers defined at `parent` or higher (non-disjoint & ancestor)?
            let uses_ok = core_op.uses().into_iter().all(|u| {
                let Some(u_defs) = defs_by_reg.get(u) else {
                    return false;
                };
                u_defs.iter().any(|&p_def| {
                    p_def == *parent || hb.pred_tree.atomic_ancestors(*parent).contains(&p_def)
                })
            });
            if !uses_ok {
                break;
            }

            target_pred = *parent;
            cur = *parent;
        }

        if target_pred != inst.pred {
            inst.pred = target_pred;
            inst.promoted = true;
        }
    }
}

// ---------------------------------------------------------------------------
// 6. Predicate-Aware Hyperblock List Scheduler & Expected BB Baseline
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
struct ScheduledHOp {
    cycle: usize,
    inst_idx: usize,
}

#[derive(Clone, Debug)]
struct HyperblockSchedule {
    entries: Vec<ScheduledHOp>,
    total_cycles: usize,
    removed_false_deps: usize,
}

fn schedule_hyperblock(hb: &Hyperblock, width: usize) -> HyperblockSchedule {
    assert!(width > 0);
    let n = hb.insts.len();
    let mut succs: Vec<Vec<(usize, usize)>> = vec![Vec::new(); n];
    let mut preds: Vec<Vec<(usize, usize)>> = vec![Vec::new(); n];
    let mut removed_false_deps = 0usize;

    let mut add_edge = |u: usize, v: usize, lat: usize| {
        if let Some(existing) = succs[u].iter_mut().find(|(dst, _)| *dst == v) {
            existing.1 = existing.1.max(lat);
            for (src, w) in &mut preds[v] {
                if *src == u {
                    *w = (*w).max(lat);
                }
            }
        } else {
            succs[u].push((v, lat));
            preds[v].push((u, lat));
        }
    };

    for u in 0..n {
        for v in (u + 1)..n {
            let iu = &hb.insts[u];
            let iv = &hb.insts[v];
            let disjoint = hb.pred_tree.are_disjoint(iu.pred, iv.pred);

            // 1. Predicate-register dependences (`cmp.zero.un`, `PredClear`, `PredOrTrue`).
            let u_pdefs = iu.op.defines_preds();
            if !u_pdefs.is_empty() {
                if u_pdefs.contains(&iv.pred) {
                    add_edge(u, v, iu.latency);
                }
                if let (HOp::PredClear { p_dst: p1 }, HOp::PredOrTrue { p_dst: p2 }) =
                    (&iu.op, &iv.op)
                {
                    if p1 == p2 {
                        add_edge(u, v, iu.latency);
                    }
                }
                if let (HOp::PredOrTrue { p_dst: p1 }, HOp::PredOrTrue { p_dst: p2 }) =
                    (&iu.op, &iv.op)
                {
                    if p1 == p2 && !disjoint {
                        add_edge(u, v, iu.latency);
                    }
                }
            }

            // 2. Data register RAW / WAW / WAR dependences (filtered by disjointness!).
            let u_dst = iu.op.dst_reg();
            let v_dst = iv.op.dst_reg();
            let u_uses = iu.op.uses_regs();
            let v_uses = iv.op.uses_regs();

            // RAW
            if let Some(d) = u_dst {
                if v_uses.contains(&d) {
                    if disjoint {
                        removed_false_deps += 1;
                    } else {
                        add_edge(u, v, iu.latency);
                    }
                }
            }
            // WAW
            if let (Some(du), Some(dv)) = (u_dst, v_dst) {
                if du == dv {
                    if disjoint {
                        removed_false_deps += 1;
                    } else {
                        add_edge(u, v, 1);
                    }
                }
            }
            // WAR
            if let Some(dv) = v_dst {
                if u_uses.contains(&dv) {
                    if disjoint {
                        removed_false_deps += 1;
                    } else {
                        add_edge(u, v, 1);
                    }
                }
            }

            // 3. Memory dependences (filtered by disjointness!).
            let u_w = matches!(iu.op, HOp::Core(ref o) if o.writes_mem());
            let u_r = matches!(iu.op, HOp::Core(ref o) if o.reads_mem());
            let v_w = matches!(iv.op, HOp::Core(ref o) if o.writes_mem());
            let v_r = matches!(iv.op, HOp::Core(ref o) if o.reads_mem());
            if (u_w && (v_w || v_r)) || (u_r && v_w) {
                if disjoint {
                    removed_false_deps += 1;
                } else {
                    add_edge(u, v, iu.latency.max(1));
                }
            }

            // 4. SideExit / Ret control ordering: side-effecting `Store` and `Ret`
            // from blocks after a `SideExit` stay after the `SideExit` (unless disjoint).
            if matches!(iu.op, HOp::SideExit { .. })
                && (v_w || matches!(iv.op, HOp::Ret(_) | HOp::SideExit { .. }))
                && !disjoint
            {
                add_edge(u, v, 1);
            }
            if matches!(iv.op, HOp::Ret(_)) && !disjoint {
                add_edge(u, v, iu.latency.max(1));
            }
        }
    }

    // Critical-path height for list-scheduling priority.
    let mut height = vec![0usize; n];
    for u in (0..n).rev() {
        let mut h = hb.insts[u].latency;
        for &(v, lat) in &succs[u] {
            h = h.max(lat + height[v]);
        }
        height[u] = h;
    }

    let mut unscheduled_preds: Vec<usize> = preds.iter().map(|p| p.len()).collect();
    let mut earliest = vec![0usize; n];
    let mut scheduled = vec![false; n];
    let mut entries = Vec::new();
    let mut cycle = 0usize;

    while entries.len() < n {
        let mut ready: Vec<usize> = (0..n)
            .filter(|&i| !scheduled[i] && unscheduled_preds[i] == 0 && earliest[i] <= cycle)
            .collect();
        ready.sort_by(|&a, &b| height[b].cmp(&height[a]).then_with(|| a.cmp(&b)));

        for idx in ready.into_iter().take(width) {
            scheduled[idx] = true;
            entries.push(ScheduledHOp {
                cycle,
                inst_idx: idx,
            });
            for &(v, lat) in &succs[idx] {
                unscheduled_preds[v] -= 1;
                earliest[v] = earliest[v].max(cycle + lat);
            }
        }
        cycle += 1;
    }

    // Total hyperblock cycles includes latency of the final issued instruction(s).
    let mut finish_cycle = 0usize;
    for e in &entries {
        finish_cycle = finish_cycle.max(e.cycle + hb.insts[e.inst_idx].latency);
    }

    HyperblockSchedule {
        entries,
        total_cycles: finish_cycle,
        removed_false_deps,
    }
}

/// Computes single-block list-schedule cycle count for unpredicated CFG baseline.
fn list_schedule_single_block(insts: &[Inst], width: usize) -> usize {
    let n = insts.len();
    let mut succs: Vec<Vec<(usize, usize)>> = vec![Vec::new(); n];
    let mut indeg = vec![0usize; n];
    for u in 0..n {
        for v in (u + 1)..n {
            let iu = &insts[u];
            let iv = &insts[v];
            let mut dep = None;
            if let Some(d) = iu.op.dst() {
                if iv.op.uses().contains(&d) {
                    dep = Some(iu.latency);
                }
            }
            if iu.op.dst().is_some() && iu.op.dst() == iv.op.dst() {
                dep = Some(dep.unwrap_or(1).max(1));
            }
            if let Some(dv) = iv.op.dst() {
                if iu.op.uses().contains(&dv) {
                    dep = Some(dep.unwrap_or(1).max(1));
                }
            }
            if (iu.op.writes_mem() && (iv.op.writes_mem() || iv.op.reads_mem()))
                || (iu.op.reads_mem() && iv.op.writes_mem())
            {
                dep = Some(dep.unwrap_or(1).max(iu.latency));
            }
            if let Some(lat) = dep {
                succs[u].push((v, lat));
                indeg[v] += 1;
            }
        }
    }

    let mut ready_at = vec![0usize; n];
    let mut done = vec![false; n];
    let mut finish = 0usize;
    let mut scheduled_count = 0usize;
    let mut cycle = 0usize;
    while scheduled_count < n {
        let ready: Vec<usize> = (0..n)
            .filter(|&i| !done[i] && indeg[i] == 0 && ready_at[i] <= cycle)
            .collect();
        for &idx in ready.iter().take(width) {
            done[idx] = true;
            scheduled_count += 1;
            finish = finish.max(cycle + insts[idx].latency);
            for &(v, lat) in &succs[idx] {
                indeg[v] -= 1;
                ready_at[v] = ready_at[v].max(cycle + lat);
            }
        }
        cycle += 1;
    }
    // +1 cycle for the block-ending branch / jump / ret terminator.
    finish + 1
}

/// Computes exact weighted expected cycles across all paths of the unpredicated CFG.
fn expected_bb_cycles(cfg: &CfgProgram, width: usize, branch_mispredict_penalty: usize) -> f64 {
    let block_cycs: BTreeMap<usize, usize> = cfg
        .blocks
        .iter()
        .map(|(&id, b)| (id, list_schedule_single_block(&b.insts, width)))
        .collect();

    fn walk(
        cfg: &CfgProgram,
        b: usize,
        prob: f64,
        acc_cycles: usize,
        block_cycs: &BTreeMap<usize, usize>,
        mispredict_penalty: usize,
    ) -> f64 {
        let blk = &cfg.blocks[&b];
        let c = acc_cycles + block_cycs[&b];
        match &blk.term {
            Term::Ret(_) => prob * (c as f64),
            Term::Jump(t) => walk(cfg, *t, prob, c, block_cycs, mispredict_penalty),
            Term::BrZero {
                if_zero,
                if_nonzero,
                ..
            } => {
                let fz = cfg.blocks[if_zero].freq as f64;
                let fnz = cfg.blocks[if_nonzero].freq as f64;
                let tot = fz + fnz;
                // Static predictor guesses the hotter arm; the colder arm pays `mispredict_penalty`.
                let hot_is_zero = fz >= fnz;
                let pen_zero = if hot_is_zero { 0 } else { mispredict_penalty };
                let pen_nz = if hot_is_zero { mispredict_penalty } else { 0 };
                walk(
                    cfg,
                    *if_zero,
                    prob * (fz / tot),
                    c + pen_zero,
                    block_cycs,
                    mispredict_penalty,
                ) + walk(
                    cfg,
                    *if_nonzero,
                    prob * (fnz / tot),
                    c + pen_nz,
                    block_cycs,
                    mispredict_penalty,
                )
            }
        }
    }

    walk(
        cfg,
        cfg.entry,
        1.0,
        0,
        &block_cycs,
        branch_mispredict_penalty,
    )
}

// ---------------------------------------------------------------------------
// 7. Original CFG VM vs. Predicated Hyperblock Hardware VM
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq)]
struct SimResult {
    ret_val: i64,
    mem: Vec<i64>,
    visited: Vec<String>,
    traps: usize,
    squashed_ops: usize,
    side_exited: bool,
}

fn eval_core_op(op: &Op, regs: &mut BTreeMap<String, i64>, mem: &mut [i64], traps: &mut usize) {
    match op {
        Op::Const { dst, val } => {
            regs.insert(dst.clone(), *val);
        }
        Op::LoadSafe { dst, slot } => {
            regs.insert(dst.clone(), mem[*slot]);
        }
        Op::AddImm { dst, src, imm } => {
            regs.insert(dst.clone(), regs[src] + imm);
        }
        Op::MulImm { dst, src, imm } => {
            regs.insert(dst.clone(), regs[src] * imm);
        }
        Op::Add { dst, lhs, rhs } => {
            regs.insert(dst.clone(), regs[lhs] + regs[rhs]);
        }
        Op::DivChecked { dst, num, den } => {
            let d = regs[den];
            if d == 0 {
                *traps += 1;
                panic!("Hardware trap: division by zero in {op}");
            }
            regs.insert(dst.clone(), num / d);
        }
        Op::Store { slot, src } => {
            mem[*slot] = regs[src];
        }
    }
}

fn simulate_cfg(cfg: &CfgProgram, init_regs: &[(&str, i64)], init_mem: &[i64]) -> SimResult {
    let mut regs: BTreeMap<String, i64> =
        init_regs.iter().map(|&(k, v)| (k.to_string(), v)).collect();
    let mut mem = init_mem.to_vec();
    let mut visited = Vec::new();
    let mut traps = 0usize;
    let mut cur = cfg.entry;

    loop {
        visited.push(format!("B{cur}"));
        let blk = &cfg.blocks[&cur];
        for inst in &blk.insts {
            eval_core_op(&inst.op, &mut regs, &mut mem, &mut traps);
        }
        match &blk.term {
            Term::BrZero {
                cond,
                if_zero,
                if_nonzero,
            } => {
                cur = if regs[cond] == 0 {
                    *if_zero
                } else {
                    *if_nonzero
                };
            }
            Term::Jump(t) => cur = *t,
            Term::Ret(r) => {
                return SimResult {
                    ret_val: regs[r],
                    mem,
                    visited,
                    traps,
                    squashed_ops: 0,
                    side_exited: false,
                };
            }
        }
    }
}

/// Simulates the scheduled predicated hyperblock + off-hyperblock cold exit blocks.
fn simulate_hyperblock(
    cfg: &CfgProgram,
    hb: &Hyperblock,
    sched: &HyperblockSchedule,
    init_regs: &[(&str, i64)],
    init_mem: &[i64],
) -> SimResult {
    let mut regs: BTreeMap<String, i64> =
        init_regs.iter().map(|&(k, v)| (k.to_string(), v)).collect();
    let mut mem = init_mem.to_vec();
    let mut preds: BTreeMap<usize, bool> = BTreeMap::from([(0usize, true)]);
    let mut traps = 0usize;
    let mut squashed_ops = 0usize;

    let mut side_target: Option<usize> = None;
    let mut ret_val: Option<i64> = None;

    // Group entries by cycle to execute one VLIW packet per cycle faithfully.
    let mut by_cycle: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for e in &sched.entries {
        by_cycle.entry(e.cycle).or_default().push(e.inst_idx);
    }

    for (_cyc, idxs) in by_cycle {
        let mut jump_out: Option<usize> = None;
        let mut cyc_ret: Option<i64> = None;

        for idx in idxs {
            let hinst = &hb.insts[idx];
            let active = *preds.get(&hinst.pred).unwrap_or(&false);

            match &hinst.op {
                // `.un` compares always write both output predicates even when `guard == false`.
                HOp::CmpZeroUn {
                    p_true,
                    p_false,
                    src,
                } => {
                    let is_zero = active && regs[src] == 0;
                    let is_nz = active && regs[src] != 0;
                    preds.insert(*p_true, is_zero);
                    preds.insert(*p_false, is_nz);
                    if !active {
                        squashed_ops += 1;
                    }
                }
                HOp::PredClear { p_dst } => {
                    preds.insert(*p_dst, false);
                }
                HOp::PredOrTrue { p_dst } => {
                    if active {
                        preds.insert(*p_dst, true);
                    } else {
                        squashed_ops += 1;
                    }
                }
                HOp::Core(op) => {
                    if active {
                        eval_core_op(op, &mut regs, &mut mem, &mut traps);
                    } else {
                        squashed_ops += 1;
                    }
                }
                HOp::SideExit { target } => {
                    if active {
                        jump_out = Some(*target);
                    } else {
                        squashed_ops += 1;
                    }
                }
                HOp::Ret(r) => {
                    if active {
                        cyc_ret = Some(regs[r]);
                    } else {
                        squashed_ops += 1;
                    }
                }
            }
        }

        if let Some(t) = jump_out {
            side_target = Some(t);
            break;
        }
        if let Some(v) = cyc_ret {
            ret_val = Some(v);
            break;
        }
    }

    if let Some(t) = side_target {
        let mut cur = t;
        let mut visited = vec!["HB".to_string()];
        loop {
            visited.push(format!("B{cur}"));
            let blk = &cfg.blocks[&cur];
            for inst in &blk.insts {
                eval_core_op(&inst.op, &mut regs, &mut mem, &mut traps);
            }
            match &blk.term {
                Term::BrZero {
                    cond,
                    if_zero,
                    if_nonzero,
                } => {
                    cur = if regs[cond] == 0 {
                        *if_zero
                    } else {
                        *if_nonzero
                    };
                }
                Term::Jump(nxt) => cur = *nxt,
                Term::Ret(r) => {
                    return SimResult {
                        ret_val: regs[r],
                        mem,
                        visited,
                        traps,
                        squashed_ops,
                        side_exited: true,
                    };
                }
            }
        }
    }

    SimResult {
        ret_val: ret_val.expect("Hyperblock must finish with an active Ret or SideExit"),
        mem,
        visited: vec!["HB".to_string()],
        traps,
        squashed_ops,
        side_exited: false,
    }
}

// ---------------------------------------------------------------------------
// 8. Sample Diamond-with-Cold-Side-Exit CFG & Driver
// ---------------------------------------------------------------------------

/// Builds a 6-block CFG modeled on §10.4 hyperblock formation:
/// - `B0` (freq 100): slow `load_safe(mem[0])` + `r2 = r1 + 1`, unbiased `52/48` split on `r_x`
/// - `B1` (freq 52, `r_x != 0`): rare error check `br_zero r_err ? B3 : B5` (B5 cold freq 2!)
/// - `B3` (freq 50, `B1_ok`): promoted `r7 = r_y * 3`, trapping `r3 = 100 / r_x`, `r5 = r3 + r7`
/// - `B2` (freq 48, `r_x == 0`): promoted `r6 = r_y + 10`, `r5 = r6 * 2` (same dst `r5`!)
/// - `B4` (freq 98, join): `r8 = r5 + r2`, `mem[1] = r8`, `ret r8`
/// - `B5` (freq 2, cold exit): `r_err_val = -1`, `mem[1] = r_err_val`, `ret r_err_val`
fn sample_hyperblock_cfg() -> CfgProgram {
    let mut blocks = BTreeMap::new();
    blocks.insert(
        0,
        Block {
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
                        imm: 1,
                    },
                    latency: 1,
                },
            ],
            term: Term::BrZero {
                cond: "r_x".into(),
                if_zero: 2,
                if_nonzero: 1,
            },
        },
    );
    blocks.insert(
        1,
        Block {
            freq: 52,
            insts: Vec::new(),
            term: Term::BrZero {
                cond: "r_err".into(),
                if_zero: 3,
                if_nonzero: 5,
            },
        },
    );
    blocks.insert(
        2,
        Block {
            freq: 48,
            insts: vec![
                Inst {
                    op: Op::AddImm {
                        dst: "r6".into(),
                        src: "r_y".into(),
                        imm: 10,
                    },
                    latency: 1,
                },
                Inst {
                    op: Op::MulImm {
                        dst: "r5".into(),
                        src: "r6".into(),
                        imm: 2,
                    },
                    latency: 2,
                },
            ],
            term: Term::Jump(4),
        },
    );
    blocks.insert(
        3,
        Block {
            freq: 50,
            insts: vec![
                Inst {
                    op: Op::MulImm {
                        dst: "r7".into(),
                        src: "r_y".into(),
                        imm: 3,
                    },
                    latency: 2,
                },
                Inst {
                    op: Op::DivChecked {
                        dst: "r3".into(),
                        num: 100,
                        den: "r_x".into(),
                    },
                    latency: 3,
                },
                Inst {
                    op: Op::Add {
                        dst: "r5".into(),
                        lhs: "r3".into(),
                        rhs: "r7".into(),
                    },
                    latency: 1,
                },
            ],
            term: Term::Jump(4),
        },
    );
    blocks.insert(
        4,
        Block {
            freq: 98,
            insts: vec![
                Inst {
                    op: Op::Add {
                        dst: "r8".into(),
                        lhs: "r5".into(),
                        rhs: "r2".into(),
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
        },
    );
    blocks.insert(
        5,
        Block {
            freq: 2,
            insts: vec![
                Inst {
                    op: Op::Const {
                        dst: "r_fail".into(),
                        val: -1,
                    },
                    latency: 1,
                },
                Inst {
                    op: Op::Store {
                        slot: 1,
                        src: "r_fail".into(),
                    },
                    latency: 1,
                },
            ],
            term: Term::Ret("r_fail".into()),
        },
    );

    CfgProgram {
        blocks,
        entry: 0,
        params: vec!["r_x".into(), "r_y".into(), "r_err".into()],
    }
}

fn main() {
    let cfg = sample_hyperblock_cfg();
    let live_in = compute_liveness(&cfg);
    let hb_blocks = select_hyperblock(&cfg, 20);
    let mut hb = if_convert(&cfg, &hb_blocks);
    promote_predicates(&cfg, &mut hb, &live_in);

    let width = 2usize;
    let mispredict_penalty = 3usize;
    let sched = schedule_hyperblock(&hb, width);
    let bb_no_pen = expected_bb_cycles(&cfg, width, 0);
    let bb_with_pen = expected_bb_cycles(&cfg, width, mispredict_penalty);

    println!("=== Hardware-Predicated Execution & Hyperblock If-Conversion (§10.4) ===");
    println!(
        "selected hyperblock: {:?} (excluded cold exits: {:?})",
        hb.blocks
            .iter()
            .map(|b| format!("B{b}"))
            .collect::<Vec<_>>(),
        hb.side_exits
            .iter()
            .map(|b| format!("B{b}"))
            .collect::<Vec<_>>()
    );
    println!(
        "block predicates: {:?}",
        hb.block_pred
            .iter()
            .map(|(b, p)| (format!("B{b}"), format!("p{p}")))
            .collect::<BTreeMap<_, _>>()
    );
    println!(
        "expected BB cycles (no mispredict): {bb_no_pen:.2}, (3-cyc mispredict on cold arm): {bb_with_pen:.2}"
    );
    println!(
        "predicated hyperblock schedule (width={width}): {} cycles (removed {} false disjoint deps)",
        sched.total_cycles, sched.removed_false_deps
    );
    println!("\nScheduled VLIW hyperblock:");
    let mut by_cyc: BTreeMap<usize, Vec<&HInst>> = BTreeMap::new();
    for e in &sched.entries {
        by_cyc
            .entry(e.cycle)
            .or_default()
            .push(&hb.insts[e.inst_idx]);
    }
    for (c, ops) in by_cyc {
        let joined = ops
            .into_iter()
            .map(|h| h.to_string())
            .collect::<Vec<_>>()
            .join("  ||  ");
        println!("  cyc {c}: {joined}");
    }

    println!("\nEquivalence simulation (init mem=[50, 0]):");
    for (label, rx, ry, rerr) in [
        ("then_path (r_x=5, r_y=4, r_err=0)", 5, 4, 0),
        ("else_path (r_x=0, r_y=4, r_err=0)", 0, 4, 0),
        ("cold_exit (r_x=5, r_y=4, r_err=1)", 5, 4, 1),
    ] {
        let regs = [("r_x", rx), ("r_y", ry), ("r_err", rerr)];
        let mem = [50, 0];
        let orig = simulate_cfg(&cfg, &regs, &mem);
        let pred = simulate_hyperblock(&cfg, &hb, &sched, &regs, &mem);
        assert_eq!(orig.ret_val, pred.ret_val);
        assert_eq!(orig.mem, pred.mem);
        println!(
            "  {label}: ret={}, mem={:?}, traps={}, squashed_ops={}, side_exited={}",
            pred.ret_val, pred.mem, pred.traps, pred.squashed_ops, pred.side_exited
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hyperblock_if_conversion_matches_cfg_on_all_paths() {
        let cfg = sample_hyperblock_cfg();
        let live_in = compute_liveness(&cfg);
        let hb_blocks = select_hyperblock(&cfg, 20);
        assert_eq!(hb_blocks, BTreeSet::from([0, 1, 2, 3, 4]));

        let mut hb = if_convert(&cfg, &hb_blocks);
        promote_predicates(&cfg, &mut hb, &live_in);

        // Verify pure ops `r7 = r_y * 3` and `r6 = r_y + 10` promoted to p0,
        // while trapping `DivChecked` (`r3 = 100 / r_x`) and `Store` stayed guarded.
        let r7_inst = hb
            .insts
            .iter()
            .find(|i| matches!(&i.op, HOp::Core(Op::MulImm { dst, .. }) if dst == "r7"))
            .unwrap();
        let r6_inst = hb
            .insts
            .iter()
            .find(|i| matches!(&i.op, HOp::Core(Op::AddImm { dst, .. }) if dst == "r6"))
            .unwrap();
        let div_inst = hb
            .insts
            .iter()
            .find(|i| matches!(&i.op, HOp::Core(Op::DivChecked { .. })))
            .unwrap();
        let store_inst = hb
            .insts
            .iter()
            .find(|i| matches!(&i.op, HOp::Core(Op::Store { .. })))
            .unwrap();
        assert_eq!(r7_inst.pred, 0);
        assert!(r7_inst.promoted);
        assert_eq!(r6_inst.pred, 0);
        assert!(r6_inst.promoted);
        assert_ne!(div_inst.pred, 0);
        assert!(!div_inst.promoted);
        assert_ne!(store_inst.pred, 0);

        // Verify predicate disjointness: B2 (`p_zero`), B1_ok, and B5 (`p_exit`) are pairwise disjoint.
        let p_b2 = hb.block_pred[&2];
        let p_b3 = hb.block_pred[&3];
        let p_b4 = hb.block_pred[&4];
        assert!(hb.pred_tree.are_disjoint(p_b2, p_b3));
        assert!(!hb.pred_tree.are_disjoint(p_b3, p_b4));
        let exit_inst = hb
            .insts
            .iter()
            .find(|i| matches!(&i.op, HOp::SideExit { target: 5 }))
            .unwrap();
        assert!(hb.pred_tree.are_disjoint(exit_inst.pred, p_b4));

        let sched = schedule_hyperblock(&hb, 2);
        let bb_exp = expected_bb_cycles(&cfg, 2, 0);
        assert!(
            (sched.total_cycles as f64) < bb_exp,
            "hyperblock ({}) should beat expected BB cycles ({bb_exp:.2})",
            sched.total_cycles
        );
        assert!(sched.removed_false_deps > 0);

        // Exact equivalence across all 4 input combinations including r_x=0 (trap squash!) and r_err=1!
        for (rx, ry, rerr, expect_ret, expect_exit) in [
            (5, 4, 0, 83, false),
            (0, 4, 0, 79, false),
            (5, 4, 1, -1, true),
            (0, 4, 1, 79, false),
        ] {
            let regs = [("r_x", rx), ("r_y", ry), ("r_err", rerr)];
            let mem = [50, 0];
            let orig = simulate_cfg(&cfg, &regs, &mem);
            let pred = simulate_hyperblock(&cfg, &hb, &sched, &regs, &mem);
            assert_eq!(orig.ret_val, expect_ret);
            assert_eq!(pred.ret_val, orig.ret_val);
            assert_eq!(pred.mem, orig.mem);
            assert_eq!(pred.traps, 0);
            assert_eq!(pred.side_exited, expect_exit);
        }
    }

    #[test]
    fn predicate_promotion_blocked_when_dst_live_in_sibling_branch() {
        let mut blocks = BTreeMap::new();
        blocks.insert(
            0,
            Block {
                freq: 100,
                insts: Vec::new(),
                term: Term::BrZero {
                    cond: "r_cond".into(),
                    if_zero: 2,
                    if_nonzero: 1,
                },
            },
        );
        blocks.insert(
            1,
            Block {
                freq: 50,
                insts: vec![Inst {
                    op: Op::AddImm {
                        dst: "r_val".into(),
                        src: "r_in".into(),
                        imm: 10,
                    },
                    latency: 1,
                }],
                term: Term::Jump(3),
            },
        );
        blocks.insert(
            2,
            Block {
                freq: 50,
                insts: Vec::new(),
                term: Term::Jump(3),
            },
        );
        blocks.insert(
            3,
            Block {
                freq: 100,
                insts: Vec::new(),
                term: Term::Ret("r_val".into()),
            },
        );

        let cfg = CfgProgram {
            blocks,
            entry: 0,
            params: vec!["r_cond".into(), "r_in".into(), "r_val".into()],
        };
        let live_in = compute_liveness(&cfg);
        assert!(live_in[&2].contains("r_val"));

        let hb_blocks = select_hyperblock(&cfg, 20);
        let mut hb = if_convert(&cfg, &hb_blocks);
        promote_predicates(&cfg, &mut hb, &live_in);

        let add_inst = hb
            .insts
            .iter()
            .find(|i| matches!(&i.op, HOp::Core(Op::AddImm { .. })))
            .unwrap();
        assert_ne!(add_inst.pred, 0);
        assert!(!add_inst.promoted);

        let sched = schedule_hyperblock(&hb, 2);
        for (rc, rv_init, expect) in [(1, 99, 15), (0, 99, 99)] {
            let regs = [("r_cond", rc), ("r_in", 5), ("r_val", rv_init)];
            let orig = simulate_cfg(&cfg, &regs, &[]);
            let pred = simulate_hyperblock(&cfg, &hb, &sched, &regs, &[]);
            assert_eq!(orig.ret_val, expect);
            assert_eq!(pred.ret_val, orig.ret_val);
        }
    }

    #[test]
    fn disjoint_predicates_allow_concurrent_stores_and_waw_writes() {
        let mut blocks = BTreeMap::new();
        blocks.insert(
            0,
            Block {
                freq: 100,
                insts: Vec::new(),
                term: Term::BrZero {
                    cond: "r_cond".into(),
                    if_zero: 2,
                    if_nonzero: 1,
                },
            },
        );
        blocks.insert(
            1,
            Block {
                freq: 50,
                insts: vec![
                    Inst {
                        op: Op::Const {
                            dst: "r_out".into(),
                            val: 111,
                        },
                        latency: 1,
                    },
                    Inst {
                        op: Op::Store {
                            slot: 0,
                            src: "r_out".into(),
                        },
                        latency: 1,
                    },
                ],
                term: Term::Jump(3),
            },
        );
        blocks.insert(
            2,
            Block {
                freq: 50,
                insts: vec![
                    Inst {
                        op: Op::Const {
                            dst: "r_out".into(),
                            val: 222,
                        },
                        latency: 1,
                    },
                    Inst {
                        op: Op::Store {
                            slot: 0,
                            src: "r_out".into(),
                        },
                        latency: 1,
                    },
                ],
                term: Term::Jump(3),
            },
        );
        blocks.insert(
            3,
            Block {
                freq: 100,
                insts: Vec::new(),
                term: Term::Ret("r_out".into()),
            },
        );

        let cfg = CfgProgram {
            blocks,
            entry: 0,
            params: vec!["r_cond".into()],
        };
        let live_in = compute_liveness(&cfg);
        let hb_blocks = select_hyperblock(&cfg, 20);
        let mut hb = if_convert(&cfg, &hb_blocks);
        promote_predicates(&cfg, &mut hb, &live_in);

        let sched = schedule_hyperblock(&hb, 2);
        // Both `(p2) r_out = 111` and `(p1) r_out = 222` (plus both `mem[0] = r_out`)
        // have WAW/mem edges eliminated via predicate disjointness -> co-issue!
        assert!(sched.removed_false_deps >= 2);
        let const_cycles: Vec<usize> = sched
            .entries
            .iter()
            .filter(|e| matches!(&hb.insts[e.inst_idx].op, HOp::Core(Op::Const { .. })))
            .map(|e| e.cycle)
            .collect();
        assert_eq!(const_cycles[0], const_cycles[1]);

        for (rc, expect) in [(1, 111), (0, 222)] {
            let regs = [("r_cond", rc)];
            let mem = [0];
            let orig = simulate_cfg(&cfg, &regs, &mem);
            let pred = simulate_hyperblock(&cfg, &hb, &sched, &regs, &mem);
            assert_eq!(orig.ret_val, expect);
            assert_eq!(pred.ret_val, orig.ret_val);
            assert_eq!(pred.mem, [expect]);
        }
    }
}
