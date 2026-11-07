# Type unification and the occurs check

Solve equality constraints between finite function types. See the [Rust source](main.rs).

```sh
cargo run --example unification
cargo test --example unification
```

## What the example teaches

`unify` resolves known substitutions, splits matching function types into argument and result equations, and binds variables. Before binding a variable, `occurs` checks whether the proposed type already contains it.

**Unification** asks whether two type expressions can be made equal by consistently substituting types for variables. For example, if `α → β` must equal `Int → Bool`, the argument positions imply `α = Int`, and the result positions imply `β = Bool`. A compiler can use this kind of solver after it has generated type constraints from a program.

### Process a list of equations

1. `unify` takes an equation from a work list. `resolve` applies existing substitutions recursively, so a chain such as `α = β` followed by `β = Int` resolves the variable `α` to `Int`.
2. Equal resolved types need no further work. Two function types contribute two new equations, one for their parameters and one for their results. Distinct concrete constructors, such as `Int` and `Bool`, produce a clash.
3. When one side is a variable, the solver can bind it to the other side if the **occurs check** succeeds. It rejects `α = (α → Int)` because substituting the proposed type for `α` would put another `α` inside it indefinitely.

### Follow a cycle through an existing binding

Suppose an earlier equation bound `α` to `β`. The next equation `β = (α → Int)` resolves to `β = (β → Int)`. The occurs check catches the cycle after substitution. By contrast, if the next equation is `β = Int`, both variables resolve to `Int`. The substitution map can keep links between variables because `resolve` follows them when needed.

## What to look for

The demo specializes a function with two type variables to an integer-to-Boolean function. The test checks chained substitutions, a constructor mismatch, and a cycle hidden behind an earlier binding.

## Check your understanding

- Work out the result of unifying `α → α` with `Int → Bool`. Which two constraints are forced on the same variable?
- Predict the resolved type after solving `α = β` and `β = Int → Bool`. Compare it with resolving `α` before the second equation.

## Edition mapping and scope

| 1977 Green | 1986 Red | 2006 Purple |
| --- | --- | --- |
| — | 6 | — |

Numbers refer to chapters. Related readings and appendices are marked where applicable. See the [edition guide](../../README.md#edition-guide) for the bibliographic references and chapter coverage.

The program solves supplied equations over a small set of types. Generating constraints from source and generalizing type schemes are separate steps.
