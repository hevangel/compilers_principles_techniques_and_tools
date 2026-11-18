# Register/Address Descriptors, Next-Use, and `getReg` Code Generator

Compute backward next-use/liveness information and generate basic-block machine code using register/address descriptors and spilling. See the [Rust source](main.rs) and [Python source](main.py).

```sh
cargo run --example reg_desc_codegen
cargo test --example reg_desc_codegen
python3 examples/reg_desc_codegen/main.py
python3 -m unittest examples/reg_desc_codegen/main.py
```

## What the example teaches

### Why we need this

Unlike single expression trees (`register_codegen`), real basic blocks contain sequences of three-address statements that **reuse variables** multiple times (`t = a - b; u = a - c; v = t + u; a = d; d = v + u`). A naive code generator that loads every operand from memory and stores every result back to memory wastes loads and stores—for instance, loading `a` twice, loading `u` twice, and writing dead temporaries `t` and `v` to memory even though nobody reads them after the block ends!

To emit fast code while staying correct under a fixed register count `num_regs`, the Dragon Book local code generator tracks two runtime-model tables during code emission:
- **Register descriptor (`reg_desc[r]`):** which variables currently have their up-to-date value in register `r` (a copy `a = d` lets both `a` and `d` share one register without emitting a `Mov` or `St`!).
- **Address descriptor (`addr_desc[v]`):** all locations (registers and/or `Mem(v)`) that currently hold `v`'s latest value, so `pick_victim` can evict variables that already have a memory copy for free before spilling a register-only value.

Guided by a backward **next-use and liveness pass** (`compute_next_use`), `get_reg_for_dst` can also immediately reuse an operand register when the operand is dead after the instruction (for example, `t`'s register in `v = t + u`), and at block exit only `live_on_exit` variables (`{a, d}`) whose memory home is stale get flushed with `St`.

### Two-pass basic-block algorithm

Good local code generation avoids redundant loads by tracking which variables currently reside in registers (`reg_desc`) and which locations—registers or memory—hold up-to-date values (`addr_desc`), guided by backward next-use and block-exit liveness.

| Pass | Direction | Purpose |
| --- | --- | --- |
| `compute_next_use` (`§8.4.2`) | Backward | Annotate every def and use with `(live, next_use)` |
| `generate_block` (`§8.6`) | Forward | Pick reuse or minimal-spill victims via `get_reg_for_dst`, flush dirty live-outs |

In the classic 5-instruction block (`live_out = {a, d}`):
- **3 registers:** `a`, `b`, `c`, `d` are loaded once as needed; `t` and `v` never touch memory; `a = d` aliases `a` into `d`'s register; and only `a` and `d` are stored at exit.
- **2 registers (under higher temporary pressure like `t1 = a+b; t2 = c+d; t3 = e+f; out1 = t1-t2; out = out1+t3`):** when all registers hold live-only values, `pick_victim` chooses the variable with the **farthest next use**, emits `St` to spill it, and `ensure_in_reg` emits `Ld` to reload it later.

### How to read the code

| Component (`main.rs` / `main.py`) | Role |
| --- | --- |
| `Quad`, `RiscInstr` | Three-address input IR (`dst = arg1 op arg2` or copy) and target RISC instructions (`Ld`, `St`, `Mov`, `Add`, `Sub`). |
| `compute_next_use` | Backward scan from `live_on_exit` producing `VarUseInfo(live, next_use)` for each instruction's `dst`, `arg1`, `arg2`. |
| `CodegenState` | Maintains `reg_desc`, `addr_desc`, `pick_victim` (empty -> backed-up -> farthest-next-use), `spill_reg`, `ensure_in_reg`, and `get_reg_for_dst`. |
| `generate_block` | Computes `live_after` per instruction, loads operands, selects `rz`, updates descriptors, and flushes dirty `live_on_exit` variables. |
| `eval_quads` / `eval_risc` | Reference 3-address interpreter and RISC machine interpreter verifying exact semantic equivalence. |

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
