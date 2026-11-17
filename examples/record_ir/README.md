# Record Field Layout and Quadruple / Triple / Indirect-Triple IR

Compute aligned field offsets and padding for nested `Struct` and `Union` types, and compare three-address code representations under instruction reordering. See the [Rust source](main.rs) and [Python source](main.py).

```sh
cargo run --example record_ir
cargo test --example record_ir
python3 examples/record_ir/main.py
python3 -m unittest examples/record_ir/main.py
```

## What the example teaches

Type translation must compute exact byte offsets respecting alignment padding while three-address IR design trades explicit temporary names (`Quadruple`) for positional operand indices (`Triple`) and reorderable instruction-index lists (`IndirectTriples`).

### Comparing the representations

| Representation | Value reference | Instruction motion cost |
| --- | --- | --- |
| `Quadruple` | Named temporary (`t0`) | Move entry directly |
| `Triple` | Triple table slot `(k)` | Must rewrite every `(k)` reference |
| `IndirectTriples` | Triple table slot `(k)` | Reorder `order` array; `(k)` stays unchanged |

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
