# Semilattice Constant Propagation (MFP vs MOP Nondistributivity)

Solve constant propagation over the three-level flat semilattice (`Top`, `Const(c)`, `Bottom`) and demonstrate why the framework is monotone but non-distributive. See the [Rust source](main.rs) and [Python source](main.py).

```sh
cargo run --example const_prop
cargo test --example const_prop
python3 examples/const_prop/main.py
python3 -m unittest examples/const_prop/main.py
```

## What the example teaches

Bitvector frameworks (`reaching_defs`, `liveness`, `available_exprs`) are distributive, so the Maximum Fixed Point (MFP) solution equals the ideal Meet-Over-All-Paths (MOP) solution. Constant propagation is monotone *without* distributivity, so MFP can strictly lose precision at control-flow joins.

### The nondistributivity witness (`§9.4.5`)

| Branch | `x` | `y` | `z = x + y` |
| --- | --- | --- | --- |
| Then (`B1`) | `2` | `3` | `5` |
| Else (`B2`) | `3` | `2` | `5` |
| **MFP join at `B3`** | `NAC` (`Bottom`) | `NAC` (`Bottom`) | `NAC` (`Bottom`) |
| **MOP exact over paths** | — | — | `5` |

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
