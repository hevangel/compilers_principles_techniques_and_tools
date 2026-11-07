# Tokens and longest match

Split source text into token kinds, lexemes, and byte offsets. See the [Rust source](main.rs).

```sh
cargo run --example lexer
cargo test --example lexer
```

## What the example teaches

`scan` reads a complete identifier before checking the keyword table. That keeps `letter` as one name even though it begins with `let`. For operators, the scanner looks ahead to distinguish `=` from `==`.

A scanner, or **lexer**, reads characters and returns **tokens**. The token kind is a category such as `Name`. The **lexeme** is the particular source text in that category, such as `letter`. The byte offset tells the caller where the token begins. A parser can use the kind for grammar decisions and keep the text and offset for later use.

### Follow the scanner's cursor

1. `scan` records the current byte index in `start` and advances `end` past the first byte. The first byte tells it which token rules could apply.
2. For a name or integer, it advances `end` as long as the next byte can extend that token. This is the **longest-match** rule. After scanning an entire name, it checks whether the spelling is the reserved word `let`.
3. For `=`, it inspects one additional byte. If that byte is another equals sign, it consumes both bytes as one `EqualEqual` token. Otherwise it consumes only the first.
4. It stores the source slice `start..end` in the token and resumes at `end`. Whitespace advances the cursor without producing a token.

### Read the output

For `let letter = 12; letter == 12;`, the first tokens begin at offsets 0, 4, 11, 13, and 15. The first `let` is a keyword. At offset 4, the scanner consumes all six letters of `letter` and classifies it as a name. The later `==` starts at offset 24 and occupies two bytes. Token offsets still include the whitespace in the source.

## What to look for

The trace distinguishes both forms of the equality operator. The test checks the offset of `==` and verifies that an unsupported character is reported at its starting byte.

## Check your understanding

- Predict the kinds and offsets for `let let2==007;` and add an assertion to the existing test. The scanner preserves the lexeme `007`, including its leading zeros.
- Try `x===y`. Explain why the scanner emits `EqualEqual` followed by `Equal` even if a parser would reject the sequence.

## Edition mapping and scope

| 1977 Green | 1986 Red | 2006 Purple |
| --- | --- | --- |
| 3 | 2–3 | 2–3 |

Numbers refer to chapters. Related readings and appendices are marked where applicable. See the [edition guide](../../README.md#edition-guide) for the bibliographic references and chapter coverage.

The scanner uses in-memory ASCII token rules. It stops at the first unsupported character.
