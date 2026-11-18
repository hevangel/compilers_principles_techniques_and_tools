# Lexical Display vs Static Links and Stack VLAs

Compare O(1) lexical display lookups against static-link chain walks and allocate stack variable-length arrays (VLAs). See the [Rust source](main.rs) and [Python source](main.py).

```sh
cargo run --example display_vla
cargo test --example display_vla
python3 examples/display_vla/main.py
python3 -m unittest examples/display_vla/main.py
```

## What the example teaches

### Why we need this

Two classic runtime bottlenecks arise when compiling nested procedures and runtime-sized arrays:

1. **Pointer-chasing static links (`O(depth)` vs `O(1)`):** Walking static links to access a variable `k` lexical levels outward requires `k` dependent memory loads on every access. In deeply nested or recursive code—such as `ping` and `pong` at depth 2 repeatedly reading `outer` (depth 0) and `mid` (depth 1)—that overhead adds up on every nonlocal read. A **lexical display** `display[0..=max_depth]` solves this by caching the currently active frame ID for each lexical depth, turning every nonlocal access into a single indexed lookup. On call, a procedure saves `display[depth]` inside its frame and sets `display[depth] = frame_id`; on return, it restores `saved_display`. Without that save/restore step, calling a sibling at the same depth (like `mid_a` calling `mid_b` at depth 1) would permanently overwrite `display[1]`, leaving `mid_a` unreachable after `mid_b` returns.
2. **Stack-allocated variable-length arrays (VLAs):** When an array's length `n` is unknown at compile time, the compiler cannot bake offsets of later variables into fixed frame offsets if the VLA sits before them. Placing fixed-size locals first (`locals_start..vla_ptr`) and dynamic VLA storage afterward (`vla_ptr..vla_ptr + vla_len`) keeps all scalar locals at compile-time constant offsets while allowing `pop_frame` to free both fixed locals and the VLA in one stack truncation.

### How the display and stack frame work

A display array `d[0..=max_depth]` caches the active frame pointer for each lexical nesting depth so nonlocal accesses execute in one indexed lookup rather than walking `k` static links, while saving and restoring `d[depth]` on call/return keeps sibling and recursive frames isolated.

In `simulate_mutual_recursion(10, 3, 4)`:
- `outer` (depth 0) pushes fixed local `[10]` and sets `display[0] = 0`.
- `mid` (depth 1) pushes fixed local `[3]` plus a 4-element VLA `[1, 2, 3, 4]` and sets `display[1] = 1`.
- `ping(4)` and `pong(3)` alternate at depth 2, each saving the previous `display[2]` before updating it and reading `outer[0]` (`10`) and `mid[0]` (`3`) in 1 display hop apiece (versus 2–3 static-link hops).

### Stack frame layout with VLAs

| Offset region | Contents | Lifetime |
| --- | --- | --- |
| `locals_start..vla_ptr` | Fixed-size parameters and scalar locals | Frame duration |
| `vla_ptr..vla_ptr + vla_len` | Dynamic array storage pointed to at fixed offset | Truncated on return |

### How to read the code

| Component (`main.rs` / `main.py`) | Role |
| --- | --- |
| `Frame` | Stores `depth`, `static_link`, `saved_display`, stack base `locals_start`, and optional `vla_ptr` / `vla_len`. |
| `RuntimeMachine::push_frame` | Links `static_link` to `display[depth - 1]`, saves `display[depth]`, allocates fixed locals then VLA, and updates `display[depth]`. |
| `RuntimeMachine::pop_frame` | Restores `display[depth]` from `saved_display` and truncates the stack back to `locals_start`. |
| `read_via_display` / `read_via_static_link` | Reads a nonlocal variable via `O(1)` display lookup vs `O(depth)` static-link walk while counting hops. |
| `write_vla` / `read_vla` | Bounds-checked element access relative to `frame.vla_ptr`. |
| `simulate_mutual_recursion` | Runs `outer -> mid -> ping <-> pong` and asserts display reads always match static-link reads. |

## What to look for

Under mutually recursive `ping`/`pong` calls at depth 2 accessing depth-0 and depth-1 locals, `read_via_display` and `read_via_static_link` return identical values on every lookup while `display_hops` stays strictly below `static_hops`. When sibling `mid_b` at depth 1 returns, restoring `saved_display` re-exposes `mid_a` and its VLA elements intact.

## Check your understanding

- Why does calling `mid_b` from `mid_a` (both at depth 1) require saving `display[1]` before overwriting it?
- Why are VLA elements placed after fixed-size locals rather than at the top of the parent frame?

## Edition mapping and scope

| 1977 Green | 1986 Red | 2006 Purple |
| --- | --- | --- |
| 2, 10 | 7 | 7 |

Numbers refer to chapters. Related readings and appendices are marked where applicable. See the [edition guide](../../README.md#edition-guide) for the bibliographic references and chapter coverage.

The machine models call frames on an explicit integer stack; calling-convention callee-save registers and heap-allocated closures are omitted.
