//! Widen primitive types across a hierarchy and resolve overloads in two passes.

use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Type {
    Bool,
    Char,
    Int,
    Float,
    Double,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CoercionOp {
    B2C,
    C2I,
    I2F,
    F2D,
}

#[derive(Clone, Debug)]
struct Signature {
    params: Vec<Type>,
    ret: Type,
}

#[derive(Clone, Debug)]
enum Expr {
    Lit(Type),
    Call(String, Vec<Expr>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum ResolvedExpr {
    Lit(Type),
    Call {
        name: String,
        ret: Type,
        args: Vec<(ResolvedExpr, Vec<CoercionOp>)>,
    },
}

#[derive(Debug, PartialEq, Eq)]
enum OverloadError {
    UnknownFunction(String),
    NoViableCandidate,
    Ambiguous(Vec<Type>),
}

fn next_wider(ty: Type) -> Option<(Type, CoercionOp)> {
    match ty {
        Type::Bool => Some((Type::Char, CoercionOp::B2C)),
        Type::Char => Some((Type::Int, CoercionOp::C2I)),
        Type::Int => Some((Type::Float, CoercionOp::I2F)),
        Type::Float => Some((Type::Double, CoercionOp::F2D)),
        Type::Double => None,
    }
}

fn coercion_chain(from: Type, to: Type) -> Option<Vec<CoercionOp>> {
    let mut cur = from;
    let mut ops = Vec::new();
    while cur != to {
        let (next, op) = next_wider(cur)?;
        ops.push(op);
        cur = next;
    }
    Some(ops)
}

fn widen(t1: Type, t2: Type) -> Option<(Type, Vec<CoercionOp>, Vec<CoercionOp>)> {
    if let Some(ops) = coercion_chain(t1, t2) {
        Some((t2, ops, Vec::new()))
    } else {
        coercion_chain(t2, t1).map(|ops| (t1, Vec::new(), ops))
    }
}

#[derive(Clone, Debug)]
struct Candidate {
    ret: Type,
    cost: usize,
    expr: ResolvedExpr,
}

// Pass 1 (bottom-up): synthesize all viable return types and conversion costs.
fn synthesize_candidates(
    expr: &Expr,
    overloads: &BTreeMap<String, Vec<Signature>>,
) -> Result<Vec<Candidate>, OverloadError> {
    match expr {
        Expr::Lit(ty) => Ok(vec![Candidate {
            ret: *ty,
            cost: 0,
            expr: ResolvedExpr::Lit(*ty),
        }]),
        Expr::Call(name, arg_exprs) => {
            let sigs = overloads
                .get(name)
                .ok_or_else(|| OverloadError::UnknownFunction(name.clone()))?;
            let mut arg_cands = Vec::with_capacity(arg_exprs.len());
            for arg in arg_exprs {
                arg_cands.push(synthesize_candidates(arg, overloads)?);
            }

            let mut results = Vec::new();
            for sig in sigs {
                if sig.params.len() != arg_cands.len() {
                    continue;
                }
                // Pick the cheapest conversion of each argument to match sig.params[i].
                let mut matched_args = Vec::with_capacity(arg_cands.len());
                let mut total_cost = 0usize;
                let mut feasible = true;
                for (cands, &param_ty) in arg_cands.iter().zip(&sig.params) {
                    let mut best: Option<(usize, ResolvedExpr, Vec<CoercionOp>)> = None;
                    let mut tie_at_best = false;
                    for c in cands {
                        if let Some(ops) = coercion_chain(c.ret, param_ty) {
                            let cost = c.cost + ops.len();
                            match &best {
                                None => {
                                    best = Some((cost, c.expr.clone(), ops));
                                    tie_at_best = false;
                                }
                                Some((best_cost, _, _)) if cost < *best_cost => {
                                    best = Some((cost, c.expr.clone(), ops));
                                    tie_at_best = false;
                                }
                                Some((best_cost, _, _)) if cost == *best_cost => {
                                    tie_at_best = true;
                                }
                                _ => {}
                            }
                        }
                    }
                    if tie_at_best {
                        return Err(OverloadError::Ambiguous(
                            cands.iter().map(|c| c.ret).collect(),
                        ));
                    }
                    if let Some((cost, chosen_expr, ops)) = best {
                        total_cost += cost;
                        matched_args.push((chosen_expr, ops));
                    } else {
                        feasible = false;
                        break;
                    }
                }
                if feasible {
                    results.push(Candidate {
                        ret: sig.ret,
                        cost: total_cost,
                        expr: ResolvedExpr::Call {
                            name: name.clone(),
                            ret: sig.ret,
                            args: matched_args,
                        },
                    });
                }
            }
            if results.is_empty() {
                return Err(OverloadError::NoViableCandidate);
            }
            Ok(results)
        }
    }
}

// Pass 2 (top-down): narrow candidates using expected parent return type and minimum cost.
fn resolve_overload(
    expr: &Expr,
    expected_ret: Option<Type>,
    overloads: &BTreeMap<String, Vec<Signature>>,
) -> Result<ResolvedExpr, OverloadError> {
    let cands = synthesize_candidates(expr, overloads)?;
    let filtered: Vec<Candidate> = match expected_ret {
        Some(target) => cands
            .into_iter()
            .filter_map(|c| {
                coercion_chain(c.ret, target).map(|ops| Candidate {
                    ret: c.ret,
                    cost: c.cost + ops.len(),
                    expr: c.expr,
                })
            })
            .collect(),
        None => cands,
    };
    if filtered.is_empty() {
        return Err(OverloadError::NoViableCandidate);
    }
    let min_cost = filtered.iter().map(|c| c.cost).min().unwrap();
    let best: Vec<Candidate> = filtered
        .into_iter()
        .filter(|c| c.cost == min_cost)
        .collect();
    if best.len() > 1 {
        return Err(OverloadError::Ambiguous(
            best.iter().map(|c| c.ret).collect(),
        ));
    }
    Ok(best.into_iter().next().unwrap().expr)
}

fn sample_overloads() -> BTreeMap<String, Vec<Signature>> {
    BTreeMap::from([
        (
            "f".into(),
            vec![
                Signature {
                    params: vec![Type::Int],
                    ret: Type::Int,
                },
                Signature {
                    params: vec![Type::Int],
                    ret: Type::Double,
                },
            ],
        ),
        (
            "g".into(),
            vec![
                Signature {
                    params: vec![Type::Float],
                    ret: Type::Float,
                },
                Signature {
                    params: vec![Type::Double],
                    ret: Type::Double,
                },
            ],
        ),
        (
            "sink_double".into(),
            vec![Signature {
                params: vec![Type::Double],
                ret: Type::Double,
            }],
        ),
    ])
}

fn main() {
    let overloads = sample_overloads();
    let widened_bool = widen(Type::Bool, Type::Int).unwrap();
    let widened = widen(Type::Int, Type::Float).unwrap();
    println!("Widen (Bool, Int) => {widened_bool:?}");
    println!("Widen (Int, Float) => {widened:?}");

    // f(Int) has both Int -> Int and Int -> Double; expected return type Bool/Int/Double disambiguates.
    let call_f = Expr::Call("f".into(), vec![Expr::Lit(Type::Int)]);
    println!(
        "f(Int) with expected Int    => {:?}",
        resolve_overload(&call_f, Some(Type::Int), &overloads)
    );
    println!(
        "f(Int) with expected Double => {:?}",
        resolve_overload(&call_f, Some(Type::Double), &overloads)
    );
    println!(
        "g(Char) picks cheaper Float => {:?}",
        resolve_overload(
            &Expr::Call("g".into(), vec![Expr::Lit(Type::Char)]),
            None,
            &overloads
        )
    );
    println!(
        "sink_double(f(Int))         => {:?}",
        resolve_overload(
            &Expr::Call("sink_double".into(), vec![call_f]),
            None,
            &overloads
        )
    );
}

#[test]
fn disambiguates_return_type_and_selects_minimum_widening_cost() {
    let overloads = sample_overloads();
    let call_f = Expr::Call("f".into(), vec![Expr::Lit(Type::Int)]);

    // Without return context, f(Int) -> Int and f(Int) -> Double both have cost 0 -> ambiguous!
    assert!(matches!(
        resolve_overload(&call_f, None, &overloads),
        Err(OverloadError::Ambiguous(_))
    ));

    // Return context Double picks f(Int) -> Double (cost 0 vs Int->Double cost 2).
    let res_dbl = resolve_overload(&call_f, Some(Type::Double), &overloads).unwrap();
    assert!(matches!(
        res_dbl,
        ResolvedExpr::Call {
            ret: Type::Double,
            ..
        }
    ));

    // Return context Int picks f(Int) -> Int (Double cannot coerce to Int).
    let res_int = resolve_overload(&call_f, Some(Type::Int), &overloads).unwrap();
    assert!(matches!(res_int, ResolvedExpr::Call { ret: Type::Int, .. }));

    // g(Char) prefers Float (cost 2: C2I + I2F) over Double (cost 3).
    let call_g = Expr::Call("g".into(), vec![Expr::Lit(Type::Char)]);
    let res_g = resolve_overload(&call_g, None, &overloads).unwrap();
    assert_eq!(
        res_g,
        ResolvedExpr::Call {
            name: "g".into(),
            ret: Type::Float,
            args: vec![(
                ResolvedExpr::Lit(Type::Char),
                vec![CoercionOp::C2I, CoercionOp::I2F]
            )],
        }
    );
}

#[test]
fn reports_widening_failure_and_ambiguous_nested_calls() {
    let overloads = sample_overloads();
    assert!(widen(Type::Char, Type::Double).is_some());
    assert!(coercion_chain(Type::Double, Type::Int).is_none());

    // Calling g(Double) with expected Int fails because Double cannot widen to Int.
    let call_g = Expr::Call("g".into(), vec![Expr::Lit(Type::Double)]);
    assert_eq!(
        resolve_overload(&call_g, Some(Type::Int), &overloads),
        Err(OverloadError::NoViableCandidate)
    );
}
