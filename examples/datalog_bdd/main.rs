//! Evaluate semi-naive Datalog points-to rules and solve relational fixed points with an ROBDD.

use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Term {
    Var(usize),
    Const(u32),
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Atom {
    pred: &'static str,
    terms: Vec<Term>,
}

#[derive(Clone, Debug)]
struct Rule {
    head: Atom,
    body: Vec<Atom>,
}

type Rel = BTreeSet<Vec<u32>>;
type Db = BTreeMap<&'static str, Rel>;

#[derive(Clone, Debug, PartialEq, Eq)]
struct EvalStats {
    rounds: usize,
    derivations: usize,
}

fn match_atom(atom: &Atom, tuple: &[u32], env: &mut BTreeMap<usize, u32>) -> bool {
    if atom.terms.len() != tuple.len() {
        return false;
    }
    let mut added = Vec::new();
    for (&term, &val) in atom.terms.iter().zip(tuple) {
        match term {
            Term::Const(c) => {
                if c != val {
                    for k in added {
                        env.remove(&k);
                    }
                    return false;
                }
            }
            Term::Var(v) => {
                if let Some(&bound) = env.get(&v) {
                    if bound != val {
                        for k in added {
                            env.remove(&k);
                        }
                        return false;
                    }
                } else {
                    env.insert(v, val);
                    added.push(v);
                }
            }
        }
    }
    true
}

fn instantiate_head(head: &Atom, env: &BTreeMap<usize, u32>) -> Vec<u32> {
    head.terms
        .iter()
        .map(|t| match *t {
            Term::Const(c) => c,
            Term::Var(v) => env[&v],
        })
        .collect()
}

struct JoinCtx<'a> {
    rule: &'a Rule,
    delta_slot: Option<usize>,
    db: &'a Db,
    deltas: &'a Db,
}

fn join_rule_slot(
    rule: &Rule,
    delta_slot: Option<usize>,
    db: &Db,
    deltas: &Db,
    out_tuples: &mut Rel,
    derivations: &mut usize,
) {
    fn dfs(
        ctx: &JoinCtx<'_>,
        idx: usize,
        env: &mut BTreeMap<usize, u32>,
        out_tuples: &mut Rel,
        derivations: &mut usize,
    ) {
        if idx == ctx.rule.body.len() {
            *derivations += 1;
            out_tuples.insert(instantiate_head(&ctx.rule.head, env));
            return;
        }
        let atom = &ctx.rule.body[idx];
        let source = if ctx.delta_slot == Some(idx) {
            ctx.deltas
        } else {
            ctx.db
        };
        if let Some(rel) = source.get(atom.pred) {
            for tuple in rel {
                let snapshot = env.clone();
                if match_atom(atom, tuple, env) {
                    dfs(ctx, idx + 1, env, out_tuples, derivations);
                }
                *env = snapshot;
            }
        }
    }

    let ctx = JoinCtx {
        rule,
        delta_slot,
        db,
        deltas,
    };
    let mut env = BTreeMap::new();
    dfs(&ctx, 0, &mut env, out_tuples, derivations);
}

fn evaluate_naive(edb: &Db, rules: &[Rule]) -> (Db, EvalStats) {
    let mut db = edb.clone();
    for r in rules {
        db.entry(r.head.pred).or_default();
    }
    let mut rounds = 0;
    let mut derivations = 0;

    loop {
        rounds += 1;
        let mut new_facts: Db = BTreeMap::new();
        for rule in rules {
            let mut derived = Rel::new();
            join_rule_slot(rule, None, &db, &db, &mut derived, &mut derivations);
            new_facts.entry(rule.head.pred).or_default().extend(derived);
        }
        let mut changed = false;
        for (pred, tuples) in new_facts {
            let entry = db.entry(pred).or_default();
            for t in tuples {
                if entry.insert(t) {
                    changed = true;
                }
            }
        }
        if !changed {
            return (
                db,
                EvalStats {
                    rounds,
                    derivations,
                },
            );
        }
    }
}

fn evaluate_semi_naive(edb: &Db, rules: &[Rule]) -> (Db, EvalStats) {
    let mut db = edb.clone();
    let mut idb_preds = BTreeSet::new();
    for r in rules {
        idb_preds.insert(r.head.pred);
        db.entry(r.head.pred).or_default();
    }

    let mut rounds = 0;
    let mut derivations = 0;
    let mut deltas: Db = BTreeMap::new();

    // Round 0: fire non-recursive rules (body uses only EDB predicates) to seed initial deltas.
    for rule in rules {
        if rule.body.iter().all(|a| !idb_preds.contains(a.pred)) {
            let mut derived = Rel::new();
            join_rule_slot(rule, None, &db, &db, &mut derived, &mut derivations);
            let existing = db.get(rule.head.pred).unwrap();
            let d_entry = deltas.entry(rule.head.pred).or_default();
            for t in derived {
                if !existing.contains(&t) {
                    d_entry.insert(t);
                }
            }
        }
    }
    for (pred, tuples) in &deltas {
        db.entry(*pred).or_default().extend(tuples.iter().cloned());
    }
    rounds += 1;

    while deltas.values().any(|s| !s.is_empty()) {
        rounds += 1;
        let mut next_deltas: Db = BTreeMap::new();
        for rule in rules {
            let mut derived = Rel::new();
            for (slot, atom) in rule.body.iter().enumerate() {
                if idb_preds.contains(atom.pred)
                    && deltas.get(atom.pred).is_some_and(|s| !s.is_empty())
                {
                    join_rule_slot(
                        rule,
                        Some(slot),
                        &db,
                        &deltas,
                        &mut derived,
                        &mut derivations,
                    );
                }
            }
            let existing = db.get(rule.head.pred).unwrap();
            let nd = next_deltas.entry(rule.head.pred).or_default();
            for t in derived {
                if !existing.contains(&t) {
                    nd.insert(t);
                }
            }
        }
        for (pred, tuples) in &next_deltas {
            db.entry(*pred).or_default().extend(tuples.iter().cloned());
        }
        deltas = next_deltas;
    }

    (
        db,
        EvalStats {
            rounds,
            derivations,
        },
    )
}

// --- Part 2: Arena-backed Reduced Ordered Binary Decision Diagram (ROBDD) ---

const BDD_ZERO: usize = 0;
const BDD_ONE: usize = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum BddOp {
    And,
    Or,
    Xor,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BddNode {
    Zero,
    One,
    Internal { var: usize, low: usize, high: usize },
}

#[derive(Clone, Debug)]
struct Robdd {
    nodes: Vec<BddNode>,
    unique: BTreeMap<(usize, usize, usize), usize>,
    apply_cache: BTreeMap<(BddOp, usize, usize), usize>,
}

impl Robdd {
    fn new() -> Self {
        Self {
            nodes: vec![BddNode::Zero, BddNode::One],
            unique: BTreeMap::new(),
            apply_cache: BTreeMap::new(),
        }
    }

    fn mk(&mut self, var: usize, low: usize, high: usize) -> usize {
        if low == high {
            return low;
        }
        if let Some(&id) = self.unique.get(&(var, low, high)) {
            return id;
        }
        let id = self.nodes.len();
        self.nodes.push(BddNode::Internal { var, low, high });
        self.unique.insert((var, low, high), id);
        id
    }

    fn var(&mut self, v: usize) -> usize {
        self.mk(v, BDD_ZERO, BDD_ONE)
    }

    fn not(&mut self, u: usize) -> usize {
        self.apply(BddOp::Xor, u, BDD_ONE)
    }

    fn and(&mut self, u: usize, v: usize) -> usize {
        self.apply(BddOp::And, u, v)
    }

    fn or(&mut self, u: usize, v: usize) -> usize {
        self.apply(BddOp::Or, u, v)
    }

    fn ite(&mut self, c: usize, t: usize, e: usize) -> usize {
        let ct = self.and(c, t);
        let nc = self.not(c);
        let nce = self.and(nc, e);
        self.or(ct, nce)
    }

    fn var_of(&self, u: usize) -> usize {
        match self.nodes[u] {
            BddNode::Zero | BddNode::One => usize::MAX,
            BddNode::Internal { var, .. } => var,
        }
    }

    fn restrict_step(&self, u: usize, top_var: usize, val: bool) -> usize {
        match self.nodes[u] {
            BddNode::Internal { var, low, high } if var == top_var => {
                if val {
                    high
                } else {
                    low
                }
            }
            _ => u,
        }
    }

    fn apply(&mut self, op: BddOp, u: usize, v: usize) -> usize {
        if u <= 1 && v <= 1 {
            let bu = u == BDD_ONE;
            let bv = v == BDD_ONE;
            let res = match op {
                BddOp::And => bu && bv,
                BddOp::Or => bu || bv,
                BddOp::Xor => bu ^ bv,
            };
            return if res { BDD_ONE } else { BDD_ZERO };
        }
        let key = match op {
            BddOp::And | BddOp::Or | BddOp::Xor => {
                if u <= v {
                    (op, u, v)
                } else {
                    (op, v, u)
                }
            }
        };
        if let Some(&cached) = self.apply_cache.get(&key) {
            return cached;
        }
        let top = self.var_of(u).min(self.var_of(v));
        let u0 = self.restrict_step(u, top, false);
        let v0 = self.restrict_step(v, top, false);
        let u1 = self.restrict_step(u, top, true);
        let v1 = self.restrict_step(v, top, true);
        let low = self.apply(op, u0, v0);
        let high = self.apply(op, u1, v1);
        let res = self.mk(top, low, high);
        self.apply_cache.insert(key, res);
        res
    }

    fn restrict(&mut self, u: usize, target_var: usize, val: bool) -> usize {
        match self.nodes[u] {
            BddNode::Zero | BddNode::One => u,
            BddNode::Internal { var, low, high } => {
                if var > target_var {
                    u
                } else if var == target_var {
                    if val {
                        high
                    } else {
                        low
                    }
                } else {
                    let nl = self.restrict(low, target_var, val);
                    let nh = self.restrict(high, target_var, val);
                    self.mk(var, nl, nh)
                }
            }
        }
    }

    fn exists(&mut self, vars: &[usize], mut u: usize) -> usize {
        for &v in vars {
            let f0 = self.restrict(u, v, false);
            let f1 = self.restrict(u, v, true);
            u = self.or(f0, f1);
        }
        u
    }

    fn substitute(&mut self, u: usize, mapping: &BTreeMap<usize, usize>) -> usize {
        match self.nodes[u] {
            BddNode::Zero | BddNode::One => u,
            BddNode::Internal { var, low, high } => {
                let nl = self.substitute(low, mapping);
                let nh = self.substitute(high, mapping);
                let new_var = *mapping.get(&var).unwrap_or(&var);
                let v_node = self.var(new_var);
                self.ite(v_node, nh, nl)
            }
        }
    }

    fn encode_pair(&mut self, x_vars: &[usize], y_vars: &[usize], x: u32, y: u32) -> usize {
        let mut acc = BDD_ONE;
        for (bit, &vx) in x_vars.iter().enumerate() {
            let lit = self.var(vx);
            let matched = if ((x >> bit) & 1) == 1 {
                lit
            } else {
                self.not(lit)
            };
            acc = self.and(acc, matched);
        }
        for (bit, &vy) in y_vars.iter().enumerate() {
            let lit = self.var(vy);
            let matched = if ((y >> bit) & 1) == 1 {
                lit
            } else {
                self.not(lit)
            };
            acc = self.and(acc, matched);
        }
        acc
    }

    fn decode_pairs(&self, u: usize, x_vars: &[usize], y_vars: &[usize]) -> BTreeSet<(u32, u32)> {
        let domain = 1u32 << x_vars.len();
        let mut out = BTreeSet::new();
        for x in 0..domain {
            for y in 0..domain {
                let mut cur = u;
                while let BddNode::Internal { var, low, high } = self.nodes[cur] {
                    let bit_on = if let Some(pos) = x_vars.iter().position(|&vx| vx == var) {
                        ((x >> pos) & 1) == 1
                    } else if let Some(pos) = y_vars.iter().position(|&vy| vy == var) {
                        ((y >> pos) & 1) == 1
                    } else {
                        false
                    };
                    cur = if bit_on { high } else { low };
                }
                if cur == BDD_ONE {
                    out.insert((x, y));
                }
            }
        }
        out
    }
}

fn points_to_example() -> (Db, Vec<Rule>) {
    // IDs: vars 1=a, 2=b, 3=p, 4=q; heap 10=hA, 11=hB; fields 100=next
    let mut edb = Db::new();
    edb.insert("Alloc", BTreeSet::from([vec![1, 10], vec![2, 11]]));
    edb.insert("Move", BTreeSet::from([vec![3, 1], vec![4, 3]]));
    edb.insert("Store", BTreeSet::from([vec![1, 100, 2]]));
    edb.insert("Load", BTreeSet::from([vec![5, 4, 100]]));

    // Transitive reachability edges alongside points-to rules.
    edb.insert(
        "Edge",
        BTreeSet::from([vec![0, 1], vec![1, 2], vec![2, 3], vec![3, 4], vec![1, 4]]),
    );

    let rules = vec![
        Rule {
            head: Atom {
                pred: "Pts",
                terms: vec![Term::Var(0), Term::Var(1)],
            },
            body: vec![Atom {
                pred: "Alloc",
                terms: vec![Term::Var(0), Term::Var(1)],
            }],
        },
        Rule {
            head: Atom {
                pred: "Pts",
                terms: vec![Term::Var(0), Term::Var(1)],
            },
            body: vec![
                Atom {
                    pred: "Move",
                    terms: vec![Term::Var(0), Term::Var(2)],
                },
                Atom {
                    pred: "Pts",
                    terms: vec![Term::Var(2), Term::Var(1)],
                },
            ],
        },
        Rule {
            head: Atom {
                pred: "HeapPts",
                terms: vec![Term::Var(0), Term::Var(1), Term::Var(2)],
            },
            body: vec![
                Atom {
                    pred: "Store",
                    terms: vec![Term::Var(3), Term::Var(1), Term::Var(4)],
                },
                Atom {
                    pred: "Pts",
                    terms: vec![Term::Var(3), Term::Var(0)],
                },
                Atom {
                    pred: "Pts",
                    terms: vec![Term::Var(4), Term::Var(2)],
                },
            ],
        },
        Rule {
            head: Atom {
                pred: "Pts",
                terms: vec![Term::Var(0), Term::Var(1)],
            },
            body: vec![
                Atom {
                    pred: "Load",
                    terms: vec![Term::Var(0), Term::Var(2), Term::Var(3)],
                },
                Atom {
                    pred: "Pts",
                    terms: vec![Term::Var(2), Term::Var(4)],
                },
                Atom {
                    pred: "HeapPts",
                    terms: vec![Term::Var(4), Term::Var(3), Term::Var(1)],
                },
            ],
        },
        Rule {
            head: Atom {
                pred: "Path",
                terms: vec![Term::Var(0), Term::Var(1)],
            },
            body: vec![Atom {
                pred: "Edge",
                terms: vec![Term::Var(0), Term::Var(1)],
            }],
        },
        Rule {
            head: Atom {
                pred: "Path",
                terms: vec![Term::Var(0), Term::Var(2)],
            },
            body: vec![
                Atom {
                    pred: "Edge",
                    terms: vec![Term::Var(0), Term::Var(1)],
                },
                Atom {
                    pred: "Path",
                    terms: vec![Term::Var(1), Term::Var(2)],
                },
            ],
        },
        Rule {
            head: Atom {
                pred: "ReachFrom0",
                terms: vec![Term::Var(0)],
            },
            body: vec![Atom {
                pred: "Path",
                terms: vec![Term::Const(0), Term::Var(0)],
            }],
        },
    ];
    (edb, rules)
}

fn solve_transitive_closure_bdd(edges: &[(u32, u32)]) -> (Robdd, usize, BTreeSet<(u32, u32)>) {
    let mut bdd = Robdd::new();
    let x_vars = [0, 1, 2];
    let y_vars = [3, 4, 5];
    let z_vars = [6, 7, 8];

    let mut edge_bdd = BDD_ZERO;
    for &(u, v) in edges {
        let pair = bdd.encode_pair(&x_vars, &y_vars, u, v);
        edge_bdd = bdd.or(edge_bdd, pair);
    }

    let y_to_z: BTreeMap<usize, usize> =
        y_vars.iter().copied().zip(z_vars.iter().copied()).collect();
    let x_to_y: BTreeMap<usize, usize> =
        x_vars.iter().copied().zip(y_vars.iter().copied()).collect();
    let mut xz_to_yz = BTreeMap::new();
    for (&x, &y) in x_vars.iter().zip(&y_vars) {
        xz_to_yz.insert(x, y);
    }
    for (&z, &y) in z_vars.iter().zip(&y_vars) {
        xz_to_yz.insert(z, y);
    }
    let _ = x_to_y;

    let mut path_xy = edge_bdd;
    let edge_xz = bdd.substitute(edge_bdd, &y_to_z);
    loop {
        // Compose edge(x, z) /\ path(z, y) -> exists z -> new_path(x, y).
        let mut path_zy_map = BTreeMap::new();
        for (&x, &z) in x_vars.iter().zip(&z_vars) {
            path_zy_map.insert(x, z);
        }
        let path_zy = bdd.substitute(path_xy, &path_zy_map);
        let conj = bdd.and(edge_xz, path_zy);
        let composed = bdd.exists(&z_vars, conj);
        let next_xy = bdd.or(path_xy, composed);
        if next_xy == path_xy {
            let decoded = bdd.decode_pairs(path_xy, &x_vars, &y_vars);
            return (bdd, path_xy, decoded);
        }
        path_xy = next_xy;
    }
}

fn main() {
    let (edb, rules) = points_to_example();
    let (naive_db, naive_stats) = evaluate_naive(&edb, &rules);
    let (semi_db, semi_stats) = evaluate_semi_naive(&edb, &rules);
    println!(
        "Datalog points-to + closure: naive={:?}, semi_naive={:?}, exact_match={}",
        naive_stats,
        semi_stats,
        naive_db == semi_db
    );
    println!("  Pts={:?}", semi_db["Pts"]);

    let edge_pairs: Vec<(u32, u32)> = edb["Edge"].iter().map(|t| (t[0], t[1])).collect();
    let (mut bdd, root, bdd_paths) = solve_transitive_closure_bdd(&edge_pairs);
    let dl_paths: BTreeSet<(u32, u32)> = semi_db["Path"].iter().map(|t| (t[0], t[1])).collect();
    let a = bdd.var(0);
    let b = bdd.var(1);
    let ab = bdd.and(a, b);
    let ba = bdd.and(b, a);
    println!(
        "ROBDD canonical (a&b == b&a): {} (id={}), Path root={}, BDD==Datalog: {}",
        ab == ba,
        ab,
        root,
        bdd_paths == dl_paths
    );
}

#[test]
fn semi_naive_matches_naive_with_fewer_derivations() {
    let (edb, rules) = points_to_example();
    let (naive_db, naive_stats) = evaluate_naive(&edb, &rules);
    let (semi_db, semi_stats) = evaluate_semi_naive(&edb, &rules);
    assert_eq!(naive_db, semi_db);
    assert!(semi_stats.derivations < naive_stats.derivations);
    // Load var 5 reads p.next where a points to hA=10 and hA.next points to hB=11.
    assert!(semi_db["Pts"].contains(&vec![5, 11]));
}

#[test]
fn robdd_canonical_uniqueness_and_transitive_closure_parity() {
    let mut bdd = Robdd::new();
    let a = bdd.var(0);
    let b = bdd.var(1);
    assert_eq!(bdd.and(a, b), bdd.and(b, a));
    let na = bdd.not(a);
    let lhs = bdd.and(a, b);
    let rhs = bdd.and(na, b);
    assert_eq!(bdd.or(lhs, rhs), b);

    let (edb, rules) = points_to_example();
    let (dl_db, _) = evaluate_semi_naive(&edb, &rules);
    let dl_paths: BTreeSet<(u32, u32)> = dl_db["Path"].iter().map(|t| (t[0], t[1])).collect();
    let edge_pairs: Vec<(u32, u32)> = edb["Edge"].iter().map(|t| (t[0], t[1])).collect();
    let (_, _, bdd_paths) = solve_transitive_closure_bdd(&edge_pairs);
    assert_eq!(bdd_paths, dl_paths);
}
