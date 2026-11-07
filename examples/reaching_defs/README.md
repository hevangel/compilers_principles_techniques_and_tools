# Forward reaching definitions

Find which assignments can supply a value at a control-flow join. See the [Rust source](main.rs).

```sh
cargo run --example reaching_defs
cargo test --example reaching_defs
```

## What the example teaches

`reaching` unions predecessor outputs. Each assignment removes older definitions of the same variable and adds its own definition ID. Iteration propagates information through loops until the sets stabilize.

A **definition** is a particular assignment in the program. A definition reaches a point if control can travel from that assignment to the point without another assignment to the same variable along the way. The analysis tracks assignment identities. Two assignments to `x` therefore produce two distinct facts.

### Propagate information forward

`reaching` starts each block with the union of its predecessors' outgoing facts. The union expresses **may** reach. A definition that arrives along either predecessor is relevant. The block then processes assignments in program order. Each assignment **kills** older facts for the same variable and **generates** a fact for itself.

The example stores a definition as an integer and obtains its variable from `variable_of`. Scanning assignments in order also handles a block that assigns the same variable twice. Only its last assignment to that variable survives on exit.

### Follow the diamond

| Block | Assignments | Incoming IDs | Outgoing IDs |
| --- | --- | --- | --- |
| B0 | 0 defines `x`, 1 defines `y` | `{}` | `{0, 1}` |
| B1 | 2 defines `x` | `{0, 1}` | `{1, 2}` |
| B2 | 3 defines `x` | `{0, 1}` | `{1, 3}` |
| B3 | None | `{1, 2, 3}` | `{1, 2, 3}` |

B1 and B2 both overwrite the entry definition of `x`. Their own definitions both reach B3 because they arrive along different paths. The loop test illustrates why iteration is needed. A definition generated in a loop body can arrive at the header on a later pass through the graph.

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

The program uses scalar variables and a control-flow graph specified in Rust.
