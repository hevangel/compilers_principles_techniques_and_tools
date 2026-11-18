//! Backward next-use/liveness pass, register/address descriptors, getReg codegen, and block-exit flush.

use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BinOp {
    Add,
    Sub,
}

/// Three-address instruction (quadruple): either `dst = arg1 op arg2` or copy `dst = arg1`.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Quad {
    dst: String,
    op: Option<BinOp>,
    arg1: String,
    arg2: Option<String>,
}

/// Liveness and next-use metadata for one variable *after* a given instruction.
#[derive(Clone, Debug, PartialEq, Eq)]
struct VarUseInfo {
    /// True if the value is read later in this block or belongs to `live_on_exit`.
    live: bool,
    /// Index of the next instruction in the block that reads this value (`None` if only live on exit).
    next_use: Option<usize>,
}

/// Per-instruction snapshot of `(live, next_use)` attached to `dst`, `arg1`, and `arg2`.
#[derive(Clone, Debug)]
struct InstrLiveness {
    dst_info: VarUseInfo,
    arg1_info: VarUseInfo,
    arg2_info: Option<VarUseInfo>,
}

/// Possible storage locations tracked in `addr_desc`.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Location {
    Reg(usize),
    Mem(String),
}

/// Target RISC-like machine instructions over `num_regs` registers and named memory homes.
#[derive(Clone, Debug, PartialEq, Eq)]
enum RiscInstr {
    Ld { dst: usize, mem: String },
    St { src: usize, mem: String },
    Mov { dst: usize, src: usize },
    Add { dst: usize, lhs: usize, rhs: usize },
    Sub { dst: usize, lhs: usize, rhs: usize },
}

/// Dragon Book §8.4.2 backward pass: scan quads in reverse starting from `live_on_exit`
/// to attach `(live, next_use)` information to every instruction's operands and destination.
fn compute_next_use(quads: &[Quad], live_on_exit: &BTreeSet<String>) -> Vec<InstrLiveness> {
    // At block exit, variables in `live_on_exit` are live with no in-block next use.
    let mut table: BTreeMap<String, VarUseInfo> = BTreeMap::new();
    for v in live_on_exit {
        table.insert(
            v.clone(),
            VarUseInfo {
                live: true,
                next_use: None,
            },
        );
    }
    let dead = || VarUseInfo {
        live: false,
        next_use: None,
    };

    let mut out = vec![
        InstrLiveness {
            dst_info: dead(),
            arg1_info: dead(),
            arg2_info: None,
        };
        quads.len()
    ];

    for (idx, q) in quads.iter().enumerate().rev() {
        // Record the liveness/next-use state *after* instruction `idx` executes.
        let dst_info = table.get(&q.dst).cloned().unwrap_or_else(dead);
        let arg1_info = table.get(&q.arg1).cloned().unwrap_or_else(dead);
        let arg2_info = q
            .arg2
            .as_ref()
            .map(|a2| table.get(a2).cloned().unwrap_or_else(dead));

        out[idx] = InstrLiveness {
            dst_info,
            arg1_info,
            arg2_info,
        };

        // Update backward state for instructions *before* `idx`:
        // Kill `dst` FIRST (it is overwritten here), then mark `arg1`/`arg2` live at `idx`
        // so self-updates like `a = a + b` keep `a` live before `idx`.
        table.insert(q.dst.clone(), dead());
        table.insert(
            q.arg1.clone(),
            VarUseInfo {
                live: true,
                next_use: Some(idx),
            },
        );
        if let Some(a2) = &q.arg2 {
            table.insert(
                a2.clone(),
                VarUseInfo {
                    live: true,
                    next_use: Some(idx),
                },
            );
        }
    }
    out
}

/// Machine descriptor state maintained while translating one basic block forward.
struct CodegenState {
    /// `reg_desc[r]`: set of variables whose current value resides in register `r`.
    reg_desc: Vec<BTreeSet<String>>,
    /// `addr_desc[v]`: set of locations (Reg and/or Mem) holding `v`'s up-to-date value.
    addr_desc: BTreeMap<String, BTreeSet<Location>>,
    code: Vec<RiscInstr>,
}

impl CodegenState {
    fn new(num_regs: usize, vars: &BTreeSet<String>) -> Self {
        // Initially every variable lives only in its home memory slot.
        let mut addr_desc = BTreeMap::new();
        for v in vars {
            addr_desc.insert(v.clone(), BTreeSet::from([Location::Mem(v.clone())]));
        }
        Self {
            reg_desc: vec![BTreeSet::new(); num_regs],
            addr_desc,
            code: Vec::new(),
        }
    }

    fn var_in_reg(&self, var: &str) -> Option<usize> {
        self.reg_desc.iter().position(|vars| vars.contains(var))
    }

    /// Dissociate every variable currently held in register `r` from `Reg(r)`.
    fn clear_reg(&mut self, r: usize) {
        let holding: Vec<String> = self.reg_desc[r].iter().cloned().collect();
        for v in holding {
            if let Some(locs) = self.addr_desc.get_mut(&v) {
                locs.remove(&Location::Reg(r));
            }
        }
        self.reg_desc[r].clear();
    }

    /// Evict register `r`, emitting `St` only for variables that are still live AND have no other copy.
    fn spill_reg(&mut self, r: usize, future_live: &BTreeMap<String, usize>) {
        let holding: Vec<String> = self.reg_desc[r].iter().cloned().collect();
        for v in holding {
            let locs = self.addr_desc.get(&v).cloned().unwrap_or_default();
            let has_other_copy = locs.iter().any(|loc| *loc != Location::Reg(r));
            if !has_other_copy && future_live.contains_key(&v) {
                self.code.push(RiscInstr::St {
                    src: r,
                    mem: v.clone(),
                });
                self.addr_desc
                    .entry(v.clone())
                    .or_default()
                    .insert(Location::Mem(v));
            }
        }
        self.clear_reg(r);
    }

    /// Select a register to allocate (never picking `exclude`), minimizing spill cost:
    /// 1. Empty register (zero cost).
    /// 2. Register whose variables are dead or already duplicated in `Mem`/another `Reg` (zero spill).
    /// 3. Register whose soonest next-use is farthest in the future (Belady-style spill victim).
    fn pick_victim(
        &mut self,
        exclude: Option<usize>,
        future_live: &BTreeMap<String, usize>,
    ) -> usize {
        // 1. Any empty register.
        for (r, vars) in self.reg_desc.iter().enumerate() {
            if Some(r) != exclude && vars.is_empty() {
                return r;
            }
        }
        // 2. Register whose variables all already exist elsewhere (memory or another reg) or are dead.
        for (r, vars) in self.reg_desc.iter().enumerate() {
            if Some(r) == exclude {
                continue;
            }
            let safe = vars.iter().all(|v| {
                !future_live.contains_key(v)
                    || self.addr_desc[v].iter().any(|l| *l != Location::Reg(r))
            });
            if safe {
                self.clear_reg(r);
                return r;
            }
        }
        // 3. Farthest next-use victim: maximizes distance until a reload `Ld` will be needed.
        let victim = (0..self.reg_desc.len())
            .filter(|&r| Some(r) != exclude)
            .max_by_key(|&r| {
                self.reg_desc[r]
                    .iter()
                    .map(|v| future_live.get(v).copied().unwrap_or(usize::MAX))
                    .min()
                    .unwrap_or(usize::MAX)
            })
            .unwrap();
        self.spill_reg(victim, future_live);
        victim
    }

    /// Return a register holding `var`, loading it from `Mem` if needed without evicting `exclude`.
    fn ensure_in_reg(
        &mut self,
        var: &str,
        exclude: Option<usize>,
        future_live: &BTreeMap<String, usize>,
    ) -> usize {
        if let Some(r) = self.var_in_reg(var) {
            return r;
        }
        let r = self.pick_victim(exclude, future_live);
        self.code.push(RiscInstr::Ld {
            dst: r,
            mem: var.to_string(),
        });
        self.reg_desc[r].insert(var.to_string());
        self.addr_desc
            .entry(var.to_string())
            .or_default()
            .insert(Location::Reg(r));
        r
    }

    /// Choose the destination register `rz` for `q` (Dragon Book `getReg` §8.6.3).
    fn get_reg_for_dst(
        &mut self,
        q: &Quad,
        info: &InstrLiveness,
        rx: usize,
        future_live: &BTreeMap<String, usize>,
    ) -> usize {
        // Reuse `rx` if `arg1` is the sole occupant of `rx` and either `arg1 == dst` or `arg1` dies here.
        if (q.arg1 == q.dst || !info.arg1_info.live)
            && self.reg_desc[rx].len() == 1
            && self.reg_desc[rx].contains(&q.arg1)
        {
            return rx;
        }
        self.pick_victim(None, future_live)
    }
}

/// Translate one basic block of 3-address quads into RISC instructions using `num_regs` registers.
fn generate_block(
    quads: &[Quad],
    num_regs: usize,
    live_on_exit: &BTreeSet<String>,
) -> Vec<RiscInstr> {
    let liveness = compute_next_use(quads, live_on_exit);
    let mut vars = live_on_exit.clone();
    for q in quads {
        vars.insert(q.dst.clone());
        vars.insert(q.arg1.clone());
        if let Some(a2) = &q.arg2 {
            vars.insert(a2.clone());
        }
    }

    let mut state = CodegenState::new(num_regs, &vars);

    for (idx, q) in quads.iter().enumerate() {
        // Compute what is live *after* instruction idx and each live variable's earliest next use
        // (for spill victim selection).
        let mut live_after: BTreeMap<String, usize> = live_on_exit
            .iter()
            .map(|v| (v.clone(), usize::MAX / 2))
            .collect();
        for later in idx + 1..quads.len() {
            // Only record a read at `later` if the variable wasn't redefined in `(idx+1..later)`.
            for name in [&quads[later].arg1]
                .into_iter()
                .chain(quads[later].arg2.iter())
            {
                let overwritten_earlier = quads
                    .iter()
                    .take(later)
                    .skip(idx + 1)
                    .any(|mid_q| &mid_q.dst == name);
                if !overwritten_earlier {
                    live_after.entry(name.clone()).or_insert(later);
                }
            }
        }
        // A variable in `live_on_exit` whose old value is killed before any read in `(idx+1..)`
        // does NOT need its pre-`idx` value preserved past `idx`.
        for mid in idx + 1..quads.len() {
            if !live_after.get(&quads[mid].dst).is_some_and(|&u| u < mid)
                && live_on_exit.contains(&quads[mid].dst)
            {
                let used_before = (idx + 1..mid).any(|j| {
                    quads[j].arg1 == quads[mid].dst
                        || quads[j].arg2.as_ref() == Some(&quads[mid].dst)
                });
                if !used_before {
                    live_after.remove(&quads[mid].dst);
                }
            }
        }

        // While loading operands for `idx`, `arg1` and `arg2` must not be spilled before `idx` runs.
        let mut live_during_load = live_after.clone();
        live_during_load.insert(q.arg1.clone(), idx);
        if let Some(a2) = &q.arg2 {
            live_during_load.insert(a2.clone(), idx);
        }

        let rx = state.ensure_in_reg(&q.arg1, None, &live_during_load);
        if let (Some(op), Some(a2)) = (q.op, &q.arg2) {
            // Pass `Some(rx)` as `exclude` so loading `a2` cannot evict `arg1` from `rx`.
            let ry = state.ensure_in_reg(a2, Some(rx), &live_during_load);
            let rz = state.get_reg_for_dst(q, &liveness[idx], rx, &live_after);
            let instr = match op {
                BinOp::Add => RiscInstr::Add {
                    dst: rz,
                    lhs: rx,
                    rhs: ry,
                },
                BinOp::Sub => RiscInstr::Sub {
                    dst: rz,
                    lhs: rx,
                    rhs: ry,
                },
            };
            state.code.push(instr);
            // `rz` now holds ONLY `q.dst`; any old values in `rz` and any stale locations of `q.dst` are cleared.
            state.clear_reg(rz);
            state.reg_desc[rz].insert(q.dst.clone());
            for (r, set) in state.reg_desc.iter_mut().enumerate() {
                if r != rz {
                    set.remove(&q.dst);
                }
            }
            state
                .addr_desc
                .insert(q.dst.clone(), BTreeSet::from([Location::Reg(rz)]));
        } else {
            // Copy `dst = arg1`: if `rz == rx` and `arg1` remains live, both share `rx` with no `Mov`!
            let rz = state.get_reg_for_dst(q, &liveness[idx], rx, &live_after);
            if rz != rx {
                state.code.push(RiscInstr::Mov { dst: rz, src: rx });
            }
            for (r, set) in state.reg_desc.iter_mut().enumerate() {
                if r != rz {
                    set.remove(&q.dst);
                }
            }
            if !state.reg_desc[rz].contains(&q.dst) {
                if rz == rx && info_copy_preserve(&liveness[idx], &q.arg1) {
                    // Both `arg1` and `dst` alias the same register `rz`.
                    state.reg_desc[rz].insert(q.dst.clone());
                } else {
                    state.clear_reg(rz);
                    state.reg_desc[rz].insert(q.dst.clone());
                }
            }
            state
                .addr_desc
                .insert(q.dst.clone(), BTreeSet::from([Location::Reg(rz)]));
        }
    }

    // Block-exit flush: store any `live_on_exit` variable whose memory home is stale (dead temps skipped).
    for v in live_on_exit {
        let locs = &state.addr_desc[v];
        if !locs.contains(&Location::Mem(v.clone())) {
            if let Some(Location::Reg(r)) = locs.iter().next().cloned() {
                state.code.push(RiscInstr::St {
                    src: r,
                    mem: v.clone(),
                });
            }
        }
    }
    state.code
}

fn info_copy_preserve(info: &InstrLiveness, _arg: &str) -> bool {
    info.arg1_info.live
}

fn eval_quads(quads: &[Quad], initial: &[(&str, i64)]) -> BTreeMap<String, i64> {
    let mut mem: BTreeMap<String, i64> = initial.iter().map(|&(k, v)| (k.to_string(), v)).collect();
    for q in quads {
        let v1 = *mem.get(&q.arg1).unwrap_or(&0);
        let res = match (q.op, &q.arg2) {
            (Some(BinOp::Add), Some(a2)) => v1.wrapping_add(*mem.get(a2).unwrap_or(&0)),
            (Some(BinOp::Sub), Some(a2)) => v1.wrapping_sub(*mem.get(a2).unwrap_or(&0)),
            _ => v1,
        };
        mem.insert(q.dst.clone(), res);
    }
    mem
}

fn eval_risc(
    code: &[RiscInstr],
    num_regs: usize,
    initial: &[(&str, i64)],
) -> BTreeMap<String, i64> {
    let mut mem: BTreeMap<String, i64> = initial.iter().map(|&(k, v)| (k.to_string(), v)).collect();
    let mut regs = vec![0i64; num_regs];
    for ins in code {
        match ins {
            RiscInstr::Ld { dst, mem: m } => regs[*dst] = *mem.get(m).unwrap_or(&0),
            RiscInstr::St { src, mem: m } => {
                mem.insert(m.clone(), regs[*src]);
            }
            RiscInstr::Mov { dst, src } => regs[*dst] = regs[*src],
            RiscInstr::Add { dst, lhs, rhs } => regs[*dst] = regs[*lhs].wrapping_add(regs[*rhs]),
            RiscInstr::Sub { dst, lhs, rhs } => regs[*dst] = regs[*lhs].wrapping_sub(regs[*rhs]),
        }
    }
    mem
}

fn textbook_block() -> (Vec<Quad>, BTreeSet<String>) {
    // t = a - b; u = a - c; v = t + u; a = d; d = v + u
    let quads = vec![
        Quad {
            dst: "t".into(),
            op: Some(BinOp::Sub),
            arg1: "a".into(),
            arg2: Some("b".into()),
        },
        Quad {
            dst: "u".into(),
            op: Some(BinOp::Sub),
            arg1: "a".into(),
            arg2: Some("c".into()),
        },
        Quad {
            dst: "v".into(),
            op: Some(BinOp::Add),
            arg1: "t".into(),
            arg2: Some("u".into()),
        },
        Quad {
            dst: "a".into(),
            op: None,
            arg1: "d".into(),
            arg2: None,
        },
        Quad {
            dst: "d".into(),
            op: Some(BinOp::Add),
            arg1: "v".into(),
            arg2: Some("u".into()),
        },
    ];
    let live_out = BTreeSet::from(["a".to_string(), "d".to_string()]);
    (quads, live_out)
}

fn main() {
    let (quads, live_out) = textbook_block();
    let liveness = compute_next_use(&quads, &live_out);
    for (i, info) in liveness.iter().enumerate() {
        println!(
            "q{i}: dst={:?}, arg1={:?}, arg2={:?}",
            info.dst_info, info.arg1_info, info.arg2_info
        );
    }
    let code_3reg = generate_block(&quads, 3, &live_out);
    let code_2reg = generate_block(&quads, 2, &live_out);
    println!(
        "Generated 3-reg ({} instrs): {code_3reg:?}",
        code_3reg.len()
    );
    println!(
        "Generated 2-reg ({} instrs): {code_2reg:?}",
        code_2reg.len()
    );

    let init = [("a", 10), ("b", 3), ("c", 2), ("d", 7)];
    let expected = eval_quads(&quads, &init);
    let mem3 = eval_risc(&code_3reg, 3, &init);
    let mem2 = eval_risc(&code_2reg, 2, &init);
    println!(
        "Live outputs: ref a={},d={} | 3-reg a={},d={} | 2-reg a={},d={}",
        expected["a"], expected["d"], mem3["a"], mem3["d"], mem2["a"], mem2["d"]
    );
}

#[test]
fn three_reg_and_two_reg_spill_match_three_address_semantics() {
    let (quads, live_out) = textbook_block();
    let init = [("a", 25), ("b", 9), ("c", 4), ("d", 11)];
    let expected = eval_quads(&quads, &init);

    for num_regs in [2usize, 3] {
        let code = generate_block(&quads, num_regs, &live_out);
        let mem = eval_risc(&code, num_regs, &init);
        for v in &live_out {
            assert_eq!(mem[v], expected[v], "mismatch for {v} with {num_regs} regs");
        }
    }
}

#[test]
fn forces_spill_and_reload_under_pressure_with_two_registers() {
    // Interleaved additions forcing spills when only 2 registers exist.
    let quads = vec![
        Quad {
            dst: "t1".into(),
            op: Some(BinOp::Add),
            arg1: "a".into(),
            arg2: Some("b".into()),
        },
        Quad {
            dst: "t2".into(),
            op: Some(BinOp::Add),
            arg1: "c".into(),
            arg2: Some("d".into()),
        },
        Quad {
            dst: "t3".into(),
            op: Some(BinOp::Add),
            arg1: "e".into(),
            arg2: Some("f".into()),
        },
        Quad {
            dst: "out1".into(),
            op: Some(BinOp::Sub),
            arg1: "t1".into(),
            arg2: Some("t2".into()),
        },
        Quad {
            dst: "out".into(),
            op: Some(BinOp::Add),
            arg1: "out1".into(),
            arg2: Some("t3".into()),
        },
    ];
    let live_out = BTreeSet::from(["out".to_string()]);
    let code = generate_block(&quads, 2, &live_out);
    assert!(code
        .iter()
        .any(|ins| matches!(ins, RiscInstr::St { mem, .. } if mem == "t1" || mem == "t2")));
    let init = [("a", 10), ("b", 20), ("c", 3), ("d", 4), ("e", 5), ("f", 6)];
    let expected = eval_quads(&quads, &init);
    let mem = eval_risc(&code, 2, &init);
    assert_eq!(mem["out"], expected["out"]);
}
