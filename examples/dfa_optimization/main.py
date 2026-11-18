"""Chapter 3 (§3.9): DFA Optimization — Equivalence Classes, Comb Compression & Direct Code.

Demonstrates:
1. Building a multi-token scanner DFA (keywords `let`/`while`, `IDENT`, `INT`, `FLOAT`,
   operators `<=`, `<`, `=`, `+`, `;`, `WS`) over ASCII bytes `0..128`,
2. Partitioning the alphabet into minimal character equivalence classes (`char_class[128]`),
3. Compressing the transition matrix with Dragon Book §3.9 `default / base / next / check`
   row-displacement (comb) encoding,
4. Generating and running a direct-coded scanner (explicit `match` transitions without tables),
5. Verifying identical longest-match token streams across all three DFA representations.
"""

from __future__ import annotations

from dataclasses import dataclass
from typing import Callable
import unittest

DEAD = -1


@dataclass(frozen=True)
class Token:
    kind: str
    lexeme: str
    start: int
    end: int


@dataclass
class DenseDfa:
    accept: list[str | None]
    trans: list[list[int]]


def build_scanner_dfa() -> DenseDfa:
    accept: list[str | None] = [
        None,     # 0
        "WS",     # 1
        "IDENT",  # 2
        "IDENT",  # 3 'l'
        "IDENT",  # 4 'le'
        "LET",    # 5 'let'
        "IDENT",  # 6 'w'
        "IDENT",  # 7 'wh'
        "IDENT",  # 8 'whi'
        "IDENT",  # 9 'whil'
        "WHILE",  # 10 'while'
        "INT",    # 11
        None,     # 12 '123.'
        "FLOAT",  # 13
        "LT",     # 14
        "LE",     # 15
        "EQ",     # 16
        "PLUS",   # 17
        "SEMI",   # 18
    ]
    trans = [[DEAD] * 128 for _ in accept]

    for b in range(128):
        ch = chr(b)
        is_id_start = ("a" <= ch <= "z") or ("A" <= ch <= "Z") or ch == "_"
        is_id_cont = is_id_start or ("0" <= ch <= "9")
        if ch in (" ", "\t", "\n", "\r"):
            trans[0][b] = 1
            trans[1][b] = 1
        elif is_id_start:
            trans[0][b] = 3 if ch == "l" else (6 if ch == "w" else 2)
        elif "0" <= ch <= "9":
            trans[0][b] = 11
            trans[11][b] = 11
            trans[12][b] = 13
            trans[13][b] = 13
        elif ch == "<":
            trans[0][b] = 14
        elif ch == "=":
            trans[0][b] = 16
            trans[14][b] = 15
        elif ch == "+":
            trans[0][b] = 17
        elif ch == ";":
            trans[0][b] = 18
        elif ch == ".":
            trans[11][b] = 12

        if is_id_cont:
            for s in range(2, 11):
                trans[s][b] = 2

    trans[3][ord("e")] = 4
    trans[4][ord("t")] = 5
    trans[6][ord("h")] = 7
    trans[7][ord("i")] = 8
    trans[8][ord("l")] = 9
    trans[9][ord("e")] = 10
    return DenseDfa(accept=accept, trans=trans)


@dataclass
class EquivClassDfa:
    char_class: list[int]
    num_classes: int
    class_rep: list[int]
    accept: list[str | None]
    table: list[list[int]]


def build_equiv_classes(dfa: DenseDfa) -> EquivClassDfa:
    sig_to_class: dict[tuple[int, ...], int] = {}
    char_class = [0] * 128
    class_rep: list[int] = []

    for b in range(128):
        sig = tuple(row[b] for row in dfa.trans)
        cid = sig_to_class.get(sig)
        if cid is None:
            cid = len(class_rep)
            sig_to_class[sig] = cid
            class_rep.append(b)
        char_class[b] = cid

    num_classes = len(class_rep)
    table = [[dfa.trans[s][rep] for rep in class_rep] for s in range(len(dfa.accept))]
    return EquivClassDfa(
        char_class=char_class,
        num_classes=num_classes,
        class_rep=class_rep,
        accept=list(dfa.accept),
        table=table,
    )


class CompressedCombDfa:
    def __init__(self, eq: EquivClassDfa) -> None:
        self.char_class = eq.char_class
        self.num_classes = eq.num_classes
        self.accept = list(eq.accept)
        n = len(eq.accept)
        c = eq.num_classes
        self.default: list[int | None] = [None] * n
        diffs: list[list[tuple[int, int]]] = []

        for s in range(n):
            best_def: int | None = None
            best_diff = [(cls, eq.table[s][cls]) for cls in range(c) if eq.table[s][cls] != DEAD]
            for cand in range(s):
                cand_diff = [
                    (cls, eq.table[s][cls])
                    for cls in range(c)
                    if eq.table[s][cls] != eq.table[cand][cls]
                ]
                if len(cand_diff) < len(best_diff):
                    best_diff = cand_diff
                    best_def = cand
            self.default[s] = best_def
            diffs.append(best_diff)

        self.base = [0] * n
        self.next: list[int] = []
        self.check: list[int | None] = []
        for s in range(n):
            entries = diffs[s]
            if not entries:
                continue
            b = 0
            while True:
                needed = b + c
                if len(self.next) < needed:
                    pad = needed - len(self.next)
                    self.next.extend([DEAD] * pad)
                    self.check.extend([None] * pad)
                if all(self.check[b + cls] is None for cls, _ in entries):
                    self.base[s] = b
                    for cls, target in entries:
                        self.check[b + cls] = s
                        self.next[b + cls] = target
                    break
                b += 1

        while self.check and self.check[-1] is None:
            self.check.pop()
            self.next.pop()

    def step(self, state: int, byte: int) -> int:
        if byte >= 128:
            return DEAD
        cls = self.char_class[byte]
        cur: int | None = state
        while cur is not None:
            idx = self.base[cur] + cls
            if idx < len(self.check) and self.check[idx] == cur:
                return self.next[idx]
            cur = self.default[cur]
        return DEAD

    def storage_cells(self) -> int:
        return 128 + 3 * len(self.accept) + 2 * len(self.next)


def direct_coded_step(state: int, b: int) -> int:
    ch = chr(b) if b < 128 else ""
    is_id_cont = ("a" <= ch <= "z") or ("A" <= ch <= "Z") or ("0" <= ch <= "9") or ch == "_"
    if state == 0:
        if ch in (" ", "\t", "\n", "\r"):
            return 1
        if ch == "l":
            return 3
        if ch == "w":
            return 6
        if ("a" <= ch <= "z") or ("A" <= ch <= "Z") or ch == "_":
            return 2
        if "0" <= ch <= "9":
            return 11
        if ch == "<":
            return 14
        if ch == "=":
            return 16
        if ch == "+":
            return 17
        if ch == ";":
            return 18
        return DEAD
    if state == 1:
        return 1 if ch in (" ", "\t", "\n", "\r") else DEAD
    if state == 3:
        return 4 if ch == "e" else (2 if is_id_cont else DEAD)
    if state == 4:
        return 5 if ch == "t" else (2 if is_id_cont else DEAD)
    if state == 6:
        return 7 if ch == "h" else (2 if is_id_cont else DEAD)
    if state == 7:
        return 8 if ch == "i" else (2 if is_id_cont else DEAD)
    if state == 8:
        return 9 if ch == "l" else (2 if is_id_cont else DEAD)
    if state == 9:
        return 10 if ch == "e" else (2 if is_id_cont else DEAD)
    if state in (2, 5, 10):
        return 2 if is_id_cont else DEAD
    if state == 11:
        if "0" <= ch <= "9":
            return 11
        if ch == ".":
            return 12
        return DEAD
    if state in (12, 13):
        return 13 if "0" <= ch <= "9" else DEAD
    if state == 14:
        return 15 if ch == "=" else DEAD
    return DEAD


def scan_with_step(
    src: str, accept: list[str | None], step: Callable[[int, int], int]
) -> list[Token]:
    raw = src.encode("utf-8")
    pos = 0
    tokens: list[Token] = []
    while pos < len(raw):
        state = 0
        cur = pos
        last_accept: tuple[str, int] | None = None
        while cur < len(raw):
            nxt = step(state, raw[cur])
            if nxt == DEAD:
                break
            state = nxt
            cur += 1
            if accept[state] is not None:
                last_accept = (accept[state], cur)  # type: ignore[arg-type]
        if last_accept is None:
            raise ValueError(f"unexpected byte {chr(raw[pos])!r} at {pos}")
        kind, end = last_accept
        if kind != "WS":
            tokens.append(Token(kind=kind, lexeme=raw[pos:end].decode("utf-8"), start=pos, end=end))
        pos = end
    return tokens


def emit_direct_coded_snippet(dfa: DenseDfa, max_states: int = 4) -> str:
    lines = ["def direct_step(state: int, b: int) -> int:"]
    for s in range(min(max_states, len(dfa.accept))):
        by_target: dict[int, list[int]] = {}
        for b in range(128):
            t = dfa.trans[s][b]
            if t != DEAD:
                by_target.setdefault(t, []).append(b)
        if not by_target:
            continue
        lines.append(f"    if state == {s}:")
        for target, blist in sorted(by_target.items()):
            if len(blist) <= 3:
                lits = ", ".join(repr(chr(b)) if 32 <= b <= 126 else str(b) for b in blist)
                lines.append(f"        if chr(b) in ({lits},): return {target}")
            else:
                lines.append(f"        # {len(blist)}-byte class -> {target}")
        lines.append("        return DEAD")
    lines.append("    return DEAD")
    return "\n".join(lines)


def main() -> None:
    dense = build_scanner_dfa()
    eq = build_equiv_classes(dense)
    comb = CompressedCombDfa(eq)

    dense_cells = len(dense.accept) * 128
    eq_cells = 128 + len(dense.accept) * eq.num_classes
    comb_cells = comb.storage_cells()

    print("=== DFA Optimization: Equivalence Classes, Comb Compression & Direct Code (§3.9) ===")
    print(f"DFA states: {len(dense.accept)}")
    print(f"Alphabet compression: 128 ASCII bytes -> {eq.num_classes} equivalence classes")
    print("\nTable footprint (integer/option cells):")
    print(f"  1. Full 2D DFA (states * 128):            {dense_cells} cells")
    print(f"  2. Equiv-classed 2D (128 + states * C):   {eq_cells} cells")
    print(f"  3. Comb default/base/next/check:          {comb_cells} cells (next.len = {len(comb.next)})")

    sample = "let while1 = 12 + 3.5; while let2 <= while1;"
    t_dense = scan_with_step(sample, dense.accept, lambda s, b: dense.trans[s][b] if b < 128 else DEAD)
    t_eq = scan_with_step(sample, eq.accept, lambda s, b: eq.table[s][eq.char_class[b]] if b < 128 else DEAD)
    t_comb = scan_with_step(sample, comb.accept, comb.step)
    t_direct = scan_with_step(sample, dense.accept, direct_coded_step)

    assert t_dense == t_eq == t_comb == t_direct
    print(f"\nAll 4 DFA engines matched identical {len(t_comb)} tokens on {sample!r}:")
    for t in t_comb:
        print(f"  [{t.start:02d}..{t.end:02d}] {t.kind:<5} {t.lexeme!r}")
    print(f"\nEmitted direct-coded preview (first 4 states):\n{emit_direct_coded_snippet(dense, 4)}")


class DfaOptimizationTests(unittest.TestCase):
    def test_equivalence_classes_and_comb_compression_preserve_all_transitions(self) -> None:
        dense = build_scanner_dfa()
        eq = build_equiv_classes(dense)
        comb = CompressedCombDfa(eq)
        for s in range(len(dense.accept)):
            for b in range(128):
                d_next = dense.trans[s][b]
                self.assertEqual(eq.table[s][eq.char_class[b]], d_next)
                self.assertEqual(comb.step(s, b), d_next)
                self.assertEqual(direct_coded_step(s, b), d_next)

    def test_compression_achieves_large_footprint_reduction(self) -> None:
        dense = build_scanner_dfa()
        eq = build_equiv_classes(dense)
        comb = CompressedCombDfa(eq)
        dense_cells = len(dense.accept) * 128
        eq_cells = 128 + len(dense.accept) * eq.num_classes
        self.assertLess(eq.num_classes, 20)
        self.assertLess(eq_cells, dense_cells // 4)
        self.assertLess(comb.storage_cells(), eq_cells)

    def test_keyword_prefixes_and_operators_match_identically(self) -> None:
        dense = build_scanner_dfa()
        comb = CompressedCombDfa(build_equiv_classes(dense))
        src = "let letter while while_x 12 12.5 <= < = +;"
        tokens = scan_with_step(src, comb.accept, comb.step)
        self.assertEqual(
            [(t.kind, t.lexeme) for t in tokens],
            [
                ("LET", "let"),
                ("IDENT", "letter"),
                ("WHILE", "while"),
                ("IDENT", "while_x"),
                ("INT", "12"),
                ("FLOAT", "12.5"),
                ("LE", "<="),
                ("LT", "<"),
                ("EQ", "="),
                ("PLUS", "+"),
                ("SEMI", ";"),
            ],
        )


if __name__ == "__main__":
    main()
