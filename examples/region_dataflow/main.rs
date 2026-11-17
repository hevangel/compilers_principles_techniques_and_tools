//! Construct T1/T2 reducible region trees and compose affine (GEN, KILL) transfer functions.

use std::collections::{BTreeMap, BTreeSet};

// Affine bitvector transfer function `f(X) = gen | (X & !kill)` for up to 64 facts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct TransferFn {
    gen: u64,
    kill: u64,
}

impl TransferFn {
    fn identity() -> Self {
        Self { gen: 0, kill: 0 }
    }

    fn apply(&self, x: u64) -> u64 {
        self.gen | (x & !self.kill)
    }

    // Compose `second(first(X))`.
    fn compose(second: Self, first: Self) -> Self {
        Self {
            gen: second.gen | (first.gen & !second.kill),
            kill: second.kill | first.kill,
        }
    }

    // Meet (may/union) `f1(X) | f2(X)` = `(gen1 | gen2) | (X & !(kill1 & kill2))`.
    fn union_meet(a: Self, b: Self) -> Self {
        Self {
            gen: a.gen | b.gen,
            kill: a.kill & b.kill,
        }
    }

    // Kleene loop closure for monotone may-analysis:
    // `(GEN | (X & !KILL))` repeated >= 0 times with identity yields `(GEN, 0)`?
    // Careful: `f*(X) = I(X) | f(X) | f(f(X)) | ... = X | GEN_R` where `X` is NOT killed because
    // 0 trips through a self-loop preserves incoming X, OR for region exit after body it executes >= 1 time!
    // Dragon Book §9.7.5 (L18100+): f* = I /\ f /\ f^2 ... = (gen, 0) for union meet!
    fn star(self) -> Self {
        Self {
            gen: self.gen,
            kill: 0,
        }
    }
}

#[derive(Clone, Debug)]
enum Region {
    Leaf {
        block_id: usize,
        tf: TransferFn,
    },
    Seq {
        id: usize,
        header: Box<Region>,
        body: Box<Region>,
        // Transfer out of header and out of body back-edge/exit.
        tf_body_in: TransferFn,
        tf_out: TransferFn,
    },
    Loop {
        id: usize,
        inner: Box<Region>,
        tf_out: TransferFn,
    },
}

impl Region {
    fn id(&self) -> usize {
        match self {
            Region::Leaf { block_id, .. } => *block_id,
            Region::Seq { id, .. } | Region::Loop { id, .. } => *id,
        }
    }

    fn summary(&self) -> TransferFn {
        match self {
            Region::Leaf { tf, .. } => *tf,
            Region::Seq { tf_out, .. } | Region::Loop { tf_out, .. } => *tf_out,
        }
    }
    fn tree_shape(&self) -> String {
        match self {
            Region::Leaf { block_id, .. } => format!("B{block_id}"),
            Region::Seq {
                id,
                header,
                body,
                tf_body_in,
                ..
            } => format!(
                "Seq#{id}({} ->[gen={:b}] {})",
                header.tree_shape(),
                tf_body_in.gen,
                body.tree_shape()
            ),
            Region::Loop { id, inner, .. } => format!("Loop#{id}({})", inner.tree_shape()),
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
struct IrreducibleError {
    remaining_nodes: Vec<usize>,
}

// Build region tree via T1 (self-edge loop) & T2 (single-predecessor folding) for linear/single-exit chains & loops,
// plus proper meet across branches (`join`) when folding into a region!
// Let's handle general DAG/reducible CFG via structural T1/T2:
// - T1: node `u` has self-edge `u -> u` -> wrap `u` in `Loop` with `f* = f.star()` (preserving self-loop).
// - T2: node `v != entry` has unique predecessor `u` (not `v`) AND `u`'s successors allow absorbing `(u, v)` cleanly:
//   Wait: if `u` has 2 successors (`v` and `w`) and `v` also flows into `w` (`if/else` or `loop`),
//   Notice standard T2 folds `v` into `u` whenever `preds[v] == {u}`, redirecting `v`'s outgoing edges to come from merged `(u, v)`.
//   When `u` ends up with BOTH a self-edge (`u -> u` via absorbed latch) and an exit edge, T1 fires on `u`!
//   Even for a diamond `0 -> {1, 2} -> 3`, wait: `3` has TWO preds (`1, 2`), while `1` and `2` each have ONE pred (`0`)!
//   When `0` absorbs `1` (`01 -> {2, 3}`) and then `01` absorbs `2` (`012 -> {3, 3}`), `3` now has ONLY ONE pred (`012`) so `012` absorbs `3`!
//   THAT'S the full power of T1/T2 reduction when multiple edges are tracked properly via sets!

#[derive(Clone, Debug)]
struct EdgeSummary {
    // Summary from region entry to exit edge `(target)`.
    tf: TransferFn,
}

fn reduce_t1_t2(
    num_blocks: usize,
    edges: &[(usize, usize)],
    block_tfs: &[TransferFn],
) -> Result<RegionGraphSummary, IrreducibleError> {
    let mut regions: BTreeMap<usize, Region> = (0..num_blocks)
        .map(|b| {
            (
                b,
                Region::Leaf {
                    block_id: b,
                    tf: block_tfs[b],
                },
            )
        })
        .collect();
    // Outgoing summary transfer functions per `(u, v)` edge (from entry of `u` to just before `v`).
    let mut out_edges: BTreeMap<usize, BTreeMap<usize, EdgeSummary>> =
        (0..num_blocks).map(|b| (b, BTreeMap::new())).collect();
    let mut in_preds: BTreeMap<usize, BTreeSet<usize>> =
        (0..num_blocks).map(|b| (b, BTreeSet::new())).collect();

    for &(u, v) in edges {
        out_edges
            .get_mut(&u)
            .unwrap()
            .insert(v, EdgeSummary { tf: block_tfs[u] });
        in_preds.get_mut(&v).unwrap().insert(u);
    }

    // Structural history log for top-down IN propagation.
    #[derive(Clone, Debug)]
    enum Step {
        T1 {
            new_id: usize,
            old_id: usize,
            self_tf: TransferFn,
        },
        T2 {
            new_id: usize,
            u: usize,
            v: usize,
            u_to_v_tf: TransferFn,
        },
    }

    let mut steps = Vec::new();
    let mut next_id = num_blocks;
    let mut entry = 0usize;

    loop {
        if regions.len() == 1 && out_edges[&entry].is_empty() {
            break;
        }
        // Try T1: any node `u` with a self-edge `u -> u`.
        let t1_node = regions
            .keys()
            .copied()
            .find(|u| out_edges[u].contains_key(u));
        if let Some(u) = t1_node {
            let self_edge = out_edges.get_mut(&u).unwrap().remove(&u).unwrap();
            in_preds.get_mut(&u).unwrap().remove(&u);
            let closure = self_edge.tf.star();

            let old_reg = regions.remove(&u).unwrap();
            let new_id = next_id;
            next_id += 1;
            let new_reg = Region::Loop {
                id: new_id,
                inner: Box::new(old_reg),
                tf_out: TransferFn::compose(closure, TransferFn::identity()),
            };
            regions.insert(new_id, new_reg);

            // Rewire outgoing edges of `u`: new transfer to each target `w` is `compose(old_edge(u->w), closure)`.
            let old_out = out_edges.remove(&u).unwrap();
            let mut new_out = BTreeMap::new();
            for (w, summ) in old_out {
                in_preds.get_mut(&w).unwrap().remove(&u);
                in_preds.get_mut(&w).unwrap().insert(new_id);
                new_out.insert(
                    w,
                    EdgeSummary {
                        tf: TransferFn::compose(summ.tf, closure),
                    },
                );
            }
            out_edges.insert(new_id, new_out);

            // Rewire incoming edges into `u` to point to `new_id`.
            let preds_u: Vec<usize> = in_preds.remove(&u).unwrap().into_iter().collect();
            in_preds.insert(new_id, preds_u.iter().copied().collect());
            for p in preds_u {
                let edge_p_u = out_edges.get_mut(&p).unwrap().remove(&u).unwrap();
                out_edges.get_mut(&p).unwrap().insert(new_id, edge_p_u);
            }
            if entry == u {
                entry = new_id;
            }
            steps.push(Step::T1 {
                new_id,
                old_id: u,
                self_tf: self_edge.tf,
            });
            continue;
        }

        // Try T2: any node `v != entry` with a unique predecessor `u != v`.
        let t2_pair = regions.keys().copied().find_map(|v| {
            if v == entry {
                return None;
            }
            let ps = &in_preds[&v];
            if ps.len() == 1 {
                let u = *ps.iter().next().unwrap();
                if u != v {
                    return Some((u, v));
                }
            }
            None
        });

        if let Some((u, v)) = t2_pair {
            let u_to_v = out_edges.get_mut(&u).unwrap().remove(&v).unwrap();
            in_preds.remove(&v);

            let reg_u = regions.remove(&u).unwrap();
            let reg_v = regions.remove(&v).unwrap();
            let new_id = next_id;
            next_id += 1;

            let v_out = out_edges.remove(&v).unwrap();
            let u_out = out_edges.remove(&u).unwrap();
            let mut new_out: BTreeMap<usize, EdgeSummary> = BTreeMap::new();

            for (w, summ_uw) in u_out {
                let target = if w == u || w == v { new_id } else { w };
                if w != u && w != v {
                    in_preds.get_mut(&w).unwrap().remove(&u);
                    in_preds.get_mut(&w).unwrap().insert(new_id);
                }
                new_out
                    .entry(target)
                    .and_modify(|e| e.tf = TransferFn::union_meet(e.tf, summ_uw.tf))
                    .or_insert(summ_uw);
            }
            for (w, summ_vw) in v_out {
                let composed = TransferFn::compose(summ_vw.tf, u_to_v.tf);
                let target = if w == u || w == v { new_id } else { w };
                if w != u && w != v {
                    in_preds.get_mut(&w).unwrap().remove(&v);
                    in_preds.get_mut(&w).unwrap().insert(new_id);
                }
                new_out
                    .entry(target)
                    .and_modify(|e| e.tf = TransferFn::union_meet(e.tf, composed))
                    .or_insert(EdgeSummary { tf: composed });
            }

            // Rewire incoming edges to `u` -> `new_id`.
            let mut new_preds: BTreeSet<usize> = BTreeSet::new();
            for p in in_preds.remove(&u).unwrap() {
                if p == u || p == v {
                    continue;
                }
                if let Some(edge_pu) = out_edges.get_mut(&p).unwrap().remove(&u) {
                    out_edges
                        .get_mut(&p)
                        .unwrap()
                        .entry(new_id)
                        .and_modify(|e| e.tf = TransferFn::union_meet(e.tf, edge_pu.tf))
                        .or_insert(edge_pu);
                    new_preds.insert(p);
                }
            }
            if new_out.contains_key(&new_id) {
                new_preds.insert(new_id);
            }
            in_preds.insert(new_id, new_preds);
            out_edges.insert(new_id, new_out);

            let comb_tf = reg_u.summary();
            regions.insert(
                new_id,
                Region::Seq {
                    id: new_id,
                    header: Box::new(reg_u),
                    body: Box::new(reg_v),
                    tf_body_in: u_to_v.tf,
                    tf_out: comb_tf,
                },
            );
            if entry == u {
                entry = new_id;
            }
            steps.push(Step::T2 {
                new_id,
                u,
                v,
                u_to_v_tf: u_to_v.tf,
            });
            continue;
        }

        // Neither T1 nor T2 applies with > 1 nodes (or irreducible cycles left).
        return Err(IrreducibleError {
            remaining_nodes: regions.keys().copied().collect(),
        });
    }

    // Top-down IN propagation along reverse construction steps.
    let mut region_in: BTreeMap<usize, u64> = BTreeMap::new();
    region_in.insert(entry, 0u64);
    for step in steps.iter().rev() {
        match step {
            Step::T1 {
                new_id,
                old_id,
                self_tf,
            } => {
                let outer_in = region_in[new_id];
                let loop_in = self_tf.star().apply(outer_in);
                region_in.insert(*old_id, loop_in);
            }
            Step::T2 {
                new_id,
                u,
                v,
                u_to_v_tf,
            } => {
                let in_parent = region_in[new_id];
                region_in.insert(*u, in_parent);
                region_in.insert(*v, u_to_v_tf.apply(in_parent));
            }
        }
    }

    let in_blocks: Vec<u64> = (0..num_blocks).map(|b| region_in[&b]).collect();
    let root_shape = format!(
        "R{}: {}",
        regions[&entry].id(),
        regions[&entry].tree_shape()
    );
    Ok(RegionGraphSummary {
        in_blocks,
        step_count: steps.len(),
        root_shape,
    })
}

#[derive(Debug, PartialEq, Eq)]
struct RegionGraphSummary {
    in_blocks: Vec<u64>,
    step_count: usize,
    root_shape: String,
}

fn iterative_mfp_reaching_defs(
    num_blocks: usize,
    edges: &[(usize, usize)],
    block_tfs: &[TransferFn],
) -> Vec<u64> {
    let mut preds = vec![Vec::new(); num_blocks];
    for &(u, v) in edges {
        preds[v].push(u);
    }
    let mut in_b = vec![0u64; num_blocks];
    let mut out_b: Vec<u64> = block_tfs.iter().map(|tf| tf.apply(0)).collect();
    loop {
        let mut changed = false;
        for b in 0..num_blocks {
            let new_in = preds[b].iter().fold(0u64, |acc, &p| acc | out_b[p]);
            let new_out = block_tfs[b].apply(new_in);
            if new_in != in_b[b] || new_out != out_b[b] {
                in_b[b] = new_in;
                out_b[b] = new_out;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    in_b
}

fn multi_loop_reducible_sample() -> (usize, Vec<(usize, usize)>, Vec<TransferFn>) {
    // Entry 0 -> outer header 1 -> {inner header 2, exit 5}
    // inner header 2 -> {inner body 3, outer latch 4}
    // inner body 3 -> inner header 2
    // outer latch 4 -> outer header 1
    let edges = vec![(0, 1), (1, 2), (1, 5), (2, 3), (2, 4), (3, 2), (4, 1)];
    let block_tfs = vec![
        TransferFn {
            gen: 0b0001,
            kill: 0b0000,
        }, // B0 defs d0
        TransferFn {
            gen: 0b0010,
            kill: 0b0001,
        }, // B1 defs d1, kills d0
        TransferFn {
            gen: 0b0100,
            kill: 0b0000,
        }, // B2 defs d2
        TransferFn {
            gen: 0b1000,
            kill: 0b0010,
        }, // B3 defs d3, kills d1
        TransferFn {
            gen: 0b1_0000,
            kill: 0b0100,
        }, // B4 defs d4, kills d2
        TransferFn { gen: 0, kill: 0 }, // B5 exit
    ];
    (6, edges, block_tfs)
}

fn main() {
    let (n, edges, tfs) = multi_loop_reducible_sample();
    let reg_res = reduce_t1_t2(n, &edges, &tfs).expect("sample should be reducible");
    let mfp = iterative_mfp_reaching_defs(n, &edges, &tfs);
    assert_eq!(reg_res.in_blocks, mfp);
    println!(
        "T1/T2 steps = {}, tree = {}",
        reg_res.step_count, reg_res.root_shape
    );
    for (b, (reg_in, mfp_in)) in reg_res.in_blocks.iter().zip(&mfp).enumerate() {
        println!("  B{b}: region IN={reg_in:05b} | iterative MFP IN={mfp_in:05b}");
    }

    // Irreducible double-entry loop: 0 -> {1, 2}, 1 -> 2, 2 -> 1.
    let irred = reduce_t1_t2(
        3,
        &[(0, 1), (0, 2), (1, 2), (2, 1)],
        &[TransferFn::identity(); 3],
    );
    println!("Irreducible CFG reduction result: {irred:?}");
}

#[test]
fn region_based_reaching_defs_matches_iterative_mfp_on_nested_loops() {
    let (n, edges, tfs) = multi_loop_reducible_sample();
    let reg_res = reduce_t1_t2(n, &edges, &tfs).expect("expected reducible CFG");
    let mfp = iterative_mfp_reaching_defs(n, &edges, &tfs);
    assert_eq!(reg_res.in_blocks, mfp);
}

#[test]
fn detects_irreducible_double_entry_loop_with_t1_t2_stymie() {
    let res = reduce_t1_t2(
        3,
        &[(0, 1), (0, 2), (1, 2), (2, 1)],
        &[TransferFn::identity(); 3],
    );
    assert!(
        matches!(res, Err(IrreducibleError { remaining_nodes }) if remaining_nodes == vec![0, 1, 2])
    );
}
