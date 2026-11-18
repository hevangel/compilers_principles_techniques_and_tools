"""Chapter 3 Extension: Lexer Start Conditions (%s / %x) and Contextual Mode Stack.

Demonstrates:
1. Multiple finite-automaton rule tables keyed by lexer start condition:
   `INITIAL`, `%x COMMENT` (supporting nested `/* /* ... */ */`),
   `%x STRING` (decoding escapes and `${...}` expression interpolation),
   and `%x DIRECTIVE` (tokenizing `#define NAME value` to end-of-line),
2. Explicit mode stack (`push_mode` / `pop_mode`) + brace-depth tracking so expressions
   inside `"hi ${a + {1}}"` return cleanly back into `STRING` mode on the matching `}`,
3. Mode-transition trace showing how contextual lexing keeps each rule DFA small and clean.
"""

from __future__ import annotations

from dataclasses import dataclass
import unittest


@dataclass(frozen=True)
class Token:
    kind: str
    lexeme: str
    mode: str


@dataclass(frozen=True)
class ModeTransition:
    at: int
    src_mode: str
    dst_mode: str
    reason: str


class ModalLexer:
    def __init__(self, src: str) -> None:
        self.src = src
        self.n = len(src)
        self.pos = 0
        self.mode_stack: list[str] = ["INITIAL"]
        self.interp_brace_depth: list[int] = []
        self.comment_depth = 0
        self.tokens: list[Token] = []
        self.transitions: list[ModeTransition] = []

    def mode(self) -> str:
        return self.mode_stack[-1]

    def push_mode(self, nxt: str, reason: str) -> None:
        prev = self.mode()
        self.mode_stack.append(nxt)
        self.transitions.append(ModeTransition(self.pos, prev, nxt, reason))

    def pop_mode(self, reason: str) -> None:
        if len(self.mode_stack) > 1:
            prev = self.mode_stack.pop()
            self.transitions.append(ModeTransition(self.pos, prev, self.mode(), reason))

    def run(self) -> tuple[list[Token], list[ModeTransition]]:
        while self.pos < self.n:
            m = self.mode()
            if m == "INITIAL":
                self._step_initial()
            elif m == "%x COMMENT":
                self._step_comment()
            elif m == "%x STRING":
                self._step_string()
            else:
                self._step_directive()

        m = self.mode()
        if m == "%x COMMENT":
            raise ValueError("unterminated nested block comment at EOF")
        if m == "%x STRING":
            raise ValueError("unterminated interpolated string at EOF")
        if m == "%x DIRECTIVE":
            self.pop_mode("EOF ends directive")
        elif self.interp_brace_depth:
            raise ValueError("unclosed `${ ...` interpolation at EOF")
        return self.tokens, self.transitions

    def _step_initial(self) -> None:
        ch = self.src[self.pos]
        if ch.isspace():
            self.pos += 1
            return
        if ch == "#":
            self.pos += 1
            self.push_mode("%x DIRECTIVE", "saw '#' pragma/directive start")
            return
        if ch == "/" and self.pos + 1 < self.n and self.src[self.pos + 1] == "*":
            self.comment_depth = 1
            self.pos += 2
            self.push_mode("%x COMMENT", "saw '/*' comment open (depth=1)")
            return
        if ch == '"':
            self.pos += 1
            self.tokens.append(Token("STR_OPEN", '"', "INITIAL"))
            self.push_mode("%x STRING", "saw '\"' string open")
            return
        if ch == "}" and self.interp_brace_depth:
            if self.interp_brace_depth[-1] == 0:
                self.interp_brace_depth.pop()
                self.pos += 1
                self.tokens.append(Token("INTERP_END", "}", "INITIAL"))
                self.pop_mode("matching '}' closes `${...}` back to STRING")
                return
            self.interp_brace_depth[-1] -= 1
            self.pos += 1
            self.tokens.append(Token("RBRACE", "}", "INITIAL"))
            return
        if ch == "{":
            if self.interp_brace_depth:
                self.interp_brace_depth[-1] += 1
            self.pos += 1
            self.tokens.append(Token("LBRACE", "{", "INITIAL"))
            return
        if ch.isalpha() or ch == "_":
            start = self.pos
            self.pos += 1
            while self.pos < self.n and (self.src[self.pos].isalnum() or self.src[self.pos] == "_"):
                self.pos += 1
            lexeme = self.src[start:self.pos]
            kind = "LET" if lexeme == "let" else "IDENT"
            self.tokens.append(Token(kind, lexeme, "INITIAL"))
            return
        if ch.isdigit():
            start = self.pos
            self.pos += 1
            while self.pos < self.n and self.src[self.pos].isdigit():
                self.pos += 1
            self.tokens.append(Token("INT", self.src[start:self.pos], "INITIAL"))
            return
        single = {"=": "EQ", "+": "PLUS", ";": "SEMI"}
        if ch in single:
            self.pos += 1
            self.tokens.append(Token(single[ch], ch, "INITIAL"))
            return
        raise ValueError(f"unexpected character '{ch}' in INITIAL")

    def _step_comment(self) -> None:
        while self.pos < self.n:
            if self.src.startswith("/*", self.pos):
                self.comment_depth += 1
                self.pos += 2
                continue
            if self.src.startswith("*/", self.pos):
                self.comment_depth -= 1
                self.pos += 2
                if self.comment_depth == 0:
                    self.pop_mode("closed outermost '*/'")
                    return
                continue
            self.pos += 1

    def _step_string(self) -> None:
        decoded: list[str] = []
        while self.pos < self.n:
            ch = self.src[self.pos]
            if ch == '"':
                if decoded:
                    self.tokens.append(Token("STR_TEXT", "".join(decoded), "%x STRING"))
                self.pos += 1
                self.tokens.append(Token("STR_CLOSE", '"', "%x STRING"))
                self.pop_mode("saw closing '\"'")
                return
            if ch == "$" and self.pos + 1 < self.n and self.src[self.pos + 1] == "{":
                if decoded:
                    self.tokens.append(Token("STR_TEXT", "".join(decoded), "%x STRING"))
                self.pos += 2
                self.tokens.append(Token("INTERP_START", "${", "%x STRING"))
                self.interp_brace_depth.append(0)
                self.push_mode("INITIAL", "saw '${' -> push INITIAL for interpolated expr")
                return
            if ch == "\\":
                self.pos += 1
                if self.pos >= self.n:
                    raise ValueError("trailing backslash inside string")
                esc_map = {"n": "\n", "t": "\t", '"': '"', "\\": "\\", "$": "$"}
                decoded.append(esc_map.get(self.src[self.pos], self.src[self.pos]))
                self.pos += 1
                continue
            decoded.append(ch)
            self.pos += 1

    def _step_directive(self) -> None:
        start = self.pos
        while self.pos < self.n and self.src[self.pos] != "\n":
            self.pos += 1
        parts = self.src[start:self.pos].strip().split()
        if parts:
            self.tokens.append(Token("DIR_CMD", parts[0], "%x DIRECTIVE"))
            for arg in parts[1:]:
                self.tokens.append(Token("DIR_ARG", arg, "%x DIRECTIVE"))
        if self.pos < self.n and self.src[self.pos] == "\n":
            self.pos += 1
        self.pop_mode("newline closes directive")


def lex_with_modes(src: str) -> tuple[list[Token], list[ModeTransition]]:
    return ModalLexer(src).run()


def main() -> None:
    src = (
        "#define LIMIT 10\n"
        "/* outer /* nested comment */ still ignored */\n"
        'let msg = "sum=${a + "inner_${k}"}";'
    )
    tokens, transitions = lex_with_modes(src)
    print("=== Lexer Start Conditions (%x) and Contextual Mode Stack ===")
    print(f"Source:\n{src}\n")
    print(f"Mode transitions ({len(transitions)}):")
    for tr in transitions:
        print(f"  @{tr.at:02d} {tr.src_mode:<12} -> {tr.dst_mode:<12} ({tr.reason})")
    print(f"\nEmitted tokens ({len(tokens)}):")
    for tok in tokens:
        print(f"  [{tok.mode:<12}] {tok.kind:<12} {tok.lexeme!r}")


class LexerModesTests(unittest.TestCase):
    def test_supports_nested_block_comments_and_resumes_initial(self) -> None:
        src = "/* lvl1 /* lvl2 */ still comment */ let x = 1;"
        tokens, _ = lex_with_modes(src)
        self.assertEqual(
            [(t.kind, t.lexeme) for t in tokens],
            [("LET", "let"), ("IDENT", "x"), ("EQ", "="), ("INT", "1"), ("SEMI", ";")],
        )

    def test_supports_nested_string_interpolation_and_directives(self) -> None:
        src = '#pragma fast\nlet s = "a=${b + "c=${d}"}";'
        tokens, _ = lex_with_modes(src)
        self.assertEqual(
            [(t.kind, t.lexeme) for t in tokens],
            [
                ("DIR_CMD", "pragma"),
                ("DIR_ARG", "fast"),
                ("LET", "let"),
                ("IDENT", "s"),
                ("EQ", "="),
                ("STR_OPEN", '"'),
                ("STR_TEXT", "a="),
                ("INTERP_START", "${"),
                ("IDENT", "b"),
                ("PLUS", "+"),
                ("STR_OPEN", '"'),
                ("STR_TEXT", "c="),
                ("INTERP_START", "${"),
                ("IDENT", "d"),
                ("INTERP_END", "}"),
                ("STR_CLOSE", '"'),
                ("INTERP_END", "}"),
                ("STR_CLOSE", '"'),
                ("SEMI", ";"),
            ],
        )

    def test_rejects_unclosed_nested_comment(self) -> None:
        with self.assertRaises(ValueError):
            lex_with_modes("/* a /* b */")


if __name__ == "__main__":
    main()
