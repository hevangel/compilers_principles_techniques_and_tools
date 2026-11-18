# Type unification and the occurs check

Solve equality constraints between finite function types. See the [Rust source](main.rs) and [Python source](main.py).

```sh
cargo run --example unification
cargo test --example unification
python3 examples/unification/main.py
python3 -m unittest examples/unification/main.py
```

## What the example teaches

### Why we need this

When a language has type inference (so programmers do not have to annotate every variable and lambda), the compiler often encounters unknown types before it knows what they stand for. Checking `f(x)` generates placeholders—type variables like `α` (`Var(0)`) and `β` (`Var(1)`)—along with equality constraints such as `(α → β) = (Int → Bool)` or chained constraints `α = β` and `β = Int`.

A naive structural equality check (`==`) fails immediately because `α` is not literally `Int`. Even a simple variable-lookup table fails on two traps:
1. **Indirect chains**: after `β = Int` and `α = β`, resolving `α` only one step returns `β` instead of `Int`.
2. **Self-referential cycles**: solving `β = (α → Int)` after `α = β` tries to make `β` equal to `(β → Int)`—an **infinite type** that would expand forever without terminating!

**First-order unification** solves equality constraints structurally while recursively following existing bindings (`resolve`) and running an **occurs check** (`occurs`) before creating any cycle.

`unify` resolves known substitutions, splits matching function types into argument and result equations, and binds variables. Before binding a variable, `occurs` checks whether the proposed type already contains it.

**Unification** asks whether two type expressions can be made equal by consistently substituting types for variables. For example, if `α → β` must equal `Int → Bool`, the argument positions imply `α = Int`, and the result positions imply `β = Bool`. A compiler can use this kind of solver after it has generated type constraints from a program.

### Process a list of equations

1. `unify` pops an equation from a work list (`equations` / `work`). `resolve` applies existing substitutions recursively, so a chain such as `α = β` followed by `β = Int` resolves the variable `α` to `Int`.
2. Equal resolved types need no further work. Two function types contribute two new equations, one for their parameters and one for their results. Distinct concrete constructors, such as `Int` and `Bool`, produce a `Clash`.
3. When one side is a variable, the solver binds it to the other side only if the **occurs check** succeeds. It rejects `α = (α → Int)` because substituting the proposed type for `α` would put another `α` inside it indefinitely.

### Follow chained bindings and indirect cycles

Notice `equations.pop()` processes the last equation in the vector first:

| Test case | Equations popped (LIFO) | `resolve` output | Substitution / Error |
| --- | --- | --- | --- |
| Chained binding | `(Var(0), Var(1))` | `Var(0) = Var(1)` | `subst[0] = Var(1)` |
| (continued) | `(Var(1), Int)` | `Var(1) = Int` | `subst[1] = Int` (`resolve(Var(0)) -> Int`) |
| Hidden cycle | `(Var(0), Var(1))` | `Var(0) = Var(1)` | `subst[0] = Var(1)` |
| (continued) | `(Var(1), Var(0) -> Int)` | `Var(1) = (Var(1) -> Int)` | `occurs(1, Var(1) -> Int)` is true -> `InfiniteType(1)` |

## How to read the code

| Concept | Rust (`main.rs`) | Python (`main.py`) | Role |
| --- | --- | --- | --- |
| Type expressions | `enum Type` (`Var`, `Int`, `Bool`, `Function`) | `TVar`, `TInt`, `TBool`, `TFunc` | Type terms to unify |
| Substitution map | `type Substitutions = BTreeMap<u8, Type>` | `dict[int, Type]` | Maps bound type variable IDs to types |
| Path compression / lookup | `fn resolve(ty, substitutions)` | `def resolve(ty, substitutions)` | Recursively substitutes bound variables |
| Cycle guard | `fn occurs(id, ty)` | `def occurs(var_id, ty)` | Checks if variable appears inside proposed type |
| Constraint solver | `fn unify(equations)` | `def unify(equations)` | Worklist loop decomposing functions and binding vars |

## What to look for

The demo specializes a function with two type variables to an integer-to-Boolean function. The test checks chained substitutions, a constructor mismatch, and a cycle hidden behind an earlier binding.

## Check your understanding

- Work out the result of unifying `α → α` with `Int → Bool`. Which two constraints are forced on the same variable?
- Predict the resolved type after solving `α = β` and `β = Int → Bool`. Compare it with resolving `α` before the second equation.

## Edition mapping and scope

| 1977 Green | 1986 Red | 2006 Purple |
| --- | --- | --- |
| — | 6 | 6 |

Numbers refer to chapters. Related readings and appendices are marked where applicable. See the [edition guide](../../README.md#edition-guide) for the bibliographic references and chapter coverage.

The program solves supplied equations over a small set of types. Generating constraints from source and generalizing type schemes are separate steps.
