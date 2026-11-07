# Backpatching and short-circuit jumps

Fill forward jump targets after their destination instructions are emitted. See the [Rust source](main.rs).

```sh
cargo run --example backpatch
cargo test --example backpatch
```

## What the example teaches

`emit` returns unresolved true and false jump sites. AND patches its left true exits to the right operand. OR patches its left false exits there. `compile` connects the remaining exits to the two return instructions.

A **short-circuit** operator can decide its result after evaluating just the left operand. For `a && b`, false skips `b`. For `a || b`, true skips `b`. A compiler can implement these rules with branches. When it emits a forward branch, the destination instruction may not have been generated yet.

**Backpatching** fills those missing targets later. In this program an unfilled target is `None`, and a patch list stores instruction indices. A compiled condition returns a true-exit list and a false-exit list.

### Combine branch fragments

| Condition | When the left side is true | When the left side is false |
| --- | --- | --- |
| `left && right` | Patch the left true exits to the beginning of `right` | Keep the left false exits for the whole expression |
| `left || right` | Keep the left true exits for the whole expression | Patch the left false exits to the beginning of `right` |

An input leaf emits an `IfTrue` followed by an unconditional jump for the false case. Once all fragments have been emitted, `compile` appends true and false return instructions and patches the remaining exits to them. `patch` checks that each site was previously unfilled.

### Trace `(a && b) || c`

If `a` is false, the AND fragment takes its false exit, and the OR fragment redirects it to `c`. With inputs `[false, false, true]`, the read trace is `[0, 2]`. If both `a` and `b` are true, the OR has its answer and never reads `c`. The test checks the returned Boolean separately from the read trace.

## What to look for

The tests compare all eight inputs against the truth table for `(a && b) || c`. They also inspect which inputs were read to verify both short-circuit paths.

## Check your understanding

- Predict the returned value and read trace for `[true, false, false]`. Check the sequence against the branch table.
- For `a || b`, draw the four instructions for the two leaves and mark which one of `a`'s two exits leads to the start of `b`.

## Edition mapping and scope

| 1977 Green | 1986 Red | 2006 Purple |
| --- | --- | --- |
| 7 (Boolean translation) | 8 | 6 |

Numbers refer to chapters. Related readings and appendices are marked where applicable. See the [edition guide](../../README.md#edition-guide) for the bibliographic references and chapter coverage.

The input is an in-code condition tree. The generated code branches to Boolean return values on a small interpreter.
