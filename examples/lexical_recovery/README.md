# Lexical Error Handling and Panic-Mode Recovery (Ch. 3)

Distinguish lexical errors from syntax errors, diagnose illegal characters, unterminated strings, unclosed block comments, and malformed numeric literals with exact spans, and recover without halting tokenization. See the [Rust source](main.rs) and [Python source](main.py).

```sh
cargo run --example lexical_recovery
cargo test --example lexical_recovery
python3 examples/lexical_recovery/main.py
python3 -m unittest examples/lexical_recovery/main.py
```

## What the example teaches

### Why we need this / What problem this solves

A naive scanner either panics on the first unexpected byte `@` or lets an unclosed `"hello` swallow the rest of the source file until EOF, turning a one-line typo into hundreds of phantom parser errors. Meanwhile, beginners often confuse **lexical errors** (the scanner cannot form any valid token from the next characters) with **syntax errors** (valid tokens arranged in an invalid grammar order, like `let = 5;`).

### Four lexical recovery strategies

| Failure mode | Example | Recovery strategy | Why it avoids cascading errors |
| --- | --- | --- | --- |
| Stray unknown character | `@` | `DeleteChar` (panic skip) | Reports the exact byte span and resumes on the next character |
| Unterminated string literal | `"oops\n` | `TerminateStringAtNewline` | Synchronizes at unescaped `\n` so the next line's `let b = 10;` lexes cleanly |
| Unclosed block comment | `/* ...` | `TerminateCommentAtEof` | Reports the opening `/*` span instead of pointing at a blank EOF line |
| Malformed number | `12e+`, `0x`, `99xyz` | `ConsumeMalformedNumber` | Consumes the whole malformed literal as `LEX_ERR` so `x`/`xyz` doesn't spawn fake `IDENT` errors |

### Lexing vs. parsing boundary

On `"let = 5;"`, every individual token (`LET`, `EQ`, `INT`, `SEMI`) matches the lexical grammar cleanly: `scan_with_recovery` emits **0** lexical diagnostics, leaving `has_valid_let_syntax` (the parser) to report the missing identifier.

### How to read the code

| Component | Rust (`main.rs`) | Python (`main.py`) | Role |
| --- | --- | --- | --- |
| Token & error types | `TokKind`, `Token`, `RecoveryStrategy`, `LexDiagnostic` | `Token`, `LexDiagnostic` | Pairs recovered token streams with exact `(line, col, start..end)` diagnostics |
| Scanner with recovery | `fn scan_with_recovery` | `def scan_with_recovery` | Implements single-pass scanning with escape-aware string/comment/number recovery |
| Diagnostic formatter | `fn format_diagnostic` | `def format_diagnostic` | Renders caret underlines against offending source lines |

## What to look for

The sample program packs an illegal `@`, a broken exponent `12e+`, an empty hex prefix `0x`, an unterminated string on line 2, and an unclosed `/*` comment at EOF—yet line 3 (`let ok = 42 + 0x1a;`) tokenizes cleanly with zero false errors.

## Check your understanding

- Why does `scan_with_recovery` consume `99xyz` as one `LEX_ERR` token instead of emitting `INT("99")` followed by `IDENT("xyz")`?
- What happens in C-style languages if a multi-line raw string intentionally allows `\n`, and why must the scanner know?

## Edition mapping and scope

| 1977 Green | 1986 Red | 2006 Purple |
| --- | --- | --- |
| 3, 11 | 3 (§3.1.4) | 3 (§3.1.4) |

Numbers refer to chapters. Compare with [Panic-mode recovery](../recovery/README.md) and [LL(1) & LR Parser Error Recovery](../parser_error_recovery/README.md) for grammar-level recovery.
