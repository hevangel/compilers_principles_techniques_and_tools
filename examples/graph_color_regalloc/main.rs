//! Global register allocation via interference graph construction and Kempe simplify/select coloring.

use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug)]
struct Instr {
    def: Option<String>,
    uses: Vec<String>,
}

#[derive(Clone, Debug)]
struct CfgBlock {
    loop_depth: u32,
    instrs: Vec<Instr>,
    succs: Vec<usize>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ColoringResult {
    colors: BTreeMap<String, usize>,
    spills: BTreeSet<String>,
    total_spill_cost: u64,
    interference: BTreeMap<String, BTreeSet<String>>,
}

fn build_interference_and_costs(
    blocks: &[CfgBlock],
) -> (BTreeMap<String, BTreeSet<String>>, BTreeMap<String, u64>) {
    let n = blocks.len();
    let mut use_blk: Vec<BTreeSet<String>> = vec![BTreeSet::new(); n];
    let mut def_blk: Vec<BTreeSet<String>> = vec![BTreeSet::new(); n];
    let mut all_vars: BTreeSet<String> = BTreeSet::new();
    let mut costs: BTreeMap<String, u64> = BTreeMap::new();

    for (b, blk) in blocks.iter().enumerate() {
        let weight = 10u64.pow(blk.loop_depth);
        for ins in &blk.instrs {
            for u in &ins.uses {
                all_vars.insert(u.clone());
                *costs.entry(u.clone()).or_insert(0) += weight;
                if !def_blk[b].contains(u) {
                    use_blk[b].insert(u.clone());
                }
            }
            if let Some(d) = &ins.def {
                all_vars.insert(d.clone());
                *costs.entry(d.clone()).or_insert(0) += weight;
                def_blk[b].insert(d.clone());
            }
        }
    }

    let mut live_in: Vec<BTreeSet<String>> = vec![BTreeSet::new(); n];
    let mut live_out: Vec<BTreeSet<String>> = vec![BTreeSet::new(); n];
    loop {
        let mut changed = false;
        for b in (0..n).rev() {
            let mut new_out = BTreeSet::new();
            for &s in &blocks[b].succs {
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

    let mut interference: BTreeMap<String, BTreeSet<String>> = all_vars
        .iter()
        .map(|v| (v.clone(), BTreeSet::new()))
        .collect();
    let add_edge = |u: &str, v: &str, g: &mut BTreeMap<String, BTreeSet<String>>| {
        if u != v {
            g.get_mut(u).unwrap().insert(v.to_string());
            g.get_mut(v).unwrap().insert(u.to_string());
        }
    };

    for (b, blk) in blocks.iter().enumerate() {
        let mut live = live_out[b].clone();
        // Variables simultaneously live at block exit interfere with each other.
        let exit_vec: Vec<String> = live.iter().cloned().collect();
        for i in 0..exit_vec.len() {
            for j in i + 1..exit_vec.len() {
                add_edge(&exit_vec[i], &exit_vec[j], &mut interference);
            }
        }
        for ins in blk.instrs.iter().rev() {
            if let Some(d) = &ins.def {
                for l in &live {
                    add_edge(d, l, &mut interference);
                }
                live.remove(d);
            }
            for u in &ins.uses {
                live.insert(u.clone());
            }
            let live_vec: Vec<String> = live.iter().cloned().collect();
            for i in 0..live_vec.len() {
                for j in i + 1..live_vec.len() {
                    add_edge(&live_vec[i], &live_vec[j], &mut interference);
                }
            }
        }
    }
    (interference, costs)
}

fn kempe_color(
    interference: &BTreeMap<String, BTreeSet<String>>,
    costs: &BTreeMap<String, u64>,
    k: usize,
) -> ColoringResult {
    let mut active: BTreeMap<String, BTreeSet<String>> = interference.clone();
    let mut stack: Vec<String> = Vec::new();

    while !active.is_empty() {
        // Prefer lowest-cost-neutral trivial node (degree < k).
        let trivial = active
            .iter()
            .find_map(|(v, nbrs)| (nbrs.len() < k).then(|| v.clone()));
        let chosen = if let Some(v) = trivial {
            v
        } else {
            // Potential spill heuristic: minimize spill_cost(v) / deg(v).
            active
                .iter()
                .min_by(|(va, na), (vb, nb)| {
                    let ca = (*costs.get(*va).unwrap_or(&1) as u128) * (nb.len() as u128);
                    let cb = (*costs.get(*vb).unwrap_or(&1) as u128) * (na.len() as u128);
                    ca.cmp(&cb).then_with(|| va.cmp(vb))
                })
                .map(|(v, _)| v.clone())
                .unwrap()
        };
        let nbrs = active.remove(&chosen).unwrap();
        for nbr in &nbrs {
            if let Some(set) = active.get_mut(nbr) {
                set.remove(&chosen);
            }
        }
        stack.push(chosen);
    }

    let mut colors: BTreeMap<String, usize> = BTreeMap::new();
    let mut spills: BTreeSet<String> = BTreeSet::new();
    let mut total_spill_cost = 0u64;

    while let Some(v) = stack.pop() {
        let mut used = BTreeSet::new();
        for nbr in &interference[&v] {
            if let Some(&c) = colors.get(nbr) {
                used.insert(c);
            }
        }
        if let Some(c) = (0..k).find(|c| !used.contains(c)) {
            colors.insert(v, c);
        } else {
            spills.insert(v.clone());
            total_spill_cost += *costs.get(&v).unwrap_or(&1);
        }
    }

    ColoringResult {
        colors,
        spills,
        total_spill_cost,
        interference: interference.clone(),
    }
}

fn verify_coloring(res: &ColoringResult) {
    for (u, nbrs) in &res.interference {
        if let Some(&cu) = res.colors.get(u) {
            for v in nbrs {
                if let Some(&cv) = res.colors.get(v) {
                    assert_ne!(cu, cv, "interfering vars {u} and {v} share color {cu}");
                }
            }
        }
    }
}

fn main() {
    // Outer setup sets `outer_cold`, inner loop keeps `a, b, c` simultaneously live in a K4 clique with `outer_cold`.
    let blocks = vec![
        CfgBlock {
            loop_depth: 0,
            instrs: vec![Instr {
                def: Some("outer_cold".into()),
                uses: vec![],
            }],
            succs: vec![1],
        },
        CfgBlock {
            loop_depth: 2,
            instrs: vec![
                Instr {
                    def: Some("a".into()),
                    uses: vec![],
                },
                Instr {
                    def: Some("b".into()),
                    uses: vec!["a".into()],
                },
                Instr {
                    def: Some("c".into()),
                    uses: vec!["a".into(), "b".into()],
                },
                Instr {
                    def: None,
                    uses: vec!["a".into(), "b".into(), "c".into()],
                },
            ],
            succs: vec![1, 2],
        },
        CfgBlock {
            loop_depth: 0,
            instrs: vec![Instr {
                def: None,
                uses: vec!["c".into(), "outer_cold".into()],
            }],
            succs: vec![],
        },
    ];
    let (ig, costs) = build_interference_and_costs(&blocks);
    let res = kempe_color(&ig, &costs, 3);
    verify_coloring(&res);
    println!("Interference: {ig:?}");
    println!("Weighted spill costs: {costs:?}");
    println!(
        "K=3 coloring: {:?}, spills={:?}, spill_cost={}",
        res.colors, res.spills, res.total_spill_cost
    );
}

#[test]
fn colors_planar_k3_graph_without_spills() {
    // Diamond-shaped 3-colorable interference graph (not K4).
    let ig = BTreeMap::from([
        ("a".into(), BTreeSet::from(["b".into(), "c".into()])),
        (
            "b".into(),
            BTreeSet::from(["a".into(), "c".into(), "d".into()]),
        ),
        (
            "c".into(),
            BTreeSet::from(["a".into(), "b".into(), "d".into()]),
        ),
        ("d".into(), BTreeSet::from(["b".into(), "c".into()])),
    ]);
    let costs = BTreeMap::from([
        ("a".into(), 10),
        ("b".into(), 10),
        ("c".into(), 10),
        ("d".into(), 10),
    ]);
    let res = kempe_color(&ig, &costs, 3);
    verify_coloring(&res);
    assert!(res.spills.is_empty());
    assert_eq!(res.colors.len(), 4);
}

#[test]
fn spills_lowest_cost_outer_variable_on_k4_clique_with_k3() {
    let blocks = vec![
        CfgBlock {
            loop_depth: 0,
            instrs: vec![Instr {
                def: Some("outer_cold".into()),
                uses: vec![],
            }],
            succs: vec![1],
        },
        CfgBlock {
            loop_depth: 2,
            instrs: vec![
                Instr {
                    def: Some("a".into()),
                    uses: vec![],
                },
                Instr {
                    def: Some("b".into()),
                    uses: vec!["a".into()],
                },
                Instr {
                    def: Some("c".into()),
                    uses: vec!["a".into(), "b".into()],
                },
                Instr {
                    def: None,
                    uses: vec!["a".into(), "b".into(), "c".into()],
                },
            ],
            succs: vec![1, 2],
        },
        CfgBlock {
            loop_depth: 0,
            instrs: vec![Instr {
                def: None,
                uses: vec!["c".into(), "outer_cold".into()],
            }],
            succs: vec![],
        },
    ];
    let (ig, costs) = build_interference_and_costs(&blocks);
    // All 4 vars mutually interfere inside B1.
    assert_eq!(ig["outer_cold"].len(), 3);
    let res = kempe_color(&ig, &costs, 3);
    verify_coloring(&res);
    assert_eq!(res.spills, BTreeSet::from(["outer_cold".to_string()]));
    assert_eq!(res.colors.len(), 3);
}
