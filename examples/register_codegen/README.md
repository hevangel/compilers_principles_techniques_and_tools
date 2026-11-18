# Register needs for expression trees

Choose an evaluation order that uses fewer registers. See the [Rust source](main.rs) and [Python source](main.py).

```sh
cargo run --example register_codegen
cargo test --example register_codegen
python3 examples/register_codegen/main.py
python3 -m unittest examples/register_codegen/main.py
```

## What the example teaches

### Why we need this

CPU registers are small in number (a handful of ultra-fast slots) compared to stack/heap memory. When compiling an expression tree like `a - (b + c)`, evaluating children blindly left-to-right wastes registers:

1. **Naive left-first order:** Load `a` into `r0` (1 register occupied), then evaluate `b + c` in `r1` and `r2` (`b -> r1`, `c -> r2`, `r1 = r1 + r2`), then subtract `r0 = r0 - r1`. That uses **3 registers**—or forces a slow memory spill if the machine only has 2 registers!
2. **Sethi–Ullman (Ershov) order:** Label each subtree with its register need (`1` for `a`, `2` for `b + c`), evaluate the **harder** subtree `b + c` first using `r0` and `r1` (leaving its sum in `r0`), and only then load `a` into `r1` and emit `r0 = r1 - r0`. Total registers needed: **2**.

`Plan::for_expr` labels subtrees with their register needs. Equal needs require one extra register to retain the first result. `generate` evaluates the more demanding subtree first and records the original operand order in the emitted instruction.

### Label the expression tree

A **register** holds a value directly in the target processor or virtual machine. Evaluating an expression tree uses registers for intermediate results. The order of evaluation can change how many results have to be held at once. The **Sethi–Ullman** labeling idea computes the register needs of subtrees before choosing their evaluation order.

A leaf needs one register. For a binary node, suppose its children need `a` and `b` registers.

- If one child needs more, evaluate it first. It leaves one result in a register, and the remaining registers suffice for the smaller child. The total is `max(a, b)`.
- If they need the same number, the first result occupies a register while evaluating the second child. The total is `a + 1`.

These rules assume that both operands are loaded into registers and the target instruction can overwrite an operand register after reading it.

### Generate code for `a - (b + c)`

The left leaf needs one register. The right subtree needs two, so it is evaluated first. The emitter loads `b` into register 0 and `c` into register 1, then adds them into register 0. It loads `a` into register 1 and emits a subtraction with register 1 as the **left** operand and register 0 as the **right** operand. The subtraction writes the result to register 0.

Evaluating the right child first changes instruction order. The generated subtraction still computes `a - (b + c)`. For 30, 4, and 5, the result is 21.

### How to read the code

| Component (`main.rs` / `main.py`) | Role |
| --- | --- |
| `Expr` (`Input`, `Binary`/`BinaryExpr`) | Source expression AST referencing input slots `0, 1, 2`. |
| `Plan` / `Plan::for_expr` (`plan_for`) | Bottom-up pass annotating every subtree with its Sethi–Ullman / Ershov register `need`. |
| `Inst` (`Load`, `Binary`) | Target machine instructions with explicit register indices (`dst`, `left`, `right`). |
| `generate` | Top-down recursive emitter: evaluates the higher-need child first at `base`, the lower-need child at `base + 1`, and preserves `left`/`right` operand order for `Sub`. |
| `run` | Executes the emitted register instructions against `[30, 4, 5]` and returns `regs[0]`. |

## What to look for

For `a - (b + c)`, the right subtree runs first and the code uses two registers. The test confirms both the evaluation order and the subtraction result.

## Check your understanding

- Label the tree for `(a + b) - (c + d)`. Why do the two sums need three registers at the root?
- Trace what would happen if the emitter forgot to swap the operand register numbers after evaluating the right child first.

## Edition mapping and scope

| 1977 Green | 1986 Red | 2006 Purple |
| --- | --- | --- |
| 15 | 9 | 8 |

Numbers refer to chapters. Related readings and appendices are marked where applicable. See the [edition guide](../../README.md#edition-guide) for the bibliographic references and chapter coverage.

The model uses pure expressions and an abstract register machine with enough registers for the chosen tree.
