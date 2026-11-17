# Hindley-Milner Let-Polymorphic Type Inference and Type Equivalence

Infer polymorphic types via generalization and fresh instantiation in `let` bindings, reject self-application with the occurs check, and compare structural vs. nominal record equivalence. See the [Rust source](main.rs) and [Python source](main.py).

```sh
cargo run --example poly_infer
cargo test --example poly_infer
python3 examples/poly_infer/main.py
python3 -m unittest examples/poly_infer/main.py
```

## What the example teaches

First-order unification alone solves supplied type equations. Hindley-Milner type inference generates those equations from a functional AST and uses `let`-generalization (`TypeScheme`) so a `let`-bound function can be instantiated at different concrete types at different call sites, while lambda-bound parameters remain monomorphic.

### Inference and equivalence mechanisms

1. `generalize` quantifies type variables in the inferred right-hand side that are not free in the enclosing `TypeEnv`.
2. `instantiate` replaces each quantified variable with a fresh `TVar` whenever a `let`-bound variable is referenced.
3. `unify` applies substitutions and enforces the occurs check (`InfiniteType`) to forbid cyclic types like `\x. x x`.
4. `structural_eq` vs. `nominal_eq` contrasts record types with identical fields (`{x: Int, y: Int}`) under different tags (`Point` vs. `Vec2`).

### Polymorphic vs. monomorphic binding

| Expression | Typing result | Reason |
| --- | --- | --- |
| `let id = \x. x in (id 1, id true)` | `(Int, Bool)` | `id` generalizes to `forall t0. t0 -> t0` |
| `(\id. (id 1, id true)) (\x. x)` | Clash (`Int` vs `Bool`) | Lambda parameter `id` is monomorphic |
| `\x. x x` | `InfiniteType` | Occurs check rejects `t0 = t0 -> t1` |

## What to look for

The worked trace infers `(Int, Bool)` for `let id = \x. x in (id 1, id true)` and prints both structural (`true`) and nominal (`false`) equivalence for `Point{x: Int, y: Int}` vs. `Vec2{x: Int, y: Int}`. Unit tests verify monomorphic lambda rejection, occurs-check detection, and unbound variable errors.

## Check your understanding

- Why must free type variables of `TypeEnv` be excluded when generalizing a `let` binding?
- Why does removing the record tag check turn nominal equivalence into structural equivalence?

## Edition mapping and scope

| 1977 Green | 1986 Red | 2006 Purple |
| --- | --- | --- |
| — | 6 | 6 |

Numbers refer to chapters. Related readings and appendices are marked where applicable. See the [edition guide](../../README.md#edition-guide) for the bibliographic references and chapter coverage.

The AST covers variables, literals, pairs, single-argument lambdas, `let`, and flat record literals without recursive `let rec` bindings.
