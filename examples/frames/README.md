# Activation records and static links

Distinguish the frame that called a procedure from its lexical parent. See the [Rust source](main.rs).

```sh
cargo run --example frames
cargo test --example frames
```

## What the example teaches

The caller link records where execution returns. The static link points to the active frame of the enclosing lexical scope. `read_nonlocal` walks a fixed number of static links to read a local there.

## What to look for

A helper calls its lexical sibling. The sibling returns to the helper but finds the enclosing local value 21 in the outer frame. The test checks zero, one, and two static hops.

## Edition mapping and scope

| 1977 Green | 1986 Red | 2006 Purple |
| --- | --- | --- |
| 2, 10 | 7 | 7 |

Numbers refer to chapters. Related readings and appendices are marked where applicable. See the [edition guide](../../README.md#edition-guide) for the bibliographic references and chapter coverage.

The stack snapshot is assembled by hand. It models links and local lookup with one slot per frame.
