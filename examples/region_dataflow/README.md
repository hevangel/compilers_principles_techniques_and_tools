# Region-Based Data-Flow Analysis (T1/T2 Reducible Regions)

Construct a hierarchical region tree via T1/T2 graph reductions, compose affine `(gen, kill)` bitvector transfer functions bottom-up with loop closure `f*`, and propagate concrete `IN[B]` facts top-down. See the [Rust source](main.rs) and [Python source](main.py).

```sh
cargo run --example region_dataflow
cargo test --example region_dataflow
python3 examples/region_dataflow/main.py
python3 -m unittest examples/region_dataflow/main.py
```

## What the example teaches

Instead of iterating bitvectors repeatedly over arbitrary CFG edges, region-based analysis decomposes reducible flow graphs into nested single-entry regions and solves data-flow equations structurally in one bottom-up summary pass and one top-down instantiation pass.

### Structural T1 / T2 reductions and transfer algebra

| Operation | Structural meaning | Transfer rule |
| --- | --- | --- |
| `T2(u, v)` | Absorb `v` whose sole predecessor is `u` | Sequential composition `(G2 | (G1 & !K2), K2 | K1)` |
| `T1(u)` | Absorb self-loop edge `u -> u` | Loop Kleene closure `(gen, 0)` for may-analysis |
| Meet at join | Multiple paths between regions | `(G1 | G2, K1 & K2)` |

## What to look for

On a nested two-loop CFG (`B0 -> B1 -> {B2, B5}`, `B2 -> {B3 -> B2, B4 -> B1}`), `reduce_t1_t2` collapses the entire graph cleanly and computes reaching-definitions `IN[B]` matching `iterative_mfp_reaching_defs` bit-for-bit across all 6 blocks. On a double-entry loop `0 -> {1, 2}, 1 <-> 2`, neither T1 nor T2 applies, demonstrating irreducibility detection.

## Check your understanding

- Why does a self-loop closure `(gen, 0)` clear `kill` in a reaching-definitions may-analysis?
- How does node splitting convert an irreducible double-entry loop into a reducible CFG at the cost of code duplication?

## Edition mapping and scope

| 1977 Green | 1986 Red | 2006 Purple |
| --- | --- | --- |
| 13–14 | 10 | 9 |

Numbers refer to chapters. Related readings and appendices are marked where applicable. See the [edition guide](../../README.md#edition-guide) for the bibliographic references and chapter coverage.

Bitvector transfer functions pack up to 64 facts per word and illustrate reaching-definitions union meet without node splitting for irreducible graphs.
