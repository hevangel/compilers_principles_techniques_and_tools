# Backward liveness analysis

Find which values may still be read along a future path. See the [Rust source](main.rs) and [Python source](main.py).

```sh
cargo run --example liveness
cargo test --example liveness
python3 examples/liveness/main.py
python3 -m unittest examples/liveness/main.py
```

## What the example teaches

### Why we need this and when it is sound

Two core compiler backends depend on knowing which variables still hold useful values at each program point:

1. **Register allocation:** Two variables that are live at the same program point interfere with each other and cannot share a machine register; variables whose lifetimes never overlap can reuse the same register.
2. **Dead-code elimination:** An assignment `x = expr` whose destination `x` is *dead* (not live) immediately after the instruction can be deleted (assuming `expr` has no side effects).

A variable `v` is **live** at a program point if **there exists at least one path** from that point to a read of `v` before `v` is overwritten by a new assignment. Because discarding a needed value or overwriting its register silently corrupts execution, liveness must be a **backward may-analysis**:

- **May (union) confluence:** `OUT[B]` is the union (`U`) of `IN[S]` across all successors `S` of `B`. Over-approximating liveness is safe (the compiler keeps an extra value alive); under-approximating is unsound.
- **Empty initialization (`∅`):** Starting every set at `∅` grows sets monotonically upward toward the least fixed point, collecting every variable reachable along *some* future path.

### Propagate information backward

A **control-flow graph (CFG)** links basic blocks by possible control transfers. For each block `B`, the solver computes:

```text
OUT[B] = union of IN[S] for every successor S
IN[B]  = USE[B] union (OUT[B] minus DEF[B])
```

Here `USE[B]` is the set of **upward-exposed uses**—variables read in `B` *before* any assignment to them in `B`—and `DEF[B]` is the set of variables assigned in `B`. Subtracting `DEF[B]` kills incoming needs from `OUT[B]`, and unioning `USE[B]` adds reads that occur before the block's first definition. In `x = x + step`, the old `x` is read before the new `x` is written, so `x` belongs to *both* `USE` and `DEF`, keeping `x` in `IN`.

### Follow the loop

The sample CFG models `x = ...; while (again) { x = x + step; } return x;`:

```text
B0 (def x) ──▶ B1 (use again) ──▶ B3 (use x; return)
                 ▲        │
                 │        ▼
               B2 (use x, step; def x)
```

Iterating in reverse block order (`B3 -> B2 -> B1 -> B0`) converges in two computation passes (plus a verification pass):

| Block | `USE` / `DEF` | Pass 1 `OUT` → `IN` | Pass 2 (Fixed Point) `OUT` → `IN` |
| --- | --- | --- | --- |
| `B3` | `USE={x}`, `DEF={}` | `{}` → `{x}` | `{}` → `{x}` |
| `B2` | `USE={x, step}`, `DEF={x}` | `{}` → `{x, step}` | `{again, step, x}` → `{again, step, x}` |
| `B1` | `USE={again}`, `DEF={}` | `{x, step}` → `{again, step, x}` | `{again, step, x}` → `{again, step, x}` |
| `B0` | `USE={}`, `DEF={x}` | `{again, step, x}` → `{again, step}` | `{again, step, x}` → `{again, step}` |

Repeating until `live_in` stops changing is essential: on Pass 1, `B2` did not yet know `again` was needed by the header `B1` on the next iteration, so `again` only reaches `B2` on Pass 2 via the back edge `B2 -> B1`.

### How to read the code

| Component | Rust ([`main.rs`](main.rs)) | Python ([`main.py`](main.py)) | Purpose |
| --- | --- | --- | --- |
| CFG node | `Block`, `Vars` | `Block` | Stores upward-exposed `uses`, `defs`, and `successors` indices |
| Backward solver | `liveness(blocks)` | `liveness(blocks)` | Iterates blocks in reverse order updating `OUT` (`U`) and `IN` until fixed point |
| Sample & test | `sample`, `main`, test fn | `sample`, `main`, `LivenessTests` | Builds the 4-block loop CFG and asserts `IN`/`OUT` at entry, body, and exit |

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
