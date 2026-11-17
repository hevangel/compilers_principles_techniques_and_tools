//! Compute aligned record/union field offsets and compare quadruple, triple, and indirect-triple IR.

use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Eq)]
enum Type {
    U8,
    U32,
    U64,
    Struct(Vec<(String, Type)>),
    Union(Vec<(String, Type)>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct FieldLayout {
    name: String,
    offset: usize,
    size: usize,
    align: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct RecordLayout {
    size: usize,
    align: usize,
    padding_bytes: usize,
    fields: Vec<FieldLayout>,
}

fn align_up(offset: usize, align: usize) -> usize {
    if align <= 1 {
        offset
    } else {
        offset.div_ceil(align) * align
    }
}

fn layout_type(ty: &Type) -> RecordLayout {
    match ty {
        Type::U8 => RecordLayout {
            size: 1,
            align: 1,
            padding_bytes: 0,
            fields: vec![],
        },
        Type::U32 => RecordLayout {
            size: 4,
            align: 4,
            padding_bytes: 0,
            fields: vec![],
        },
        Type::U64 => RecordLayout {
            size: 8,
            align: 8,
            padding_bytes: 0,
            fields: vec![],
        },
        Type::Struct(members) => {
            let mut cursor = 0usize;
            let mut max_align = 1usize;
            let mut payload = 0usize;
            let mut fields = Vec::with_capacity(members.len());
            for (name, mty) in members {
                let m = layout_type(mty);
                let off = align_up(cursor, m.align);
                max_align = max_align.max(m.align);
                payload += m.size;
                fields.push(FieldLayout {
                    name: name.clone(),
                    offset: off,
                    size: m.size,
                    align: m.align,
                });
                cursor = off + m.size;
            }
            let total = align_up(cursor, max_align).max(1);
            let padding_bytes = total.saturating_sub(payload);
            RecordLayout {
                size: total,
                align: max_align,
                padding_bytes,
                fields,
            }
        }
        Type::Union(members) => {
            let mut max_size = 1usize;
            let mut max_align = 1usize;
            let mut fields = Vec::with_capacity(members.len());
            for (name, mty) in members {
                let m = layout_type(mty);
                max_size = max_size.max(m.size);
                max_align = max_align.max(m.align);
                fields.push(FieldLayout {
                    name: name.clone(),
                    offset: 0,
                    size: m.size,
                    align: m.align,
                });
            }
            let total = align_up(max_size, max_align);
            let max_member = fields.iter().map(|f| f.size).max().unwrap_or(0);
            RecordLayout {
                size: total,
                align: max_align,
                padding_bytes: total - max_member,
                fields,
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BinOp {
    Add,
    Mul,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Expr {
    Var(String),
    FieldAddr(String, usize),
    Bin(BinOp, Box<Expr>, Box<Expr>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Quadruple {
    op: String,
    arg1: String,
    arg2: String,
    result: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum ArgRef {
    Sym(String),
    Const(usize),
    TripleIdx(usize),
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Triple {
    op: String,
    arg1: ArgRef,
    arg2: ArgRef,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct IndirectTriples {
    triples: Vec<Triple>,
    order: Vec<usize>,
}

fn lower_to_quads(stmts: &[(String, Expr)]) -> Vec<Quadruple> {
    fn emit_expr(expr: &Expr, next_t: &mut usize, out: &mut Vec<Quadruple>) -> String {
        match expr {
            Expr::Var(v) => v.clone(),
            Expr::FieldAddr(base, offset) => {
                let res = format!("t{}", *next_t);
                *next_t += 1;
                out.push(Quadruple {
                    op: "addr+".into(),
                    arg1: base.clone(),
                    arg2: offset.to_string(),
                    result: res.clone(),
                });
                res
            }
            Expr::Bin(op, l, r) => {
                let lv = emit_expr(l, next_t, out);
                let rv = emit_expr(r, next_t, out);
                let res = format!("t{}", *next_t);
                *next_t += 1;
                out.push(Quadruple {
                    op: match op {
                        BinOp::Add => "+".into(),
                        BinOp::Mul => "*".into(),
                    },
                    arg1: lv,
                    arg2: rv,
                    result: res.clone(),
                });
                res
            }
        }
    }
    let mut next_t = 0usize;
    let mut out = Vec::new();
    for (dst, expr) in stmts {
        let src = emit_expr(expr, &mut next_t, &mut out);
        out.push(Quadruple {
            op: "=".into(),
            arg1: src,
            arg2: String::new(),
            result: dst.clone(),
        });
    }
    out
}

fn lower_to_triples(stmts: &[(String, Expr)]) -> IndirectTriples {
    fn emit_expr(expr: &Expr, triples: &mut Vec<Triple>) -> ArgRef {
        match expr {
            Expr::Var(v) => ArgRef::Sym(v.clone()),
            Expr::FieldAddr(base, offset) => {
                let idx = triples.len();
                triples.push(Triple {
                    op: "addr+".into(),
                    arg1: ArgRef::Sym(base.clone()),
                    arg2: ArgRef::Const(*offset),
                });
                ArgRef::TripleIdx(idx)
            }
            Expr::Bin(op, l, r) => {
                let la = emit_expr(l, triples);
                let ra = emit_expr(r, triples);
                let idx = triples.len();
                triples.push(Triple {
                    op: match op {
                        BinOp::Add => "+".into(),
                        BinOp::Mul => "*".into(),
                    },
                    arg1: la,
                    arg2: ra,
                });
                ArgRef::TripleIdx(idx)
            }
        }
    }
    let mut triples = Vec::new();
    for (dst, expr) in stmts {
        let rhs = emit_expr(expr, &mut triples);
        triples.push(Triple {
            op: "=".into(),
            arg1: ArgRef::Sym(dst.clone()),
            arg2: rhs,
        });
    }
    let order = (0..triples.len()).collect();
    IndirectTriples { triples, order }
}

fn eval_quads(quads: &[Quadruple], initial: &[(&str, i64)]) -> BTreeMap<String, i64> {
    let mut env: BTreeMap<String, i64> = initial.iter().map(|&(k, v)| (k.to_string(), v)).collect();
    let get = |s: &str, env: &BTreeMap<String, i64>| -> i64 {
        s.parse::<i64>()
            .unwrap_or_else(|_| *env.get(s).unwrap_or(&0))
    };
    for q in quads {
        let v = match q.op.as_str() {
            "+" => get(&q.arg1, &env) + get(&q.arg2, &env),
            "*" => get(&q.arg1, &env) * get(&q.arg2, &env),
            "addr+" => get(&q.arg1, &env) + get(&q.arg2, &env),
            "=" => get(&q.arg1, &env),
            other => panic!("unknown op {other}"),
        };
        env.insert(q.result.clone(), v);
    }
    env
}

fn eval_indirect_triples(it: &IndirectTriples, initial: &[(&str, i64)]) -> BTreeMap<String, i64> {
    let mut env: BTreeMap<String, i64> = initial.iter().map(|&(k, v)| (k.to_string(), v)).collect();
    let mut results: BTreeMap<usize, i64> = BTreeMap::new();
    let resolve =
        |a: &ArgRef, env: &BTreeMap<String, i64>, results: &BTreeMap<usize, i64>| -> i64 {
            match a {
                ArgRef::Sym(s) => *env.get(s).unwrap_or(&0),
                ArgRef::Const(c) => *c as i64,
                ArgRef::TripleIdx(i) => results[i],
            }
        };
    for &idx in &it.order {
        let t = &it.triples[idx];
        match t.op.as_str() {
            "+" => {
                let v = resolve(&t.arg1, &env, &results) + resolve(&t.arg2, &env, &results);
                results.insert(idx, v);
            }
            "*" => {
                let v = resolve(&t.arg1, &env, &results) * resolve(&t.arg2, &env, &results);
                results.insert(idx, v);
            }
            "addr+" => {
                let v = resolve(&t.arg1, &env, &results) + resolve(&t.arg2, &env, &results);
                results.insert(idx, v);
            }
            "=" => {
                if let ArgRef::Sym(dst) = &t.arg1 {
                    let v = resolve(&t.arg2, &env, &results);
                    env.insert(dst.clone(), v);
                    results.insert(idx, v);
                }
            }
            other => panic!("unknown op {other}"),
        }
    }
    env
}

fn main() {
    let unpadded = Type::Struct(vec![
        ("a".into(), Type::U8),
        ("b".into(), Type::U32),
        ("c".into(), Type::U8),
    ]);
    let packed = Type::Struct(vec![
        ("b".into(), Type::U32),
        ("a".into(), Type::U8),
        ("c".into(), Type::U8),
    ]);
    let nested_union = Type::Struct(vec![
        ("tag".into(), Type::U8),
        (
            "payload".into(),
            Type::Union(vec![
                ("word".into(), Type::U64),
                ("pair".into(), unpadded.clone()),
            ]),
        ),
    ]);
    let lu = layout_type(&unpadded);
    let lp = layout_type(&packed);
    let lnu = layout_type(&nested_union);
    println!(
        "{{u8, u32, u8}} => size={}, padding={}, fields={:?}",
        lu.size, lu.padding_bytes, lu.fields
    );
    println!(
        "{{u32, u8, u8}} => size={}, padding={}, fields={:?}",
        lp.size, lp.padding_bytes, lp.fields
    );
    println!(
        "nested union struct => size={}, align={}, padding={}",
        lnu.size, lnu.align, lnu.padding_bytes
    );

    let stmts = vec![
        (
            "p".into(),
            Expr::FieldAddr("base".into(), lnu.fields[1].offset),
        ),
        (
            "x".into(),
            Expr::Bin(
                BinOp::Add,
                Box::new(Expr::Bin(
                    BinOp::Mul,
                    Box::new(Expr::Var("a".into())),
                    Box::new(Expr::Var("b".into())),
                )),
                Box::new(Expr::Var("c".into())),
            ),
        ),
    ];
    let quads = lower_to_quads(&stmts);
    let mut it = lower_to_triples(&stmts);
    // Hoist the independent `a * b` (triple 2) ahead of `p = base + off` via indirect order.
    it.order = vec![2, 0, 1, 3, 4];
    let env_q = eval_quads(&quads, &[("base", 1000), ("a", 3), ("b", 4), ("c", 5)]);
    let env_t = eval_indirect_triples(&it, &[("base", 1000), ("a", 3), ("b", 4), ("c", 5)]);
    println!(
        "Quad x={}, IndirectTriple x={}, p={}",
        env_q["x"], env_t["x"], env_t["p"]
    );
}

#[test]
fn computes_struct_and_union_alignment_padding() {
    let unpadded = Type::Struct(vec![
        ("a".into(), Type::U8),
        ("b".into(), Type::U32),
        ("c".into(), Type::U8),
    ]);
    let lu = layout_type(&unpadded);
    assert_eq!(lu.size, 12);
    assert_eq!(lu.align, 4);
    assert_eq!(lu.padding_bytes, 6);
    assert_eq!(lu.fields[0].offset, 0);
    assert_eq!(lu.fields[1].offset, 4);
    assert_eq!(lu.fields[2].offset, 8);

    let packed = Type::Struct(vec![
        ("b".into(), Type::U32),
        ("a".into(), Type::U8),
        ("c".into(), Type::U8),
    ]);
    let lp = layout_type(&packed);
    assert_eq!(lp.size, 8);
    assert_eq!(lp.padding_bytes, 2);

    let u = Type::Union(vec![("x".into(), Type::U8), ("y".into(), Type::U64)]);
    let lu_union = layout_type(&u);
    assert_eq!(lu_union.size, 8);
    assert_eq!(lu_union.align, 8);
    assert!(lu_union.fields.iter().all(|f| f.offset == 0));
}

#[test]
fn indirect_triple_reordering_preserves_operand_references_and_values() {
    let stmts = vec![
        ("p".into(), Expr::FieldAddr("base".into(), 8)),
        (
            "res".into(),
            Expr::Bin(
                BinOp::Add,
                Box::new(Expr::Bin(
                    BinOp::Mul,
                    Box::new(Expr::Var("x".into())),
                    Box::new(Expr::Var("y".into())),
                )),
                Box::new(Expr::Var("p".into())),
            ),
        ),
    ];
    let quads = lower_to_quads(&stmts);
    let mut it = lower_to_triples(&stmts);
    let initial = [("base", 100), ("x", 6), ("y", 7)];
    let before = eval_indirect_triples(&it, &initial);
    // Hoist the independent `x * y` triple (index 2) before `base + 8` and its store to `p`.
    it.order = vec![2, 0, 1, 3, 4];
    let after = eval_indirect_triples(&it, &initial);
    let q_env = eval_quads(&quads, &initial);
    assert_eq!(before["p"], 108);
    assert_eq!(before["res"], 150);
    assert_eq!(after["p"], before["p"]);
    assert_eq!(after["res"], before["res"]);
    assert_eq!(q_env["res"], before["res"]);
    // Notice operand indices inside `it.triples` did not need to change when `it.order` changed!
    assert_eq!(it.triples[3].arg1, ArgRef::TripleIdx(2));
}
