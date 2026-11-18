# Unicode-Aware Source Lexing *(Modern Extension)*

Scan UTF-8 source files with independent tracking of **byte offsets**, **Unicode scalar indices**, and **visual display columns**, classify international identifiers (`café`, `αβγ`, `変数`) using UAX #31-style rules, and emit security diagnostics for Trojan Source bidi overrides (`U+202E`), invisible zero-width characters, and mixed-script homoglyph identifiers. See the [Rust source](main.rs) and [Python source](main.py).

> **Edition note:** UTF-8/Unicode identifiers (UAX #31) and Trojan Source / homoglyph security checks are modern compiler front-end practices placed under **Advanced parsing techniques** rather than classical ASCII-era Dragon Book chapters.

```sh
cargo run --example unicode_lexing
cargo test --example unicode_lexing
python3 examples/unicode_lexing/main.py
python3 -m unittest examples/unicode_lexing/main.py
```

## What the example teaches

### Three distinct source coordinates

In ASCII-only scanners (`byte == char == column`), one integer serves all roles. In modern UTF-8 compilers, three independent coordinates diverge:

| Coordinate | Example on `"let café = 変数 + αβγ;"` | Purpose |
| --- | --- | --- |
| **Byte offset** | `café`: `4..9` (`5` bytes); `変数`: `12..18` (`6` bytes) | Fast slice indexing and incremental CST widths |
| **Scalar index** | `café`: `4..8` (`4` scalars); `変数`: `11..13` (`2` scalars) | Unicode code-point indexing (UTF-32 / Python `str`) |
| **Display column** | `変数` at col `12`, `+` at col `17` (East Asian width `2` per CJK char) | Caret alignment in diagnostics and terminal reports |

### Security diagnostics in modern lexers

1. **Bidirectional overrides (`BidiControl`):** Code points such as `U+202E` (RLO) or `U+2066..U+2069` can visually reorder code inside editors ("Trojan Source"). Flagged inside comments, strings, and token gaps.
2. **Invisible zero-width code points (`ZeroWidthChar`):** `U+200B` / `U+FEFF` inside or adjacent to identifiers creates visually identical yet distinct symbols.
3. **Mixed-script confusables (`MixedScriptConfusable`):** An identifier mixing Latin `p` with Cyrillic `а` (`U+0430`) emits a homoglyph warning, whereas clean single-script identifiers (`café`, `αβγ`, `変数`) pass with zero warnings.

### How to read the code

| Component | Rust (`main.rs`) | Python (`main.py`) | Role |
| --- | --- | --- | --- |
| Unicode classification | `fn is_xid_start`, `is_xid_continue`, `letter_script` | `def is_xid_start`, `is_xid_continue`, `letter_script` | Deterministic identifier & script classification across languages |
| Coordinate scanner | `struct Scanner`, `UnicodeSpan` | `class Scanner`, `UnicodeSpan` | Tracks byte offsets, scalar indices, and display widths simultaneously |
| Security checks | `enum DiagKind`, `LexDiagnostic` | `LexDiagnostic` | Flags Trojan Source bidi controls, zero-width code points, and homoglyphs |

## What to look for

Clean international identifiers (`café`, `αβγ`, `変数`) and Unicode ideographic space (`U+3000`) tokenize cleanly with zero warnings, while `"let v\u0430r = 1; // \u202Elocked"` triggers both `MixedScriptConfusable` and `BidiControl` with exact line, column, byte, and scalar offsets.

## Check your understanding

- Why does slicing source buffers for error carets require byte offsets while terminal `^` padding needs display width (`display_width`)?
- Why should a lexer allow `αβγ` (Greek-only) quietly while warning on `pаypal` (Latin + Cyrillic `а`)?

## Edition mapping and scope

| 1977 Green | 1986 Red | 2006 Purple |
| --- | --- | --- |
| — *(modern extension; cf. Ch. 3)* | — *(modern extension; cf. Ch. 3)* | — *(modern extension; cf. Ch. 3)* |
