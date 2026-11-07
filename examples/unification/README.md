# Type unification and the occurs check

Solve equality constraints between finite function types. See the [Rust source](main.rs).

```sh
cargo run --example unification
cargo test --example unification
```

## What the example teaches

`unify` resolves known substitutions, splits matching function types into argument and result equations, and binds variables. Before binding a variable, `occurs` checks whether the proposed type already contains it.

## What to look for

The demo specializes a function with two type variables to an integer-to-Boolean function. The test checks chained substitutions, a constructor mismatch, and a cycle hidden behind an earlier binding.

## Edition mapping and scope

| 1977 Green | 1986 Red | 2006 Purple |
| --- | --- | --- |
| — | 6 | — |

Numbers refer to chapters. Related readings and appendices are marked where applicable. See the [edition guide](../../README.md#edition-guide) for the bibliographic references and chapter coverage.

The program solves supplied equations over a small set of types. Generating constraints from source and generalizing type schemes are separate steps.
