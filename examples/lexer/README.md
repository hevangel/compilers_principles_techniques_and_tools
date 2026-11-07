# Tokens and longest match

Split source text into token kinds, lexemes, and byte offsets. See the [Rust source](main.rs).

```sh
cargo run --example lexer
cargo test --example lexer
```

## What the example teaches

`scan` reads a complete identifier before checking the keyword table. That keeps `letter` as one name even though it begins with `let`. For operators, the scanner looks ahead to distinguish `=` from `==`.

## What to look for

The trace distinguishes both forms of the equality operator. The test checks the offset of `==` and verifies that an unsupported character is reported at its starting byte.

## Edition mapping and scope

| 1977 Green | 1986 Red | 2006 Purple |
| --- | --- | --- |
| 3 | 2–3 | 2–3 |

Numbers refer to chapters. Related readings and appendices are marked where applicable. See the [edition guide](../../README.md#edition-guide) for the bibliographic references and chapter coverage.

The scanner uses in-memory ASCII token rules. It stops at the first unsupported character.
