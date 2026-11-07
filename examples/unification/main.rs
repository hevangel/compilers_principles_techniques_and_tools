use std::collections::BTreeMap;

#[derive(Clone, Debug, Eq, PartialEq)]
enum Type {
    Var(u8),
    Int,
    Bool,
    Function(Box<Type>, Box<Type>),
}

type Substitutions = BTreeMap<u8, Type>;

fn function(from: Type, to: Type) -> Type {
    Type::Function(Box::new(from), Box::new(to))
}

fn resolve(ty: &Type, substitutions: &Substitutions) -> Type {
    match ty {
        Type::Var(id) => substitutions
            .get(id)
            .map_or_else(|| ty.clone(), |value| resolve(value, substitutions)),
        Type::Function(from, to) => {
            function(resolve(from, substitutions), resolve(to, substitutions))
        }
        _ => ty.clone(),
    }
}

fn occurs(id: u8, ty: &Type) -> bool {
    match ty {
        Type::Var(other) => id == *other,
        Type::Function(from, to) => occurs(id, from) || occurs(id, to),
        _ => false,
    }
}

#[derive(Debug, PartialEq, Eq)]
enum Error {
    InfiniteType(u8),
    Clash(Type, Type),
}

fn unify(mut equations: Vec<(Type, Type)>) -> Result<Substitutions, Error> {
    let mut substitutions = Substitutions::new();
    while let Some((left, right)) = equations.pop() {
        let left = resolve(&left, &substitutions);
        let right = resolve(&right, &substitutions);
        if left == right {
            continue;
        }
        match (left, right) {
            (Type::Var(id), ty) | (ty, Type::Var(id)) => {
                if occurs(id, &ty) {
                    return Err(Error::InfiniteType(id));
                }
                substitutions.insert(id, ty);
            }
            (Type::Function(a, b), Type::Function(c, d)) => {
                equations.extend([(*a, *c), (*b, *d)]);
            }
            (left, right) => return Err(Error::Clash(left, right)),
        }
    }
    Ok(substitutions)
}

fn main() {
    let generic = function(Type::Var(0), Type::Var(1));
    let concrete = function(Type::Int, Type::Bool);
    let substitutions = unify(vec![(generic.clone(), concrete)]).unwrap();
    println!(
        "Constraints solved as {:?}",
        resolve(&generic, &substitutions)
    );
}

#[test]
fn resolves_chained_bindings_and_rejects_clashes_and_cycles() {
    use Type::{Bool, Int, Var};
    let substitutions = unify(vec![(Var(1), Int), (Var(0), Var(1))]).unwrap();
    assert_eq!(resolve(&Var(0), &substitutions), Int);
    assert_eq!(unify(vec![(Int, Bool)]), Err(Error::Clash(Int, Bool)));
    assert_eq!(
        unify(vec![(Var(1), function(Var(0), Int)), (Var(0), Var(1))]),
        Err(Error::InfiniteType(1))
    );
}
