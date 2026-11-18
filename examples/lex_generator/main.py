"""Lex-style `.l` scanner-generator DSL compiler: expands macro definitions, builds a unified
multi-pattern Thompson NFA -> DFA with longest-match + rule-priority tie-breaking and `/`
trailing-context lookahead, and executes the generated scanner on sample source text.
"""

from collections import deque
from dataclasses import dataclass
import unittest


# ---------------------------------------------------------------------------
# 1. Finite Byte Domain & Regex AST
# ---------------------------------------------------------------------------

def all_alphabet_bytes() -> frozenset[int]:
    """Finite byte alphabet for generated DFA rows: `\\t`, `\\n`, and printable ASCII."""
    chars = {ord("\t"), ord("\n")}
    chars.update(range(0x20, 0x7F))
    return frozenset(chars)


ALPHABET: frozenset[int] = all_alphabet_bytes()


@dataclass(frozen=True)
class RegexAst:
    kind: str  # "empty", "charset", "cat", "alt", "star"
    charset: frozenset[int] = frozenset()
    left: "RegexAst | None" = None
    right: "RegexAst | None" = None


# ---------------------------------------------------------------------------
# 2. `.l` Specification Parser (`defs %% rules %% subroutines`)
# ---------------------------------------------------------------------------

@dataclass(frozen=True)
class LexRuleSpec:
    rule_id: int
    raw_pattern: str
    action: str
    head_ast: RegexAst
    tail_ast: RegexAst | None


@dataclass(frozen=True)
class LexSpec:
    defs: dict[str, str]
    rules: list[LexRuleSpec]
    user_code: str


def expand_macros(pattern: str, defs: dict[str, str], visiting: set[str]) -> str:
    """Expand `{name}` references inside a Lex pattern, wrapping each substitution in `(...)`."""
    out: list[str] = []
    i = 0
    n = len(pattern)
    in_quotes = False
    in_class = False

    while i < n:
        ch = pattern[i]
        if ch == "\\" and i + 1 < n:
            out.append(pattern[i : i + 2])
            i += 2
            continue
        if not in_class and ch == '"':
            in_quotes = not in_quotes
            out.append(ch)
            i += 1
            continue
        if not in_quotes and ch == "[":
            in_class = True
            out.append(ch)
            i += 1
            continue
        if in_class and ch == "]":
            in_class = False
            out.append(ch)
            i += 1
            continue
        if not in_quotes and not in_class and ch == "{":
            end = pattern.find("}", i + 1)
            if end == -1:
                raise ValueError(f"unclosed '{{macro}}' in pattern: {pattern}")
            name = pattern[i + 1 : end]
            if name not in defs:
                raise ValueError(f"undefined Lex macro: {{{name}}}")
            if name in visiting:
                raise ValueError(f"cyclic macro definition: {{{name}}}")
            visiting.add(name)
            expanded = expand_macros(defs[name], defs, visiting)
            visiting.remove(name)
            out.append(f"({expanded})")
            i = end + 1
            continue
        out.append(ch)
        i += 1
    return "".join(out)


def split_trailing_context(pattern: str) -> tuple[str, str | None]:
    """Split an expanded pattern at a top-level, unquoted `/` lookahead operator."""
    i = 0
    n = len(pattern)
    in_quotes = False
    in_class = False
    paren_depth = 0
    split_at: int | None = None

    while i < n:
        ch = pattern[i]
        if ch == "\\" and i + 1 < n:
            i += 2
            continue
        if not in_class and ch == '"':
            in_quotes = not in_quotes
            i += 1
            continue
        if not in_quotes and ch == "[":
            in_class = True
            i += 1
            continue
        if in_class and ch == "]":
            in_class = False
            i += 1
            continue
        if not in_quotes and not in_class:
            if ch == "(":
                paren_depth += 1
            elif ch == ")":
                if paren_depth == 0:
                    raise ValueError(f"unmatched ')' in pattern: {pattern}")
                paren_depth -= 1
            elif ch == "/":
                if paren_depth != 0:
                    raise ValueError(f"trailing context '/' not allowed inside parentheses: {pattern}")
                if split_at is not None:
                    raise ValueError(f"multiple trailing-context '/' operators in rule: {pattern}")
                split_at = i
        i += 1

    if split_at is not None:
        return pattern[:split_at], pattern[split_at + 1 :]
    return pattern, None


class LexRegexParser:
    def __init__(self, src: str) -> None:
        self.bytes = src.encode("utf-8")
        self.pos = 0

    def peek(self) -> int | None:
        return self.bytes[self.pos] if self.pos < len(self.bytes) else None

    def decode_escape(self) -> int:
        if self.pos >= len(self.bytes):
            raise ValueError("dangling '\\' escape at end of regex")
        ch = self.bytes[self.pos]
        self.pos += 1
        if ch == ord("n"):
            return ord("\n")
        if ch == ord("t"):
            return ord("\t")
        return ch

    def parse(self) -> RegexAst:
        if not self.bytes:
            raise ValueError("empty regex pattern")
        ast = self.parse_alt()
        if self.pos < len(self.bytes):
            raise ValueError(f"unexpected trailing regex token '{chr(self.bytes[self.pos])}'")
        return ast

    def parse_alt(self) -> RegexAst:
        node = self.parse_cat()
        while self.peek() == ord("|"):
            self.pos += 1
            rhs = self.parse_cat()
            node = RegexAst("alt", left=node, right=rhs)
        return node

    def parse_cat(self) -> RegexAst:
        items: list[RegexAst] = []
        while (ch := self.peek()) is not None:
            if ch in (ord("|"), ord(")")):
                break
            items.append(self.parse_postfix())
        if not items:
            return RegexAst("empty")
        node = items[0]
        for nxt in items[1:]:
            node = RegexAst("cat", left=node, right=nxt)
        return node

    def parse_postfix(self) -> RegexAst:
        node = self.parse_atom()
        while (op := self.peek()) is not None:
            if op == ord("*"):
                self.pos += 1
                node = RegexAst("star", left=node)
            elif op == ord("+"):
                self.pos += 1
                node = RegexAst("cat", left=node, right=RegexAst("star", left=node))
            elif op == ord("?"):
                self.pos += 1
                node = RegexAst("alt", left=node, right=RegexAst("empty"))
            else:
                break
        return node

    def parse_atom(self) -> RegexAst:
        ch = self.peek()
        if ch is None:
            raise ValueError("unexpected end of regex atom")
        if ch in (ord("*"), ord("+"), ord("?"), ord("|"), ord(")")):
            raise ValueError(f"unexpected operator '{chr(ch)}' in regex")
        if ch == ord("("):
            self.pos += 1
            inner = self.parse_alt()
            if self.peek() != ord(")"):
                raise ValueError("unclosed '(' in regex")
            self.pos += 1
            return inner
        if ch == ord('"'):
            self.pos += 1
            parts: list[RegexAst] = []
            while (q := self.peek()) is not None:
                if q == ord('"'):
                    self.pos += 1
                    if not parts:
                        return RegexAst("empty")
                    node = parts[0]
                    for nxt in parts[1:]:
                        node = RegexAst("cat", left=node, right=nxt)
                    return node
                if q == ord("\\"):
                    self.pos += 1
                    byte = self.decode_escape()
                else:
                    self.pos += 1
                    byte = q
                parts.append(RegexAst("charset", charset=frozenset({byte})))
            raise ValueError('unterminated quoted literal "..." in Lex rule')
        if ch == ord("["):
            return self.parse_char_class()
        if ch == ord("."):
            self.pos += 1
            return RegexAst("charset", charset=ALPHABET - {ord("\n")})
        if ch == ord("\\"):
            self.pos += 1
            byte = self.decode_escape()
            return RegexAst("charset", charset=frozenset({byte}))
        self.pos += 1
        return RegexAst("charset", charset=frozenset({ch}))

    def parse_char_class(self) -> RegexAst:
        self.pos += 1
        negated = self.peek() == ord("^")
        if negated:
            self.pos += 1
        members: set[int] = set()
        prev: int | None = None
        closed = False

        while self.pos < len(self.bytes):
            ch = self.bytes[self.pos]
            if ch == ord("]") and (prev is not None or members):
                self.pos += 1
                closed = True
                break
            if (
                ch == ord("-")
                and prev is not None
                and self.pos + 1 < len(self.bytes)
                and self.bytes[self.pos + 1] != ord("]")
            ):
                self.pos += 1
                if self.bytes[self.pos] == ord("\\"):
                    self.pos += 1
                    end_b = self.decode_escape()
                else:
                    end_b = self.bytes[self.pos]
                    self.pos += 1
                if prev > end_b:
                    raise ValueError(f"inverted character range {chr(prev)}-{chr(end_b)}")
                members.update(range(prev, end_b + 1))
                prev = None
                continue
            if ch == ord("\\"):
                self.pos += 1
                byte = self.decode_escape()
            else:
                self.pos += 1
                byte = ch
            members.add(byte)
            prev = byte

        if not closed:
            raise ValueError("unclosed character class [...]")
        final_set = (ALPHABET - members) if negated else frozenset(members)
        return RegexAst("charset", charset=final_set)


def split_rule_line(line: str) -> tuple[str, str]:
    i = 0
    n = len(line)
    in_quotes = False
    in_class = False
    while i < n:
        ch = line[i]
        if ch == "\\" and i + 1 < n:
            i += 2
            continue
        if not in_class and ch == '"':
            in_quotes = not in_quotes
            i += 1
            continue
        if not in_quotes and ch == "[":
            in_class = True
            i += 1
            continue
        if in_class and ch == "]":
            in_class = False
            i += 1
            continue
        if not in_quotes and not in_class and ch.isspace():
            pattern = line[:i]
            action = line[i:].strip()
            if action.startswith("{") and action.endswith("}") and len(action) >= 2:
                action = action[1:-1].strip()
            if not pattern or not action:
                raise ValueError(f"invalid rule line: {line}")
            return pattern, action
        i += 1
    raise ValueError(f"rule missing action: {line}")


def parse_lex_spec(src: str) -> LexSpec:
    sections: list[list[str]] = [[]]
    for raw_line in src.splitlines():
        if raw_line.strip() == "%%":
            sections.append([])
        else:
            sections[-1].append(raw_line)
    if len(sections) < 2:
        raise ValueError("Lex specification requires at least one '%%' section divider")

    defs: dict[str, str] = {}
    for line in sections[0]:
        trimmed = line.strip()
        if not trimmed or trimmed.startswith("//") or trimmed.startswith("#"):
            continue
        parts = trimmed.split(None, 1)
        if len(parts) != 2 or not parts[1].strip():
            raise ValueError(f"invalid definition line: {trimmed}")
        defs[parts[0].strip()] = parts[1].strip()

    rules: list[LexRuleSpec] = []
    for line in sections[1]:
        trimmed = line.strip()
        if not trimmed or trimmed.startswith("//"):
            continue
        raw_pat, action = split_rule_line(trimmed)
        expanded = expand_macros(raw_pat, defs, set())
        head_str, tail_str = split_trailing_context(expanded)
        head_ast = LexRegexParser(head_str).parse()
        tail_ast = LexRegexParser(tail_str).parse() if tail_str is not None else None
        rules.append(LexRuleSpec(len(rules), raw_pat, action, head_ast, tail_ast))

    if not rules:
        raise ValueError("Lex rules section contains zero rules")

    user_code = "\n".join(sections[2]) if len(sections) >= 3 else ""
    return LexSpec(defs, rules, user_code)


# ---------------------------------------------------------------------------
# 3. Combined NFA & Subset-Construction Multi-Pattern DFA
# ---------------------------------------------------------------------------

class Nfa:
    def __init__(self) -> None:
        self.eps: dict[int, set[int]] = {}
        self.trans: dict[int, list[tuple[frozenset[int], int]]] = {}
        self.accept_rule: dict[int, int] = {}
        self.next_state = 0

    def new_state(self) -> int:
        s = self.next_state
        self.next_state += 1
        return s

    def add_eps(self, src: int, dst: int) -> None:
        self.eps.setdefault(src, set()).add(dst)

    def build(self, ast: RegexAst) -> tuple[int, int]:
        if ast.kind == "empty":
            s, e = self.new_state(), self.new_state()
            self.add_eps(s, e)
            return s, e
        if ast.kind == "charset":
            s, e = self.new_state(), self.new_state()
            self.trans.setdefault(s, []).append((ast.charset, e))
            return s, e
        if ast.kind == "cat":
            assert ast.left is not None and ast.right is not None
            s1, e1 = self.build(ast.left)
            s2, e2 = self.build(ast.right)
            self.add_eps(e1, s2)
            return s1, e2
        if ast.kind == "alt":
            assert ast.left is not None and ast.right is not None
            s, e = self.new_state(), self.new_state()
            s1, e1 = self.build(ast.left)
            s2, e2 = self.build(ast.right)
            self.add_eps(s, s1)
            self.add_eps(s, s2)
            self.add_eps(e1, e)
            self.add_eps(e2, e)
            return s, e
        assert ast.left is not None
        s, e = self.new_state(), self.new_state()
        s1, e1 = self.build(ast.left)
        self.add_eps(s, s1)
        self.add_eps(s, e)
        self.add_eps(e1, s1)
        self.add_eps(e1, e)
        return s, e

    def eps_closure(self, seeds: set[int] | frozenset[int]) -> frozenset[int]:
        closure = set(seeds)
        work = list(seeds)
        while work:
            st = work.pop()
            for nxt in self.eps.get(st, ()):
                if nxt not in closure:
                    closure.add(nxt)
                    work.append(nxt)
        return frozenset(closure)

    def move_byte(self, states: frozenset[int], byte: int) -> set[int]:
        out: set[int] = set()
        for st in states:
            for chars, tgt in self.trans.get(st, ()):
                if byte in chars:
                    out.add(tgt)
        return out


@dataclass
class Dfa:
    start: int
    trans: list[dict[int, int]]
    accept_rule: list[int | None]

    @classmethod
    def from_nfa(cls, nfa: Nfa, root: int, alphabet: frozenset[int]) -> "Dfa":
        init_set = nfa.eps_closure({root})
        state_ids: dict[frozenset[int], int] = {init_set: 0}
        queue: deque[tuple[int, frozenset[int]]] = deque([(0, init_set)])
        trans: list[dict[int, int]] = [{}]
        accept_rule: list[int | None] = [None]
        sorted_bytes = sorted(alphabet)

        while queue:
            sid, cur_set = queue.popleft()
            winners = [nfa.accept_rule[ns] for ns in cur_set if ns in nfa.accept_rule]
            accept_rule[sid] = min(winners) if winners else None

            for byte in sorted_bytes:
                moved = nfa.move_byte(cur_set, byte)
                if not moved:
                    continue
                next_set = nfa.eps_closure(moved)
                nid = state_ids.get(next_set)
                if nid is None:
                    nid = len(trans)
                    state_ids[next_set] = nid
                    queue.append((nid, next_set))
                    trans.append({})
                    accept_rule.append(None)
                trans[sid][byte] = nid

        return cls(0, trans, accept_rule)

    def matches_exact(self, data: bytes) -> bool:
        st = self.start
        for b in data:
            nxt = self.trans[st].get(b)
            if nxt is None:
                return False
            st = nxt
        return self.accept_rule[st] is not None


@dataclass
class CompiledScanner:
    spec: LexSpec
    nfa_state_count: int
    combined_dfa: Dfa
    lookahead_dfas: dict[int, tuple[Dfa, Dfa]]


def compile_single_regex_dfa(ast: RegexAst) -> Dfa:
    nfa = Nfa()
    root = nfa.new_state()
    s, e = nfa.build(ast)
    nfa.add_eps(root, s)
    nfa.accept_rule[e] = 0
    return Dfa.from_nfa(nfa, root, ALPHABET)


def compile_lex_spec(spec_src: str) -> CompiledScanner:
    spec = parse_lex_spec(spec_src)
    nfa = Nfa()
    root = nfa.new_state()
    lookahead_dfas: dict[int, tuple[Dfa, Dfa]] = {}

    for rule in spec.rules:
        full_ast = (
            RegexAst("cat", left=rule.head_ast, right=rule.tail_ast)
            if rule.tail_ast is not None
            else rule.head_ast
        )
        rs, re = nfa.build(full_ast)
        nfa.add_eps(root, rs)
        nfa.accept_rule[re] = rule.rule_id
        if rule.tail_ast is not None:
            lookahead_dfas[rule.rule_id] = (
                compile_single_regex_dfa(rule.head_ast),
                compile_single_regex_dfa(rule.tail_ast),
            )

    return CompiledScanner(
        spec=spec,
        nfa_state_count=nfa.next_state,
        combined_dfa=Dfa.from_nfa(nfa, root, ALPHABET),
        lookahead_dfas=lookahead_dfas,
    )


# ---------------------------------------------------------------------------
# 4. Generated Scanner Driver (Maximal Munch + Priority + `/` Retract)
# ---------------------------------------------------------------------------

@dataclass(frozen=True)
class EmittedToken:
    rule_id: int
    action: str
    lexeme: str
    line: int

    def __str__(self) -> str:
        return f"line {self.line}: {self.action} ({self.lexeme!r}) [rule #{self.rule_id}]"


def scan_with_compiled(compiled: CompiledScanner, text: str) -> list[EmittedToken]:
    data = text.encode("utf-8")
    pos = 0
    line = 1
    tokens: list[EmittedToken] = []

    while pos < len(data):
        st = compiled.combined_dfa.start
        cursor = pos
        best: tuple[int, int] | None = None

        while cursor < len(data):
            nxt = compiled.combined_dfa.trans[st].get(data[cursor])
            if nxt is None:
                break
            st = nxt
            cursor += 1
            rid = compiled.combined_dfa.accept_rule[st]
            if rid is not None:
                best = (cursor, rid)

        if best is None:
            raise ValueError(f"line {line}: lexical error at byte '{chr(data[pos])}'")

        match_end, rule_id = best
        if rule_id in compiled.lookahead_dfas:
            head_dfa, tail_dfa = compiled.lookahead_dfas[rule_id]
            found_split: int | None = None
            for split in range(match_end, pos - 1, -1):
                if head_dfa.matches_exact(data[pos:split]) and tail_dfa.matches_exact(
                    data[split:match_end]
                ):
                    found_split = split
                    break
            if found_split is None:
                raise ValueError(f"internal error: failed trailing-context split for rule #{rule_id}")
            if found_split == pos:
                raise ValueError(f"line {line}: zero-length head match in rule #{rule_id}")
            match_end = found_split

        lexeme = data[pos:match_end].decode("utf-8")
        action = compiled.spec.rules[rule_id].action
        if action != "SKIP":
            tokens.append(EmittedToken(rule_id, action, lexeme, line))
        line += lexeme.count("\n")
        pos = match_end

    return tokens


# ---------------------------------------------------------------------------
# 5. Worked Demo & Tests
# ---------------------------------------------------------------------------

SAMPLE_LEX_SPEC = r"""
digit   [0-9]
letter  [A-Za-z_]
id      {letter}({letter}|{digit})*
ws      [ \t\n]+

%%
{ws}                     { SKIP }
"while"                  { WHILE }
{id}                     { IDENT }
{digit}+/".."            { INT }
{digit}+"."{digit}*      { REAL }
{digit}+                 { INT }
".."                     { RANGE }
"+"                      { PLUS }
"="                      { ASSIGN }
";"                      { SEMI }
.                        { UNKNOWN }
%%
// User subroutine section preserved verbatim by the generator.
"""

SAMPLE_INPUT = "while while1 = 1..4 + 12.5 + 3.;\n"


def main() -> None:
    compiled = compile_lex_spec(SAMPLE_LEX_SPEC)
    print(
        f"Lex specification compiled: {len(compiled.spec.defs)} definitions, "
        f"{len(compiled.spec.rules)} rules, {compiled.nfa_state_count} NFA states -> "
        f"{len(compiled.combined_dfa.trans)} combined DFA states"
    )
    print("Rule table (priority = lowest rule index):")
    for r in compiled.spec.rules:
        la = " [trailing context /]" if r.tail_ast is not None else ""
        print(f"  #{r.rule_id} {r.raw_pattern:<22} -> {r.action}{la}")
    print(f"\nUser code footer: {compiled.spec.user_code.strip()}")

    tokens = scan_with_compiled(compiled, SAMPLE_INPUT)
    print(f"\nScanned tokens for {SAMPLE_INPUT!r}:")
    for tok in tokens:
        print(f"  {tok}")


class LexGeneratorTests(unittest.TestCase):
    def test_longest_match_rule_priority_and_trailing_context(self) -> None:
        compiled = compile_lex_spec(SAMPLE_LEX_SPEC)
        tokens = scan_with_compiled(compiled, SAMPLE_INPUT)
        summary = [(t.action, t.lexeme) for t in tokens]
        self.assertEqual(
            summary,
            [
                ("WHILE", "while"),
                ("IDENT", "while1"),
                ("ASSIGN", "="),
                ("INT", "1"),
                ("RANGE", ".."),
                ("INT", "4"),
                ("PLUS", "+"),
                ("REAL", "12.5"),
                ("PLUS", "+"),
                ("REAL", "3."),
                ("SEMI", ";"),
            ],
        )

    def test_negated_char_classes_and_bad_specs(self) -> None:
        str_spec = r"""
%%
\"[^"\n]*\"  { STRING }
[ \t]+       { SKIP }
%%
"""
        compiled = compile_lex_spec(str_spec)
        toks = scan_with_compiled(compiled, '  "hello world"  ')
        self.assertEqual(len(toks), 1)
        self.assertEqual(toks[0].action, "STRING")
        self.assertEqual(toks[0].lexeme, '"hello world"')

        with self.assertRaisesRegex(ValueError, "section divider"):
            compile_lex_spec("no divider here")
        with self.assertRaisesRegex(ValueError, "cyclic macro"):
            compile_lex_spec("a {a}\n%%\n{a} { TOK }\n%%")
        with self.assertRaisesRegex(ValueError, "multiple trailing-context"):
            compile_lex_spec("%%\na/b/c { TOK }\n%%")


if __name__ == "__main__":
    main()
