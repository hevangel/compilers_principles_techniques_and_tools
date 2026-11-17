# Attribute Dependency Graph and Topological Evaluator

Build a dependency graph for syntax-directed definitions, classify S- and L-attributed rules, and evaluate attributes in topological order. See the [Rust source](main.rs) and [Python source](main.py).

```sh
cargo run --example sdd_eval
cargo test --example sdd_eval
python3 examples/sdd_eval/main.py
python3 -m unittest examples/sdd_eval/main.py
```

## What the example teaches

A syntax-directed definition associates semantic rules with parse-tree nodes. Whenever an attribute instance `b` uses `c` in its defining rule, there is a directed dependency edge `c -> b`. If the resulting dependency graph is acyclic, any topological sort gives a valid evaluation order; if it contains a cycle, no evaluation order exists.

### Classification and evaluation

1. `classify_sdd` inspects each dependency edge across parse-tree parent/child links to distinguish **S-attributed** (synthesized attributes depending only on children), **L-attributed** (inherited attributes depending only on parent or left siblings), and **general acyclic** definitions.
2. `evaluate_sdd` runs Kahn's topological sort deterministically over `(NodeId, AttrName)` pairs, computing each attribute once all its dependencies have values.
3. Any remaining nodes with non-zero in-degree trigger an explicit `Cycle` error.

### Dependency flow for accumulator `3` followed by `5`

| Attribute | Depends on | Value |
| --- | --- | --- |
| `(1, syn_val)` | — | `3` |
| `(2, inh_acc)` | `(1, syn_val)` | `3` |
| `(3, syn_val)` | — | `5` |
| `(4, inh_acc)` | `(2, inh_acc)`, `(3, syn_val)` | `35` |
| `(0, syn_val)` | `(2, syn_val)` | `35` |

## What to look for

The sample trace classifies the left-to-right digit-accumulator tree as L-attributed and prints its topological evaluation order ending at `35`. Unit tests also verify a right-to-left acyclic SDD (valid via general topological sort, though not L-attributed) and reject a circular dependency graph cleanly.

## Check your understanding

- Why is every L-attributed SDD guaranteed to be acyclic?
- Why can an acyclic SDD fail to be L-attributed even though topological evaluation succeeds?

## Edition mapping and scope

| 1977 Green | 1986 Red | 2006 Purple |
| --- | --- | --- |
| 7 | 5 | 5 |

Numbers refer to chapters. Related readings and appendices are marked where applicable. See the [edition guide](../../README.md#edition-guide) for the bibliographic references and chapter coverage.

Semantic rules use a compact enum of arithmetic actions rather than arbitrary host closures so graphs are inspectable and identically structured in Rust and Python.
