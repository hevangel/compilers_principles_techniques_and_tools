"""Chapter 3 (§3.1.4, §3.4): Lexical Error Handling and Panic-Mode Recovery.

Demonstrates:
1. Invalid stray characters (`@`, `?`) recovered via single-character panic deletion,
2. Unterminated string literals recovered at unescaped newlines so subsequent lines stay valid,
3. Unterminated block comments (`/* ...`) diagnosed at their opening `/*` span to EOF,
4. Malformed numeric literals (`0x`, `12e+`, `99bad`) consumed intact as recovered tokens,
5. Clear separation between lexical errors and downstream parser syntax errors (`let = 5;`).
"""

from __future__ import annotations

from dataclasses import dataclass
import unittest


@dataclass(frozen=True)
class Span:
    start: int
    end: int
    line: int
    col: int


@dataclass(frozen=True)
class Token:
    kind: str  # LET | IDENT | INT | FLOAT | STR | EQ | PLUS | SEMI | LEX_ERR
    lexeme: str
    span: Span


@dataclass(frozen=True)
class LexDiagnostic:
    span: Span
    strategy: str
    message: str


def format_diagnostic(src: str, diag: LexDiagnostic) -> str:
    lines = src.splitlines()
    line_str = lines[diag.span.line - 1] if 0 <= diag.span.line - 1 < len(lines) else ""
    pad = " " * max(0, diag.span.col - 1)
    avail = max(1, len(line_str) - max(0, diag.span.col - 1))
    width = min(avail, max(1, diag.span.end - diag.span.start))
    carets = "^" * width
    return (
        f"lex error at line {diag.span.line}, col {diag.span.col} ({diag.strategy}): {diag.message}\n"
        f"  | {line_str}\n"
        f"  | {pad}{carets}"
    )


def _is_hex(ch: str) -> bool:
    return ("0" <= ch <= "9") or ("a" <= ch <= "f") or ("A" <= ch <= "F")


def _is_alpha_or_us(ch: str) -> bool:
    return ("a" <= ch <= "z") or ("A" <= ch <= "Z") or ch == "_"


def _is_alnum_or_us(ch: str) -> bool:
    return _is_alpha_or_us(ch) or ("0" <= ch <= "9")


def scan_with_recovery(src: str) -> tuple[list[Token], list[LexDiagnostic]]:
    n = len(src)
    i = 0
    line = 1
    line_start = 0
    tokens: list[Token] = []
    diags: list[LexDiagnostic] = []

    while i < n:
        ch = src[i]
        if ch == "\n":
            i += 1
            line += 1
            line_start = i
            continue
        if ch.isspace():
            i += 1
            continue

        # Line comment: // ...
        if ch == "/" and i + 1 < n and src[i + 1] == "/":
            i += 2
            while i < n and src[i] != "\n":
                i += 1
            continue

        # Block comment: /* ... */ with EOF recovery pinned to opening `/*`
        if ch == "/" and i + 1 < n and src[i + 1] == "*":
            start = i
            open_line = line
            open_col = start - line_start + 1
            i += 2
            closed = False
            while i < n:
                if src[i] == "\n":
                    i += 1
                    line += 1
                    line_start = i
                elif src[i] == "*" and i + 1 < n and src[i + 1] == "/":
                    i += 2
                    closed = True
                    break
                else:
                    i += 1
            if not closed:
                diags.append(
                    LexDiagnostic(
                        span=Span(start, start + 2, open_line, open_col),
                        strategy="TerminateCommentAtEof",
                        message="unterminated block comment; reached end of file",
                    )
                )
            continue

        start = i
        col = start - line_start + 1

        # String literal with escape handling + newline synchronization
        if ch == '"':
            i += 1
            closed = False
            while i < n:
                if src[i] == "\n":
                    break
                if src[i] == "\\" and i + 1 < n and src[i + 1] != "\n":
                    i += 2
                    continue
                if src[i] == '"':
                    i += 1
                    closed = True
                    break
                i += 1
            span = Span(start, i, line, col)
            if closed:
                tokens.append(Token("STR", src[start:i], span))
            else:
                diags.append(
                    LexDiagnostic(
                        span=span,
                        strategy="TerminateStringAtNewline",
                        message="unterminated string literal; synchronized at end of line",
                    )
                )
                tokens.append(Token("STR", src[start:i], span))
            continue

        # Identifiers / keywords
        if _is_alpha_or_us(ch):
            i += 1
            while i < n and _is_alnum_or_us(src[i]):
                i += 1
            lexeme = src[start:i]
            kind = "LET" if lexeme == "let" else "IDENT"
            tokens.append(Token(kind, lexeme, Span(start, i, line, col)))
            continue

        # Numbers with malformed recovery
        if "0" <= ch <= "9":
            kind = "INT"
            err_msg: str | None = None

            if ch == "0" and i + 1 < n and src[i + 1] in ("x", "X"):
                i += 2
                hex_start = i
                while i < n and _is_hex(src[i]):
                    i += 1
                if i == hex_start:
                    err_msg = "hex literal '0x' requires at least one hex digit"
                if i < n and _is_alnum_or_us(src[i]):
                    while i < n and _is_alnum_or_us(src[i]):
                        i += 1
                    err_msg = f"malformed hex literal '{src[start:i]}'"
            else:
                while i < n and "0" <= src[i] <= "9":
                    i += 1
                if i + 1 < n and src[i] == "." and ("0" <= src[i + 1] <= "9"):
                    kind = "FLOAT"
                    i += 1
                    while i < n and "0" <= src[i] <= "9":
                        i += 1
                if i < n and src[i] in ("e", "E"):
                    kind = "FLOAT"
                    i += 1
                    if i < n and src[i] in ("+", "-"):
                        i += 1
                    exp_start = i
                    while i < n and "0" <= src[i] <= "9":
                        i += 1
                    if i == exp_start:
                        err_msg = f"malformed exponent in numeric literal '{src[start:i]}'"
                if i < n and _is_alpha_or_us(src[i]):
                    while i < n and _is_alnum_or_us(src[i]):
                        i += 1
                    err_msg = f"invalid suffix in numeric literal '{src[start:i]}'"

            span = Span(start, i, line, col)
            if err_msg is not None:
                diags.append(
                    LexDiagnostic(
                        span=span,
                        strategy="ConsumeMalformedNumber",
                        message=err_msg,
                    )
                )
                tokens.append(Token("LEX_ERR", src[start:i], span))
            else:
                tokens.append(Token(kind, src[start:i], span))
            continue

        single_map = {"=": "EQ", "+": "PLUS", ";": "SEMI"}
        if ch in single_map:
            i += 1
            tokens.append(Token(single_map[ch], ch, Span(start, i, line, col)))
            continue

        i += 1
        diags.append(
            LexDiagnostic(
                span=Span(start, i, line, col),
                strategy="DeleteChar",
                message=f"unexpected character '{ch}'; skipped",
            )
        )

    return tokens, diags


def has_valid_let_syntax(tokens: list[Token]) -> str | None:
    if len(tokens) < 5:
        return "incomplete statement"
    if tokens[0].kind != "LET":
        return "expected 'let'"
    if tokens[1].kind != "IDENT":
        t = tokens[1]
        return f"syntax error at L{t.span.line}:C{t.span.col}: expected identifier after 'let', found {t.kind}"
    return None


def main() -> None:
    src = (
        "let x = @ 12e+ + 0x;\n"
        'let msg = "unterminated string\n'
        "let ok = 42 + 0x1a;\n"
        "/* unclosed block comment"
    )
    tokens, diags = scan_with_recovery(src)
    print("=== Lexical Error Handling and Panic-Mode Recovery (Ch. 3) ===")
    print(f"Recovered {len(tokens)} tokens across 4 lines despite {len(diags)} lexical errors:\n")
    for tok in tokens:
        print(f"  L{tok.span.line}:C{tok.span.col:<2} {tok.kind:<7} {tok.lexeme!r}")
    print("\nDiagnostics:")
    for d in diags:
        print(format_diagnostic(src, d))

    syntax_only = "let = 5;"
    syn_tokens, syn_lex_diags = scan_with_recovery(syntax_only)
    print(f"\nLexing vs. Parsing boundary check on {syntax_only!r}:")
    print(f"  Lexical diagnostics: {len(syn_lex_diags)}")
    print(f"  Parser check: {has_valid_let_syntax(syn_tokens)}")


class LexicalRecoveryTests(unittest.TestCase):
    def test_recovers_multiple_lexical_errors_without_halting(self) -> None:
        src = 'let a = @ 0x;\nlet s = "abc\nlet b = 99xyz + 7;\n/* open'
        tokens, diags = scan_with_recovery(src)
        self.assertEqual(len(diags), 5)
        self.assertEqual(
            [d.strategy for d in diags],
            [
                "DeleteChar",
                "ConsumeMalformedNumber",
                "TerminateStringAtNewline",
                "ConsumeMalformedNumber",
                "TerminateCommentAtEof",
            ],
        )
        self.assertTrue(any(t.kind == "IDENT" and t.lexeme == "b" for t in tokens))
        self.assertTrue(any(t.kind == "INT" and t.lexeme == "7" for t in tokens))

    def test_unterminated_string_does_not_swallow_following_line(self) -> None:
        src = 'let a = "oops\nlet b = 10;'
        tokens, diags = scan_with_recovery(src)
        self.assertEqual(len(diags), 1)
        self.assertEqual(diags[0].span.line, 1)
        line2 = [t.lexeme for t in tokens if t.span.line == 2]
        self.assertEqual(line2, ["let", "b", "=", "10", ";"])

    def test_syntax_error_produces_zero_lexical_diagnostics(self) -> None:
        tokens, diags = scan_with_recovery("let = 5;")
        self.assertEqual(diags, [])
        self.assertEqual(len(tokens), 4)
        self.assertIsNotNone(has_valid_let_syntax(tokens))


if __name__ == "__main__":
    main()
