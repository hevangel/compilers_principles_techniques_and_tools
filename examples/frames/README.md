# Activation records and static links

Distinguish the frame that called a procedure from its lexical parent. See the [Rust source](main.rs) and [Python source](main.py).

```sh
cargo run --example frames
cargo test --example frames
python3 examples/frames/main.py
python3 -m unittest examples/frames/main.py
```

## What the example teaches

### Why we need this

In languages with lexically nested procedures (such as Pascal, Ada, or closures in functional languages), a procedure can read variables declared in an enclosing procedure—not just its own locals and globals. When a procedure runs, its local variables live in an **activation record** (stack frame) created for that specific call; recursive calls create multiple frames for the same procedure, each with its own locals.

That creates a subtle problem: **who called you** is not always **where you were declared**. If `outer` declares two sibling helpers `helper` and `inner`, and `outer` calls `helper` which then calls `inner`, then:

- When `inner` **returns**, it must jump back to its caller `helper` (frame 1).
- When `inner` **reads a variable declared in `outer`**, it must look in `outer`'s frame (frame 0)—*not* `helper`'s frame!

Without two separate links, one of those two operations breaks: using the caller link for variable lookup would accidentally read `helper`'s local `90` instead of `outer`'s `21`, while using the lexical parent to return would skip `helper` entirely.

### Two links answer different questions

A **caller link**, sometimes called a dynamic link, follows the chain of active calls. A **static link** follows the source language's nesting of procedure declarations. In this model the caller link identifies the caller's frame, while a static link identifies the current activation of the lexically enclosing procedure. A complete runtime also needs to know the return instruction address.

`read_nonlocal` walks a fixed number of static links to read a local there. The source structure and the order of calls differ in the sample:

| Frame index | Procedure | Local value | Caller index | Static parent index |
| --- | --- | --- | --- | --- |
| 0 | `outer` | 21 | — | — |
| 1 | `helper` | 90 | 0 | 0 |
| 2 | `inner` | 7 | 1 | 0 |

Both `helper` and `inner` are declared inside `outer`, so both static links point to frame 0. The call sequence is `outer → helper → inner`, so the caller of `inner` is frame 1.

### Read a nonlocal variable

For a source name resolved one lexical level outward, generated code can follow one static link and read a known slot in that frame. `read_nonlocal` models this operation with a frame index and hop count. Zero hops reads the local in the starting frame (`7` in frame 2). One hop follows `static_parent` from frame 2 to frame 0 and reads `21`. A hop past the outermost frame yields `None`.

### How to read the code

| Component (`main.rs` / `main.py`) | Role |
| --- | --- |
| `Frame` | Models one activation record with a `local` slot, dynamic `caller` index, and lexical `static_parent` index. |
| `read_nonlocal` | Follows `static_parent` for `lexical_hops` steps from the current frame and returns the target frame's `local` (or `None` if out of bounds). |
| `sample` | Builds the 3-frame snapshot (`outer` at 0, `helper` at 1, `inner` at 2) where call order and lexical nesting diverge. |
| `main` / test | Prints `inner`'s return target (`Some(1)` / `1`) and outer lookup (`Some(21)` / `21`), then verifies 0, 1, and 2 static hops. |

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
