//! Infer let-polymorphic Hindley-Milner types and compare nominal vs structural records.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

#[derive(Clone, Debug, PartialEq, Eq)]
enum Type {
    Var(u32),
    Int,
    Bool,
    Pair(Box<Type>, Box<Type>),
    Fun(Box<Type>, Box<Type>),
    Record {
        tag: String,
        fields: Vec<(String, Type)>,
    },
}

impl fmt::Display for Type {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Type::Var(id) => write!(f, "t{id}"),
            Type::Int => write!(f, "Int"),
            Type::Bool => write!(f, "Bool"),
            Type::Pair(a, b) => write!(f, "({a}, {b})"),
            Type::Fun(a, b) => write!(f, "({a} -> {b})"),
            Type::Record { tag, fields } => {
                let fs: Vec<String> = fields.iter().map(|(n, t)| format!("{n}: {t}")).collect();
                write!(f, "{tag}{{{}}}", fs.join(", "))
            }
        }
    }
}

#[derive(Clone, Debug)]
struct TypeScheme {
    quantified: BTreeSet<u32>,
    ty: Type,
}

#[derive(Clone, Debug)]
enum Expr {
    Var(String),
    IntLit,
    BoolLit,
    Pair(Box<Expr>, Box<Expr>),
    Lam(String, Box<Expr>),
    App(Box<Expr>, Box<Expr>),
    Let(String, Box<Expr>, Box<Expr>),
    RecordLit(String, Vec<(String, Expr)>),
}

#[derive(Debug, PartialEq, Eq)]
enum TypeError {
    UnboundVar(String),
    InfiniteType(u32),
    Clash(String, String),
}

type Subst = BTreeMap<u32, Type>;
type TypeEnv = BTreeMap<String, TypeScheme>;

fn apply_subst(ty: &Type, subst: &Subst) -> Type {
    match ty {
        Type::Var(id) => subst
            .get(id)
            .map_or_else(|| ty.clone(), |t| apply_subst(t, subst)),
        Type::Int | Type::Bool => ty.clone(),
        Type::Pair(a, b) => Type::Pair(
            Box::new(apply_subst(a, subst)),
            Box::new(apply_subst(b, subst)),
        ),
        Type::Fun(a, b) => Type::Fun(
            Box::new(apply_subst(a, subst)),
            Box::new(apply_subst(b, subst)),
        ),
        Type::Record { tag, fields } => Type::Record {
            tag: tag.clone(),
            fields: fields
                .iter()
                .map(|(n, t)| (n.clone(), apply_subst(t, subst)))
                .collect(),
        },
    }
}

fn free_vars_type(ty: &Type, out: &mut BTreeSet<u32>) {
    match ty {
        Type::Var(id) => {
            out.insert(*id);
        }
        Type::Int | Type::Bool => {}
        Type::Pair(a, b) | Type::Fun(a, b) => {
            free_vars_type(a, out);
            free_vars_type(b, out);
        }
        Type::Record { fields, .. } => {
            for (_, t) in fields {
                free_vars_type(t, out);
            }
        }
    }
}

fn free_vars_scheme(scheme: &TypeScheme) -> BTreeSet<u32> {
    let mut fv = BTreeSet::new();
    free_vars_type(&scheme.ty, &mut fv);
    for q in &scheme.quantified {
        fv.remove(q);
    }
    fv
}

fn free_vars_env(env: &TypeEnv) -> BTreeSet<u32> {
    let mut fv = BTreeSet::new();
    for scheme in env.values() {
        fv.extend(free_vars_scheme(scheme));
    }
    fv
}

fn apply_subst_env(env: &TypeEnv, subst: &Subst) -> TypeEnv {
    env.iter()
        .map(|(k, scheme)| {
            let filtered: Subst = subst
                .iter()
                .filter(|(id, _)| !scheme.quantified.contains(id))
                .map(|(id, t)| (*id, t.clone()))
                .collect();
            (
                k.clone(),
                TypeScheme {
                    quantified: scheme.quantified.clone(),
                    ty: apply_subst(&scheme.ty, &filtered),
                },
            )
        })
        .collect()
}

fn occurs(id: u32, ty: &Type) -> bool {
    match ty {
        Type::Var(other) => *other == id,
        Type::Int | Type::Bool => false,
        Type::Pair(a, b) | Type::Fun(a, b) => occurs(id, a) || occurs(id, b),
        Type::Record { fields, .. } => fields.iter().any(|(_, t)| occurs(id, t)),
    }
}

fn unify(t1: &Type, t2: &Type, subst: &mut Subst) -> Result<(), TypeError> {
    let a = apply_subst(t1, subst);
    let b = apply_subst(t2, subst);
    if a == b {
        return Ok(());
    }
    match (a, b) {
        (Type::Var(id), ty) | (ty, Type::Var(id)) => {
            if occurs(id, &ty) {
                return Err(TypeError::InfiniteType(id));
            }
            subst.insert(id, ty);
            Ok(())
        }
        (Type::Pair(a1, a2), Type::Pair(b1, b2)) | (Type::Fun(a1, a2), Type::Fun(b1, b2)) => {
            unify(&a1, &b1, subst)?;
            unify(&a2, &b2, subst)
        }
        (
            Type::Record {
                tag: t1,
                fields: f1,
            },
            Type::Record {
                tag: t2,
                fields: f2,
            },
        ) if t1 == t2 && f1.len() == f2.len() => {
            for ((n1, ty1), (n2, ty2)) in f1.iter().zip(&f2) {
                if n1 != n2 {
                    return Err(TypeError::Clash(n1.clone(), n2.clone()));
                }
                unify(ty1, ty2, subst)?;
            }
            Ok(())
        }
        (left, right) => Err(TypeError::Clash(left.to_string(), right.to_string())),
    }
}

fn generalize(env: &TypeEnv, ty: &Type) -> TypeScheme {
    let mut fv_ty = BTreeSet::new();
    free_vars_type(ty, &mut fv_ty);
    let fv_env = free_vars_env(env);
    let quantified: BTreeSet<u32> = fv_ty.difference(&fv_env).copied().collect();
    TypeScheme {
        quantified,
        ty: ty.clone(),
    }
}

struct InferCtx {
    next_id: u32,
    subst: Subst,
}

impl InferCtx {
    fn new() -> Self {
        Self {
            next_id: 0,
            subst: BTreeMap::new(),
        }
    }

    fn fresh(&mut self) -> Type {
        let id = self.next_id;
        self.next_id += 1;
        Type::Var(id)
    }

    fn instantiate(&mut self, scheme: &TypeScheme) -> Type {
        let mut rename = BTreeMap::new();
        for &q in &scheme.quantified {
            rename.insert(q, self.fresh());
        }
        apply_subst(&scheme.ty, &rename)
    }

    fn infer(&mut self, env: &TypeEnv, expr: &Expr) -> Result<Type, TypeError> {
        match expr {
            Expr::Var(name) => {
                let scheme = env
                    .get(name)
                    .ok_or_else(|| TypeError::UnboundVar(name.clone()))?;
                Ok(self.instantiate(scheme))
            }
            Expr::IntLit => Ok(Type::Int),
            Expr::BoolLit => Ok(Type::Bool),
            Expr::Pair(a, b) => {
                let ta = self.infer(env, a)?;
                let tb = self.infer(env, b)?;
                Ok(Type::Pair(
                    Box::new(apply_subst(&ta, &self.subst)),
                    Box::new(apply_subst(&tb, &self.subst)),
                ))
            }
            Expr::Lam(param, body) => {
                let param_ty = self.fresh();
                let mut inner = env.clone();
                inner.insert(
                    param.clone(),
                    TypeScheme {
                        quantified: BTreeSet::new(),
                        ty: param_ty.clone(),
                    },
                );
                let body_ty = self.infer(&inner, body)?;
                Ok(Type::Fun(
                    Box::new(apply_subst(&param_ty, &self.subst)),
                    Box::new(apply_subst(&body_ty, &self.subst)),
                ))
            }
            Expr::App(fun, arg) => {
                let f_ty = self.infer(env, fun)?;
                let env_after_fun = apply_subst_env(env, &self.subst);
                let a_ty = self.infer(&env_after_fun, arg)?;
                let res_ty = self.fresh();
                let expected = Type::Fun(Box::new(a_ty), Box::new(res_ty.clone()));
                unify(&f_ty, &expected, &mut self.subst)?;
                Ok(apply_subst(&res_ty, &self.subst))
            }
            Expr::Let(name, rhs, body) => {
                let rhs_ty = self.infer(env, rhs)?;
                let env_after_rhs = apply_subst_env(env, &self.subst);
                let rhs_resolved = apply_subst(&rhs_ty, &self.subst);
                let scheme = generalize(&env_after_rhs, &rhs_resolved);
                let mut inner = env_after_rhs;
                inner.insert(name.clone(), scheme);
                self.infer(&inner, body)
            }
            Expr::RecordLit(tag, fields) => {
                let mut typed = Vec::with_capacity(fields.len());
                for (fname, fexpr) in fields {
                    let ft = self.infer(env, fexpr)?;
                    typed.push((fname.clone(), apply_subst(&ft, &self.subst)));
                }
                Ok(Type::Record {
                    tag: tag.clone(),
                    fields: typed,
                })
            }
        }
    }
}

fn infer_type(expr: &Expr) -> Result<Type, TypeError> {
    let mut ctx = InferCtx::new();
    let ty = ctx.infer(&BTreeMap::new(), expr)?;
    Ok(apply_subst(&ty, &ctx.subst))
}

fn structural_eq(a: &Type, b: &Type) -> bool {
    match (a, b) {
        (Type::Var(x), Type::Var(y)) => x == y,
        (Type::Int, Type::Int) | (Type::Bool, Type::Bool) => true,
        (Type::Pair(a1, a2), Type::Pair(b1, b2)) | (Type::Fun(a1, a2), Type::Fun(b1, b2)) => {
            structural_eq(a1, b1) && structural_eq(a2, b2)
        }
        (Type::Record { fields: f1, .. }, Type::Record { fields: f2, .. }) => {
            f1.len() == f2.len()
                && f1
                    .iter()
                    .zip(f2)
                    .all(|((n1, t1), (n2, t2))| n1 == n2 && structural_eq(t1, t2))
        }
        _ => false,
    }
}

fn nominal_eq(a: &Type, b: &Type) -> bool {
    a == b
}

fn main() {
    // let id = \x. x in (id 1, id true)
    let poly = Expr::Let(
        "id".into(),
        Box::new(Expr::Lam("x".into(), Box::new(Expr::Var("x".into())))),
        Box::new(Expr::Pair(
            Box::new(Expr::App(
                Box::new(Expr::Var("id".into())),
                Box::new(Expr::IntLit),
            )),
            Box::new(Expr::App(
                Box::new(Expr::Var("id".into())),
                Box::new(Expr::BoolLit),
            )),
        )),
    );
    println!(
        "let id = \\x. x in (id 1, id true) : {}",
        infer_type(&poly).unwrap()
    );

    let p1 = infer_type(&Expr::RecordLit(
        "Point".into(),
        vec![("x".into(), Expr::IntLit), ("y".into(), Expr::IntLit)],
    ))
    .unwrap();
    let p2 = infer_type(&Expr::RecordLit(
        "Vec2".into(),
        vec![("x".into(), Expr::IntLit), ("y".into(), Expr::IntLit)],
    ))
    .unwrap();
    println!(
        "{p1} vs {p2} => structural_eq={}, nominal_eq={}",
        structural_eq(&p1, &p2),
        nominal_eq(&p1, &p2)
    );
}

#[test]
fn let_polymorphism_allows_multiple_instantiations_unlike_lambda() {
    let poly = Expr::Let(
        "id".into(),
        Box::new(Expr::Lam("x".into(), Box::new(Expr::Var("x".into())))),
        Box::new(Expr::Pair(
            Box::new(Expr::App(
                Box::new(Expr::Var("id".into())),
                Box::new(Expr::IntLit),
            )),
            Box::new(Expr::App(
                Box::new(Expr::Var("id".into())),
                Box::new(Expr::BoolLit),
            )),
        )),
    );
    assert_eq!(
        infer_type(&poly),
        Ok(Type::Pair(Box::new(Type::Int), Box::new(Type::Bool)))
    );

    // (\id. (id 1, id true)) (\x. x) must fail because lambda parameters are monomorphic.
    let mono_fail = Expr::App(
        Box::new(Expr::Lam(
            "id".into(),
            Box::new(Expr::Pair(
                Box::new(Expr::App(
                    Box::new(Expr::Var("id".into())),
                    Box::new(Expr::IntLit),
                )),
                Box::new(Expr::App(
                    Box::new(Expr::Var("id".into())),
                    Box::new(Expr::BoolLit),
                )),
            )),
        )),
        Box::new(Expr::Lam("x".into(), Box::new(Expr::Var("x".into())))),
    );
    assert!(matches!(
        infer_type(&mono_fail),
        Err(TypeError::Clash(_, _))
    ));
}

#[test]
fn detects_occurs_check_and_distinguishes_nominal_vs_structural_records() {
    // \x. x x triggers InfiniteType
    let self_app = Expr::Lam(
        "x".into(),
        Box::new(Expr::App(
            Box::new(Expr::Var("x".into())),
            Box::new(Expr::Var("x".into())),
        )),
    );
    assert!(matches!(
        infer_type(&self_app),
        Err(TypeError::InfiniteType(_))
    ));
    assert!(matches!(
        infer_type(&Expr::Var("missing".into())),
        Err(TypeError::UnboundVar(_))
    ));

    let p1 = infer_type(&Expr::RecordLit(
        "Point".into(),
        vec![("x".into(), Expr::IntLit), ("y".into(), Expr::IntLit)],
    ))
    .unwrap();
    let p2 = infer_type(&Expr::RecordLit(
        "Vec2".into(),
        vec![("x".into(), Expr::IntLit), ("y".into(), Expr::IntLit)],
    ))
    .unwrap();
    assert!(structural_eq(&p1, &p2));
    assert!(!nominal_eq(&p1, &p2));
}
