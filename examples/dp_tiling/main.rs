//! Bottom-up DP tree tiling with multi-register cost vectors and sliding-window peephole optimization.

use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Eq)]
enum Expr {
    Mem(String),
    Imm(i64),
    Add(Box<Expr>, Box<Expr>),
    Mul(Box<Expr>, Box<Expr>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum TileCode {
    Ld {
        dst: usize,
        mem: String,
    },
    LdImm {
        dst: usize,
        val: i64,
    },
    LdIndexed {
        dst: usize,
        base: usize,
        scale: i64,
        idx: usize,
    },
    AddReg {
        dst: usize,
        lhs: usize,
        rhs: usize,
    },
    AddImm {
        dst: usize,
        lhs: usize,
        imm: i64,
    },
    MulReg {
        dst: usize,
        lhs: usize,
        rhs: usize,
    },
    MulImm {
        dst: usize,
        lhs: usize,
        imm: i64,
    },
    ShlImm {
        dst: usize,
        lhs: usize,
        shift: i64,
    },
    St {
        src: usize,
        mem: String,
    },
    Jmp(String),
    Label(String),
}

#[derive(Clone, Debug)]
struct DpSolution {
    cost: usize,
    left_first: bool,
    tile: SelectedTile,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SelectedTile {
    LdMem,
    LdImm,
    AddImmTile,
    AddRegTile,
    MulRegTile,
    LeaIndexedTile, // base + scale * idx (1 instruction, cost 1 vs Mul+Add cost 3)
}

const INF: usize = 1_000_000;

struct DpNode {
    expr: Expr,
    left: Option<Box<DpNode>>,
    right: Option<Box<DpNode>>,
    // best[regs_available]: solution with result in register, for 1..=max_regs.
    best: Vec<Option<DpSolution>>,
}

fn solve_dp(expr: &Expr, max_regs: usize) -> DpNode {
    let (left, right) = match expr {
        Expr::Mem(_) | Expr::Imm(_) => (None, None),
        Expr::Add(l, r) | Expr::Mul(l, r) => (
            Some(Box::new(solve_dp(l, max_regs))),
            Some(Box::new(solve_dp(r, max_regs))),
        ),
    };

    let mut best: Vec<Option<DpSolution>> = vec![None; max_regs + 1];
    let update = |best: &mut Vec<Option<DpSolution>>, k: usize, cand: DpSolution| {
        let replace = best[k].as_ref().is_none_or(|cur| cand.cost < cur.cost);
        if replace {
            best[k] = Some(cand);
        }
    };

    for k in 1..=max_regs {
        match expr {
            Expr::Mem(_) => update(
                &mut best,
                k,
                DpSolution {
                    cost: 2,
                    left_first: true,
                    tile: SelectedTile::LdMem,
                },
            ),
            Expr::Imm(_) => update(
                &mut best,
                k,
                DpSolution {
                    cost: 1,
                    left_first: true,
                    tile: SelectedTile::LdImm,
                },
            ),
            Expr::Add(_l, r) => {
                let ln = left.as_ref().unwrap();
                let rn = right.as_ref().unwrap();
                // Tile 1: AddImm (if right is Imm) -> evaluates left in k regs + AddImm (cost 1).
                if matches!(**r, Expr::Imm(_)) {
                    if let Some(ls) = &ln.best[k] {
                        update(
                            &mut best,
                            k,
                            DpSolution {
                                cost: ls.cost + 1,
                                left_first: true,
                                tile: SelectedTile::AddImmTile,
                            },
                        );
                    }
                }
                // Tile 2: Fused LEA base + scale * idx when right is Mul(idx, Imm(2|4|8)).
                if let Expr::Mul(_, mr) = &**r {
                    if let Expr::Imm(scale) = &**mr {
                        if matches!(scale, 2 | 4 | 8) && k >= 2 {
                            let idx_node = rn.left.as_ref().unwrap();
                            let c1 = ln.best[k]
                                .as_ref()
                                .zip(idx_node.best[k - 1].as_ref())
                                .map(|(a, b)| a.cost + b.cost + 1);
                            let c2 = idx_node.best[k]
                                .as_ref()
                                .zip(ln.best[k - 1].as_ref())
                                .map(|(b, a)| a.cost + b.cost + 1);
                            if let Some(c) = c1 {
                                update(
                                    &mut best,
                                    k,
                                    DpSolution {
                                        cost: c,
                                        left_first: true,
                                        tile: SelectedTile::LeaIndexedTile,
                                    },
                                );
                            }
                            if let Some(c) = c2 {
                                update(
                                    &mut best,
                                    k,
                                    DpSolution {
                                        cost: c,
                                        left_first: false,
                                        tile: SelectedTile::LeaIndexedTile,
                                    },
                                );
                            }
                        }
                    }
                }
                // Tile 3: Generic AddReg (contiguous eval with >= 2 regs).
                if k >= 2 {
                    if let (Some(la), Some(rb)) = (&ln.best[k], &rn.best[k - 1]) {
                        update(
                            &mut best,
                            k,
                            DpSolution {
                                cost: la.cost + rb.cost + 1,
                                left_first: true,
                                tile: SelectedTile::AddRegTile,
                            },
                        );
                    }
                    if let (Some(ra), Some(lb)) = (&rn.best[k], &ln.best[k - 1]) {
                        update(
                            &mut best,
                            k,
                            DpSolution {
                                cost: ra.cost + lb.cost + 1,
                                left_first: false,
                                tile: SelectedTile::AddRegTile,
                            },
                        );
                    }
                }
                // Tile 4: Spill left when only 1 reg available (`k == 1` non-contiguous).
                if let (Some(ls), Some(rs)) = (&ln.best[k], &rn.best[k]) {
                    update(
                        &mut best,
                        k,
                        DpSolution {
                            cost: ls.cost + rs.cost + 5, // spill ST + reload LD + Add
                            left_first: true,
                            tile: SelectedTile::AddRegTile,
                        },
                    );
                }
            }
            Expr::Mul(_, _) => {
                let ln = left.as_ref().unwrap();
                let rn = right.as_ref().unwrap();
                if k >= 2 {
                    if let (Some(la), Some(rb)) = (&ln.best[k], &rn.best[k - 1]) {
                        update(
                            &mut best,
                            k,
                            DpSolution {
                                cost: la.cost + rb.cost + 2,
                                left_first: true,
                                tile: SelectedTile::MulRegTile,
                            },
                        );
                    }
                    if let (Some(ra), Some(lb)) = (&rn.best[k], &ln.best[k - 1]) {
                        update(
                            &mut best,
                            k,
                            DpSolution {
                                cost: ra.cost + lb.cost + 2,
                                left_first: false,
                                tile: SelectedTile::MulRegTile,
                            },
                        );
                    }
                }
                if best[k].is_none() {
                    let c = ln.best[k].as_ref().map_or(INF, |x| x.cost)
                        + rn.best[k].as_ref().map_or(INF, |x| x.cost)
                        + 5;
                    if c < INF {
                        update(
                            &mut best,
                            k,
                            DpSolution {
                                cost: c,
                                left_first: true,
                                tile: SelectedTile::MulRegTile,
                            },
                        );
                    }
                }
            }
        }
    }

    DpNode {
        expr: expr.clone(),
        left,
        right,
        best,
    }
}

fn emit_dp(node: &DpNode, base_reg: usize, regs_avail: usize, out: &mut Vec<TileCode>) {
    let sol = node.best[regs_avail].as_ref().expect("no valid tiling");
    match sol.tile {
        SelectedTile::LdMem => {
            if let Expr::Mem(m) = &node.expr {
                out.push(TileCode::Ld {
                    dst: base_reg,
                    mem: m.clone(),
                });
            }
        }
        SelectedTile::LdImm => {
            if let Expr::Imm(v) = &node.expr {
                out.push(TileCode::LdImm {
                    dst: base_reg,
                    val: *v,
                });
            }
        }
        SelectedTile::AddImmTile => {
            let ln = node.left.as_ref().unwrap();
            let Expr::Add(_, r) = &node.expr else {
                unreachable!()
            };
            let Expr::Imm(imm) = **r else { unreachable!() };
            emit_dp(ln, base_reg, regs_avail, out);
            out.push(TileCode::AddImm {
                dst: base_reg,
                lhs: base_reg,
                imm,
            });
        }
        SelectedTile::LeaIndexedTile => {
            let ln = node.left.as_ref().unwrap();
            let rn = node.right.as_ref().unwrap();
            let idx_node = rn.left.as_ref().unwrap();
            let Expr::Imm(scale) = rn.right.as_ref().unwrap().expr else {
                unreachable!()
            };
            if sol.left_first {
                emit_dp(ln, base_reg, regs_avail, out);
                emit_dp(idx_node, base_reg + 1, regs_avail - 1, out);
                out.push(TileCode::LdIndexed {
                    dst: base_reg,
                    base: base_reg,
                    scale,
                    idx: base_reg + 1,
                });
            } else {
                emit_dp(idx_node, base_reg + 1, regs_avail, out);
                emit_dp(ln, base_reg, regs_avail - 1, out);
                out.push(TileCode::LdIndexed {
                    dst: base_reg,
                    base: base_reg,
                    scale,
                    idx: base_reg + 1,
                });
            }
        }
        SelectedTile::AddRegTile | SelectedTile::MulRegTile => {
            let ln = node.left.as_ref().unwrap();
            let rn = node.right.as_ref().unwrap();
            if sol.left_first {
                emit_dp(ln, base_reg, regs_avail, out);
                emit_dp(rn, base_reg + 1, (regs_avail - 1).max(1), out);
            } else {
                emit_dp(rn, base_reg + 1, regs_avail, out);
                emit_dp(ln, base_reg, (regs_avail - 1).max(1), out);
            }
            if sol.tile == SelectedTile::AddRegTile {
                out.push(TileCode::AddReg {
                    dst: base_reg,
                    lhs: base_reg,
                    rhs: base_reg + 1,
                });
            } else {
                out.push(TileCode::MulReg {
                    dst: base_reg,
                    lhs: base_reg,
                    rhs: base_reg + 1,
                });
            }
        }
    }
}

fn naive_postorder_emit(expr: &Expr, reg: usize, out: &mut Vec<TileCode>) -> usize {
    match expr {
        Expr::Mem(m) => {
            out.push(TileCode::Ld {
                dst: reg,
                mem: m.clone(),
            });
            2
        }
        Expr::Imm(v) => {
            out.push(TileCode::LdImm { dst: reg, val: *v });
            1
        }
        Expr::Add(l, r) => {
            let cl = naive_postorder_emit(l, reg, out);
            let cr = naive_postorder_emit(r, reg + 1, out);
            out.push(TileCode::AddReg {
                dst: reg,
                lhs: reg,
                rhs: reg + 1,
            });
            cl + cr + 1
        }
        Expr::Mul(l, r) => {
            let cl = naive_postorder_emit(l, reg, out);
            let cr = naive_postorder_emit(r, reg + 1, out);
            out.push(TileCode::MulReg {
                dst: reg,
                lhs: reg,
                rhs: reg + 1,
            });
            cl + cr + 2
        }
    }
}

fn peephole_optimize(code: &[TileCode]) -> Vec<TileCode> {
    let mut cur = code.to_vec();
    loop {
        let mut next = Vec::with_capacity(cur.len());
        let mut i = 0usize;
        let mut changed = false;
        while i < cur.len() {
            // Pattern 1: ST r, x; LD r, x -> ST r, x
            if i + 1 < cur.len() {
                if let (TileCode::St { src, mem: m1 }, TileCode::Ld { dst, mem: m2 }) =
                    (&cur[i], &cur[i + 1])
                {
                    if src == dst && m1 == m2 {
                        next.push(cur[i].clone());
                        i += 2;
                        changed = true;
                        continue;
                    }
                }
                // Pattern 2: Jmp L followed immediately by Label L -> remove Jmp L
                if let (TileCode::Jmp(t), TileCode::Label(l)) = (&cur[i], &cur[i + 1]) {
                    if t == l {
                        i += 1;
                        changed = true;
                        continue;
                    }
                }
            }
            // Pattern 3: ADD r, r, 0 -> delete
            if let TileCode::AddImm { dst, lhs, imm: 0 } = &cur[i] {
                if dst == lhs {
                    i += 1;
                    changed = true;
                    continue;
                }
            }
            // Pattern 4: MUL r, r, 2 -> SHL r, r, 1
            if let TileCode::MulImm { dst, lhs, imm: 2 } = &cur[i] {
                next.push(TileCode::ShlImm {
                    dst: *dst,
                    lhs: *lhs,
                    shift: 1,
                });
                i += 1;
                changed = true;
                continue;
            }
            next.push(cur[i].clone());
            i += 1;
        }
        cur = next;
        if !changed {
            break;
        }
    }
    cur
}

fn eval_expr(expr: &Expr, mem: &BTreeMap<String, i64>) -> i64 {
    match expr {
        Expr::Mem(m) => mem[m],
        Expr::Imm(v) => *v,
        Expr::Add(l, r) => eval_expr(l, mem) + eval_expr(r, mem),
        Expr::Mul(l, r) => eval_expr(l, mem) * eval_expr(r, mem),
    }
}

fn eval_tiles(code: &[TileCode], initial: &BTreeMap<String, i64>) -> i64 {
    let mut mem = initial.clone();
    let mut regs = [0i64; 16];
    let labels: BTreeMap<&str, usize> = code
        .iter()
        .enumerate()
        .filter_map(|(i, ins)| match ins {
            TileCode::Label(l) => Some((l.as_str(), i)),
            _ => None,
        })
        .collect();
    let mut pc = 0usize;
    while pc < code.len() {
        match &code[pc] {
            TileCode::Ld { dst, mem: m } => {
                regs[*dst] = mem[m];
                pc += 1;
            }
            TileCode::LdImm { dst, val } => {
                regs[*dst] = *val;
                pc += 1;
            }
            TileCode::LdIndexed {
                dst,
                base,
                scale,
                idx,
            } => {
                regs[*dst] = regs[*base] + (*scale) * regs[*idx];
                pc += 1;
            }
            TileCode::AddReg { dst, lhs, rhs } => {
                regs[*dst] = regs[*lhs] + regs[*rhs];
                pc += 1;
            }
            TileCode::AddImm { dst, lhs, imm } => {
                regs[*dst] = regs[*lhs] + *imm;
                pc += 1;
            }
            TileCode::MulReg { dst, lhs, rhs } => {
                regs[*dst] = regs[*lhs] * regs[*rhs];
                pc += 1;
            }
            TileCode::MulImm { dst, lhs, imm } => {
                regs[*dst] = regs[*lhs] * *imm;
                pc += 1;
            }
            TileCode::ShlImm { dst, lhs, shift } => {
                regs[*dst] = regs[*lhs] << *shift;
                pc += 1;
            }
            TileCode::St { src, mem: m } => {
                mem.insert(m.clone(), regs[*src]);
                pc += 1;
            }
            TileCode::Jmp(t) => pc = labels[t.as_str()],
            TileCode::Label(_) => pc += 1,
        }
    }
    regs[0]
}

fn indexed_sample_expr() -> Expr {
    // (base + (idx * 4)) + 8
    Expr::Add(
        Box::new(Expr::Add(
            Box::new(Expr::Mem("base".into())),
            Box::new(Expr::Mul(
                Box::new(Expr::Mem("idx".into())),
                Box::new(Expr::Imm(4)),
            )),
        )),
        Box::new(Expr::Imm(8)),
    )
}

fn main() {
    let expr = indexed_sample_expr();
    let dp = solve_dp(&expr, 4);
    let mut dp_code = Vec::new();
    emit_dp(&dp, 0, 4, &mut dp_code);

    let mut naive_code = Vec::new();
    let naive_cost = naive_postorder_emit(&expr, 0, &mut naive_code);
    let dp_cost = dp.best[4].as_ref().unwrap().cost;
    println!(
        "DP cost = {dp_cost} ({} instrs) vs naive cost = {naive_cost} ({} instrs)",
        dp_code.len(),
        naive_code.len()
    );

    let raw_peep = vec![
        TileCode::Ld {
            dst: 0,
            mem: "idx".into(),
        },
        TileCode::MulImm {
            dst: 0,
            lhs: 0,
            imm: 2,
        },
        TileCode::AddImm {
            dst: 0,
            lhs: 0,
            imm: 0,
        },
        TileCode::St {
            src: 0,
            mem: "out".into(),
        },
        TileCode::Ld {
            dst: 0,
            mem: "out".into(),
        },
        TileCode::Jmp("L_done".into()),
        TileCode::Label("L_done".into()),
    ];
    let opt = peephole_optimize(&raw_peep);
    let mem = BTreeMap::from([("base".into(), 1000), ("idx".into(), 7)]);
    println!(
        "Eval expr={}, dp={}, naive={}; peephole {} -> {} instrs (val={})",
        eval_expr(&expr, &mem),
        eval_tiles(&dp_code, &mem),
        eval_tiles(&naive_code, &mem),
        raw_peep.len(),
        opt.len(),
        eval_tiles(&opt, &mem),
    );
}

#[test]
fn dp_selects_cheaper_fused_indexed_and_imm_tiles_preserving_semantics() {
    let expr = indexed_sample_expr();
    let dp = solve_dp(&expr, 4);
    let mut dp_code = Vec::new();
    emit_dp(&dp, 0, 4, &mut dp_code);
    let mut naive_code = Vec::new();
    let naive_cost = naive_postorder_emit(&expr, 0, &mut naive_code);

    let dp_cost = dp.best[4].as_ref().unwrap().cost;
    assert!(dp_cost < naive_cost);
    assert!(dp_code.len() < naive_code.len());
    assert!(dp_code
        .iter()
        .any(|i| matches!(i, TileCode::LdIndexed { scale: 4, .. })));

    let mem = BTreeMap::from([("base".into(), 500), ("idx".into(), 12)]);
    assert_eq!(eval_tiles(&dp_code, &mem), eval_expr(&expr, &mem));
    assert_eq!(eval_tiles(&naive_code, &mem), eval_expr(&expr, &mem));
}

#[test]
fn peephole_shrinks_redundant_store_load_identity_and_jumps_preserving_value() {
    let raw = vec![
        TileCode::Ld {
            dst: 0,
            mem: "x".into(),
        },
        TileCode::MulImm {
            dst: 0,
            lhs: 0,
            imm: 2,
        },
        TileCode::AddImm {
            dst: 0,
            lhs: 0,
            imm: 0,
        },
        TileCode::St {
            src: 0,
            mem: "tmp".into(),
        },
        TileCode::Ld {
            dst: 0,
            mem: "tmp".into(),
        },
        TileCode::Jmp("L_next".into()),
        TileCode::Label("L_next".into()),
    ];
    let opt = peephole_optimize(&raw);
    assert!(opt.len() <= 4);
    assert!(opt
        .iter()
        .any(|i| matches!(i, TileCode::ShlImm { shift: 1, .. })));
    let mem = BTreeMap::from([("x".into(), 9)]);
    assert_eq!(eval_tiles(&opt, &mem), eval_tiles(&raw, &mem));
}
