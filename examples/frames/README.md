# Activation records and static links

Distinguish the frame that called a procedure from its lexical parent. See the [Rust source](main.rs).

```sh
cargo run --example frames
cargo test --example frames
```

## What the example teaches

The caller link records where execution returns. The static link points to the active frame of the enclosing lexical scope. `read_nonlocal` walks a fixed number of static links to read a local there.

A procedure call creates an **activation record**, also called a stack frame, for one invocation. It can hold local variables, saved control information, and links to other frames. Recursive calls can have multiple frames for the same procedure, each with its own locals.

### Two links answer different questions

A **caller link**, sometimes called a dynamic link, follows the chain of active calls. A **static link** follows the source language's nesting of procedure declarations. In this model the caller link identifies the caller's frame, while a static link identifies the current activation of the lexically enclosing procedure. A complete runtime also needs to know the return instruction address.

The source structure and the order of calls differ in the sample.

| Frame index | Procedure | Local value | Caller index | Static parent index |
| --- | --- | --- | --- | --- |
| 0 | `outer` | 21 | — | — |
| 1 | `helper` | 90 | 0 | 0 |
| 2 | `inner` | 7 | 1 | 0 |

Both `helper` and `inner` are declared inside `outer`, so both static links point to frame 0. The call sequence is `outer → helper → inner`, so the caller of `inner` is frame 1.

### Read a nonlocal variable

For a source name resolved one lexical level outward, generated code can follow one static link and read a known slot in that frame. `read_nonlocal` models this operation with a frame index and hop count. Zero hops reads the local in the starting frame. A hop past the outermost frame yields `None`.

## What to look for

A helper calls its lexical sibling. The sibling returns to the helper but finds the enclosing local value 21 in the outer frame. The test checks zero, one, and two static hops.

## Check your understanding

- From frame 2, compare the value reached by following one static link with the value reached by following one caller link.
- If `inner` were declared inside `helper` and called during the same activation, which link on frame 2 would change?

## Edition mapping and scope

| 1977 Green | 1986 Red | 2006 Purple |
| --- | --- | --- |
| 2, 10 | 7 | 7 |

Numbers refer to chapters. Related readings and appendices are marked where applicable. See the [edition guide](../../README.md#edition-guide) for the bibliographic references and chapter coverage.

The stack snapshot is assembled by hand. It models links and local lookup with one slot per frame.
