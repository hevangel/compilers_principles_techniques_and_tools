"""Chapter 3 (§3.2): Double-Buffer Input Scanning with EOF Sentinels.

Demonstrates:
1. Buffer pair of size `2 * half_size + 2` with an out-of-band `EOF_SENTINEL` at the end
   of each half so normal character advance performs a single test (`c == EOF_SENTINEL`),
2. `lexeme_begin` and `forward` pointers spanning half-buffer boundaries,
3. Safe multi-step `retract()` across boundaries (e.g. `1234567..4` failing float lookahead
   and retracting both `.` bytes back across the half seam) using `half_reloaded_for_cycle`
   so advancing past the boundary again never overwrites unread data,
4. Overflow detection when a single lexeme exceeds `half_size` before overwriting `lexeme_begin`,
5. Exact equivalence between buffered scanning (`N = 8`) and direct slice scanning.
"""

from __future__ import annotations

from dataclasses import dataclass
import unittest

EOF_SENTINEL = -1


@dataclass(frozen=True)
class Token:
    kind: str
    lexeme: str
    start: int
    end: int


@dataclass(frozen=True)
class BufferEvent:
    kind: str
    detail: str


class BufferError(Exception):
    def __init__(self, offset: int, message: str) -> None:
        super().__init__(f"lexical buffer error at {offset}: {message}")
        self.offset = offset
        self.message = message


class DoubleBuffer:
    def __init__(self, source: bytes, half_size: int) -> None:
        assert half_size >= 2, "half_size must be at least 2"
        self.source = source
        self.src_pos = 0
        self.half_size = half_size
        self.buf = [EOF_SENTINEL] * (2 * half_size + 2)
        self.lexeme_begin = 0
        self.forward = 0
        self.lexeme_global_start = 0
        self.half_reloaded_for_cycle = [False, False]
        self.events: list[BufferEvent] = []
        self._load_half(0)

    def _sentinel_slot(self, half: int) -> int:
        return self.half_size if half == 0 else 2 * self.half_size + 1

    def _half_start(self, half: int) -> int:
        return 0 if half == 0 else self.half_size + 1

    def _lexeme_begin_half(self) -> int:
        return 0 if self.lexeme_begin <= self._sentinel_slot(0) else 1

    def _load_half(self, half: int) -> None:
        start = self._half_start(half)
        remaining = max(0, len(self.source) - self.src_pos)
        to_copy = min(remaining, self.half_size)
        for i in range(to_copy):
            self.buf[start + i] = self.source[self.src_pos + i]
        self.buf[start + to_copy] = EOF_SENTINEL
        self.buf[self._sentinel_slot(half)] = EOF_SENTINEL
        self.src_pos += to_copy
        self.half_reloaded_for_cycle[half] = True
        self.events.append(
            BufferEvent("load_half", f"load_half({half}) -> copied {to_copy} byte(s) + EOF sentinel")
        )

    def next_char(self) -> int | None:
        while True:
            ch = self.buf[self.forward]
            if ch != EOF_SENTINEL:
                self.forward += 1
                return ch
            if self.forward == self._sentinel_slot(0):
                self.events.append(
                    BufferEvent("hit_sentinel", f"hit_sentinel(slot={self.forward}) -> switch buffer half")
                )
                if not self.half_reloaded_for_cycle[1]:
                    if self._lexeme_begin_half() == 1:
                        raise BufferError(
                            self.lexeme_global_start,
                            f"lexeme length exceeds half-buffer size N={self.half_size}",
                        )
                    self._load_half(1)
                self.half_reloaded_for_cycle[0] = False
                self.forward = self._half_start(1)
            elif self.forward == self._sentinel_slot(1):
                self.events.append(
                    BufferEvent("hit_sentinel", f"hit_sentinel(slot={self.forward}) -> switch buffer half")
                )
                if not self.half_reloaded_for_cycle[0]:
                    if self._lexeme_begin_half() == 0:
                        raise BufferError(
                            self.lexeme_global_start,
                            f"lexeme length exceeds half-buffer size N={self.half_size}",
                        )
                    self._load_half(0)
                self.half_reloaded_for_cycle[1] = False
                self.forward = self._half_start(0)
            else:
                return None

    def retract(self) -> None:
        if self.forward == self._half_start(1):
            from_slot = self.forward
            self.half_reloaded_for_cycle[1] = True
            self.forward = self._sentinel_slot(0) - 1
            self.events.append(
                BufferEvent(
                    "retract",
                    f"retract({from_slot} -> {self.forward}) across boundary (fence prevents double-reload)",
                )
            )
        elif self.forward == self._half_start(0):
            from_slot = self.forward
            self.half_reloaded_for_cycle[0] = True
            self.forward = self._sentinel_slot(1) - 1
            self.events.append(
                BufferEvent(
                    "retract",
                    f"retract({from_slot} -> {self.forward}) across boundary (fence prevents double-reload)",
                )
            )
        else:
            self.forward -= 1

    def commit_lexeme(self) -> tuple[str, int, int]:
        out = bytearray()
        idx = self.lexeme_begin
        s0 = self._sentinel_slot(0)
        s1 = self._sentinel_slot(1)
        h0 = self._half_start(0)
        h1 = self._half_start(1)
        while idx != self.forward:
            if idx == s0:
                idx = h1
                continue
            if idx == s1:
                idx = h0
                continue
            ch = self.buf[idx]
            if ch == EOF_SENTINEL:
                break
            out.append(ch)
            idx += 1
        start = self.lexeme_global_start
        end = start + len(out)
        self.lexeme_begin = self.forward
        self.lexeme_global_start = end
        self.current_lexeme_len = 0
        return out.decode("utf-8", errors="replace"), start, end

    def discard_whitespace(self) -> None:
        while True:
            b = self.next_char()
            if b is None:
                break
            if not chr(b).isspace():
                self.retract()
                break
            self.lexeme_begin = self.forward
            self.lexeme_global_start += 1
            self.current_lexeme_len = 0


def _is_ident_start(b: int) -> bool:
    return (ord("a") <= b <= ord("z")) or (ord("A") <= b <= ord("Z")) or b == ord("_")


def _is_ident_cont(b: int) -> bool:
    return _is_ident_start(b) or (ord("0") <= b <= ord("9"))


def scan_buffered(src: str, half_size: int) -> tuple[list[Token], list[BufferEvent]]:
    db = DoubleBuffer(src.encode("utf-8"), half_size)
    tokens: list[Token] = []

    while True:
        db.discard_whitespace()
        first = db.next_char()
        if first is None:
            break

        if _is_ident_start(first):
            while True:
                c = db.next_char()
                if c is None or not _is_ident_cont(c):
                    if c is not None:
                        db.retract()
                    break
            kind = "IDENT"
        elif ord("0") <= first <= ord("9"):
            while True:
                c = db.next_char()
                if c is None or not (ord("0") <= c <= ord("9")):
                    if c is not None:
                        db.retract()
                    break
            nxt = db.next_char()
            if nxt == ord("."):
                nxt2 = db.next_char()
                if nxt2 is not None and (ord("0") <= nxt2 <= ord("9")):
                    while True:
                        d2 = db.next_char()
                        if d2 is None or not (ord("0") <= d2 <= ord("9")):
                            if d2 is not None:
                                db.retract()
                            break
                    kind = "FLOAT"
                elif nxt2 is not None:
                    db.retract()
                    db.retract()
                    kind = "INT"
                else:
                    db.retract()
                    kind = "INT"
            else:
                if nxt is not None:
                    db.retract()
                kind = "INT"
        elif first == ord("."):
            if db.next_char() == ord("."):
                kind = "RANGE"
            else:
                raise BufferError(db.lexeme_global_start, "unexpected '.'")
        elif first == ord("<"):
            nxt = db.next_char()
            if nxt == ord("="):
                kind = "LE"
            else:
                if nxt is not None:
                    db.retract()
                kind = "LT"
        elif first == ord("="):
            nxt = db.next_char()
            if nxt == ord("="):
                kind = "EQEQ"
            else:
                if nxt is not None:
                    db.retract()
                kind = "ASSIGN"
        elif first == ord("+"):
            kind = "PLUS"
        elif first == ord(";"):
            kind = "SEMI"
        else:
            raise BufferError(db.lexeme_global_start, f"unexpected byte '{chr(first)}'")

        lexeme, start, end = db.commit_lexeme()
        tokens.append(Token(kind=kind, lexeme=lexeme, start=start, end=end))

    return tokens, db.events


def scan_direct(src: str) -> list[Token]:
    raw = src.encode("utf-8")
    i = 0
    tokens: list[Token] = []
    while i < len(raw):
        if chr(raw[i]).isspace():
            i += 1
            continue
        start = i
        if _is_ident_start(raw[i]):
            i += 1
            while i < len(raw) and _is_ident_cont(raw[i]):
                i += 1
            kind = "IDENT"
        elif ord("0") <= raw[i] <= ord("9"):
            i += 1
            while i < len(raw) and (ord("0") <= raw[i] <= ord("9")):
                i += 1
            if i + 1 < len(raw) and raw[i] == ord(".") and (ord("0") <= raw[i + 1] <= ord("9")):
                i += 1
                while i < len(raw) and (ord("0") <= raw[i] <= ord("9")):
                    i += 1
                kind = "FLOAT"
            else:
                kind = "INT"
        elif raw[i] == ord(".") and i + 1 < len(raw) and raw[i + 1] == ord("."):
            i += 2
            kind = "RANGE"
        elif raw[i] == ord("<"):
            if i + 1 < len(raw) and raw[i + 1] == ord("="):
                i += 2
                kind = "LE"
            else:
                i += 1
                kind = "LT"
        elif raw[i] == ord("="):
            if i + 1 < len(raw) and raw[i + 1] == ord("="):
                i += 2
                kind = "EQEQ"
            else:
                i += 1
                kind = "ASSIGN"
        elif raw[i] == ord("+"):
            i += 1
            kind = "PLUS"
        elif raw[i] == ord(";"):
            i += 1
            kind = "SEMI"
        else:
            raise BufferError(start, f"unexpected byte '{chr(raw[i])}'")
        tokens.append(
            Token(kind=kind, lexeme=raw[start:i].decode("utf-8"), start=start, end=i)
        )
    return tokens


def main() -> None:
    src = "alpha = beta1 <= 2.5 + 1..4 == 42;"
    tokens, events = scan_buffered(src, 8)
    direct = scan_direct(src)
    assert tokens == direct

    print("=== Double-Buffer Input Scanning with Sentinels (§3.2) ===")
    print(f"Source ({len(src)} bytes), half-buffer size N = 8:")
    print(f"  {src}\n")
    print(f"Tokens matched ({len(tokens)}, identical to direct slice scan):")
    for t in tokens:
        print(f"  [{t.start:02d}..{t.end:02d}] {t.kind:<6} {t.lexeme!r}")
    print(f"\nBuffer reloads, sentinel hits & boundary retractions ({len(events)}):")
    for ev in events:
        print(f"  {ev.detail}")


class InputBufferingTests(unittest.TestCase):
    def test_buffered_scan_matches_direct_across_varied_half_sizes(self) -> None:
        src = "x1 <= y2 + 12.34 + 1..9; a==b; c<d; total = 99;"
        direct = scan_direct(src)
        for half_size in (8, 9, 10, 11, 16, 64):
            buffered, events = scan_buffered(src, half_size)
            self.assertEqual(buffered, direct)
            self.assertTrue(any(ev.kind == "load_half" for ev in events))

    def test_retract_across_boundary_does_not_reload_same_half_twice(self) -> None:
        src = "1234567..4"
        tokens, events = scan_buffered(src, 8)
        self.assertEqual(
            tokens,
            [
                Token("INT", "1234567", 0, 7),
                Token("RANGE", "..", 7, 9),
                Token("INT", "4", 9, 10),
            ],
        )
        half1_loads = [ev for ev in events if ev.kind == "load_half" and "load_half(1)" in ev.detail]
        self.assertEqual(len(half1_loads), 1)
        self.assertTrue(any(ev.kind == "retract" for ev in events))

    def test_rejects_lexeme_longer_than_half_buffer(self) -> None:
        with self.assertRaises(BufferError):
            scan_buffered("very_long_identifier", 8)


if __name__ == "__main__":
    main()
