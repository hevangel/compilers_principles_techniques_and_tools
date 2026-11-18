"""Modern Extension: Unicode-Aware Source Lexing, Display Columns & Security Diagnostics.

Explicitly labeled as a modern compiler front-end extension complementing
Dragon Book Ch. 3:
1. Triple coordinate tracking: UTF-8 byte offsets (`byte_start..byte_end`), Unicode
   scalar indices (`scalar_start..scalar_end`), and visual `line:col` coordinates
   with Unicode whitespace handling (e.g., U+00A0, U+3000),
2. Deterministic UAX #31 style `XID_Start` / `XID_Continue` identifier scanning
   for ASCII, accented Latin (`café`), Greek (`αβγ`), and CJK (`変数`) identifiers,
3. Security diagnostics detecting bidirectional overrides (Trojan Source,
   U+202A..U+202E / U+2066..U+2069), invisible zero-width characters (U+200B),
   and mixed-script homoglyph/confusable identifiers (Latin + Cyrillic `а`).
"""

from __future__ import annotations

from dataclasses import dataclass
import unittest

BIDI_CONTROLS = {0x200E, 0x200F, *range(0x202A, 0x202F), *range(0x2066, 0x206A)}
ZERO_WIDTH = {0x200B, 0x200C, 0x200D, 0xFEFF}
UNICODE_WS = {" ", "\t", "\r", "\n", "\u00A0", "\u3000"}


@dataclass(frozen=True)
class UnicodeSpan:
    byte_start: int
    byte_end: int
    scalar_start: int
    scalar_end: int
    line: int
    col: int


@dataclass(frozen=True)
class Token:
    kind: str
    lexeme: str
    span: UnicodeSpan


@dataclass(frozen=True)
class LexDiagnostic:
    kind: str
    byte_offset: int
    scalar_offset: int
    line: int
    col: int
    message: str


def letter_script(ch: str) -> str | None:
    cp = ord(ch)
    if (0x0041 <= cp <= 0x005A) or (0x0061 <= cp <= 0x007A):
        return "Latin"
    if (0x00C0 <= cp <= 0x00D6) or (0x00D8 <= cp <= 0x00F6) or (0x00F8 <= cp <= 0x024F):
        return "Latin"
    if 0x0370 <= cp <= 0x03FF and ch.isalpha():
        return "Greek"
    if 0x0400 <= cp <= 0x04FF and ch.isalpha():
        return "Cyrillic"
    if (0x3041 <= cp <= 0x3096) or (0x30A1 <= cp <= 0x30FA):
        return "Kana"
    if 0x4E00 <= cp <= 0x9FFF:
        return "Han"
    return None


def is_xid_start(ch: str) -> bool:
    return ch == "_" or letter_script(ch) is not None


def is_combining_mark(ch: str) -> bool:
    return 0x0300 <= ord(ch) <= 0x036F


def is_xid_continue(ch: str) -> bool:
    return is_xid_start(ch) or ("0" <= ch <= "9") or is_combining_mark(ch)


def display_width(ch: str) -> int:
    cp = ord(ch)
    if is_combining_mark(ch) or cp in ZERO_WIDTH or cp in BIDI_CONTROLS:
        return 0
    if (0x3000 <= cp <= 0x30FF) or (0x4E00 <= cp <= 0x9FFF):
        return 2
    return 1


class Scanner:
    def __init__(self, src: str) -> None:
        self.src = src
        self.byte_pos = 0
        self.scalar_pos = 0
        self.line = 1
        self.col = 1

    def peek(self) -> str | None:
        return self.src[self.scalar_pos] if self.scalar_pos < len(self.src) else None

    def advance(self) -> str | None:
        ch = self.peek()
        if ch is None:
            return None
        self.byte_pos += len(ch.encode("utf-8"))
        self.scalar_pos += 1
        if ch == "\n":
            self.line += 1
            self.col = 1
        elif ch == "\t":
            tab_stop = 4
            self.col += tab_stop - ((self.col - 1) % tab_stop)
        else:
            self.col += display_width(ch)
        return ch

    def lex(self) -> tuple[list[Token], list[LexDiagnostic]]:
        tokens: list[Token] = []
        diags: list[LexDiagnostic] = []

        while (ch := self.peek()) is not None:
            if ch in UNICODE_WS:
                self.advance()
                continue

            b_start = self.byte_pos
            s_start = self.scalar_pos
            line = self.line
            col = self.col
            cp = ord(ch)

            if cp in BIDI_CONTROLS:
                self.advance()
                diags.append(
                    LexDiagnostic(
                        "BidiControl",
                        b_start,
                        s_start,
                        line,
                        col,
                        f"Trojan Source bidi control U+{cp:04X} rejected outside literals",
                    )
                )
                continue

            if cp in ZERO_WIDTH:
                self.advance()
                diags.append(
                    LexDiagnostic(
                        "ZeroWidthChar",
                        b_start,
                        s_start,
                        line,
                        col,
                        f"invisible zero-width code point U+{cp:04X}",
                    )
                )
                continue

            if self.src.startswith("//", self.scalar_pos):
                while (c := self.peek()) is not None and c != "\n":
                    if ord(c) in BIDI_CONTROLS:
                        diags.append(
                            LexDiagnostic(
                                "BidiControl",
                                self.byte_pos,
                                self.scalar_pos,
                                self.line,
                                self.col,
                                f"Trojan Source bidi control U+{ord(c):04X} inside comment",
                            )
                        )
                    self.advance()
                continue

            if is_xid_start(ch):
                scripts: set[str] = set()
                while (c := self.peek()) is not None:
                    if ord(c) in ZERO_WIDTH:
                        diags.append(
                            LexDiagnostic(
                                "ZeroWidthChar",
                                self.byte_pos,
                                self.scalar_pos,
                                self.line,
                                self.col,
                                f"invisible zero-width character U+{ord(c):04X} inside identifier",
                            )
                        )
                        self.advance()
                        break
                    if not is_xid_continue(c):
                        break
                    sc = letter_script(c)
                    if sc is not None:
                        scripts.add(sc)
                    self.advance()
                lexeme = self.src[s_start : self.scalar_pos]
                if len(scripts) > 1:
                    diags.append(
                        LexDiagnostic(
                            "MixedScriptConfusable",
                            b_start,
                            s_start,
                            line,
                            col,
                            f"mixed-script identifier '{lexeme}' mixes [{', '.join(sorted(scripts))}] (possible homoglyph)",
                        )
                    )
                kind = "KwLet" if lexeme == "let" else "Ident"
                tokens.append(
                    Token(
                        kind,
                        lexeme,
                        UnicodeSpan(b_start, self.byte_pos, s_start, self.scalar_pos, line, col),
                    )
                )
                continue

            if "0" <= ch <= "9":
                while (c := self.peek()) is not None and "0" <= c <= "9":
                    self.advance()
                tokens.append(
                    Token(
                        "Int",
                        self.src[s_start : self.scalar_pos],
                        UnicodeSpan(b_start, self.byte_pos, s_start, self.scalar_pos, line, col),
                    )
                )
                continue

            if ch == '"':
                self.advance()
                closed = False
                while (c := self.peek()) is not None:
                    if ord(c) in BIDI_CONTROLS:
                        diags.append(
                            LexDiagnostic(
                                "BidiControl",
                                self.byte_pos,
                                self.scalar_pos,
                                self.line,
                                self.col,
                                f"Trojan Source bidi control U+{ord(c):04X} inside string literal",
                            )
                        )
                    if c == "\n":
                        break
                    self.advance()
                    if c == '"':
                        closed = True
                        break
                if not closed:
                    diags.append(
                        LexDiagnostic(
                            "UnterminatedString",
                            b_start,
                            s_start,
                            line,
                            col,
                            "unterminated UTF-8 string literal",
                        )
                    )
                tokens.append(
                    Token(
                        "Str",
                        self.src[s_start : self.scalar_pos],
                        UnicodeSpan(b_start, self.byte_pos, s_start, self.scalar_pos, line, col),
                    )
                )
                continue

            simple = {"=": "Eq", "+": "Plus", ";": "Semi"}.get(ch)
            self.advance()
            if simple is not None:
                tokens.append(
                    Token(
                        simple,
                        ch,
                        UnicodeSpan(b_start, self.byte_pos, s_start, self.scalar_pos, line, col),
                    )
                )
            else:
                diags.append(
                    LexDiagnostic(
                        "InvalidChar",
                        b_start,
                        s_start,
                        line,
                        col,
                        f"unexpected Unicode character '{ch}' (U+{cp:04X})",
                    )
                )

        tokens.append(
            Token(
                "Eof",
                "$",
                UnicodeSpan(self.byte_pos, self.byte_pos, self.scalar_pos, self.scalar_pos, self.line, self.col),
            )
        )
        return tokens, diags


def lex_unicode(src: str) -> tuple[list[Token], list[LexDiagnostic]]:
    return Scanner(src).lex()


def main() -> None:
    print("=== Advanced Parsing (Modern): Unicode-Aware Lexing & Security Diagnostics ===")
    clean = "let café = 10;\nlet αβγ\u3000= café + 変数;"
    tokens, diags = lex_unicode(clean)
    assert not diags
    print(f"Clean multilingual program ({len(tokens)} tokens, 0 warnings):")
    for t in tokens:
        sp = t.span
        print(
            f"  {t.kind:<6} {t.lexeme:<6} bytes={sp.byte_start}..{sp.byte_end} "
            f"scalars={sp.scalar_start}..{sp.scalar_end} @{sp.line}:{sp.col}"
        )

    suspect = "let v\u0430r = 1; // \u202Elocked"
    _, sec_diags = lex_unicode(suspect)
    print(f"Security diagnostics on spoofed source ({len(sec_diags)}):")
    for d in sec_diags:
        print(
            f"  {d.kind} at {d.line}:{d.col} (byte {d.byte_offset}, scalar {d.scalar_offset}): {d.message}"
        )


class UnicodeLexingTests(unittest.TestCase):
    def test_tracks_byte_offsets_scalar_indices_and_display_columns(self) -> None:
        src = "let café = 変数 + αβγ;"
        tokens, diags = lex_unicode(src)
        self.assertEqual(diags, [])
        self.assertEqual(tokens[1].lexeme, "café")
        self.assertEqual((tokens[1].span.byte_start, tokens[1].span.byte_end), (4, 9))
        self.assertEqual((tokens[1].span.scalar_start, tokens[1].span.scalar_end), (4, 8))
        self.assertEqual(tokens[1].span.col, 5)
        cjk = tokens[3]
        self.assertEqual(cjk.lexeme, "変数")
        self.assertEqual(cjk.span.byte_end - cjk.span.byte_start, 6)
        self.assertEqual(cjk.span.scalar_end - cjk.span.scalar_start, 2)
        self.assertEqual(tokens[4].lexeme, "+")
        self.assertEqual(tokens[4].span.col, cjk.span.col + 4 + 1)

    def test_flags_mixed_script_homoglyph_trojan_bidi_and_zero_width(self) -> None:
        src = "let p\u0430ypal = 1; // \u202Eadmin\nlet x\u200By = 2;"
        _, diags = lex_unicode(src)
        self.assertEqual(
            [d.kind for d in diags],
            ["MixedScriptConfusable", "BidiControl", "ZeroWidthChar"],
        )

    def test_rejects_emoji_without_splitting_utf8(self) -> None:
        src = "let x = 🚀;"
        tokens, diags = lex_unicode(src)
        self.assertEqual(len(diags), 1)
        self.assertEqual(diags[0].kind, "InvalidChar")
        self.assertEqual(sum(1 for t in tokens if t.kind == "Semi"), 1)


if __name__ == "__main__":
    main()
