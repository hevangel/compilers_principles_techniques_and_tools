"""Scan a source string into tokens with lexemes and UTF-8 byte offsets."""

from dataclasses import dataclass
from enum import Enum
import unittest


class Kind(Enum):
    """Token category consumed by a parser to make grammatical decisions."""

    LET = "Let"
    NAME = "Name"
    INTEGER = "Integer"
    EQUAL = "Equal"
    EQUAL_EQUAL = "EqualEqual"
    SEMICOLON = "Semicolon"


@dataclass(frozen=True)
class Token:
    """Scanned token carrying its category, exact lexeme text, and UTF-8 byte offset."""

    kind: Kind
    lexeme: str
    start: int


class LexError(ValueError):
    """Raised when the scanner encounters an unsupported byte, preserving its byte offset."""

    def __init__(self, offset: int) -> None:
        super().__init__(f"unexpected byte at offset {offset}")
        self.offset = offset


def scan(source: str) -> list[Token]:
    """Scan `source` into tokens using maximal-munch rules over UTF-8 bytes."""
    # Encode to UTF-8 bytes so `start` measures byte offsets (matching Rust's &str indices)
    # rather than Unicode code-point counts.
    raw = source.encode("utf-8")
    tokens: list[Token] = []
    end = 0
    while end < len(raw):
        start = end
        end += 1
        byte = raw[start]
        # Skip ASCII whitespace without emitting a token while keeping byte offsets accurate.
        if byte < 128 and chr(byte).isspace():
            continue
        if (65 <= byte <= 90) or (97 <= byte <= 122) or byte == ord("_"):
            # Longest match: consume all identifier characters before checking keywords,
            # so `letter` becomes Name("letter") instead of Let + Name("ter").
            while end < len(raw) and (
                48 <= raw[end] <= 57
                or 65 <= raw[end] <= 90
                or 97 <= raw[end] <= 122
                or raw[end] == ord("_")
            ):
                end += 1
            lexeme = raw[start:end].decode("ascii")
            kind = Kind.LET if lexeme == "let" else Kind.NAME
        elif 48 <= byte <= 57:
            # Consecutive digits form one Integer token; leading zeros remain in `lexeme`.
            while end < len(raw) and 48 <= raw[end] <= 57:
                end += 1
            lexeme = raw[start:end].decode("ascii")
            kind = Kind.INTEGER
        elif byte == ord("=") and end < len(raw) and raw[end] == ord("="):
            # One-byte lookahead: prefer `==` over `=` whenever the next byte is `=`.
            end += 1
            lexeme = "=="
            kind = Kind.EQUAL_EQUAL
        elif byte == ord("="):
            lexeme = "="
            kind = Kind.EQUAL
        elif byte == ord(";"):
            lexeme = ";"
            kind = Kind.SEMICOLON
        else:
            # Unknown ASCII punctuation or any multi-byte UTF-8 lead byte stops here.
            raise LexError(start)
        tokens.append(Token(kind=kind, lexeme=lexeme, start=start))
    return tokens


def main() -> None:
    for token in scan("let letter = 12; letter == 12;"):
        print(f"{token.start} {token.kind.value} {token.lexeme!r}")


class LexerTests(unittest.TestCase):
    def test_recognizes_longest_token_and_reports_byte_offsets(self) -> None:
        tokens = scan("let letter==12;")
        self.assertEqual(tokens[0].kind, Kind.LET)
        self.assertEqual(tokens[1].kind, Kind.NAME)
        self.assertEqual(
            tokens[2],
            Token(kind=Kind.EQUAL_EQUAL, lexeme="==", start=10),
        )
        with self.assertRaises(LexError) as ctx:
            scan("a = \u03bb")
        self.assertEqual(ctx.exception.offset, 4)


if __name__ == "__main__":
    main()
