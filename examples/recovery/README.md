# Panic-mode recovery

Report a malformed statement and resume at the next semicolon. See the [Rust source](main.rs).

```sh
cargo run --example recovery
cargo test --example recovery
```

## What the example teaches

`recover` treats the statement delimiter as a synchronization point. After a malformed statement, it discards the rest of that statement and continues scanning the following one. Diagnostics carry starting byte offsets.

A compiler can find more than one error in a file if it knows how to resume after a bad construct. **Panic-mode recovery** discards input up to a **synchronization point**. A synchronization point is a token where the parser can reliably start again. For this flat language, every nonempty statement has the shape `emit N;`, so a semicolon marks a statement boundary.

### Recover one statement at a time

1. `split_inclusive(';')` divides the source into chunks while keeping the delimiter. Keeping it lets the checker distinguish a complete statement from an unterminated final chunk.
2. For each nonempty chunk, `recover` checks for exactly the keyword `emit` and a string of decimal digits. Parsing the digits as `u32` separately catches numbers outside the range.
3. A valid statement contributes its value. An invalid one contributes a diagnostic and the next iteration resumes at the next chunk. The function keeps a running byte offset and adds the chunk's leading whitespace to locate the first significant byte.

### Walk through the sample

| Statement | Result |
| --- | --- |
| `emit x;` | Report a malformed statement at byte 0 |
| `emit 7;` | Keep 7 |
| `bad;` | Report a malformed statement at byte 16 |
| `emit 19;` | Keep 19 |

This example reports one problem per discarded chunk. In a language with strings or nested constructs, the same punctuation can occur inside a construct. A parser would choose synchronization tokens based on its current grammatical context.

## What to look for

The sample recovers values `[7, 19]` from a mix of valid and invalid statements. The test also checks byte offsets after a multibyte character and a missing final semicolon.

## Check your understanding

- Try `emit 4294967296; emit 3;` in the sample. Predict the diagnostic and the recovered values.
- Remove the final semicolon from a valid statement. Why does the checker report a missing delimiter even though the keyword and integer are recognizable?

## Edition mapping and scope

| 1977 Green | 1986 Red | 2006 Purple |
| --- | --- | --- |
| 11 | 4 | 4 |

Numbers refer to chapters. Related readings and appendices are marked where applicable. See the [edition guide](../../README.md#edition-guide) for the bibliographic references and chapter coverage.

The language is a sequence of flat `emit N;` statements. Semicolons are always safe synchronization points in this grammar.
