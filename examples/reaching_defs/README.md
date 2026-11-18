# Forward reaching definitions

Find which assignments can supply a value at a control-flow join. See the [Rust source](main.rs) and [Python source](main.py).

```sh
cargo run --example reaching_defs
cargo test --example reaching_defs
python3 examples/reaching_defs/main.py
python3 -m unittest examples/reaching_defs/main.py
```

## What the example teaches

### Why we need this and when it is sound

When a compiler optimizes a read of variable `x`—for example, deciding whether `x` always holds a constant, whether a copy `y = x` can be substituted, or whether an operand in a loop is loop-invariant—it needs **use-definition (UD) chains**: the exact set of assignment statements in the program that could have produced `x`'s current value.

A **definition** `d` of variable `v` **reaches** a program point `p` if **there is at least one control-flow path** from `d` to `p` along which `v` is not overwritten by another definition. Because missing even one possible reaching assignment could falsely convince the compiler that `v` has only a single constant source, reaching definitions must be a **forward may-analysis**:

- **May (union) confluence:** `IN[B] = U_{P in pred(B)} OUT[P]`. Any definition arriving from *any* predecessor survives into `B`.
- **Empty initialization (`∅`):** Starting sets at `∅` grows them monotonically to the least fixed point; omitting an unreachable or unvisited definition is avoided once all reachable paths are iterated to convergence.
- **Identity vs. name:** Two separate assignments to `x` (`d0` and `d2`) carry distinct integer IDs so the compiler knows *which* site(s) supplied `x`.

### Propagate information forward

`reaching` computes `IN[B]` as the union of its predecessors' `OUT` sets, then simulates each assignment `d` inside `B` in program order:

1. **Kill:** remove every definition ID `old` in the current set whose `variable_of[old] == variable_of[d]`.
2. **Generate:** insert `d` itself.

Scanning assignments in order naturally handles blocks that assign the same variable multiple times: earlier writes in the same block are killed by later writes so only the final assignment to each variable escapes in `OUT[B]`. (Equivalently, block-level `OUT[B] = GEN[B] U (IN[B] \ KILL[B])`.)

### Follow the diamond

The sample CFG forms a diamond (`B0 -> {B1, B2} -> B3`) with `variable_of = ["x", "y", "x", "x"]`:

| Block | Predecessors | Assignments | `IN` (Incoming IDs) | Killed IDs | `OUT` (Outgoing IDs) |
| --- | --- | --- | --- | --- | --- |
| `B0` | `[]` | `0` (`x`), `1` (`y`) | `{}` | — | `{0, 1}` |
| `B1` | `[0]` | `2` (`x`) | `{0, 1}` | `0` (`x`) | `{1, 2}` |
| `B2` | `[0]` | `3` (`x`) | `{0, 1}` | `0` (`x`) | `{1, 3}` |
| `B3` | `[1, 2]` | None | `{1, 2, 3}` | — | `{1, 2, 3}` |

`B1` and `B2` both kill `0` (`x` from `B0`), while preserving `1` (`y` from `B0`). At `B3`, unioning `OUT[B1] = {1, 2}` and `OUT[B2] = {1, 3}` yields `{1, 2, 3}`. The second test checks a two-block loop (`B0 (def 0:x) -> B1 (def 1:x) -> B1`): on pass 1 `IN[B1] = {0}` and `OUT[B1] = {1}`; on pass 2 the back edge feeds `OUT[B1]` back into `B1`, giving `IN[B1] = {0, 1}`.

### How to read the code

| Component | Rust ([`main.rs`](main.rs)) | Python ([`main.py`](main.py)) | Purpose |
| --- | --- | --- | --- |
| CFG node | `Block`, `Definitions` | `Block` | Holds `predecessors` indices and ordered `assignments` definition IDs |
| Forward solver | `reaching(blocks, variable_of)` | `reaching(blocks, variable_of)` | Iterates predecessor unions and sequential kill/gen transfers until stable |
| Sample & test | `sample`, `main`, test fn | `sample`, `main`, `ReachingDefsTests` | Tests both the 4-block diamond join and a 2-block self-loop back-edge |

## What to look for

At the join, both branch definitions of `x` reach the block, along with the entry definition of `y`. A second test case checks that a loop-carried definition reaches the loop header.

## Check your understanding

- If B2 has no assignment, which definition IDs reach B3? Identify one path for each surviving definition of `x`.
- Compare this analysis with [liveness](../liveness/README.md). Which follows the value toward its later uses, and which starts from future reads?

## Edition mapping and scope

| 1977 Green | 1986 Red | 2006 Purple |
| --- | --- | --- |
| 12, 14 | 10 | 9 |

Numbers refer to chapters. Related readings and appendices are marked where applicable. See the [edition guide](../../README.md#edition-guide) for the bibliographic references and chapter coverage.

The program uses scalar variables and a control-flow graph specified directly in code.
