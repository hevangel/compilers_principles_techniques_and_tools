# Tokens and longest match

Split source text into token kinds, lexemes, and byte offsets. See the [Rust source](main.rs) and [Python source](main.py).

```sh
cargo run --example lexer
cargo test --example lexer
python3 examples/lexer/main.py
python3 -m unittest examples/lexer/main.py
```

## What the example teaches

### Why we need this / What problem this solves

A parser wants structured units (`Let`, `Name("letter")`, `EqualEqual`, `Integer("12")`), not raw characters. A naive character-by-character or whitespace-splitting approach fails in three immediate ways:

1. **Prefix collisions (keywords vs. identifiers):** `letter` starts with `let`. If a scanner emitted `Let` as soon as it saw the prefix `let`, `let letter = 12;` would wrongly become `Let, Let, Name("ter")`.
2. **Multi-character operators without spaces:** In `letter==12;` or `x===y`, there is no whitespace separating identifiers, `==`, and numbers. Splitting on spaces misses them; stopping after the first `=` turns `==` into two `Equal` tokens instead of one `EqualEqual`.
3. **Diagnostic locations:** Skipping whitespace cannot erase byte positions, or syntax errors later in compilation will point at the wrong column/byte (`a = λ` fails at byte 4, not character/token index 2).

The **longest-match (maximal munch)** rule solves (1) and (2): keep extending a token as long as the next byte can still continue a valid lexeme, and only check reserved keywords *after* the entire identifier is read. Keeping `start` as a byte offset into the original source solves (3).

### Follow the scanner's cursor

1. `scan` records the current byte index in `start` and advances `end` past the first byte. That first byte determines which token rule applies.
2. For a name or integer, it advances `end` while the next byte can extend that token (maximal munch). Only after scanning the whole word does it check whether the lexeme is the reserved keyword `let`.
3. For `=`, it peeks one byte ahead. If the next byte is `=`, it consumes both as `EqualEqual`; otherwise it consumes one byte as `Equal`.
4. It stores the slice `start..end` (the **lexeme**) alongside `kind` and `start`, then resumes at `end`. Whitespace advances the cursor without emitting a token, preserving accurate byte offsets.

### Trace `let letter = 12; letter == 12;`

| `start` | `end` | Lexeme | `Kind` | Why this choice? |
| --- | --- | --- | --- | --- |
| `0` | `3` | `"let"` | `Let` | Scanned all ASCII letters (`let`), then matched keyword `"let"` |
| `3` | `4` | `" "` | *(skipped)* | Whitespace advances cursor only |
| `4` | `10` | `"letter"` | `Name` | Maximal munch reads all 6 letters; `"letter" != "let"` |
| `11` | `12` | `"="` | `Equal` | Next byte is `' '`, not `'='` |
| `13` | `15` | `"12"` | `Integer` | Reads consecutive ASCII digits |
| `15` | `16` | `";"` | `Semicolon` | Single-byte punctuation |
| `17` | `23` | `"letter"` | `Name` | Byte offset continues counting skipped spaces |
| `24` | `26` | `"=="` | `EqualEqual` | Lookahead sees second `'='`, consumes 2 bytes |
| `27` | `29` | `"12"` | `Integer` | Consecutive digits |
| `29` | `30` | `";"` | `Semicolon` | Single-byte punctuation |

### How to read the code

| Component | Rust (`main.rs`) | Python (`main.py`) | Role |
| --- | --- | --- | --- |
| Token categories | `enum Kind` | `class Kind(Enum)` | Grammatical tag used by the parser (`Let`, `Name`, `Integer`, `Equal`, `EqualEqual`, `Semicolon`) |
| Token record | `struct Token<'a>` | `class Token` | Pairs `kind`, `lexeme` text slice, and byte offset `start` |
| Error reporting | `Result<..., usize>` | `class LexError` | Reports the exact UTF-8 byte offset of the first invalid byte |
| Scanner loop | `fn scan` | `def scan` | Implements maximal-munch scanning over `source` bytes |

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

The scanner uses in-memory ASCII token rules and stops at the first unsupported character. For sentinel-based double-buffer streaming across chunk boundaries see [Input Buffering and Sentinels](../input_buffering/README.md), for continued tokenization on malformed inputs see [Lexical Error Handling and Recovery](../lexical_recovery/README.md), and for UTF-8/UAX #31 identifiers and Trojan Source checks see the modern extension [Unicode-Aware Source Lexing](../unicode_lexing/README.md).
