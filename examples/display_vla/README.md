# Lexical Display vs Static Links and Stack VLAs

Compare O(1) lexical display lookups against static-link chain walks and allocate stack variable-length arrays (VLAs). See the [Rust source](main.rs) and [Python source](main.py).

```sh
cargo run --example display_vla
cargo test --example display_vla
python3 examples/display_vla/main.py
python3 -m unittest examples/display_vla/main.py
```

## What the example teaches

A display array `d[0..=max_depth]` caches the active frame pointer for each lexical nesting depth so nonlocal accesses execute in one indexed lookup rather than walking `k` static links, while saving and restoring `d[depth]` on call/return keeps sibling and recursive frames isolated.

### Stack frame layout with VLAs

| Offset region | Contents | Lifetime |
| --- | --- | --- |
| `locals_start..vla_ptr` | Fixed-size parameters and scalar locals | Frame duration |
| `vla_ptr..vla_ptr + vla_len` | Dynamic array storage pointed to at fixed offset | Truncated on return |

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
