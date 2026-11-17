# Semi-Naive Datalog Engine and ROBDD Relational Solver

Evaluate field-sensitive points-to Datalog rules incrementally with delta relations, and solve the same transitive-closure relation canonically using an arena-backed ROBDD. See the [Rust source](main.rs) and [Python source](main.py).

```sh
cargo run --example datalog_bdd
cargo test --example datalog_bdd
python3 examples/datalog_bdd/main.py
python3 -m unittest examples/datalog_bdd/main.py
```

## What the example teaches

Pointer analysis over large programs scales by separating declarative constraint rules from their solver representation: semi-naive Datalog avoids re-deriving old facts each iteration, while Reduced Ordered Binary Decision Diagrams compress dense relations into shared DAG nodes.

### Incremental rules and canonical BDD operations

| Component | Key invariant | Role in demo |
| --- | --- | --- |
| Semi-naive Datalog | Joins at least one `delta` predicate per rule firing | Computes identical `Pts`/`HeapPts`/`Path` sets with fewer derivations |
| ROBDD unique table | `low == high => low`, shared `(var, low, high)` IDs | Guarantees `a & b` and `b & a` allocate the exact same node ID |
| BDD relational compose | `exists z. Edge(x, z) & Path(z, y)` | Reaches the identical fixed-point `Path` relation as Datalog |

## What to look for

Rule evaluation derives `Pts(5, 11)` across `Alloc`, `Move`, `Store`, and `Load`, with semi-naive evaluation cutting redundant rule derivations compared to naive iteration. In Part 2, `(a & b) \/ (~a & b)` collapses canonically back to `b`'s node ID, and BDD-based transitive closure produces the exact same `(u, v)` pairs as Datalog `Path`.

## Check your understanding

- Why does replacing one body atom at a time with its `delta` relation stay sound when rules contain multiple recursive IDB predicates?
- How does the ROBDD reduction rule `low == high => low` eliminate irrelevant variables automatically?

## Edition mapping and scope

| 1977 Green | 1986 Red | 2006 Purple |
| --- | --- | --- |
| — | — | 12 |

Numbers refer to chapters. Related readings and appendices are marked where applicable. See the [edition guide](../../README.md#edition-guide) for the bibliographic references and chapter coverage.

Datalog rules are unstratified positive Horn clauses; the ROBDD engine encodes small integer domains directly into ordered Boolean variable bands.
