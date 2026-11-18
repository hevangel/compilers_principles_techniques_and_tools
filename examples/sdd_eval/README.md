# Attribute Dependency Graph and Topological Evaluator

Build a dependency graph for syntax-directed definitions, classify S- and L-attributed rules, and evaluate attributes in topological order. See the [Rust source](main.rs) and [Python source](main.py).

```sh
cargo run --example sdd_eval
cargo test --example sdd_eval
python3 examples/sdd_eval/main.py
python3 -m unittest examples/sdd_eval/main.py
```

## What the example teaches

### Why we need this

A context-free grammar only checks syntax; it does not prescribe in what order semantic computations (like type checking, decimal accumulation, or address computation) can safely run. If you hardcode a naive left-to-right AST walk, two things go wrong:

1. **Non-L-attributed rules fail**: if a left child's inherited attribute depends on a right sibling's synthesized attribute (`rtl_rules` in the tests), a left-to-right pass reads an uninitialized value even though a valid evaluation order exists.
2. **Cyclic rules silently hang or read garbage**: if rule A depends on rule B and rule B depends on rule A, *no* traversal order works, and a compiler must reject the specification explicitly.

Representing semantic rules as an explicit **attribute dependency graph**—one vertex per `(node_id, attr_name)` instance and one directed edge `dep -> target` per dependency—solves both problems. The compiler can statically classify whether a cheap single-pass traversal suffices (**S-attributed** = bottom-up only; **L-attributed** = single left-to-right DFS) or whether it must fall back to a general topological sort, and Kahn's algorithm detects cycles automatically.

A syntax-directed definition associates semantic rules with parse-tree nodes. Whenever an attribute instance `b` uses `c` in its defining rule, there is a directed dependency edge `c -> b`. If the resulting dependency graph is acyclic, any topological sort gives a valid evaluation order; if it contains a cycle, no evaluation order exists.

### Classification and evaluation

1. `classify_sdd` inspects each dependency edge across parse-tree parent/child links to distinguish **S-attributed** (synthesized `syn_*` attributes depending only on direct children), **L-attributed** (synthesized attributes depending on children, plus inherited `inh_*` attributes depending only on the parent or left siblings), and **general acyclic** definitions.
2. `evaluate_sdd` runs Kahn's topological sort deterministically over `(NodeId, AttrName)` pairs, computing each attribute once all its dependencies have values.
3. Any remaining nodes with non-zero in-degree trigger an explicit `Cycle` error.

### Dependency flow for accumulator `3` followed by `5`

In `sample_l_attributed`, node `0` (`Number`) has children `[1 (Digit 3), 2 (Tail)]`, and node `2` (`Tail`) has children `[3 (Digit 5), 4 (EmptyTail)]`. The inherited accumulator `inh_acc` threads the prefix value left-to-right down the tree, while `syn_val` carries the final number `35` back up:

| Step | Attribute | Depends on | Action | Value |
| --- | --- | --- | --- | --- |
| 1 | `(1, syn_val)` | — | `Const(3)` | `3` |
| 2 | `(2, inh_acc)` | `(1, syn_val)` | `CopyFirst` | `3` |
| 3 | `(3, syn_val)` | — | `Const(5)` | `5` |
| 4 | `(4, inh_acc)` | `(2, inh_acc)`, `(3, syn_val)` | `10 * 3 + 5` | `35` |
| 5 | `(4, syn_val)` | `(4, inh_acc)` | `CopyFirst` | `35` |
| 6 | `(2, syn_val)` | `(4, syn_val)` | `CopyFirst` | `35` |
| 7 | `(0, syn_val)` | `(2, syn_val)` | `CopyFirst` | `35` |

## How to read the code

| Concept | Rust (`main.rs`) | Python (`main.py`) | Role |
| --- | --- | --- | --- |
| Attribute node key | `type AttrInstance = (usize, String)` | `AttrInstance = tuple[int, str]` | Identifies `(parse_node_id, attr_name)` |
| Parse tree & rules | `ParseNode`, `Rule`, `RuleKind` | `ParseNode`, `Rule`, `RuleKind` | Tree parent/child links and semantic actions |
| SDD classifier | `fn classify_sdd` -> `SddClass` | `def classify_sdd` -> `SddClass` | Checks S- and L-attributed edge invariants |
| Topological evaluator | `fn evaluate_sdd` | `def evaluate_sdd` | Kahn's algorithm with deterministic min-order and cycle check |
| Worked input | `fn sample_l_attributed` | `def sample_l_attributed` | 5-node parse tree accumulating `3` and `5` into `35` |

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
