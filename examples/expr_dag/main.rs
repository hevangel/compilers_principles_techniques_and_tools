//! Build expression DAGs via value-number hash-consing, algebraic folding, and alias kills.

use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Op {
    Add,
    Sub,
    Mul,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum DagNode {
    Leaf(String),
    Const(i64),
    Bin(Op, usize, usize),
    ArrayLoad {
        array: String,
        index: usize,
        epoch: usize,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum ExprAst {
    Var(String),
    Const(i64),
    Bin(Op, Box<ExprAst>, Box<ExprAst>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum BbStmt {
    Assign {
        dst: String,
        rhs: ExprAst,
    },
    ArrayRead {
        dst: String,
        array: String,
        index: ExprAst,
    },
    ArrayWrite {
        array: String,
        index: ExprAst,
        value: ExprAst,
    },
    PtrKill,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum TacInst {
    LoadConst {
        dst: String,
        val: i64,
    },
    Copy {
        dst: String,
        src: String,
    },
    Bin {
        dst: String,
        op: Op,
        left: String,
        right: String,
    },
    ArrayLoad {
        dst: String,
        array: String,
        index: String,
    },
    ArrayStore {
        array: String,
        index: String,
        value: String,
    },
}

struct DagBuilder {
    nodes: Vec<DagNode>,
    table: BTreeMap<DagNode, usize>,
    env: BTreeMap<String, usize>,
    array_epoch: BTreeMap<String, usize>,
    global_epoch: usize,
}

impl DagBuilder {
    fn new() -> Self {
        Self {
            nodes: Vec::new(),
            table: BTreeMap::new(),
            env: BTreeMap::new(),
            array_epoch: BTreeMap::new(),
            global_epoch: 0,
        }
    }

    fn intern(&mut self, node: DagNode) -> usize {
        if let Some(&id) = self.table.get(&node) {
            return id;
        }
        let id = self.nodes.len();
        self.nodes.push(node.clone());
        self.table.insert(node, id);
        id
    }

    fn leaf(&mut self, name: &str) -> usize {
        if let Some(&id) = self.env.get(name) {
            return id;
        }
        let id = self.intern(DagNode::Leaf(name.to_string()));
        self.env.insert(name.to_string(), id);
        id
    }

    fn lower_ast(&mut self, ast: &ExprAst) -> usize {
        match ast {
            ExprAst::Var(name) => self.leaf(name),
            ExprAst::Const(c) => self.intern(DagNode::Const(*c)),
            ExprAst::Bin(op, left, right) => {
                let mut l = self.lower_ast(left);
                let mut r = self.lower_ast(right);
                self.make_bin(*op, &mut l, &mut r)
            }
        }
    }

    fn make_bin(&mut self, op: Op, l: &mut usize, r: &mut usize) -> usize {
        // Constant folding
        if let (DagNode::Const(a), DagNode::Const(b)) = (&self.nodes[*l], &self.nodes[*r]) {
            let folded = match op {
                Op::Add => a.wrapping_add(*b),
                Op::Sub => a.wrapping_sub(*b),
                Op::Mul => a.wrapping_mul(*b),
            };
            return self.intern(DagNode::Const(folded));
        }

        // Algebraic identities
        match op {
            Op::Add => {
                if matches!(self.nodes[*l], DagNode::Const(0)) {
                    return *r;
                }
                if matches!(self.nodes[*r], DagNode::Const(0)) {
                    return *l;
                }
                if *l > *r {
                    std::mem::swap(l, r);
                }
            }
            Op::Mul => {
                if matches!(self.nodes[*l], DagNode::Const(0))
                    || matches!(self.nodes[*r], DagNode::Const(0))
                {
                    return self.intern(DagNode::Const(0));
                }
                if matches!(self.nodes[*l], DagNode::Const(1)) {
                    return *r;
                }
                if matches!(self.nodes[*r], DagNode::Const(1)) {
                    return *l;
                }
                if *l > *r {
                    std::mem::swap(l, r);
                }
            }
            Op::Sub => {
                if matches!(self.nodes[*r], DagNode::Const(0)) {
                    return *l;
                }
                if *l == *r {
                    return self.intern(DagNode::Const(0));
                }
            }
        }

        self.intern(DagNode::Bin(op, *l, *r))
    }

    fn name_of(
        id: usize,
        nodes: &[DagNode],
        computed: &mut BTreeMap<usize, String>,
        next_temp: &mut usize,
        tac_out: &mut Vec<TacInst>,
    ) -> String {
        if let Some(existing) = computed.get(&id) {
            return existing.clone();
        }
        match &nodes[id] {
            DagNode::Leaf(s) => s.clone(),
            DagNode::Const(c) => {
                let t = format!("t{}", *next_temp);
                *next_temp += 1;
                tac_out.push(TacInst::LoadConst {
                    dst: t.clone(),
                    val: *c,
                });
                computed.insert(id, t.clone());
                t
            }
            _ => unreachable!("non-leaf node must have been materialized first"),
        }
    }

    fn process_block(&mut self, stmts: &[BbStmt]) -> Vec<TacInst> {
        let mut tac = Vec::new();
        let mut computed_name: BTreeMap<usize, String> = BTreeMap::new();
        let mut next_temp = 0usize;

        for stmt in stmts {
            match stmt {
                BbStmt::Assign { dst, rhs } => {
                    let id = self.lower_ast(rhs);
                    self.materialize(id, &mut computed_name, &mut next_temp, &mut tac);
                    let src_name = Self::name_of(
                        id,
                        &self.nodes,
                        &mut computed_name,
                        &mut next_temp,
                        &mut tac,
                    );
                    if src_name != *dst {
                        tac.push(TacInst::Copy {
                            dst: dst.clone(),
                            src: src_name,
                        });
                    }
                    self.env.insert(dst.clone(), id);
                }
                BbStmt::ArrayRead { dst, array, index } => {
                    let idx_id = self.lower_ast(index);
                    self.materialize(idx_id, &mut computed_name, &mut next_temp, &mut tac);
                    let epoch = *self.array_epoch.get(array).unwrap_or(&self.global_epoch);
                    let key = DagNode::ArrayLoad {
                        array: array.clone(),
                        index: idx_id,
                        epoch,
                    };
                    let existed = self.table.contains_key(&key);
                    let load_id = self.intern(key);
                    if !existed {
                        let idx_name = Self::name_of(
                            idx_id,
                            &self.nodes,
                            &mut computed_name,
                            &mut next_temp,
                            &mut tac,
                        );
                        tac.push(TacInst::ArrayLoad {
                            dst: dst.clone(),
                            array: array.clone(),
                            index: idx_name,
                        });
                        computed_name.insert(load_id, dst.clone());
                    } else {
                        let prev = computed_name[&load_id].clone();
                        tac.push(TacInst::Copy {
                            dst: dst.clone(),
                            src: prev,
                        });
                    }
                    self.env.insert(dst.clone(), load_id);
                }
                BbStmt::ArrayWrite {
                    array,
                    index,
                    value,
                } => {
                    let idx_id = self.lower_ast(index);
                    let val_id = self.lower_ast(value);
                    self.materialize(idx_id, &mut computed_name, &mut next_temp, &mut tac);
                    self.materialize(val_id, &mut computed_name, &mut next_temp, &mut tac);
                    let idx_name = Self::name_of(
                        idx_id,
                        &self.nodes,
                        &mut computed_name,
                        &mut next_temp,
                        &mut tac,
                    );
                    let val_name = Self::name_of(
                        val_id,
                        &self.nodes,
                        &mut computed_name,
                        &mut next_temp,
                        &mut tac,
                    );
                    tac.push(TacInst::ArrayStore {
                        array: array.clone(),
                        index: idx_name,
                        value: val_name,
                    });
                    let e = self
                        .array_epoch
                        .entry(array.clone())
                        .or_insert(self.global_epoch);
                    *e += 1;
                }
                BbStmt::PtrKill => {
                    self.global_epoch += 1;
                    self.array_epoch.clear();
                }
            }
        }
        tac
    }

    fn materialize(
        &self,
        id: usize,
        computed: &mut BTreeMap<usize, String>,
        next_temp: &mut usize,
        tac: &mut Vec<TacInst>,
    ) {
        if computed.contains_key(&id) {
            return;
        }
        match &self.nodes[id] {
            DagNode::Leaf(_) | DagNode::ArrayLoad { .. } => {}
            DagNode::Const(c) => {
                let t = format!("t{}", *next_temp);
                *next_temp += 1;
                tac.push(TacInst::LoadConst {
                    dst: t.clone(),
                    val: *c,
                });
                computed.insert(id, t);
            }
            DagNode::Bin(op, l, r) => {
                self.materialize(*l, computed, next_temp, tac);
                self.materialize(*r, computed, next_temp, tac);
                let l_name = match &self.nodes[*l] {
                    DagNode::Leaf(s) => s.clone(),
                    _ => computed[l].clone(),
                };
                let r_name = match &self.nodes[*r] {
                    DagNode::Leaf(s) => s.clone(),
                    _ => computed[r].clone(),
                };
                let t = format!("t{}", *next_temp);
                *next_temp += 1;
                tac.push(TacInst::Bin {
                    dst: t.clone(),
                    op: *op,
                    left: l_name,
                    right: r_name,
                });
                computed.insert(id, t);
            }
        }
    }
}

fn add(a: ExprAst, b: ExprAst) -> ExprAst {
    ExprAst::Bin(Op::Add, Box::new(a), Box::new(b))
}

fn mul(a: ExprAst, b: ExprAst) -> ExprAst {
    ExprAst::Bin(Op::Mul, Box::new(a), Box::new(b))
}

fn sub(a: ExprAst, b: ExprAst) -> ExprAst {
    ExprAst::Bin(Op::Sub, Box::new(a), Box::new(b))
}

fn main() {
    let expr = add(
        add(ExprAst::Var("a".into()), ExprAst::Var("b".into())),
        mul(
            add(ExprAst::Var("a".into()), ExprAst::Var("b".into())),
            add(ExprAst::Var("b".into()), ExprAst::Var("a".into())),
        ),
    );
    let mut dag = DagBuilder::new();
    let root = dag.lower_ast(&expr);
    println!("DAG nodes for (a+b) + (a+b)*(b+a) (root=#{root}):");
    for (idx, node) in dag.nodes.iter().enumerate() {
        println!("  #{idx}: {node:?}");
    }
    let stmts = vec![
        BbStmt::Assign {
            dst: "d".into(),
            rhs: sub(ExprAst::Var("a".into()), ExprAst::Const(0)),
        },
        BbStmt::ArrayRead {
            dst: "x".into(),
            array: "A".into(),
            index: ExprAst::Var("i".into()),
        },
        BbStmt::ArrayRead {
            dst: "y".into(),
            array: "A".into(),
            index: ExprAst::Var("i".into()),
        },
        BbStmt::ArrayWrite {
            array: "A".into(),
            index: ExprAst::Var("j".into()),
            value: ExprAst::Const(99),
        },
        BbStmt::ArrayRead {
            dst: "z".into(),
            array: "A".into(),
            index: ExprAst::Var("i".into()),
        },
        BbStmt::PtrKill,
    ];
    let tac = DagBuilder::new().process_block(&stmts);
    println!("Reassembled TAC with store kill:");
    for inst in tac {
        println!("  {inst:?}");
    }
}

#[test]
fn hash_conses_commutative_and_algebraic_expressions() {
    let expr = add(
        add(ExprAst::Var("a".into()), ExprAst::Var("b".into())),
        mul(
            add(ExprAst::Var("a".into()), ExprAst::Var("b".into())),
            add(ExprAst::Var("b".into()), ExprAst::Var("a".into())),
        ),
    );
    let mut dag = DagBuilder::new();
    let root = dag.lower_ast(&expr);
    // Leaves a (#0), b (#1), shared a+b (#2), (a+b)*(b+a) (#3), root (#4)
    assert_eq!(dag.nodes.len(), 5);
    assert_eq!(root, 4);

    // Algebraic identities: (x + 0) * 1 + (x - x) -> x
    let identity = add(
        mul(
            add(ExprAst::Var("x".into()), ExprAst::Const(0)),
            ExprAst::Const(1),
        ),
        sub(ExprAst::Var("x".into()), ExprAst::Var("x".into())),
    );
    let mut dag2 = DagBuilder::new();
    let id_root = dag2.lower_ast(&identity);
    assert_eq!(dag2.nodes[id_root], DagNode::Leaf("x".into()));
}

#[test]
fn array_store_invalidates_stale_load_value_number() {
    let stmts = vec![
        BbStmt::ArrayRead {
            dst: "x".into(),
            array: "A".into(),
            index: ExprAst::Var("i".into()),
        },
        BbStmt::ArrayRead {
            dst: "y".into(),
            array: "A".into(),
            index: ExprAst::Var("i".into()),
        },
        BbStmt::ArrayWrite {
            array: "A".into(),
            index: ExprAst::Var("j".into()),
            value: ExprAst::Const(42),
        },
        BbStmt::ArrayRead {
            dst: "z".into(),
            array: "A".into(),
            index: ExprAst::Var("i".into()),
        },
    ];
    let tac = DagBuilder::new().process_block(&stmts);
    // First read loads A[i], second reuses x via Copy, write bumps epoch, third reloads A[i].
    assert!(matches!(tac[0], TacInst::ArrayLoad { ref dst, .. } if dst == "x"));
    assert!(matches!(tac[1], TacInst::Copy { ref dst, ref src } if dst == "y" && src == "x"));
    assert!(matches!(tac[3], TacInst::ArrayStore { .. }));
    assert!(matches!(tac[4], TacInst::ArrayLoad { ref dst, .. } if dst == "z"));
}
