# Expression DAGs and Value-Number Hashing

Hash-cons expression ASTs into a shared DAG with commutative normalization, algebraic folding, and alias-epoch invalidation for memory accesses. See the [Rust source](main.rs) and [Python source](main.py).

```sh
cargo run --example expr_dag
cargo test --example expr_dag
python3 examples/expr_dag/main.py
python3 -m unittest examples/expr_dag/main.py
```

## What the example teaches

Constructing a basic-block DAG eliminates local common subexpressions structurally. By canonicalizing commutative operand order and folding identity rules (`x + 0 -> x`, `x * 1 -> x`, `x - x -> 0`), structurally identical computations reuse the same DAG node ID (`value_number`). Memory operations track per-array modification epochs so an intervening store kills stale cached loads.

### Hash-consing and kill epochs

1. `intern` maps each canonical `DagNode` (`Leaf`, `Const`, `Bin`, `ArrayLoad`) to a unique integer ID.
2. `make_bin` folds constants, applies algebraic identities, and orders operands of `+` and `*` so `(a + b)` and `(b + a)` share one node.
3. `process_block` increments an array's epoch on `ArrayWrite` (and all epochs on `PtrKill`), ensuring `A[i]` before and after `A[j] = 99` receive distinct DAG nodes while consecutive reads in the same epoch reuse the earlier result.

### DAG for `(a + b) + (a + b) * (b + a)`

| Node | Definition | Meaning |
| --- | --- | --- |
| `#0` | `Leaf("a")` | Input `a` |
| `#1` | `Leaf("b")` | Input `b` |
| `#2` | `Bin(Add, #0, #1)` | Shared `a + b` |
| `#3` | `Bin(Mul, #2, #2)` | `(a + b) * (b + a)` |
| `#4` | `Bin(Add, #2, #3)` | Root expression |

## What to look for

The 11-node expression tree `(a + b) + (a + b) * (b + a)` collapses to 5 DAG nodes, and `(x + 0) * 1 + (x - x)` simplifies directly to `x`. In the basic-block sequence, the second `A[i]` read becomes a copy of `x`, whereas the read after `A[j] = 99` emits a fresh `ArrayLoad`.

## Check your understanding

- Why can `a + b` and `b + a` share a DAG node while `a - b` and `b - a` cannot?
- Why must a write to `A[j]` invalidate `A[i]` in the DAG even when `i` and `j` have different variable names?

## Edition mapping and scope

| 1977 Green | 1986 Red | 2006 Purple |
| --- | --- | --- |
| 7, 12 | 5, 9 | 5–6, 8 |

Numbers refer to chapters. Related readings and appendices are marked where applicable. See the [edition guide](../../README.md#edition-guide) for the bibliographic references and chapter coverage.

Memory aliasing uses conservative per-array epochs rather than symbolic index disambiguation.
