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

### Why we need semi-naive Datalog and ROBDDs

Whole-program pointer analysis faces two scaling barriers if implemented naively:

1. **Redundant re-derivation in naive fixed-point loops**: Writing `Pts(v, h)` and `HeapPts(h1, f, h2)` as declarative Horn clauses keeps the analysis clean, but **naive evaluation** re-joins *every* existing tuple on every round—re-computing old facts over and over just to discover a handful of new ones. **Semi-naive evaluation** fixes this by keeping a `delta` table of tuples newly discovered in the previous round and firing each rule only when at least one body predicate reads from `delta`.
2. **Memory blowup of explicit tuple sets**: Explicit `BTreeSet` / `set` tables store every `(u, v)` or `(ctx, var, heap)` tuple individually. An **ROBDD** represents an entire relation as a single boolean function over binary-encoded variables (`x_vars`, `y_vars`, `z_vars`). By enforcing a global variable order, merging identical `(var, low, high)` sub-DAGs in a **unique table**, and eliminating don't-care tests (`low == high => low`), dense relations share structure automatically and fixed-point convergence becomes an `O(1)` root-ID equality check (`next_xy == path_xy`).

### Walkthrough of the two parts

1. **Part 1 — Field-sensitive Datalog points-to**:
   - Base facts (EDB): `a = new hA` (`Alloc(1, 10)`), `b = new hB` (`Alloc(2, 11)`), `p = a` (`Move(3, 1)`), `q = p` (`Move(4, 3)`), `a.next = b` (`Store(1, 100, 2)`), `r = q.next` (`Load(5, 4, 100)`).
   - Round 0 evaluates non-recursive rules (`Pts :- Alloc`, `Path :- Edge`) to seed `deltas`.
   - Subsequent rounds propagate `Pts` through `Move`, derive `HeapPts(10, 100, 11)` (`hA.next -> hB`) from `Store`, and resolve `Load` to derive `Pts(5, 11)` (`r` points to `hB`) with strictly fewer rule derivations than naive evaluation.
2. **Part 2 — ROBDD relational transitive closure**:
   - Nodes `0..=4` are encoded as 3-bit vectors `x_vars = [0,1,2]`, `y_vars = [3,4,5]`, `z_vars = [6,7,8]`.
   - Starting from `path_xy = edge_bdd`, each step renames `edge(x, y) -> edge(x, z)` and `path(x, y) -> path(z, y)`, takes their conjunction `edge(x, z) & path(z, y)`, existentially quantifies out `z` (`f|z=0 \/ f|z=1`), and unions back into `path_xy` until `next_xy == path_xy`.

### Incremental rules and canonical BDD operations

| Component | Key invariant | Role in demo |
| --- | --- | --- |
| Semi-naive Datalog | Joins at least one `delta` predicate per rule firing | Computes identical `Pts`/`HeapPts`/`Path` sets with fewer derivations |
| ROBDD unique table | `low == high => low`, shared `(var, low, high)` IDs | Guarantees `a & b` and `b & a` allocate the exact same node ID |
| BDD relational compose | `exists z. Edge(x, z) & Path(z, y)` | Reaches the identical fixed-point `Path` relation as Datalog |

### How to read the code

| Symbol | Role in [Rust](main.rs) / [Python](main.py) |
| --- | --- |
| `Term`, `Atom`, `Rule` | Horn clause AST: variables (`Var(idx)`), constants (`Const(val)`), predicate atoms, and `head :- body` rules |
| `match_atom`, `instantiate_head`, `join_rule_slot` | Backtracking relational join over `rule.body` with environment rollback and optional `delta_slot` substitution |
| `evaluate_naive`, `evaluate_semi_naive` | Naive full-table fixed point vs semi-naive delta-driven fixed point returning `(db, stats)` |
| `Robdd` (`mk`, `apply`, `restrict`, `exists`, `substitute`) | Arena-backed ROBDD with unique table, memoized `apply_cache`, cofactoring, existential projection, and variable renaming |
| `encode_pair`, `decode_pairs`, `solve_transitive_closure_bdd` | Bit-encodes `Edge(u, v)` pairs into BDDs and iterates relational composition to fixed point |
| `points_to_example` | Builds the `Alloc`/`Move`/`Store`/`Load`/`Edge` EDB and 7 IDB rules |

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
