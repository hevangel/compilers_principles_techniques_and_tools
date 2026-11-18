# Expression DAGs and Value-Number Hashing

Hash-cons expression ASTs into a shared DAG with commutative normalization, algebraic folding, and alias-epoch invalidation for memory accesses. See the [Rust source](main.rs) and [Python source](main.py).

```sh
cargo run --example expr_dag
cargo test --example expr_dag
python3 examples/expr_dag/main.py
python3 -m unittest examples/expr_dag/main.py
```

## What the example teaches

### Why we need this

A naive AST preserves every syntactic occurrence of a subexpression as a separate tree node. In `(a + b) + (a + b) * (b + a)`, an AST has 11 nodes (three distinct `+` nodes for `a + b` and `b + a`). Generating code straight from that AST computes `a + b` three separate times. Worse, across statements in a basic block:

- Pure arithmetic identities like `(x + 0) * 1 + (x - x)` should collapse to `x` without emitting useless instructions.
- Repeated memory loads like `x = A[i]; y = A[i]` can reuse `x`—**unless** an intervening write like `A[j] = 99` occurs. Because `i` and `j` could hold the same runtime index (aliasing), caching `A[i]` purely by syntax `(A, i)` would silently return the stale pre-store value!

**Value-number hash-consing** into a DAG solves the pure redundancy problem, while **per-array modification epochs** keep cached memory loads sound across stores.

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
| `#2` | `Bin(Add, #0, #1)` | Shared `a + b` (and `b + a` after commutative swap) |
| `#3` | `Bin(Mul, #2, #2)` | `(a + b) * (b + a)` |
| `#4` | `Bin(Add, #2, #3)` | Root expression |

### Basic-block memory trace with alias epochs

| Statement | Array epoch for `A` | Interned DAG key | Emitted TAC |
| --- | --- | --- | --- |
| `x = A[i]` | `0` | `ArrayLoad("A", idx(i), epoch=0)` (new) | `ArrayLoad x = A[i]` |
| `y = A[i]` | `0` | `ArrayLoad("A", idx(i), epoch=0)` (hit!) | `Copy y = x` |
| `A[j] = 99` | `0 -> 1` | Increments `array_epoch["A"]` to `1` | `LoadConst t0 = 99; ArrayStore A[j] = t0` |
| `z = A[i]` | `1` | `ArrayLoad("A", idx(i), epoch=1)` (new) | `ArrayLoad z = A[i]` |

## How to read the code

| Concept | Rust (`main.rs`) | Python (`main.py`) | Role |
| --- | --- | --- | --- |
| AST & statements | `ExprAst`, `BbStmt` | `Var`/`Const`/`Bin`, `Assign`/`ArrayRead`/`ArrayWrite`/`PtrKill` | Input expression trees and basic-block statements |
| DAG & TAC IR | `DagNode`, `TacInst` | Tuple keys (`"Leaf"`, `"Bin"`, ...) & TAC tuples | Hash-consed value nodes and three-address output |
| Value numbering | `DagBuilder::intern`, `leaf`, `lower_ast`, `make_bin` | `DagBuilder.intern`, `leaf`, `lower_ast`, `make_bin` | Deduplicates, folds constants/identities, normalizes `+`/`*` |
| Block lowering | `DagBuilder::process_block`, `materialize`, `name_of` | `DagBuilder.process_block`, `_materialize`, `_name_of` | Tracks array/global epochs and emits TAC once per node |

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
