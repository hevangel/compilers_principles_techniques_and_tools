# Record Field Layout and Quadruple / Triple / Indirect-Triple IR

Compute aligned field offsets and padding for nested `Struct` and `Union` types, and compare three-address code representations under instruction reordering. See the [Rust source](main.rs) and [Python source](main.py).

```sh
cargo run --example record_ir
cargo test --example record_ir
python3 examples/record_ir/main.py
python3 -m unittest examples/record_ir/main.py
```

## What the example teaches

### Why we need this

A parser accepts `struct { u8 a; u32 b; u8 c; }` and `p = base + offset; x = a * b + c;` as syntax trees, but syntax alone does not tell the backend **where each field lives in memory** or **how to represent intermediate computations so optimization passes can reorder instructions safely**:

1. **Aligned record layout**: CPUs require multi-byte loads (such as 4-byte `u32` or 8-byte `u64`) to sit at addresses divisible by their alignment, or else suffer slow unaligned traps. Naive byte packing (`0, 1, 5`) breaks alignment, so the compiler must insert **internal padding** between fields and **tail padding** at the end of a struct (so `struct S arr[2]` keeps `arr[1]` aligned too).
2. **Three-address IR representations**: After layout gives each field a constant offset, expressions must be lowered to flat three-address instructions. Assigning every intermediate value a named temporary (`Quadruple`: `t0 = a * b`) lets optimizers freely move instructions, at the cost of extra symbol-table names. Referring to results by their table slot (`Triple`: `(0)`, `(1)`) saves temporary names—but moving triple `(2)` to slot `0` breaks every later `(2)` operand! **Indirect triples** separate the fixed operation table from a reorderable `order` schedule list so code motion only permutes integers in `order`.

### Trace struct and union layout

For `{u8, u32, u8}` vs `{u32, u8, u8}` vs `{tag: u8, payload: union{word: u64, pair: {u8, u32, u8}}}`:

| Type | Field offsets | Align (`max_align`) | Total size (with tail pad) | Padding bytes |
| --- | --- | --- | --- | --- |
| `{a: u8, b: u32, c: u8}` | `a@0`, `b@4`, `c@8` | `4` | `12` (`9` rounded to `4`) | `6` (`3` before `b` + `3` tail) |
| `{b: u32, a: u8, c: u8}` | `b@0`, `a@4`, `c@5` | `4` | `8` (`6` rounded to `4`) | `2` (tail only) |
| `union{word: u64, pair: ...}` | `word@0`, `pair@0` | `8` | `16` (`max_size=12` rounded to `8`) | `4` (`16 - 12`) |
| `{tag: u8, payload: union...}` | `tag@0`, `payload@8` | `8` | `24` (`8 + 16`) | `7` (before `payload`) |

### Comparing the representations

| Representation | Value reference | Instruction motion cost |
| --- | --- | --- |
| `Quadruple` | Named temporary (`t0`) | Move entry directly |
| `Triple` | Triple table slot `(k)` | Must rewrite every `(k)` reference |
| `IndirectTriples` | Triple table slot `(k)` | Reorder `order` array; `(k)` stays unchanged |

### Trace hoisting `a * b` in `IndirectTriples`

For `p = field_addr(base, 8); x = a * b + c;`, `lower_to_triples` builds 5 immutable triple slots:

| Slot | Triple | Meaning | Initial `order` | Hoisted `order` (`[2, 0, 1, 3, 4]`) |
| --- | --- | --- | --- | --- |
| `0` | `(addr+, base, 8)` | `base + 8` | Step 1 | Step 2 |
| `1` | `(=, p, (0))` | `p = (0)` | Step 2 | Step 3 |
| `2` | `(*, a, b)` | `a * b` | Step 3 | **Step 1 (hoisted!)** |
| `3` | `(+, (2), c)` | `(2) + c` | Step 4 | Step 4 — `TripleIdx(2)` is untouched! |
| `4` | `(=, x, (3))` | `x = (3)` | Step 5 | Step 5 |

Because triple `(2)` executes first in `order`, `results[2]` is already populated before triple `(3)` runs, yielding the exact same `x` and `p` as `Quadruple` without rewriting a single operand.

## How to read the code

| Concept | Rust (`main.rs`) | Python (`main.py`) | Role |
| --- | --- | --- | --- |
| Type system | `enum Type` (`U8`, `U32`, `U64`, `Struct`, `Union`) | `PrimType`, `StructType`, `UnionType` | Nested record/union definitions |
| Alignment & layout | `fn align_up`, `fn layout_type` | `def align_up`, `def layout_type` | Computes field offsets, struct/union align, size, and padding |
| Source expressions | `enum Expr`, `enum BinOp` | `VarExpr`, `FieldAddrExpr`, `BinExpr`, `BinOp` | Input expressions mixing field addressing and arithmetic |
| Quadruple IR | `struct Quadruple`, `fn lower_to_quads` | `class Quadruple`, `def lower_to_quads` | Emits four-field instructions with fresh `t0, t1, ...` temporaries |
| Indirect Triple IR | `struct Triple`, `IndirectTriples`, `lower_to_triples` | `class Triple`, `IndirectTriples`, `lower_to_triples` | Emits positional `(k)` triples plus separate `order` schedule |
| Evaluators | `fn eval_quads`, `fn eval_indirect_triples` | `def eval_quads`, `def eval_indirect_triples` | Verify semantic equivalence before and after instruction motion |

## What to look for

Reordering `{u8, u32, u8}` to `{u32, u8, u8}` cuts total size from 12 bytes (6 padding bytes) to 8 bytes (2 tail-padding bytes). Hoisting `x * y` ahead of `base + 8` in `IndirectTriples` alters only `order`, leaving every `TripleIdx` operand untouched while preserving identical evaluation results with the quadruple lowering.

## Check your understanding

- Why does a struct's alignment equal the maximum alignment of its fields?
- What happens to memory usage when `IndirectTriples` drops unused temporaries vs reordering independent instructions?

## Edition mapping and scope

| 1977 Green | 1986 Red | 2006 Purple |
| --- | --- | --- |
| 2, 8 | 8 | 6 |

Numbers refer to chapters. Related readings and appendices are marked where applicable. See the [edition guide](../../README.md#edition-guide) for the bibliographic references and chapter coverage.

Record layout models C-style natural alignment (`U8`, `U32`, `U64`, `Struct`, `Union`) without packed attributes or bitfields.
