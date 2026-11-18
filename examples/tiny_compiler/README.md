# A tiny compiler and stack machine

Follow declarations and arithmetic from text to executable stack instructions. See the [Rust source](main.rs) and [Python source](main.py).

```sh
cargo run --example tiny_compiler
cargo test --example tiny_compiler
python3 examples/tiny_compiler/main.py
python3 -m unittest examples/tiny_compiler/main.py
```

## What the example teaches

### Why we need this / What problem this solves

A naive approach to running source text—evaluating characters directly from left to right—breaks down immediately on two fundamental language features:

1. **Operator precedence and grouping:** In `boxes * perbox + 3`, you cannot evaluate left-to-right blindly; if you did `4 * 6 + 3` in a naive text evaluator with `3 + 4 * 6`, you would wrongly compute `(3 + 4) * 6 = 42` instead of `3 + (4 * 6) = 27`.
2. **Separation of compilation from execution:** Evaluating source text directly mixes syntax checking, variable name lookup, and arithmetic together. If a variable name is misspelled (`emit missing;`) or a parenthesis is unclosed (`emit (3;`), a compiler should reject the program *before* executing anything, and resolve names once into fast integer slot indices rather than re-looking up strings at runtime.

This example solves both problems with a miniature end-to-end **compiler + stack virtual machine (VM)**. `lex` turns characters into typed tokens; a recursive-descent parser enforces grammar rules and operator precedence while emitting **postfix (stack-machine) bytecode** (syntax-directed translation); a compile-time symbol table maps variable names to numbered slots; and `execute` runs the bytecode on an operand stack.

### Follow one statement through the compiler

1. **Scan the characters.** For `emit boxes * perbox + 3;`, `lex` produces keyword, name, operator, and literal tokens. The parser can reason about `Name("boxes")` as one item rather than individual characters.
2. **Recognize the structure.** `expression` handles addition (`+`), `term` handles multiplication (`*`), and `factor` handles literals, names, and parenthesized expressions. Because `expression` calls `term` for each `+` operand before emitting `Add`, multiplication automatically binds tighter than addition. Parentheses let a `factor` reset precedence by calling `expression` recursively.
3. **Resolve names and emit code.** The two preceding declarations (`let boxes = 4; let perbox = 6;`) assigned `boxes` to slot `0` and `perbox` to slot `1`. The parser translates `emit boxes * perbox + 3;` to `Load(0), Load(1), Mul, Push(3), Add, Emit`. Notice that the operators appear *after* their operands (Reverse Polish / postfix order): recognizing a grammatical construct directly triggers its code emission (**syntax-directed translation**).

### Trace the operand stack

For the default program `let boxes = 4; let perbox = 6; emit boxes * perbox + 3;`, the first two statements store `4` in slot `0` and `6` in slot `1`. Here is the stack trace for the `emit` statement (top of the stack is on the right):

| PC | Instruction | Stack afterward | Slots `[0, 1]` | Effect |
| --- | --- | --- | --- | --- |
| `00` | `Push(4)` | `[4]` | `[0, 0]` | Push initializer for `boxes` |
| `01` | `Store(0)` | `[]` | `[4, 0]` | Pop value into slot `0` |
| `02` | `Push(6)` | `[6]` | `[4, 0]` | Push initializer for `perbox` |
| `03` | `Store(1)` | `[]` | `[4, 6]` | Pop value into slot `1` |
| `04` | `Load(0)` | `[4]` | `[4, 6]` | Push the value of `boxes` |
| `05` | `Load(1)` | `[4, 6]` | `[4, 6]` | Push the value of `perbox` |
| `06` | `Mul` | `[24]` | `[4, 6]` | Pop two operands (`6`, `4`) and push `4 * 6` |
| `07` | `Push(3)` | `[24, 3]` | `[4, 6]` | Push integer literal `3` |
| `08` | `Add` | `[27]` | `[4, 6]` | Pop two operands (`3`, `24`) and push `24 + 3` |
| `09` | `Emit` | `[]` | `[4, 6]` | Pop `27` and append it to the output |

Invariant: every well-formed expression leaves **net +1** value on the stack, and every statement consumes its expression's value, so the stack is empty between statements.

### How to read the code

| Component | Rust (`main.rs`) | Python (`main.py`) | Role |
| --- | --- | --- | --- |
| Token representation | `enum Token<'a>` | `class Token` | Lexical categories (`Let`, `Emit`, `Name`, `Number`, punctuation) |
| Scanner (Lexer) | `fn lex` | `def lex` | Converts raw UTF-8/ASCII bytes into `Token` sequence with overflow checks |
| Bytecode & Program | `enum Inst`, `struct Program` | `class Inst`, `class Program` | Stack VM instructions (`Push`, `Load`, `Store`, `Add`, `Mul`, `Emit`) and slot count |
| Parser & Codegen | `struct Compiler` (`statement`, `expression`, `term`, `factor`) | `class Compiler` | Single-pass recursive-descent parser, symbol table (`slots`), and postfix code emission |
| Driver | `fn compile` | `def compile_source` | Orchestrates lexing and statement-by-statement compilation |
| Stack VM | `fn execute` | `def execute` | Interprets `Program` bytecode using an operand stack and wrapping 64-bit integer math |

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
