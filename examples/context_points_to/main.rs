//! Solve k-CFA call-string points-to analysis with on-the-fly virtual dispatch discovery.

use std::collections::{BTreeMap, BTreeSet};

type CallString = Vec<&'static str>;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct HeapSite {
    label: &'static str,
    class_type: &'static str,
}

#[derive(Clone, Debug)]
enum Stmt {
    Alloc {
        dst: &'static str,
        site: HeapSite,
    },
    Assign {
        dst: &'static str,
        src: &'static str,
    },
    DirectCall {
        site: &'static str,
        callee: &'static str,
        args: Vec<&'static str>,
        dst: &'static str,
    },
    VirtualCall {
        site: &'static str,
        receiver: &'static str,
        method: &'static str,
        args: Vec<&'static str>,
        dst: &'static str,
    },
    Return {
        src: &'static str,
    },
}

#[derive(Clone, Debug)]
struct Function {
    params: Vec<&'static str>,
    stmts: Vec<Stmt>,
}

#[derive(Clone, Debug)]
struct Program {
    entry: &'static str,
    functions: BTreeMap<&'static str, Function>,
    vtable: BTreeMap<(&'static str, &'static str), &'static str>,
}

fn push_call_string(ctx: &[&'static str], site: &'static str, k: usize) -> CallString {
    if k == 0 {
        return Vec::new();
    }
    let mut next = ctx.to_vec();
    next.push(site);
    if next.len() > k {
        let start = next.len() - k;
        next.drain(0..start);
    }
    next
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct DiscoveredCallEdge {
    caller: &'static str,
    caller_ctx: CallString,
    site: &'static str,
    callee: &'static str,
    callee_ctx: CallString,
}

#[derive(Clone, Debug)]
struct PointsToResult {
    pts: BTreeMap<(&'static str, CallString, &'static str), BTreeSet<HeapSite>>,
    call_edges: BTreeSet<DiscoveredCallEdge>,
    reachable_contexts: BTreeSet<(&'static str, CallString)>,
}

impl PointsToResult {
    fn merged_var(&self, func: &'static str, var: &'static str) -> BTreeSet<&'static str> {
        let mut out = BTreeSet::new();
        for ((f, _ctx, v), sites) in &self.pts {
            if *f == func && *v == var {
                for s in sites {
                    out.insert(s.label);
                }
            }
        }
        out
    }
}

fn solve_k_cfa(program: &Program, k: usize) -> PointsToResult {
    let mut pts: BTreeMap<(&'static str, CallString, &'static str), BTreeSet<HeapSite>> =
        BTreeMap::new();
    let mut reachable: BTreeSet<(&'static str, CallString)> = BTreeSet::new();
    let mut call_edges: BTreeSet<DiscoveredCallEdge> = BTreeSet::new();

    reachable.insert((program.entry, Vec::new()));

    loop {
        let mut changed = false;
        let current_contexts: Vec<(&'static str, CallString)> = reachable.iter().cloned().collect();

        for (func_name, ctx) in current_contexts {
            let func = &program.functions[func_name];
            for stmt in &func.stmts {
                match stmt {
                    Stmt::Alloc { dst, site } => {
                        let entry = pts.entry((func_name, ctx.clone(), *dst)).or_default();
                        if entry.insert(site.clone()) {
                            changed = true;
                        }
                    }
                    Stmt::Assign { dst, src } => {
                        let src_sites = pts
                            .get(&(func_name, ctx.clone(), *src))
                            .cloned()
                            .unwrap_or_default();
                        let dst_entry = pts.entry((func_name, ctx.clone(), *dst)).or_default();
                        let before = dst_entry.len();
                        dst_entry.extend(src_sites);
                        if dst_entry.len() != before {
                            changed = true;
                        }
                    }
                    Stmt::DirectCall {
                        site,
                        callee,
                        args,
                        dst,
                    } => {
                        let callee_ctx = push_call_string(&ctx, site, k);
                        if reachable.insert((*callee, callee_ctx.clone())) {
                            changed = true;
                        }
                        if call_edges.insert(DiscoveredCallEdge {
                            caller: func_name,
                            caller_ctx: ctx.clone(),
                            site,
                            callee,
                            callee_ctx: callee_ctx.clone(),
                        }) {
                            changed = true;
                        }
                        let callee_fn = &program.functions[callee];
                        for (&arg, &param) in args.iter().zip(&callee_fn.params) {
                            let arg_sites = pts
                                .get(&(func_name, ctx.clone(), arg))
                                .cloned()
                                .unwrap_or_default();
                            let p_entry =
                                pts.entry((*callee, callee_ctx.clone(), param)).or_default();
                            let before = p_entry.len();
                            p_entry.extend(arg_sites);
                            if p_entry.len() != before {
                                changed = true;
                            }
                        }
                        let ret_sites = pts
                            .get(&(*callee, callee_ctx, "$ret"))
                            .cloned()
                            .unwrap_or_default();
                        let dst_entry = pts.entry((func_name, ctx.clone(), *dst)).or_default();
                        let before = dst_entry.len();
                        dst_entry.extend(ret_sites);
                        if dst_entry.len() != before {
                            changed = true;
                        }
                    }
                    Stmt::VirtualCall {
                        site,
                        receiver,
                        method,
                        args,
                        dst,
                    } => {
                        let recv_sites = pts
                            .get(&(func_name, ctx.clone(), *receiver))
                            .cloned()
                            .unwrap_or_default();
                        let callee_ctx = push_call_string(&ctx, site, k);
                        for site_obj in recv_sites {
                            if let Some(&callee) =
                                program.vtable.get(&(site_obj.class_type, *method))
                            {
                                if reachable.insert((callee, callee_ctx.clone())) {
                                    changed = true;
                                }
                                if call_edges.insert(DiscoveredCallEdge {
                                    caller: func_name,
                                    caller_ctx: ctx.clone(),
                                    site,
                                    callee,
                                    callee_ctx: callee_ctx.clone(),
                                }) {
                                    changed = true;
                                }
                                let callee_fn = &program.functions[callee];
                                // Pass receiver as param[0] (`this`), then args.
                                if let Some(&this_param) = callee_fn.params.first() {
                                    let p0 = pts
                                        .entry((callee, callee_ctx.clone(), this_param))
                                        .or_default();
                                    if p0.insert(site_obj.clone()) {
                                        changed = true;
                                    }
                                }
                                for (&arg, &param) in
                                    args.iter().zip(callee_fn.params.iter().skip(1))
                                {
                                    let arg_sites = pts
                                        .get(&(func_name, ctx.clone(), arg))
                                        .cloned()
                                        .unwrap_or_default();
                                    let p_entry =
                                        pts.entry((callee, callee_ctx.clone(), param)).or_default();
                                    let before = p_entry.len();
                                    p_entry.extend(arg_sites);
                                    if p_entry.len() != before {
                                        changed = true;
                                    }
                                }
                                let ret_sites = pts
                                    .get(&(callee, callee_ctx.clone(), "$ret"))
                                    .cloned()
                                    .unwrap_or_default();
                                let dst_entry =
                                    pts.entry((func_name, ctx.clone(), *dst)).or_default();
                                let before = dst_entry.len();
                                dst_entry.extend(ret_sites);
                                if dst_entry.len() != before {
                                    changed = true;
                                }
                            }
                        }
                    }
                    Stmt::Return { src } => {
                        let ret_sites = pts
                            .get(&(func_name, ctx.clone(), *src))
                            .cloned()
                            .unwrap_or_default();
                        let ret_entry = pts.entry((func_name, ctx.clone(), "$ret")).or_default();
                        let before = ret_entry.len();
                        ret_entry.extend(ret_sites);
                        if ret_entry.len() != before {
                            changed = true;
                        }
                    }
                }
            }
        }

        if !changed {
            return PointsToResult {
                pts,
                call_edges,
                reachable_contexts: reachable,
            };
        }
    }
}

fn relay_program() -> Program {
    // a = new A; b = new B; x = c1: relay(a); y = c2: relay(b)
    // relay(p) { t = p; return t; }
    let mut functions = BTreeMap::new();
    functions.insert(
        "main",
        Function {
            params: vec![],
            stmts: vec![
                Stmt::Alloc {
                    dst: "a",
                    site: HeapSite {
                        label: "alloc_A",
                        class_type: "A",
                    },
                },
                Stmt::Alloc {
                    dst: "b",
                    site: HeapSite {
                        label: "alloc_B",
                        class_type: "B",
                    },
                },
                Stmt::DirectCall {
                    site: "c1",
                    callee: "relay",
                    args: vec!["a"],
                    dst: "x",
                },
                Stmt::DirectCall {
                    site: "c2",
                    callee: "relay",
                    args: vec!["b"],
                    dst: "y",
                },
            ],
        },
    );
    functions.insert(
        "relay",
        Function {
            params: vec!["p"],
            stmts: vec![
                Stmt::Assign { dst: "t", src: "p" },
                Stmt::Return { src: "t" },
            ],
        },
    );
    Program {
        entry: "main",
        functions,
        vtable: BTreeMap::new(),
    }
}

fn virtual_dispatch_and_recursion_program() -> Program {
    let mut functions = BTreeMap::new();
    functions.insert(
        "main",
        Function {
            params: vec![],
            stmts: vec![
                Stmt::Alloc {
                    dst: "obj_a",
                    site: HeapSite {
                        label: "h_sub_a",
                        class_type: "SubA",
                    },
                },
                Stmt::Alloc {
                    dst: "payload",
                    site: HeapSite {
                        label: "h_payload",
                        class_type: "Data",
                    },
                },
                Stmt::VirtualCall {
                    site: "v1",
                    receiver: "obj_a",
                    method: "apply",
                    args: vec!["payload"],
                    dst: "out_a",
                },
                Stmt::DirectCall {
                    site: "r0",
                    callee: "rec",
                    args: vec!["payload"],
                    dst: "out_rec",
                },
            ],
        },
    );
    functions.insert(
        "SubA::apply",
        Function {
            params: vec!["this", "arg"],
            stmts: vec![Stmt::Return { src: "arg" }],
        },
    );
    functions.insert(
        "SubB::apply",
        Function {
            params: vec!["this", "arg"],
            stmts: vec![Stmt::Return { src: "this" }],
        },
    );
    functions.insert(
        "rec",
        Function {
            params: vec!["n"],
            stmts: vec![
                Stmt::DirectCall {
                    site: "r_self",
                    callee: "rec",
                    args: vec!["n"],
                    dst: "tmp",
                },
                Stmt::Return { src: "n" },
            ],
        },
    );
    let mut vtable = BTreeMap::new();
    vtable.insert(("SubA", "apply"), "SubA::apply");
    vtable.insert(("SubB", "apply"), "SubB::apply");
    Program {
        entry: "main",
        functions,
        vtable,
    }
}

fn main() {
    let relay = relay_program();
    for k in [0usize, 1] {
        let res = solve_k_cfa(&relay, k);
        println!(
            "relay with k={k}: x -> {:?}, y -> {:?}, relay contexts={}",
            res.merged_var("main", "x"),
            res.merged_var("main", "y"),
            res.reachable_contexts
                .iter()
                .filter(|(f, _)| *f == "relay")
                .count()
        );
    }

    let dyn_prog = virtual_dispatch_and_recursion_program();
    let dyn_res = solve_k_cfa(&dyn_prog, 2);
    let rec_ctxs: Vec<CallString> = dyn_res
        .reachable_contexts
        .iter()
        .filter(|(f, _)| *f == "rec")
        .map(|(_, c)| c.clone())
        .collect();
    let callees: BTreeSet<&'static str> = dyn_res.call_edges.iter().map(|e| e.callee).collect();
    println!(
        "k=2 dynamic + recursive: discovered callees={callees:?}, out_a={:?}, rec_contexts={rec_ctxs:?}",
        dyn_res.merged_var("main", "out_a")
    );
}

#[test]
fn relay_separates_call_sites_at_k1_and_merges_at_k0() {
    let prog = relay_program();
    let k0 = solve_k_cfa(&prog, 0);
    assert_eq!(
        k0.merged_var("main", "x"),
        BTreeSet::from(["alloc_A", "alloc_B"])
    );
    assert_eq!(
        k0.merged_var("main", "y"),
        BTreeSet::from(["alloc_A", "alloc_B"])
    );

    let k1 = solve_k_cfa(&prog, 1);
    assert_eq!(k1.merged_var("main", "x"), BTreeSet::from(["alloc_A"]));
    assert_eq!(k1.merged_var("main", "y"), BTreeSet::from(["alloc_B"]));
}

#[test]
fn on_the_fly_virtual_dispatch_and_k2_recursion_terminate() {
    let prog = virtual_dispatch_and_recursion_program();
    let res = solve_k_cfa(&prog, 2);
    assert_eq!(
        res.merged_var("main", "out_a"),
        BTreeSet::from(["h_payload"])
    );
    assert_eq!(
        res.merged_var("main", "out_rec"),
        BTreeSet::from(["h_payload"])
    );
    let callees: BTreeSet<&'static str> = res.call_edges.iter().map(|e| e.callee).collect();
    assert!(callees.contains("SubA::apply"));
    assert!(!callees.contains("SubB::apply"));
    let rec_ctxs: BTreeSet<CallString> = res
        .reachable_contexts
        .iter()
        .filter(|(f, _)| *f == "rec")
        .map(|(_, c)| c.clone())
        .collect();
    assert_eq!(
        rec_ctxs,
        BTreeSet::from([vec!["r0"], vec!["r0", "r_self"], vec!["r_self", "r_self"],])
    );
}
