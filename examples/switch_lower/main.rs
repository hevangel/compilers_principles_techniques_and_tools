//! Lower multi-way switch statements by density and backpatch nested loop/switch break and continue jumps.

use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SwitchStrategy {
    LinearIfElse,
    JumpTable,
    BinarySearchTree,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum LoweredInstr {
    IfEqJump {
        val: i64,
        target: String,
    },
    IfLtJump {
        val: i64,
        target: String,
    },
    JumpTable {
        min_val: i64,
        default_label: String,
        table: Vec<String>,
    },
    Jump(String),
    Label(String),
    Emit(i64),
    IncrVar,
    IfVarLtJump {
        limit: i64,
        target: String,
    },
    BreakPlaceholder,
    ContinuePlaceholder,
    PatchedJump(String),
}

fn choose_strategy(arms: &[(i64, String)]) -> SwitchStrategy {
    if arms.len() <= 4 {
        return SwitchStrategy::LinearIfElse;
    }
    let min_v = arms.iter().map(|(v, _)| *v).min().unwrap();
    let max_v = arms.iter().map(|(v, _)| *v).max().unwrap();
    let span = (max_v - min_v) as u128 + 1;
    // Dense if count / span >= 0.5 (2 * count >= span).
    if (arms.len() as u128) * 2 >= span {
        SwitchStrategy::JumpTable
    } else {
        SwitchStrategy::BinarySearchTree
    }
}

fn lower_switch(
    arms: &[(i64, String)],
    default_label: &str,
) -> (SwitchStrategy, Vec<LoweredInstr>) {
    let mut sorted = arms.to_vec();
    sorted.sort_by_key(|(v, _)| *v);
    sorted.dedup_by_key(|(v, _)| *v);

    let strategy = choose_strategy(&sorted);
    let mut instrs = Vec::new();
    match strategy {
        SwitchStrategy::LinearIfElse => {
            for (val, label) in &sorted {
                instrs.push(LoweredInstr::IfEqJump {
                    val: *val,
                    target: label.clone(),
                });
            }
            instrs.push(LoweredInstr::Jump(default_label.to_string()));
        }
        SwitchStrategy::JumpTable => {
            let min_v = sorted.first().unwrap().0;
            let max_v = sorted.last().unwrap().0;
            let span = (max_v - min_v + 1) as usize;
            let mut table = vec![default_label.to_string(); span];
            for (val, label) in &sorted {
                table[(val - min_v) as usize] = label.clone();
            }
            instrs.push(LoweredInstr::JumpTable {
                min_val: min_v,
                default_label: default_label.to_string(),
                table,
            });
        }
        SwitchStrategy::BinarySearchTree => {
            fn emit_bst(
                slice: &[(i64, String)],
                default_label: &str,
                prefix: &str,
                instrs: &mut Vec<LoweredInstr>,
            ) {
                if slice.is_empty() {
                    instrs.push(LoweredInstr::Jump(default_label.to_string()));
                    return;
                }
                if slice.len() == 1 {
                    instrs.push(LoweredInstr::IfEqJump {
                        val: slice[0].0,
                        target: slice[0].1.clone(),
                    });
                    instrs.push(LoweredInstr::Jump(default_label.to_string()));
                    return;
                }
                let mid = slice.len() / 2;
                let (mid_val, mid_lbl) = &slice[mid];
                let left_lbl = format!("{prefix}L");
                instrs.push(LoweredInstr::IfLtJump {
                    val: *mid_val,
                    target: left_lbl.clone(),
                });
                instrs.push(LoweredInstr::IfEqJump {
                    val: *mid_val,
                    target: mid_lbl.clone(),
                });
                emit_bst(
                    &slice[mid + 1..],
                    default_label,
                    &format!("{prefix}R"),
                    instrs,
                );
                instrs.push(LoweredInstr::Label(left_lbl));
                emit_bst(&slice[..mid], default_label, &format!("{prefix}L"), instrs);
            }
            emit_bst(&sorted, default_label, "bst_", &mut instrs);
        }
    }
    (strategy, instrs)
}

fn eval_switch_dispatch(instrs: &[LoweredInstr], key: i64) -> String {
    let labels: BTreeMap<&str, usize> = instrs
        .iter()
        .enumerate()
        .filter_map(|(i, ins)| match ins {
            LoweredInstr::Label(l) => Some((l.as_str(), i)),
            _ => None,
        })
        .collect();
    let mut pc = 0usize;
    let mut steps = 0usize;
    while pc < instrs.len() {
        assert!(steps < 10_000, "infinite dispatch");
        steps += 1;
        match &instrs[pc] {
            LoweredInstr::IfEqJump { val, target } => {
                if key == *val {
                    if let Some(&idx) = labels.get(target.as_str()) {
                        pc = idx;
                        continue;
                    }
                    return target.clone();
                }
                pc += 1;
            }
            LoweredInstr::IfLtJump { val, target } => {
                if key < *val {
                    if let Some(&idx) = labels.get(target.as_str()) {
                        pc = idx;
                        continue;
                    }
                    return target.clone();
                }
                pc += 1;
            }
            LoweredInstr::JumpTable {
                min_val,
                default_label,
                table,
            } => {
                let target = if key < *min_val {
                    default_label
                } else {
                    let off = (key - *min_val) as usize;
                    table.get(off).unwrap_or(default_label)
                };
                if let Some(&idx) = labels.get(target.as_str()) {
                    pc = idx;
                } else {
                    return target.clone();
                }
            }
            LoweredInstr::Jump(target) => {
                if let Some(&idx) = labels.get(target.as_str()) {
                    pc = idx;
                } else {
                    return target.clone();
                }
            }
            LoweredInstr::Label(_) => pc += 1,
            other => panic!("unexpected dispatch instruction {other:?}"),
        }
    }
    unreachable!("dispatch must jump to an arm or default")
}

#[derive(Clone, Debug)]
enum StmtAst {
    EmitCaseValue,
    Break,
    Continue,
    Switch {
        cases: Vec<(i64, Vec<StmtAst>)>,
        default: Vec<StmtAst>,
    },
    Loop {
        limit: i64,
        body: Vec<StmtAst>,
    },
}

struct PatchContext {
    break_lists: Vec<Vec<usize>>,
    continue_lists: Vec<Vec<usize>>,
    loop_headers: Vec<String>,
    next_id: usize,
}

impl PatchContext {
    fn new() -> Self {
        Self {
            break_lists: Vec::new(),
            continue_lists: Vec::new(),
            loop_headers: Vec::new(),
            next_id: 0,
        }
    }

    fn fresh_label(&mut self, prefix: &str) -> String {
        let id = self.next_id;
        self.next_id += 1;
        format!("{prefix}_{id}")
    }
}

fn backpatch(instrs: &mut [LoweredInstr], sites: &[usize], target: &str) {
    for &idx in sites {
        instrs[idx] = LoweredInstr::PatchedJump(target.to_string());
    }
}

fn lower_stmt_list(stmts: &[StmtAst], ctx: &mut PatchContext, out: &mut Vec<LoweredInstr>) {
    for stmt in stmts {
        match stmt {
            StmtAst::EmitCaseValue => out.push(LoweredInstr::Emit(1)),
            StmtAst::Break => {
                let idx = out.len();
                out.push(LoweredInstr::BreakPlaceholder);
                ctx.break_lists
                    .last_mut()
                    .expect("break outside loop or switch")
                    .push(idx);
            }
            StmtAst::Continue => {
                let idx = out.len();
                out.push(LoweredInstr::ContinuePlaceholder);
                ctx.continue_lists
                    .last_mut()
                    .expect("continue outside loop")
                    .push(idx);
            }
            StmtAst::Switch { cases, default } => {
                let switch_end = ctx.fresh_label("sw_end");
                let def_lbl = ctx.fresh_label("sw_def");
                let mut arms = Vec::new();
                let mut case_labels = Vec::new();
                for (val, _) in cases {
                    let lbl = ctx.fresh_label(&format!("case_{val}"));
                    arms.push((*val, lbl.clone()));
                    case_labels.push(lbl);
                }
                let (_, mut disp) = lower_switch(&arms, &def_lbl);
                out.append(&mut disp);

                // Break exits the switch; continue targets the enclosing loop (unchanged stack).
                ctx.break_lists.push(Vec::new());
                for ((val, body), lbl) in cases.iter().zip(&case_labels) {
                    out.push(LoweredInstr::Label(lbl.clone()));
                    out.push(LoweredInstr::Emit(*val * 10));
                    lower_stmt_list(body, ctx, out);
                }
                out.push(LoweredInstr::Label(def_lbl));
                lower_stmt_list(default, ctx, out);
                out.push(LoweredInstr::Label(switch_end.clone()));

                let breaks = ctx.break_lists.pop().unwrap();
                backpatch(out, &breaks, &switch_end);
            }
            StmtAst::Loop { limit, body } => {
                let loop_head = ctx.fresh_label("loop_head");
                let loop_step = ctx.fresh_label("loop_step");
                let loop_exit = ctx.fresh_label("loop_exit");

                ctx.break_lists.push(Vec::new());
                ctx.continue_lists.push(Vec::new());
                ctx.loop_headers.push(loop_step.clone());

                out.push(LoweredInstr::Label(loop_head.clone()));
                lower_stmt_list(body, ctx, out);
                out.push(LoweredInstr::Label(loop_step.clone()));
                out.push(LoweredInstr::IncrVar);
                out.push(LoweredInstr::IfVarLtJump {
                    limit: *limit,
                    target: loop_head,
                });
                out.push(LoweredInstr::Label(loop_exit.clone()));

                ctx.loop_headers.pop();
                let conts = ctx.continue_lists.pop().unwrap();
                let breaks = ctx.break_lists.pop().unwrap();
                backpatch(out, &conts, &loop_step);
                backpatch(out, &breaks, &loop_exit);
            }
        }
    }
}

fn lower_program(stmts: &[StmtAst]) -> Vec<LoweredInstr> {
    let mut ctx = PatchContext::new();
    let mut out = Vec::new();
    lower_stmt_list(stmts, &mut ctx, &mut out);
    out
}

fn run_lowered_program(instrs: &[LoweredInstr]) -> (i64, Vec<i64>) {
    let labels: BTreeMap<&str, usize> = instrs
        .iter()
        .enumerate()
        .filter_map(|(i, ins)| match ins {
            LoweredInstr::Label(l) => Some((l.as_str(), i)),
            _ => None,
        })
        .collect();
    let mut pc = 0usize;
    let mut var = 0i64;
    let mut trace = Vec::new();
    let mut steps = 0usize;
    while pc < instrs.len() {
        assert!(steps < 50_000, "infinite loop in lowered program");
        steps += 1;
        match &instrs[pc] {
            LoweredInstr::IfEqJump { val, target } => {
                if var == *val {
                    pc = labels[target.as_str()];
                } else {
                    pc += 1;
                }
            }
            LoweredInstr::IfLtJump { val, target } => {
                if var < *val {
                    pc = labels[target.as_str()];
                } else {
                    pc += 1;
                }
            }
            LoweredInstr::JumpTable {
                min_val,
                default_label,
                table,
            } => {
                let target = if var < *min_val {
                    default_label
                } else {
                    table
                        .get((var - *min_val) as usize)
                        .unwrap_or(default_label)
                };
                pc = labels[target.as_str()];
            }
            LoweredInstr::Jump(t) | LoweredInstr::PatchedJump(t) => {
                pc = labels[t.as_str()];
            }
            LoweredInstr::Label(_) => pc += 1,
            LoweredInstr::Emit(v) => {
                trace.push(*v);
                pc += 1;
            }
            LoweredInstr::IncrVar => {
                var += 1;
                pc += 1;
            }
            LoweredInstr::IfVarLtJump { limit, target } => {
                if var < *limit {
                    pc = labels[target.as_str()];
                } else {
                    pc += 1;
                }
            }
            LoweredInstr::BreakPlaceholder | LoweredInstr::ContinuePlaceholder => {
                panic!("unpatched placeholder encountered at pc={pc}");
            }
        }
    }
    (var, trace)
}

fn main() {
    let dense: Vec<(i64, String)> = (0..10).map(|i| (i, format!("L{i}"))).collect();
    let sparse: Vec<(i64, String)> = (0..10).map(|i| (i * 100, format!("S{i}"))).collect();
    let tiny = vec![(1, "T1".to_string()), (2, "T2".to_string())];

    let (d_strat, d_ins) = lower_switch(&dense, "DEF");
    let (s_strat, s_ins) = lower_switch(&sparse, "DEF");
    let (t_strat, t_ins) = lower_switch(&tiny, "DEF");
    println!("Dense strategy: {d_strat:?}, instrs={}", d_ins.len());
    println!("Sparse strategy: {s_strat:?}, instrs={}", s_ins.len());
    println!("Tiny strategy: {t_strat:?}, instrs={}", t_ins.len());
    println!(
        "Sparse dispatch(500)={}, dispatch(501)={}",
        eval_switch_dispatch(&s_ins, 500),
        eval_switch_dispatch(&s_ins, 501)
    );

    let prog = vec![StmtAst::Loop {
        limit: 4,
        body: vec![
            StmtAst::Switch {
                cases: vec![
                    (0, vec![StmtAst::Break]),
                    (1, vec![StmtAst::Continue]),
                    (2, vec![StmtAst::EmitCaseValue, StmtAst::Break]),
                ],
                default: vec![StmtAst::EmitCaseValue],
            },
            // Reached only when switch breaks (not on continue).
            StmtAst::EmitCaseValue,
        ],
    }];
    let lowered = lower_program(&prog);
    let (final_var, trace) = run_lowered_program(&lowered);
    println!("Nested loop+switch: final_var={final_var}, trace={trace:?}");
}

#[test]
fn selects_switch_strategies_and_dispatches_all_keys() {
    let tiny = vec![(1, "A".to_string()), (5, "B".to_string())];
    let (s_tiny, ins_tiny) = lower_switch(&tiny, "DEF");
    assert_eq!(s_tiny, SwitchStrategy::LinearIfElse);
    assert_eq!(eval_switch_dispatch(&ins_tiny, 1), "A");
    assert_eq!(eval_switch_dispatch(&ins_tiny, 3), "DEF");

    let dense: Vec<(i64, String)> = vec![
        (10, "D10".into()),
        (11, "D11".into()),
        (12, "D12".into()),
        (14, "D14".into()),
        (15, "D15".into()),
        (16, "D16".into()),
    ];
    let (s_dense, ins_dense) = lower_switch(&dense, "DEF");
    assert_eq!(s_dense, SwitchStrategy::JumpTable);
    assert_eq!(eval_switch_dispatch(&ins_dense, 9), "DEF");
    assert_eq!(eval_switch_dispatch(&ins_dense, 12), "D12");
    assert_eq!(eval_switch_dispatch(&ins_dense, 13), "DEF");
    assert_eq!(eval_switch_dispatch(&ins_dense, 20), "DEF");

    let sparse: Vec<(i64, String)> = (0..10).map(|i| (i * 1000, format!("S{i}"))).collect();
    let (s_sparse, ins_sparse) = lower_switch(&sparse, "DEF");
    assert_eq!(s_sparse, SwitchStrategy::BinarySearchTree);
    assert_eq!(eval_switch_dispatch(&ins_sparse, -5), "DEF");
    assert_eq!(eval_switch_dispatch(&ins_sparse, 0), "S0");
    assert_eq!(eval_switch_dispatch(&ins_sparse, 4500), "DEF");
    assert_eq!(eval_switch_dispatch(&ins_sparse, 9000), "S9");
    assert_eq!(eval_switch_dispatch(&ins_sparse, 9001), "DEF");
}

#[test]
fn nested_loop_and_switch_backpatches_break_and_continue_distinctly() {
    // Loop i = 0..4:
    //   i=0: switch enters case 0 (emits 0), hits switch break -> skips other cases, reaches post-switch Emit(1)
    //   i=1: switch enters case 1 (emits 10), hits loop continue -> skips post-switch Emit(1), increments i
    //   i=2: switch enters case 2 (emits 20, Emit(1)), hits switch break -> reaches post-switch Emit(1)
    //   i=3: switch enters default (emits 1) -> reaches post-switch Emit(1)
    let prog = vec![StmtAst::Loop {
        limit: 4,
        body: vec![
            StmtAst::Switch {
                cases: vec![
                    (0, vec![StmtAst::Break]),
                    (1, vec![StmtAst::Continue]),
                    (2, vec![StmtAst::EmitCaseValue, StmtAst::Break]),
                ],
                default: vec![StmtAst::EmitCaseValue],
            },
            StmtAst::EmitCaseValue,
        ],
    }];
    let lowered = lower_program(&prog);
    let (final_i, trace) = run_lowered_program(&lowered);
    assert_eq!(final_i, 4);
    assert_eq!(trace, vec![0, 1, 10, 20, 1, 1, 1, 1]);

    // Also test breaking out of the enclosing loop after default.
    let loop_break = vec![StmtAst::Loop {
        limit: 5,
        body: vec![
            StmtAst::Switch {
                cases: vec![(0, vec![StmtAst::Break])],
                default: vec![StmtAst::EmitCaseValue],
            },
            StmtAst::Break,
        ],
    }];
    let lowered_lb = lower_program(&loop_break);
    let (final_lb, trace_lb) = run_lowered_program(&lowered_lb);
    // i=0 breaks out of switch, then hits outer loop Break, exiting with i=0.
    assert_eq!(final_lb, 0);
    assert_eq!(trace_lb, vec![0]);
}
