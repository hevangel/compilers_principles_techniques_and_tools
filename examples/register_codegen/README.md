# Register needs for expression trees

Choose an evaluation order that uses fewer registers. See the [Rust source](main.rs).

```sh
cargo run --example register_codegen
cargo test --example register_codegen
```

## What the example teaches

`Plan::for_expr` labels subtrees with their register needs. Equal needs require one extra register to retain the first result. `generate` evaluates the more demanding subtree first and records the original operand order in the emitted instruction.

A **register** holds a value directly in the target processor or virtual machine. Evaluating an expression tree uses registers for intermediate results. The order of evaluation can change how many results have to be held at once. The **Sethi–Ullman** labeling idea computes the register needs of subtrees before choosing their evaluation order.

### Label the expression tree

A leaf needs one register. For a binary node, suppose its children need `a` and `b` registers.

- If one child needs more, evaluate it first. It leaves one result in a register, and the remaining registers suffice for the smaller child. The total is `max(a, b)`.
- If they need the same number, the first result occupies a register while evaluating the second child. The total is `a + 1`.

These rules assume that both operands are loaded into registers and the target instruction can overwrite an operand register after reading it.

### Generate code for `a - (b + c)`

The left leaf needs one register. The right subtree needs two, so it is evaluated first. The emitter loads `b` into register 0 and `c` into register 1, then adds them into register 0. It loads `a` into register 1 and emits a subtraction with register 1 as the **left** operand and register 0 as the **right** operand. The subtraction writes the result to register 0.

Evaluating the right child first changes instruction order. The generated subtraction still computes `a - (b + c)`. For 30, 4, and 5, the result is 21.

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
