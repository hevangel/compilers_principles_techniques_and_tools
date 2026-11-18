"""Recover from a malformed statement by resuming after a semicolon."""

from dataclasses import dataclass
import unittest


@dataclass(frozen=True)
class Diagnostic:
    """Syntax diagnostic reporting a UTF-8 byte offset and human-readable message."""

    offset: int
    message: str


def split_inclusive_semicolon(text: str) -> list[str]:
    """Split `text` on `;` while keeping the delimiter so missing final `;` is visible."""
    chunks: list[str] = []
    start = 0
    for idx, ch in enumerate(text):
        if ch == ";":
            chunks.append(text[start : idx + 1])
            start = idx + 1
    if start < len(text):
        chunks.append(text[start:])
    return chunks


def recover(source: str) -> tuple[list[int], list[Diagnostic]]:
    """Recover values from `emit <u32>;` statements, synchronizing at `;` on errors."""
    values: list[int] = []
    diagnostics: list[Diagnostic] = []
    # Track UTF-8 byte offset (not code-point index) so offsets match Rust and multibyte text.
    offset = 0
    for chunk in split_inclusive_semicolon(source):
        chunk_bytes = len(chunk.encode("utf-8"))
        statement = chunk[:-1] if chunk.endswith(";") else chunk
        leading_space = len(statement.encode("utf-8")) - len(
            statement.lstrip().encode("utf-8")
        )
        stripped = statement.strip()
        if not stripped:
            offset += chunk_bytes
            continue
        parts = stripped.split()
        message: str | None
        # Check delimiter first so an unterminated statement reports missing `;`.
        if not chunk.endswith(";"):
            message = "expected a semicolon"
        elif len(parts) == 2 and parts[0] == "emit" and parts[1].isascii() and parts[1].isdigit():
            number = int(parts[1])
            # Match Rust u32 bounds explicitly.
            if number <= 0xFFFF_FFFF:
                values.append(number)
                message = None
            else:
                message = "number out of range"
        else:
            message = "expected 'emit' and an unsigned integer"
        if message is not None:
            diagnostics.append(Diagnostic(offset=offset + leading_space, message=message))
        offset += chunk_bytes
    return values, diagnostics


def main() -> None:
    values, diagnostics = recover("emit x; emit 7; bad; emit 19;")
    print(f"Recovered values: {values}")
    print(f"Diagnostics: {diagnostics}")


class RecoveryTests(unittest.TestCase):
    def test_resumes_after_semicolons_and_checks_unterminated_tail(self) -> None:
        values, diagnostics = recover("emit x; emit 7; \u00e9; emit 19")
        self.assertEqual(values, [7])
        self.assertEqual([d.offset for d in diagnostics], [0, 16, 20])
        self.assertEqual(diagnostics[2].message, "expected a semicolon")


if __name__ == "__main__":
    main()
