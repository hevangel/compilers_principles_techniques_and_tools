# Semilattice Constant Propagation (MFP vs MOP Nondistributivity)

Solve constant propagation over the three-level flat semilattice (`Top`, `Const(c)`, `Bottom`) and demonstrate why the framework is monotone but non-distributive. See the [Rust source](main.rs) and [Python source](main.py).

```sh
cargo run --example const_prop
cargo test --example const_prop
python3 examples/const_prop/main.py
python3 -m unittest examples/const_prop/main.py
```

## What the example teaches

### Why we need this and when it is sound

Knowing at compile time that a variable always evaluates to a fixed integer constant enables **constant folding** (evaluating `2 + 3` at compile time), **algebraic simplification** (`x * 0 -> 0`), and **dead branch elimination**.

Unlike bitvector frameworks (`reaching_defs`, `liveness`, `available_exprs`) whose sets track independent boolean facts, constant propagation maps every variable to an element of a three-level **flat semilattice**:

```text
          Top (UNDEF: uninitialized / not yet reached)
      /    |    \
 ... -1    0     1  ...  (Const(c): known constant c)
      \    |    /
        Bottom (NAC: Not A Constant — conflicting or unknown values)
```

With the meet operator `∧`:

- `Top ∧ x = x` (`Top` is the identity—optimistic starting value before paths arrive).
- `Bottom ∧ x = Bottom` (`Bottom` is absorbing—once non-constant, always non-constant).
- `Const(x) ∧ Const(y)` is `Const(x)` when `x == y`, and `Bottom` when `x != y`.

A constant-folding transformation is **sound** only when the solver returns `Const(c)` if and only if the variable equals `c` on *all* reachable paths—which means returning `Bottom` (`NAC`) is always safe (it merely declines to fold), while returning a wrong `Const(c)` would be unsound.

Bitvector frameworks are **distributive**, so the iterative **Maximum Fixed Point (MFP)** solution equals the ideal **Meet-Over-All-Paths (MOP)** solution. Constant propagation is **monotone** (`x <= y` implies `f(x) <= f(y)`), guaranteeing `MFP <= MOP` (soundness!) *without* distributivity—meaning MFP can strictly lose precision at control-flow joins.

### The nondistributivity witness (`§9.4.5`)

On the diamond CFG `B0 -> {B1, B2} -> B3` (`if c { x = 2; y = 3 } else { x = 3; y = 2 }; z = x + y`):

| Path / Point | `x` | `y` | `z = x + y` | Explanation |
| --- | --- | --- | --- | --- |
| Then (`B1` exit) | `Const(2)` | `Const(3)` | `Const(5)` | Both operands known along `B0 -> B1` |
| Else (`B2` exit) | `Const(3)` | `Const(2)` | `Const(5)` | Both operands known along `B0 -> B2` |
| **MFP join at `B3` entry** | `NAC` (`Bottom`) | `NAC` (`Bottom`) | — | Joins `x` (`2 ∧ 3`) and `y` (`3 ∧ 2`) *before* `B3`'s transfer |
| **MFP at `B3` exit** | `NAC` (`Bottom`) | `NAC` (`Bottom`) | `NAC` (`Bottom`) | `Bottom + Bottom = Bottom` |
| **MOP exact over paths** | — | — | `Const(5)` | Evaluates `B3` along each path first, then takes `5 ∧ 5 = 5` |

### How to read the code

| Component | Rust ([`main.rs`](main.rs)) | Python ([`main.py`](main.py)) | Purpose |
| --- | --- | --- | --- |
| Semilattice & meet | `ConstVal`, `meet(a, b)` | `LatticeKind`, `ConstVal`, `meet(a, b)` | Three-level flat semilattice (`Top`, `Const(c)`, `Bottom`) and meet rules |
| Transfer rules | `eval_rhs`, `transfer_block` | `eval_rhs`, `transfer_block` | Evaluates expressions (including `NAC * 0 -> Const(0)`) over an `Env` |
| MFP & MOP solvers | `solve_mfp`, `solve_mop_acyclic` | `solve_mfp`, `solve_mop_acyclic` | Iterative fixed-point solver vs. exact acyclic DFS path enumeration |
| Folding & tests | `fold_blocks`, `nondistributivity_witness`, tests | `fold_blocks`, `nondistributivity_witness`, `ConstPropTests` | Rewrites `Const(c)` sites and verifies MFP vs. MOP and loop folding |

## What to look for

Because MFP meets `x` (`2 /\ 3 = Bottom`) and `y` (`3 /\ 2 = Bottom`) independently before applying `+`, it concludes `z = Bottom` (`NAC`), whereas MOP evaluates `x + y = 5` on each path before meeting `5 /\ 5 = 5`. Transfer rules also fold `NAC * 0 -> Const(0)` safely.

## Check your understanding

- Why is a monotone, non-distributive framework still sound whenever MFP stops at `Bottom` instead of `Const(5)`?
- How do SCCP (Sparse Conditional Constant Propagation) and SSA interact with constant folding over `phi` nodes?

## Edition mapping and scope

| 1977 Green | 1986 Red | 2006 Purple |
| --- | --- | --- |
| — | — | 9 |

Numbers refer to chapters. Related readings and appendices are marked where applicable. See the [edition guide](../../README.md#edition-guide) for the bibliographic references and chapter coverage.

Constant propagation handles integer addition and multiplication without conditional-branch pruning (`SCCP`) or division-by-zero trapping rules.
