# Panic-mode recovery

Report a malformed statement and resume at the next semicolon. See the [Rust source](main.rs) and [Python source](main.py).

```sh
cargo run --example recovery
cargo test --example recovery
python3 examples/recovery/main.py
python3 -m unittest examples/recovery/main.py
```

## What the example teaches

### Why we need this / What problem this solves

A naive parser stops and exits on the very first syntax error. In real development, forcing a programmer to recompile after fixing one typo at a time is frustrating; a compiler should report *all* independent errors in a single pass without emitting a cascade of fake follow-on errors caused by a corrupted parser state.

**Panic-mode recovery** solves this by designating **synchronization tokens** (here `;`, the statement terminator in `emit N;`). When a statement is malformed, the parser emits **one** diagnostic at the start of the offending construct, discards everything up to the next `;`, and resumes cleanly at the next statement boundary.

### Recover one statement at a time

1. `split_inclusive(';')` divides the source into chunks while keeping the `;` delimiter. Keeping the delimiter lets the checker distinguish a complete statement (`emit 7;`) from an unterminated trailing chunk (`emit 19` with no `;`).
2. For each nonempty chunk (skipping pure whitespace or empty `;` statements), `recover` checks that the chunk ends with `;`, starts with `emit`, and has an ASCII unsigned 32-bit decimal integer (`u32`). Checking `is_ascii_digit` before parsing ensures Python and Rust accept the exact same digit forms and report range overflow cleanly.
3. A valid statement appends its integer to `values`. An invalid one emits one `Diagnostic` with its exact **UTF-8 byte offset** (`offset + leading_space`) and resumes at the next chunk. Notice that multibyte UTF-8 characters (like `é` in the test `"emit x; emit 7; é; emit 19"`) advance `offset` by their UTF-8 byte length (`2` bytes for `é`), keeping subsequent offsets accurate.

### Walk through the sample (`"emit x; emit 7; bad; emit 19;"`)

| Chunk | `offset` | `leading_space` | Diagnostic / Recovered value | Why? |
| --- | --- | --- | --- | --- |
| `"emit x;"` | `0` | `0` | Diagnostic at `0`: `"expected 'emit' and an unsigned integer"` | `"x"` is not ASCII digits; discard through `;` |
| `" emit 7;"` | `7` | `1` | Value `7` | Valid statement; cursor resumes at byte `15` |
| `" bad;"` | `15` | `1` | Diagnostic at `16`: `"expected 'emit' and an unsigned integer"` | `offset (15) + leading_space (1) = 16` |
| `" emit 19;"` | `20` | `1` | Value `19` | Valid statement recovered after two errors |

This example reports one problem per discarded chunk. In a language with strings or nested `{ ... }` blocks, the same punctuation can occur inside a construct, so a full parser chooses synchronization sets based on its current grammatical context (`FOLLOW` or enclosing statement/block delimiters).

### How to read the code

| Component | Rust (`main.rs`) | Python (`main.py`) | Role |
| --- | --- | --- | --- |
| Error record | `struct Diagnostic` | `class Diagnostic` | Pairs UTF-8 byte `offset` with an error `message` |
| Chunk splitter | `str::split_inclusive(';')` | `def split_inclusive_semicolon` | Splits source on `;` while retaining the delimiter to spot missing final `;` |
| Recovery loop | `fn recover` | `def recover` | Validates each chunk, tracks byte offsets, and synchronizes at `;` |

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
