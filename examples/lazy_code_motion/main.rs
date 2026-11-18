//! Lazy Code Motion (Partial Redundancy Elimination) with critical-edge splitting and 4 data-flow passes.

use std::collections::BTreeMap;

/// Basic block summarized for a single normalized expression `e`
/// (assuming any `e_use` precedes any `e_kill` within the block).
#[derive(Clone, Debug, PartialEq, Eq)]
struct LcmBlock {
    name: String,
    e_use: bool,
    e_kill: bool,
    succs: Vec<usize>,
}

/// Intermediate bitvectors and final placement/replacement decisions produced by LCM.
#[derive(Clone, Debug, PartialEq, Eq)]
struct LcmSolution {
    blocks: Vec<LcmBlock>,
    anticipated_in: Vec<bool>,
    available_in: Vec<bool>,
    earliest: Vec<bool>,
    postponable_in: Vec<bool>,
    latest: Vec<bool>,
    used_out: Vec<bool>,
    insert_at_entry: Vec<bool>,
    replace_use: Vec<bool>,
}

/// Split every critical edge `b -> s` (where `b` has >1 successor and `s` has >1 predecessor).
///
/// Why: without a dedicated landing-pad block on `b -> s`, placing `t = e` in `b` would
/// execute `e` speculatively on `b`'s other branch, while placing it in `s` would miss
/// `b -> s` or duplicate work for `s`'s other predecessors.
fn split_critical_edges(blocks: &[LcmBlock]) -> Vec<LcmBlock> {
    let n = blocks.len();
    let mut preds = vec![0usize; n];
    for blk in blocks {
        for &s in &blk.succs {
            preds[s] += 1;
        }
    }
    let mut out = blocks.to_vec();
    for b in 0..n {
        if out[b].succs.len() > 1 {
            let succs = out[b].succs.clone();
            for (slot, &s) in succs.iter().enumerate() {
                if preds[s] > 1 {
                    let split_id = out.len();
                    out.push(LcmBlock {
                        name: format!("split_{b}_{s}"),
                        e_use: false,
                        e_kill: false,
                        succs: vec![s],
                    });
                    out[b].succs[slot] = split_id;
                }
            }
        }
    }
    out
}

/// Knoop-Rüthing-Steffen Lazy Code Motion (§9.5.5) on a single target expression `e`
/// assuming `e_use` occurs before any `e_kill` in a block (or single-stmt blocks).
fn solve_lcm(raw_blocks: &[LcmBlock], entry: usize, exit: usize) -> LcmSolution {
    let blocks = split_critical_edges(raw_blocks);
    let n = blocks.len();
    let mut preds = vec![Vec::new(); n];
    for (b, blk) in blocks.iter().enumerate() {
        for &s in &blk.succs {
            preds[s].push(b);
        }
    }

    // Pass 1: Anticipated (backward must / &&, initialized to true except at exit).
    // `ant_in[b]` means *every* path from entry of `b` to `exit` evaluates `e` before any kill.
    // This guarantees soundness: inserting `e` at a non-anticipated point would be speculative.
    let mut ant_in: Vec<bool> = (0..n).map(|b| b != exit).collect();
    let mut ant_out = vec![false; n];
    loop {
        let mut changed = false;
        for b in (0..n).rev() {
            if b == exit {
                continue;
            }
            let new_out = if blocks[b].succs.is_empty() {
                false
            } else {
                blocks[b].succs.iter().all(|&s| ant_in[s])
            };
            let new_in = blocks[b].e_use || (new_out && !blocks[b].e_kill);
            if new_out != ant_out[b] || new_in != ant_in[b] {
                ant_out[b] = new_out;
                ant_in[b] = new_in;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }

    // Pass 2: Available (forward must / &&, initialized to true except at entry).
    // Marks blocks where `e` would already be computed (either anticipated earlier or flowing in)
    // and not killed; `earliest` is the frontier where anticipation first begins.
    let mut avail_in = vec![false; n];
    let mut avail_out: Vec<bool> = (0..n).map(|b| b != entry).collect();
    loop {
        let mut changed = false;
        for b in 0..n {
            if b == entry {
                let new_out = (ant_in[b] || avail_in[b]) && !blocks[b].e_kill;
                if new_out != avail_out[b] {
                    avail_out[b] = new_out;
                    changed = true;
                }
                continue;
            }
            let new_in = !preds[b].is_empty() && preds[b].iter().all(|&p| avail_out[p]);
            let new_out = (ant_in[b] || new_in) && !blocks[b].e_kill;
            if new_in != avail_in[b] || new_out != avail_out[b] {
                avail_in[b] = new_in;
                avail_out[b] = new_out;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }

    // Earliest safe placement frontier: anticipated here, and not already available from predecessors.
    let earliest: Vec<bool> = (0..n).map(|b| ant_in[b] && !avail_in[b]).collect();

    // Pass 3: Postponable (forward must / &&, initialized to true except at entry).
    // `post_in[b]` means an `earliest` placement can be delayed past predecessors down to `b`'s entry
    // without passing an `e_use` or losing full coverage.
    let mut post_in = vec![false; n];
    let mut post_out: Vec<bool> = (0..n).map(|b| b != entry).collect();
    loop {
        let mut changed = false;
        for b in 0..n {
            if b == entry {
                let new_out = (earliest[b] || post_in[b]) && !blocks[b].e_use;
                if new_out != post_out[b] {
                    post_out[b] = new_out;
                    changed = true;
                }
                continue;
            }
            let new_in = !preds[b].is_empty() && preds[b].iter().all(|&p| post_out[p]);
            let new_out = (earliest[b] || new_in) && !blocks[b].e_use;
            if new_in != post_in[b] || new_out != post_out[b] {
                post_in[b] = new_in;
                post_out[b] = new_out;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }

    // `latest[b]` is the furthest point placement can be pushed: either `b` itself uses `e`,
    // or at least one successor cannot receive a postponed placement (preventing code duplication).
    let latest: Vec<bool> = (0..n)
        .map(|b| {
            if b == exit || blocks[b].succs.is_empty() {
                false
            } else {
                let placed_at_or_before = earliest[b] || post_in[b];
                let all_succ_postponable =
                    blocks[b].succs.iter().all(|&s| earliest[s] || post_in[s]);
                placed_at_or_before && (blocks[b].e_use || !all_succ_postponable)
            }
        })
        .collect();

    // Pass 4: Used (backward may / ||, initialized to false).
    // Determines whether the temporary computed at a `latest` site is actually read on some
    // downstream path before hitting another `latest` site, avoiding needless temporary copies.
    let mut used_in = vec![false; n];
    let mut used_out = vec![false; n];
    loop {
        let mut changed = false;
        for b in (0..n).rev() {
            if b == exit {
                continue;
            }
            let new_out = blocks[b].succs.iter().any(|&s| used_in[s]);
            let new_in = (blocks[b].e_use || new_out) && !latest[b];
            if new_out != used_out[b] || new_in != used_in[b] {
                used_out[b] = new_out;
                used_in[b] = new_in;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }

    // Final code-motion decisions:
    // - Insert `t = e` at entry of `b` iff `b` is `latest` and `t` is used later (`used_out[b]`).
    // - Replace original `e_use` with `t` unless `b` is a non-reused `latest` block itself.
    let insert_at_entry: Vec<bool> = (0..n).map(|b| latest[b] && used_out[b]).collect();
    let replace_use: Vec<bool> = (0..n)
        .map(|b| blocks[b].e_use && (!latest[b] || used_out[b]))
        .collect();

    LcmSolution {
        blocks,
        anticipated_in: ant_in,
        available_in: avail_in,
        earliest,
        postponable_in: post_in,
        latest,
        used_out,
        insert_at_entry,
        replace_use,
    }
}

// Count dynamic evaluations of `e` along any path `0 -> ... -> exit` before vs after LCM.
fn count_evals_along_path(
    raw_blocks: &[LcmBlock],
    sol: &LcmSolution,
    path: &[usize],
) -> (usize, usize) {
    let before = path.iter().filter(|&&b| raw_blocks[b].e_use).count();
    // Map raw path edges onto the possibly edge-split LCM CFG.
    let mut split_path = vec![path[0]];
    for w in path.windows(2) {
        let (u, v) = (w[0], w[1]);
        if sol.blocks[u].succs.contains(&v) {
            split_path.push(v);
        } else {
            let bridge = sol.blocks[u]
                .succs
                .iter()
                .copied()
                .find(|&s| sol.blocks[s].succs == vec![v])
                .expect("missing edge bridge");
            split_path.push(bridge);
            split_path.push(v);
        }
    }
    let after: usize = split_path
        .iter()
        .map(|&b| {
            let inserted = usize::from(sol.insert_at_entry[b]);
            // Original uses not replaced by `t` remain evaluated.
            let unreplaced = usize::from(sol.blocks[b].e_use && !sol.replace_use[b]);
            inserted + unreplaced
        })
        .sum();
    (before, after)
}

fn partial_diamond_sample() -> Vec<LcmBlock> {
    // Entry(0) branches to Left(1) (computes e) or Join(2) (critical edge 0 -> 2, and 2 computes e) -> Exit(3).
    vec![
        LcmBlock {
            name: "entry".into(),
            e_use: false,
            e_kill: false,
            succs: vec![1, 2],
        },
        LcmBlock {
            name: "hot".into(),
            e_use: true,
            e_kill: false,
            succs: vec![2],
        },
        LcmBlock {
            name: "join".into(),
            e_use: true,
            e_kill: false,
            succs: vec![3],
        },
        LcmBlock {
            name: "exit".into(),
            e_use: false,
            e_kill: false,
            succs: vec![],
        },
    ]
}

fn loop_preheader_sample() -> Vec<LcmBlock> {
    // preheader(0) -> header(1) -> {body(2), exit(3)}; body(2) -> header(1). `body` computes invariant `e`.
    vec![
        LcmBlock {
            name: "preheader".into(),
            e_use: false,
            e_kill: false,
            succs: vec![1],
        },
        LcmBlock {
            name: "header".into(),
            e_use: false,
            e_kill: false,
            succs: vec![2, 3],
        },
        LcmBlock {
            name: "body".into(),
            e_use: true,
            e_kill: false,
            succs: vec![1],
        },
        LcmBlock {
            name: "exit".into(),
            e_use: false,
            e_kill: false,
            succs: vec![],
        },
    ]
}

fn main() {
    let diamond = partial_diamond_sample();
    let sol = solve_lcm(&diamond, 0, 3);
    println!("Partial diamond after critical-edge split:");
    for (b, blk) in sol.blocks.iter().enumerate() {
        println!(
            "  B{b} ({}): ant={}, avail={}, ear={}, post={}, latest={}, used_out={}, insert={}, replace={}",
            blk.name,
            sol.anticipated_in[b],
            sol.available_in[b],
            sol.earliest[b],
            sol.postponable_in[b],
            sol.latest[b],
            sol.used_out[b],
            sol.insert_at_entry[b],
            sol.replace_use[b]
        );
    }
    println!(
        "Hot path [0,1,2,3] evals before/after: {:?}",
        count_evals_along_path(&diamond, &sol, &[0, 1, 2, 3])
    );

    let loop_cfg = loop_preheader_sample();
    let loop_sol = solve_lcm(&loop_cfg, 0, 3);
    let hoisted: BTreeMap<&str, bool> = loop_sol
        .blocks
        .iter()
        .enumerate()
        .map(|(i, b)| (b.name.as_str(), loop_sol.insert_at_entry[i]))
        .collect();
    println!("Loop LCM insertions: {hoisted:?}");
}

#[test]
fn partial_diamond_splits_critical_edge_and_eliminates_double_eval() {
    let diamond = partial_diamond_sample();
    let sol = solve_lcm(&diamond, 0, 3);
    // Critical edge 0 -> 2 was split into `split_0_2` (block index 4).
    assert_eq!(sol.blocks.len(), 5);
    let split_idx = 4usize;
    assert_eq!(sol.blocks[split_idx].name, "split_0_2");
    assert!(sol.insert_at_entry[split_idx]); // hoisted onto cold edge only
    assert!(sol.insert_at_entry[1]); // computed once in hot block
    assert!(!sol.insert_at_entry[2]); // eliminated at join
    assert!(sol.replace_use[2]);

    assert_eq!(
        count_evals_along_path(&diamond, &sol, &[0, 1, 2, 3]),
        (2, 1)
    );
    assert_eq!(count_evals_along_path(&diamond, &sol, &[0, 2, 3]), (1, 1));
}

#[test]
fn loop_invariant_hoisted_without_evaluating_on_zero_trip_path() {
    let loop_cfg = loop_preheader_sample();
    // Since `header -> exit` can skip the loop (`0` trips), `body` is NOT anticipated at `preheader`.
    let sol = solve_lcm(&loop_cfg, 0, 3);
    assert!(!sol.insert_at_entry[0]);
    assert!(sol.blocks[2].e_use);

    // Do-while style loop (preheader 0 -> body 1 -> {body 1, exit 2}) IS always executed >= 1 time:
    // here invariant `e` IS anticipated and hoisted into preheader `B0`!
    let do_while = vec![
        LcmBlock {
            name: "preheader".into(),
            e_use: false,
            e_kill: false,
            succs: vec![1],
        },
        LcmBlock {
            name: "body".into(),
            e_use: true,
            e_kill: false,
            succs: vec![1, 2],
        },
        LcmBlock {
            name: "exit".into(),
            e_use: false,
            e_kill: false,
            succs: vec![],
        },
    ];
    let dw_sol = solve_lcm(&do_while, 0, 2);
    assert!(dw_sol.insert_at_entry[0]);
    assert!(!dw_sol.insert_at_entry[1]);
    assert!(dw_sol.replace_use[1]);
    let (before_3, after_3) = count_evals_along_path(&do_while, &dw_sol, &[0, 1, 1, 1, 2]);
    assert_eq!((before_3, after_3), (3, 1));
}
