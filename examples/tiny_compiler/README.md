# A tiny compiler and stack machine

Follow declarations and arithmetic from text to executable stack instructions. See the [Rust source](main.rs).

```sh
cargo run --example tiny_compiler
cargo test --example tiny_compiler
```

## What the example teaches

`lex` breaks the input into tokens. The recursive-descent parser groups multiplication inside addition and emits stack instructions after recognizing their operands. The symbol table maps declared names to numbered slots, and the interpreter executes the resulting bytecode.

A compiler translates a **source language** into a **target language**. Here the source is a tiny language with `let` and `emit`, and the target is bytecode for a virtual machine. The compiler and the virtual machine have separate jobs. The compiler decides which instructions are needed. The virtual machine runs those instructions on actual values.

### Follow one statement through the compiler

1. **Scan the characters.** For `emit boxes * perbox + 3;`, `lex` produces keyword, name, operator, and literal tokens. The parser can reason about `Name("boxes")` as one item.
2. **Recognize the structure.** `expression` handles addition, `term` handles multiplication, and `factor` handles literals, names, and parenthesized expressions. Because `expression` reads a whole term before emitting addition, multiplication binds more tightly. Parentheses let a factor contain a complete expression.
3. **Resolve names and emit code.** The two preceding declarations assigned `boxes` and `perbox` to slots 0 and 1. The parser translates the statement to `Load(0), Load(1), Mul, Push(3), Add, Emit`. This is **syntax-directed translation** because recognizing a grammatical construct triggers its code emission.

### Trace the operand stack

The top of the stack is on the right. The slot array already contains 4 and 6 from the declarations.

| Instruction | Stack afterward | Effect |
| --- | --- | --- |
| `Load(0)` | `[4]` | Push the value of `boxes` |
| `Load(1)` | `[4, 6]` | Push the value of `perbox` |
| `Mul` | `[24]` | Pop two operands and push their product |
| `Push(3)` | `[24, 3]` | Push an integer literal |
| `Add` | `[27]` | Pop two operands and push their sum |
| `Emit` | `[]` | Pop 27 and append it to the output |

A well-formed expression leaves exactly one value on the stack. Each statement consumes its expression's value, so the stack is empty between statements.

## What to look for

The default program emits `[27]`. Try `cargo run --example tiny_compiler -- 'let n = 6; emit n + 2 * 3; emit (n + 2) * 3;'` for `[12, 24]`. The tests also check invalid characters, unknown and duplicate names, unmatched parentheses, nesting limits, and integer overflow behavior.

## Check your understanding

- Predict the bytecode for `emit (2 + 3) * 4;`, then run it as the program argument. The addition should appear before the multiplication.
- Compare `emit 1 + ;` with `emit missing;`. Which fails during parsing, and which parses as an expression but fails during name resolution?

## Edition mapping and scope

| 1977 Green | 1986 Red | 2006 Purple |
| --- | --- | --- |
| 1, 2, 5, 7 | 1–2; app. A (project theme) | 1–2; app. A (front-end theme) |

Numbers refer to chapters. Related readings and appendices are marked where applicable. See the [edition guide](../../README.md#edition-guide) for the bibliographic references and chapter coverage.

The parser makes one pass over a pre-tokenized stream. The language has a single scope, integer-only expressions, and wrapping 64-bit arithmetic. Bytecode is the target.
