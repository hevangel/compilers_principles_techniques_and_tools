//! Resolve names in nested scopes and check the types of expressions.

use std::collections::BTreeMap;

// Static types tracked in the symbol table and returned by type checking.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Type {
    Integer,
    Boolean,
}

// Expression AST nodes; notice syntax allows `Add` on any subexpressions, so semantic
// analysis must enforce that both operands actually evaluate to `Type::Integer`.
enum Expr<'a> {
    Name(&'a str),
    // This tree records that a literal is an integer. The value is irrelevant to type checking.
    IntegerLiteral,
    Add(Box<Expr<'a>>, Box<Expr<'a>>),
}

// Semantic errors separated by phase/cause: duplicate decl, missing symbol, or type mismatch.
#[derive(Debug, Eq, PartialEq)]
enum Error<'a> {
    Duplicate(&'a str),
    Unknown(&'a str),
    ExpectedInteger(Type),
}

// One lexical block's symbol table. A slice `&[Scope]` represents nested blocks ordered
// outermost (index 0) to innermost (last element).
type Scope<'a> = BTreeMap<&'a str, Type>;

// Insert a binding into the *current* scope only; shadowing an outer scope is legal,
// whereas redeclaring the same name in the same scope is rejected without overwriting.
fn declare<'a>(scope: &mut Scope<'a>, name: &'a str, ty: Type) -> Result<(), Error<'a>> {
    // A duplicate in this scope must leave the original declaration intact.
    match scope.entry(name) {
        std::collections::btree_map::Entry::Vacant(entry) => {
            entry.insert(ty);
            Ok(())
        }
        std::collections::btree_map::Entry::Occupied(_) => Err(Error::Duplicate(name)),
    }
}

// Synthesize the static type of `expr` under the lexical scope stack `scopes`.
fn type_of<'a>(expr: &Expr<'a>, scopes: &[Scope<'a>]) -> Result<Type, Error<'a>> {
    match expr {
        // Search from the innermost scope outward so the nearest declaration wins.
        Expr::Name(name) => scopes
            .iter()
            .rev()
            .find_map(|scope| scope.get(name).copied())
            .ok_or(Error::Unknown(name)),
        Expr::IntegerLiteral => Ok(Type::Integer),
        Expr::Add(left, right) => {
            // Type-check both subexpressions before giving the addition its result type.
            for operand in [left, right] {
                let ty = type_of(operand, scopes)?;
                if ty != Type::Integer {
                    return Err(Error::ExpectedInteger(ty));
                }
            }
            Ok(Type::Integer)
        }
    }
}

fn main() {
    let mut scopes = vec![Scope::new(), Scope::new()];
    declare(&mut scopes[0], "width", Type::Boolean).unwrap();
    declare(&mut scopes[1], "width", Type::Integer).unwrap();
    let expr = Expr::Add(
        Box::new(Expr::Name("width")),
        Box::new(Expr::IntegerLiteral),
    );
    println!("Inner: {:?}", type_of(&expr, &scopes));
    // Leaving the inner scope exposes the outer binding again.
    scopes.pop();
    println!("Outer: {:?}", type_of(&expr, &scopes));
}

#[test]
fn resolves_shadowing_and_preserves_first_declaration() {
    let mut outer = Scope::new();
    declare(&mut outer, "width", Type::Boolean).unwrap();
    assert_eq!(
        declare(&mut outer, "width", Type::Integer),
        Err(Error::Duplicate("width"))
    );
    let expr = Expr::Add(
        Box::new(Expr::Name("width")),
        Box::new(Expr::IntegerLiteral),
    );
    assert_eq!(
        type_of(&expr, &[outer.clone()]),
        Err(Error::ExpectedInteger(Type::Boolean))
    );
    assert_eq!(
        type_of(&expr, &[outer, Scope::from([("width", Type::Integer)])]),
        Ok(Type::Integer)
    );
    assert_eq!(
        type_of(&Expr::Name("absent"), &[]),
        Err(Error::Unknown("absent"))
    );
}
