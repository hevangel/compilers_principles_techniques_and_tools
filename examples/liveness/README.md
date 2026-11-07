# Backward liveness analysis

Find which values may still be read along a future path. See the [Rust source](main.rs).

```sh
cargo run --example liveness
cargo test --example liveness
```

## What the example teaches

A block’s live-out set is the union of its successors’ live-in sets. Its live-in set adds reads that occur before local definitions, then propagates the live-out values that the block does not overwrite. The calculation repeats until stable.

A value is **live** at a program point if some later path can read it before the variable is assigned again. This is a question about the current value of the variable. A new assignment can make the old value unnecessary even if the variable's name appears later. Register allocation can use this information to decide which values need storage at the same time.

### Propagate information backward

A **control-flow graph** links basic blocks by possible next steps. For each block `B`, the example solves these equations.

```text
OUT[B] = union of IN[S] for every successor S
IN[B]  = USE[B] union (OUT[B] minus DEF[B])
```

A value live on any successor path is live on exit. An assignment kills the incoming value for later reads, so the second equation subtracts definitions before adding reads that occur before the block's first definition of each variable. In `x = x + step`, the old `x` is read before the new `x` is written, so `x` belongs to both sets.

### Follow the loop

Block 3 returns `x`, giving it live-in set `{x}` and empty live-out. Block 1 branches to the body or the return and reads `again`. Block 2 reads `x` and `step` and can branch back to block 1, so the need for `step` can travel around the loop. At the fixed point, block 0's live-out set is `{again, step, x}`. Its first assignment to `x` means the incoming set is only `{again, step}`.

The algorithm starts with empty sets and repeats until no live-in changes. Repeating is essential because a block can acquire a fact from a successor that acquired it on a previous pass.

## What to look for

The loop keeps `step` and `x` live on its back edge. At the entry block, the first assignment kills the incoming need for `x`. The test also checks that the exit has no live-out values.

## Check your understanding

- Why can a variable appear in both USE and DEF for the same block?
- If block 3 returns a constant instead of `x`, which live-in set changes immediately? Explain why the loop body can still make `x` live on other paths.

## Edition mapping and scope

| 1977 Green | 1986 Red | 2006 Purple |
| --- | --- | --- |
| 12, 14 | 10 | 9 |

Numbers refer to chapters. Related readings and appendices are marked where applicable. See the [edition guide](../../README.md#edition-guide) for the bibliographic references and chapter coverage.

The example starts with explicit control-flow edges and per-block use/definition sets.
