//! Modulo schedule inner loops with resource/recurrence bounds and modular variable expansion.

use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum ResourceUnit {
    Alu,
    Mem,
}

#[derive(Clone, Copy, Debug)]
enum OpKind {
    Load { dst: usize, base: usize },
    AddConst { dst: usize, src: usize, imm: i64 },
    MulConst { dst: usize, src: usize, imm: i64 },
    Accum { acc: usize, src: usize },
    Store { src: usize, base: usize },
}

#[derive(Clone, Debug)]
struct LoopOp {
    id: usize,
    name: &'static str,
    unit: ResourceUnit,
    latency: usize,
    kind: OpKind,
}

#[derive(Clone, Copy, Debug)]
struct DepEdge {
    from: usize,
    to: usize,
    latency: i64,
    dist: i64,
}

#[derive(Clone, Debug)]
struct LoopKernel {
    ops: Vec<LoopOp>,
    edges: Vec<DepEdge>,
    capacities: BTreeMap<ResourceUnit, usize>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ModuloSchedule {
    ii: usize,
    mii_res: usize,
    mii_rec: usize,
    start_cycles: Vec<usize>,
    num_stages: usize,
    unroll_factor: usize,
}

fn ceil_div(a: usize, b: usize) -> usize {
    a.div_ceil(b)
}

fn compute_mii_res(ops: &[LoopOp], capacities: &BTreeMap<ResourceUnit, usize>) -> usize {
    let mut counts: BTreeMap<ResourceUnit, usize> = BTreeMap::new();
    for op in ops {
        *counts.entry(op.unit).or_insert(0) += 1;
    }
    let mut max_res = 1;
    for (unit, count) in counts {
        let cap = *capacities.get(&unit).unwrap_or(&1);
        max_res = max_res.max(ceil_div(count, cap));
    }
    max_res
}

// Enumerate elementary cycles via Tarjan/Johnson-style bounded DFS from each start node.
fn dfs_cycles(
    start: usize,
    curr: usize,
    adj: &[Vec<DepEdge>],
    visited: &mut Vec<bool>,
    sum_l: i64,
    sum_d: i64,
    best_rec: &mut usize,
) {
    for &edge in &adj[curr] {
        let next_l = sum_l + edge.latency;
        let next_d = sum_d + edge.dist;
        if edge.to == start {
            if next_d > 0 && next_l > 0 {
                let bound = ceil_div(next_l as usize, next_d as usize);
                *best_rec = (*best_rec).max(bound);
            }
        } else if edge.to > start && !visited[edge.to] {
            visited[edge.to] = true;
            dfs_cycles(start, edge.to, adj, visited, next_l, next_d, best_rec);
            visited[edge.to] = false;
        }
    }
}

fn compute_mii_rec(n: usize, edges: &[DepEdge]) -> usize {
    let mut adj = vec![Vec::new(); n];
    for &e in edges {
        adj[e.from].push(e);
    }
    let mut best_rec = 1;
    let mut visited = vec![false; n];
    for start in 0..n {
        visited[start] = true;
        dfs_cycles(start, start, &adj, &mut visited, 0, 0, &mut best_rec);
        visited[start] = false;
    }
    best_rec
}

fn try_schedule_for_ii(
    kernel: &LoopKernel,
    ii: usize,
    idx: usize,
    max_cycle: usize,
    schedule: &mut [Option<usize>],
    res_table: &mut [BTreeMap<ResourceUnit, usize>],
) -> bool {
    let n = kernel.ops.len();
    if idx == n {
        return true;
    }

    // Compute tighter [earliest, latest] window from already-scheduled neighbors.
    let mut earliest = 0i64;
    let mut latest = max_cycle as i64;
    for &e in &kernel.edges {
        if e.to == idx {
            if let Some(su) = schedule[e.from] {
                earliest = earliest.max(su as i64 + e.latency - e.dist * (ii as i64));
            }
        }
        if e.from == idx {
            if let Some(sv) = schedule[e.to] {
                latest = latest.min(sv as i64 - e.latency + e.dist * (ii as i64));
            }
        }
    }

    let unit = kernel.ops[idx].unit;
    let cap = *kernel.capacities.get(&unit).unwrap_or(&1);

    let start = earliest.max(0) as usize;
    let end = latest.min(max_cycle as i64);
    if (start as i64) > end {
        return false;
    }

    for c in start..=(end as usize) {
        let slot = c % ii;
        let used = *res_table[slot].get(&unit).unwrap_or(&0);
        if used < cap {
            schedule[idx] = Some(c);
            *res_table[slot].entry(unit).or_insert(0) += 1;
            if try_schedule_for_ii(kernel, ii, idx + 1, max_cycle, schedule, res_table) {
                return true;
            }
            *res_table[slot].get_mut(&unit).unwrap() -= 1;
            schedule[idx] = None;
        }
    }
    false
}

fn modulo_schedule(kernel: &LoopKernel) -> ModuloSchedule {
    let mii_res = compute_mii_res(&kernel.ops, &kernel.capacities);
    let mii_rec = compute_mii_rec(kernel.ops.len(), &kernel.edges);
    let mut ii = mii_res.max(mii_rec);
    let total_latency: usize = kernel.ops.iter().map(|op| op.latency).sum();

    loop {
        let max_cycle = total_latency + ii * 2;
        let mut schedule = vec![None; kernel.ops.len()];
        let mut res_table = vec![BTreeMap::new(); ii];
        if try_schedule_for_ii(kernel, ii, 0, max_cycle, &mut schedule, &mut res_table) {
            let start_cycles: Vec<usize> = schedule.into_iter().map(|s| s.unwrap()).collect();
            let num_stages = start_cycles.iter().map(|&c| c / ii + 1).max().unwrap_or(1);

            // Compute inclusive register lifetime span across dependence edges plus op latency
            // so a producer at S(u) + II cannot overwrite cycle S(v) before the consumer reads it.
            let mut max_span = 0usize;
            for &e in &kernel.edges {
                let span = (start_cycles[e.to] as i64 + e.dist * (ii as i64)
                    - start_cycles[e.from] as i64)
                    .max(0) as usize;
                max_span = max_span.max(span);
            }
            for op in &kernel.ops {
                max_span = max_span.max(op.latency);
            }
            let unroll_factor = ceil_div(max_span + 1, ii).max(1);

            return ModuloSchedule {
                ii,
                mii_res,
                mii_rec,
                start_cycles,
                num_stages,
                unroll_factor,
            };
        }
        ii += 1;
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ScheduledInst {
    op_id: usize,
    name: &'static str,
    iter_offset: usize,
    mve_bank: usize,
    cycle_in_ii: usize,
}

#[derive(Clone, Debug)]
struct PipelinedProgram {
    schedule: ModuloSchedule,
    prologue: Vec<ScheduledInst>,
    kernel_body: Vec<Vec<ScheduledInst>>,
    epilogue: Vec<ScheduledInst>,
}

fn build_pipelined_program(
    kernel: &LoopKernel,
    sched: ModuloSchedule,
    trip_count: usize,
) -> PipelinedProgram {
    let stages = sched.num_stages;
    let ii = sched.ii;
    let unroll = sched.unroll_factor;

    // Emit prologue for steady-state ramp-up (stages - 1 beats of II cycles) when trip_count >= stages.
    let mut prologue = Vec::new();
    let mut epilogue = Vec::new();
    let mut kernel_body = Vec::new();

    if trip_count < stages {
        // Fallback: small trip count fits completely in flattened schedule without steady kernel.
        let mut by_cycle: BTreeMap<usize, Vec<(usize, usize)>> = BTreeMap::new();
        for iter in 0..trip_count {
            for op in &kernel.ops {
                let cyc = iter * ii + sched.start_cycles[op.id];
                by_cycle.entry(cyc).or_default().push((iter, op.id));
            }
        }
        for (cyc, items) in by_cycle {
            for (iter, op_id) in items {
                prologue.push(ScheduledInst {
                    op_id,
                    name: kernel.ops[op_id].name,
                    iter_offset: iter,
                    mve_bank: iter % unroll,
                    cycle_in_ii: cyc,
                });
            }
        }
        return PipelinedProgram {
            schedule: sched,
            prologue,
            kernel_body,
            epilogue,
        };
    }

    // Prologue: II * (stages - 1) cycles initiating iterations 0..(stages - 1).
    for beat in 0..(stages - 1) {
        for c_off in 0..ii {
            for op in &kernel.ops {
                let stage = sched.start_cycles[op.id] / ii;
                if sched.start_cycles[op.id] % ii == c_off && stage <= beat {
                    let iter = beat - stage;
                    prologue.push(ScheduledInst {
                        op_id: op.id,
                        name: op.name,
                        iter_offset: iter,
                        mve_bank: iter % unroll,
                        cycle_in_ii: beat * ii + c_off,
                    });
                }
            }
        }
    }

    // Unrolled kernel copies `unroll` consecutive II-beats with explicit MVE bank renaming.
    for u in 0..unroll {
        let mut beat_ops = Vec::new();
        for c_off in 0..ii {
            for op in &kernel.ops {
                if sched.start_cycles[op.id] % ii == c_off {
                    let stage = sched.start_cycles[op.id] / ii;
                    // Inside kernel beat, iteration for `stage` shifts by `u`.
                    let iter_mod = (stages - 1 + u + unroll - stage % unroll) % unroll;
                    beat_ops.push(ScheduledInst {
                        op_id: op.id,
                        name: op.name,
                        iter_offset: stage,
                        mve_bank: iter_mod,
                        cycle_in_ii: c_off,
                    });
                }
            }
        }
        kernel_body.push(beat_ops);
    }

    // Epilogue: finish remaining stages for final (stages - 1) iterations.
    for beat in 0..(stages - 1) {
        for c_off in 0..ii {
            for op in &kernel.ops {
                let stage = sched.start_cycles[op.id] / ii;
                if sched.start_cycles[op.id] % ii == c_off && stage > beat {
                    let iter = trip_count - stage + beat;
                    epilogue.push(ScheduledInst {
                        op_id: op.id,
                        name: op.name,
                        iter_offset: iter,
                        mve_bank: iter % unroll,
                        cycle_in_ii: beat * ii + c_off,
                    });
                }
            }
        }
    }

    PipelinedProgram {
        schedule: sched,
        prologue,
        kernel_body,
        epilogue,
    }
}

fn eval_op(
    op: &LoopOp,
    iter: usize,
    mve_bank: usize,
    mem: &mut [Vec<i64>],
    regs: &mut BTreeMap<(usize, usize), i64>,
    accum: &mut BTreeMap<usize, i64>,
) {
    match op.kind {
        OpKind::Load { dst, base } => {
            let val = mem[base][iter];
            regs.insert((dst, mve_bank), val);
        }
        OpKind::AddConst { dst, src, imm } => {
            let val = regs[&(src, mve_bank)] + imm;
            regs.insert((dst, mve_bank), val);
        }
        OpKind::MulConst { dst, src, imm } => {
            let val = regs[&(src, mve_bank)] * imm;
            regs.insert((dst, mve_bank), val);
        }
        OpKind::Accum { acc, src } => {
            let prev = *accum.get(&acc).unwrap_or(&0);
            let val = prev + regs[&(src, mve_bank)];
            accum.insert(acc, val);
            regs.insert((acc, mve_bank), val);
        }
        OpKind::Store { src, base } => {
            let val = regs[&(src, mve_bank)];
            mem[base][iter] = val;
        }
    }
}

fn execute_sequential(
    kernel: &LoopKernel,
    mut mem: Vec<Vec<i64>>,
    trip_count: usize,
) -> (Vec<Vec<i64>>, BTreeMap<usize, i64>) {
    let mut regs = BTreeMap::new();
    let mut accum = BTreeMap::new();
    for iter in 0..trip_count {
        for op in &kernel.ops {
            eval_op(op, iter, 0, &mut mem, &mut regs, &mut accum);
        }
    }
    (mem, accum)
}

fn execute_pipelined(
    kernel: &LoopKernel,
    prog: &PipelinedProgram,
    mut mem: Vec<Vec<i64>>,
    trip_count: usize,
) -> (Vec<Vec<i64>>, BTreeMap<usize, i64>) {
    let mut regs = BTreeMap::new();
    let mut accum = BTreeMap::new();
    let stages = prog.schedule.num_stages;

    if trip_count < stages {
        for inst in &prog.prologue {
            eval_op(
                &kernel.ops[inst.op_id],
                inst.iter_offset,
                inst.mve_bank,
                &mut mem,
                &mut regs,
                &mut accum,
            );
        }
        return (mem, accum);
    }

    for inst in &prog.prologue {
        eval_op(
            &kernel.ops[inst.op_id],
            inst.iter_offset,
            inst.mve_bank,
            &mut mem,
            &mut regs,
            &mut accum,
        );
    }

    let kernel_beats = trip_count - (stages - 1);
    let unroll = prog.schedule.unroll_factor;
    for k in 0..kernel_beats {
        let beat_template = &prog.kernel_body[k % unroll];
        for inst in beat_template {
            let iter = k + (stages - 1) - inst.iter_offset;
            eval_op(
                &kernel.ops[inst.op_id],
                iter,
                iter % unroll,
                &mut mem,
                &mut regs,
                &mut accum,
            );
        }
    }

    for inst in &prog.epilogue {
        eval_op(
            &kernel.ops[inst.op_id],
            inst.iter_offset,
            inst.mve_bank,
            &mut mem,
            &mut regs,
            &mut accum,
        );
    }

    (mem, accum)
}

fn resource_bound_kernel() -> LoopKernel {
    // A[i] -> +1 -> *2 -> Store B[i]; two ALU ops on 1 ALU => MII_res = 2, MII_rec = 1.
    let ops = vec![
        LoopOp {
            id: 0,
            name: "ld_a",
            unit: ResourceUnit::Mem,
            latency: 2,
            kind: OpKind::Load { dst: 0, base: 0 },
        },
        LoopOp {
            id: 1,
            name: "add1",
            unit: ResourceUnit::Alu,
            latency: 1,
            kind: OpKind::AddConst {
                dst: 1,
                src: 0,
                imm: 1,
            },
        },
        LoopOp {
            id: 2,
            name: "mul2",
            unit: ResourceUnit::Alu,
            latency: 1,
            kind: OpKind::MulConst {
                dst: 2,
                src: 1,
                imm: 2,
            },
        },
        LoopOp {
            id: 3,
            name: "st_b",
            unit: ResourceUnit::Mem,
            latency: 1,
            kind: OpKind::Store { src: 2, base: 1 },
        },
    ];
    let edges = vec![
        DepEdge {
            from: 0,
            to: 1,
            latency: 2,
            dist: 0,
        },
        DepEdge {
            from: 1,
            to: 2,
            latency: 1,
            dist: 0,
        },
        DepEdge {
            from: 2,
            to: 3,
            latency: 1,
            dist: 0,
        },
    ];
    let mut capacities = BTreeMap::new();
    capacities.insert(ResourceUnit::Alu, 1);
    capacities.insert(ResourceUnit::Mem, 2);
    LoopKernel {
        ops,
        edges,
        capacities,
    }
}

fn recurrence_bound_kernel() -> LoopKernel {
    // Load A[i] -> MulConst *3 -> Accumulate into s (3-cycle recurrence, dist = 1) -> Store s into B[i].
    let ops = vec![
        LoopOp {
            id: 0,
            name: "ld_a",
            unit: ResourceUnit::Mem,
            latency: 2,
            kind: OpKind::Load { dst: 0, base: 0 },
        },
        LoopOp {
            id: 1,
            name: "mul3",
            unit: ResourceUnit::Alu,
            latency: 1,
            kind: OpKind::MulConst {
                dst: 1,
                src: 0,
                imm: 3,
            },
        },
        LoopOp {
            id: 2,
            name: "accum",
            unit: ResourceUnit::Alu,
            latency: 3,
            kind: OpKind::Accum { acc: 2, src: 1 },
        },
        LoopOp {
            id: 3,
            name: "st_b",
            unit: ResourceUnit::Mem,
            latency: 1,
            kind: OpKind::Store { src: 2, base: 1 },
        },
    ];
    let edges = vec![
        DepEdge {
            from: 0,
            to: 1,
            latency: 2,
            dist: 0,
        },
        DepEdge {
            from: 1,
            to: 2,
            latency: 1,
            dist: 0,
        },
        DepEdge {
            from: 2,
            to: 2,
            latency: 3,
            dist: 1,
        }, // cross-iteration recurrence D=1, L=3 => MII_rec=3
        DepEdge {
            from: 2,
            to: 3,
            latency: 3,
            dist: 0,
        },
    ];
    let mut capacities = BTreeMap::new();
    capacities.insert(ResourceUnit::Alu, 2);
    capacities.insert(ResourceUnit::Mem, 2);
    LoopKernel {
        ops,
        edges,
        capacities,
    }
}

fn verify_schedule_constraints(kernel: &LoopKernel, sched: &ModuloSchedule) {
    let ii = sched.ii;
    let mut table: BTreeMap<(usize, ResourceUnit), usize> = BTreeMap::new();
    for op in &kernel.ops {
        let slot = sched.start_cycles[op.id] % ii;
        *table.entry((slot, op.unit)).or_insert(0) += 1;
        let cap = *kernel.capacities.get(&op.unit).unwrap_or(&1);
        assert!(table[&(slot, op.unit)] <= cap);
    }
    for &e in &kernel.edges {
        let su = sched.start_cycles[e.from] as i64;
        let sv = sched.start_cycles[e.to] as i64;
        assert!(su + e.latency - e.dist * (ii as i64) <= sv);
    }
}

fn main() {
    let k_res = resource_bound_kernel();
    let s_res = modulo_schedule(&k_res);
    verify_schedule_constraints(&k_res, &s_res);
    let p_res = build_pipelined_program(&k_res, s_res.clone(), 8);
    let mem_res = vec![vec![1, 2, 3, 4, 5, 6, 7, 8], vec![0; 8]];
    let (seq_mem, _) = execute_sequential(&k_res, mem_res.clone(), 8);
    let (pipe_mem, _) = execute_pipelined(&k_res, &p_res, mem_res, 8);
    println!(
        "Resource-bound kernel: MII_res={}, MII_rec={}, II={}, stages={}, MVE unroll={}",
        s_res.mii_res, s_res.mii_rec, s_res.ii, s_res.num_stages, s_res.unroll_factor
    );
    println!(
        "  cycles={:?}, match_seq={}",
        s_res.start_cycles,
        seq_mem == pipe_mem
    );

    let k_rec = recurrence_bound_kernel();
    let s_rec = modulo_schedule(&k_rec);
    verify_schedule_constraints(&k_rec, &s_rec);
    let p_rec = build_pipelined_program(&k_rec, s_rec.clone(), 8);
    let mem_rec = vec![vec![2, 4, 1, 5, 3, 6, 2, 1], vec![0; 8]];
    let (seq_rec, seq_acc) = execute_sequential(&k_rec, mem_rec.clone(), 8);
    let (pipe_rec, pipe_acc) = execute_pipelined(&k_rec, &p_rec, mem_rec, 8);
    let mve_banks: BTreeSet<usize> = p_rec
        .kernel_body
        .iter()
        .flatten()
        .map(|i| i.mve_bank)
        .collect();
    println!(
        "Recurrence-bound kernel: MII_res={}, MII_rec={}, II={}, stages={}, MVE banks={:?}",
        s_rec.mii_res, s_rec.mii_rec, s_rec.ii, s_rec.num_stages, mve_banks
    );
    println!(
        "  cycles={:?}, final_out={:?}, match_seq={}",
        s_rec.start_cycles,
        pipe_rec[1],
        seq_rec == pipe_rec && seq_acc == pipe_acc
    );
}

#[test]
fn resource_bound_modulo_schedule_matches_sequential() {
    let kernel = resource_bound_kernel();
    let sched = modulo_schedule(&kernel);
    assert_eq!(sched.mii_res, 2);
    assert_eq!(sched.mii_rec, 1);
    assert_eq!(sched.ii, 2);
    verify_schedule_constraints(&kernel, &sched);

    let mem = vec![vec![10, 20, 30, 40, 50, 60, 70], vec![0; 7]];
    let prog = build_pipelined_program(&kernel, sched, 7);
    let (seq, _) = execute_sequential(&kernel, mem.clone(), 7);
    let (pipe, _) = execute_pipelined(&kernel, &prog, mem, 7);
    assert_eq!(pipe, seq);
    assert_eq!(pipe[1], vec![22, 42, 62, 82, 102, 122, 142]);
}

#[test]
fn recurrence_bound_modulo_schedule_and_mve_preserve_semantics() {
    let kernel = recurrence_bound_kernel();
    let sched = modulo_schedule(&kernel);
    assert_eq!(sched.mii_res, 1);
    assert_eq!(sched.mii_rec, 3);
    assert_eq!(sched.ii, 3);
    verify_schedule_constraints(&kernel, &sched);

    let mem = vec![vec![1, 2, 3, 4, 5, 6, 7, 8], vec![0; 8]];
    let prog = build_pipelined_program(&kernel, sched, 8);
    let (seq_mem, seq_acc) = execute_sequential(&kernel, mem.clone(), 8);
    let (pipe_mem, pipe_acc) = execute_pipelined(&kernel, &prog, mem, 8);
    assert_eq!(pipe_mem, seq_mem);
    assert_eq!(pipe_acc, seq_acc);
    assert_eq!(pipe_mem[1], vec![3, 9, 18, 30, 45, 63, 84, 108]);
}
