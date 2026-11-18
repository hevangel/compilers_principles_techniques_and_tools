# Hindley-Milner Let-Polymorphic Type Inference and Type Equivalence

Infer polymorphic types via generalization and fresh instantiation in `let` bindings, reject self-application with the occurs check, and compare structural vs. nominal record equivalence. See the [Rust source](main.rs) and [Python source](main.py).

```sh
cargo run --example poly_infer
cargo test --example poly_infer
python3 examples/poly_infer/main.py
python3 -m unittest examples/poly_infer/main.py
```

## What the example teaches

### Why we need this

First-order unification alone solves a fixed list of type equations, and monomorphic type inference assigns every variable a single fixed type. Neither handles reusable functions like the identity function `id = \x. x`:

- If `id` stays **monomorphic** (`t0 -> t0`), using `id 1` constrains `t0 = Int`, and then `id true` tries to constrain `Int = Bool` and fails! (This is exactly what happens with the lambda parameter in `(\id. (id 1, id true)) (\x. x)`.)
- On the other hand, if we blindly quantify *every* type variable everywhere, a variable captured from an enclosing lambda parameter (which is free in the environment `TypeEnv`) could be instantiated inconsistently and break type soundness.
- Meanwhile, when user-defined record types share identical field names (`Point{x: Int, y: Int}` vs. `Vec2{x: Int, y: Int}`), languages must choose between **nominal** equivalence (tags must match) and **structural** equivalence (field names and types match).

**Hindley-Milner (`Algorithm W`) let-polymorphism** solves the polymorphism problem by generalizing only type variables not free in `TypeEnv` at `let` boundaries, and instantiating fresh `TVar`s at each `Var` use.

First-order unification alone solves supplied type equations. Hindley-Milner type inference generates those equations from a functional AST and uses `let`-generalization (`TypeScheme`) so a `let`-bound function can be instantiated at different concrete types at different call sites, while lambda-bound parameters remain monomorphic.

### Inference and equivalence mechanisms

1. `generalize` quantifies type variables in the inferred right-hand side that are not free in the enclosing `TypeEnv`.
2. `instantiate` replaces each quantified variable with a fresh `TVar` whenever a `let`-bound variable is referenced.
3. `unify` applies substitutions and enforces the occurs check (`InfiniteType`) to forbid cyclic types like `\x. x x`.
4. `structural_eq` vs. `nominal_eq` contrasts record types with identical fields (`{x: Int, y: Int}`) under different tags (`Point` vs. `Vec2`).

### Tracing `let id = \x. x in (id 1, id true)`

| Step | Expression / Action | Inferred type / Scheme | Substitution update |
| --- | --- | --- | --- |
| 1 | Infer `\x. x` | `t0 -> t0` | — |
| 2 | `generalize({}, t0 -> t0)` | `forall t0. t0 -> t0` stored for `id` | — |
| 3 | Lookup `id` in `id 1`, `instantiate` | `t1 -> t1` | — |
| 4 | Unify `t1 -> t1` with `Int -> t2` | Result `t2` -> `Int` | `t1 = Int, t2 = Int` |
| 5 | Lookup `id` in `id true`, `instantiate` | `t3 -> t3` | — |
| 6 | Unify `t3 -> t3` with `Bool -> t4` | Result `t4` -> `Bool` | `t3 = Bool, t4 = Bool` |
| 7 | Pair `(id 1, id true)` | `(Int, Bool)` | — |

### Polymorphic vs. monomorphic binding

| Expression | Typing result | Reason |
| --- | --- | --- |
| `let id = \x. x in (id 1, id true)` | `(Int, Bool)` | `id` generalizes to `forall t0. t0 -> t0` |
| `(\id. (id 1, id true)) (\x. x)` | Clash (`Int` vs `Bool`) | Lambda parameter `id` has empty `quantified` set (monomorphic) |
| `\x. x x` | `InfiniteType` | Occurs check rejects `t0 = t0 -> t1` |

## How to read the code

| Concept | Rust (`main.rs`) | Python (`main.py`) | Role |
| --- | --- | --- | --- |
| Types & schemes | `enum Type`, `struct TypeScheme` | `TVar`..`TRecord`, `TypeScheme` | Type terms and `forall`-quantified schemes |
| Expression AST | `enum Expr` | `VarExpr`..`RecordLitExpr` | Functional AST with `Lam`, `App`, `Let`, `RecordLit` |
| Substitutions & FVs | `apply_subst`, `apply_subst_env`, `free_vars_*` | `apply_subst`, `apply_subst_env`, `free_vars_*` | Respects bound/quantified variables when substituting |
| Unifier | `occurs`, `unify` | `occurs`, `unify` | Solves equations and rejects self-application cycles |
| Algorithm W engine | `InferCtx` (`fresh`, `instantiate`, `infer`), `generalize` | `InferCtx` (`fresh`, `instantiate`, `infer`) | Generates constraints and generalizes at `Let` |
| Record equivalence | `structural_eq`, `nominal_eq` | `structural_eq`, `nominal_eq` | Ignores vs. requires matching record tags |

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
