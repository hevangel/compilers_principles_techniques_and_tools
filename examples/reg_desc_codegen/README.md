# Register/Address Descriptors, Next-Use, and `getReg` Code Generator

Compute backward next-use/liveness information and generate basic-block machine code using register/address descriptors and spilling. See the [Rust source](main.rs) and [Python source](main.py).

```sh
cargo run --example reg_desc_codegen
cargo test --example reg_desc_codegen
python3 examples/reg_desc_codegen/main.py
python3 -m unittest examples/reg_desc_codegen/main.py
```

## What the example teaches

Good local code generation avoids redundant loads by tracking which variables currently reside in registers (`reg_desc`) and which locations—registers or memory—hold up-to-date values (`addr_desc`), guided by backward next-use and block-exit liveness.

### Two-pass basic-block algorithm

| Pass | Direction | Purpose |
| --- | --- | --- |
| `compute_next_use` (`§8.4.2`) | Backward | Annotate every def and use with `(live, next_use)` |
| `generate_block` (`§8.6`) | Forward | Pick reuse or minimal-spill victims via `get_reg_for_dst`, flush dirty live-outs |

## What to look for

For the classic `t = a - b; u = a - c; v = t + u; a = d; d = v + u` block (`live_out = {a, d}`), 3 registers require zero spills (`St` appears only for the final live-out flush), while constraining the target to 2 registers under higher temporary pressure forces an explicit spill `St` and reload `Ld` while matching the 3-address interpreter output identically.

## Check your understanding

- Why can `a` remain purely in a register after `t = a - b` and even share a register with `d` after `a = d` until flushed at block exit?
- Why does `pick_victim` prefer evicting a variable that already has an up-to-date `Mem` or secondary `Reg` copy before spilling a live-only-in-register value?

## Edition mapping and scope

| 1977 Green | 1986 Red | 2006 Purple |
| --- | --- | --- |
| 15 | 9 | 8 |

Numbers refer to chapters. Related readings and appendices are marked where applicable. See the [edition guide](../../README.md#edition-guide) for the bibliographic references and chapter coverage.

Code generation targets a single basic block with explicit `Ld`/`St`/`Mov`/`Add`/`Sub` RISC instructions; cross-block register allocation is covered in `graph_color_regalloc`.
