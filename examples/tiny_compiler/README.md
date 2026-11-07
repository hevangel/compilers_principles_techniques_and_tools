# A tiny compiler and stack machine

Follow declarations and arithmetic from text to executable stack instructions. See the [Rust source](main.rs).

```sh
cargo run --example tiny_compiler
cargo test --example tiny_compiler
```

## What the example teaches

`lex` breaks the input into tokens. The recursive-descent parser groups multiplication inside addition and emits stack instructions after recognizing their operands. The symbol table maps declared names to numbered slots, and the interpreter executes the resulting bytecode.

## What to look for

The default program emits `[27]`. Try `cargo run --example tiny_compiler -- 'let n = 6; emit n + 2 * 3; emit (n + 2) * 3;'` for `[12, 24]`. The tests also check invalid characters, unknown and duplicate names, unmatched parentheses, nesting limits, and integer overflow behavior.

## Edition mapping and scope

| 1977 Green | 1986 Red | 2006 Purple |
| --- | --- | --- |
| 1, 2, 5, 7 | 1–2; app. A (project theme) | 1–2; app. A (front-end theme) |

Numbers refer to chapters. Related readings and appendices are marked where applicable. See the [edition guide](../../README.md#edition-guide) for the bibliographic references and chapter coverage.

The parser makes one pass over a pre-tokenized stream. The language has a single scope, integer-only expressions, and wrapping 64-bit arithmetic. Bytecode is the target.
