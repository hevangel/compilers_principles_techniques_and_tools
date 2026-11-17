"""Compile regular expressions into Thompson NFAs and direct followpos DFAs."""

from dataclasses import dataclass
import unittest


@dataclass(frozen=True)
class Empty:
    pass


@dataclass(frozen=True)
class Literal:
    ch: str
    pos: int


@dataclass(frozen=True)
class Cat:
    left: "RegexNode"
    right: "RegexNode"


@dataclass(frozen=True)
class Alt:
    left: "RegexNode"
    right: "RegexNode"


@dataclass(frozen=True)
class Star:
    inner: "RegexNode"


@dataclass(frozen=True)
class EndMarker:
    pos: int


RegexNode = Empty | Literal | Cat | Alt | Star | EndMarker


@dataclass(frozen=True)
class _RawEmpty:
    pass


@dataclass(frozen=True)
class _RawLit:
    ch: str


@dataclass(frozen=True)
class _RawCat:
    left: object
    right: object


@dataclass(frozen=True)
class _RawAlt:
    left: object
    right: object


@dataclass(frozen=True)
class _RawStar:
    inner: object


@dataclass(frozen=True)
class _RawPlus:
    inner: object


@dataclass(frozen=True)
class _RawOpt:
    inner: object


class _Parser:
    def __init__(self, pattern: str) -> None:
        self.pattern = pattern
        self.pos = 0

    def peek(self) -> str | None:
        return self.pattern[self.pos] if self.pos < len(self.pattern) else None

    def parse_alt(self) -> object:
        node = self.parse_cat()
        while self.peek() == "|":
            self.pos += 1
            node = _RawAlt(node, self.parse_cat())
        return node

    def parse_cat(self) -> object:
        factors: list[object] = []
        while (ch := self.peek()) is not None and ch not in ("|", ")"):
            factors.append(self.parse_postfix())
        if not factors:
            raise ValueError("empty alternative in regular expression")
        node = factors[0]
        for nxt in factors[1:]:
            node = _RawCat(node, nxt)
        return node

    def parse_postfix(self) -> object:
        node = self.parse_atom()
        while (op := self.peek()) in ("*", "+", "?"):
            self.pos += 1
            if op == "*":
                node = _RawStar(node)
            elif op == "+":
                node = _RawPlus(node)
            else:
                node = _RawOpt(node)
        return node

    def parse_atom(self) -> object:
        ch = self.peek()
        if ch is None:
            raise ValueError("unexpected end of regular expression")
        if ch == "(":
            self.pos += 1
            if self.peek() == ")":
                self.pos += 1
                return _RawEmpty()
            inner = self.parse_alt()
            if self.peek() != ")":
                raise ValueError("unclosed '(' in regular expression")
            self.pos += 1
            return inner
        if ch in ("*", "+", "?", "|", ")"):
            raise ValueError(f"unexpected operator at byte {self.pos}")
        if ch == "\\":
            self.pos += 1
            escaped = self.peek()
            if escaped is None:
                raise ValueError("trailing backslash in regular expression")
            self.pos += 1
            return _RawLit(escaped)
        self.pos += 1
        return _RawLit(ch)


def _assign_positions(raw: object, counter: list[int]) -> RegexNode:
    if isinstance(raw, _RawEmpty):
        return Empty()
    if isinstance(raw, _RawLit):
        pos = counter[0]
        counter[0] += 1
        return Literal(raw.ch, pos)
    if isinstance(raw, _RawCat):
        return Cat(_assign_positions(raw.left, counter), _assign_positions(raw.right, counter))
    if isinstance(raw, _RawAlt):
        return Alt(_assign_positions(raw.left, counter), _assign_positions(raw.right, counter))
    if isinstance(raw, _RawStar):
        return Star(_assign_positions(raw.inner, counter))
    if isinstance(raw, _RawPlus):
        return Cat(
            _assign_positions(raw.inner, counter),
            Star(_assign_positions(raw.inner, counter)),
        )
    assert isinstance(raw, _RawOpt)
    return Alt(_assign_positions(raw.inner, counter), Empty())


def parse_regex(pattern: str) -> RegexNode:
    if not pattern:
        return Empty()
    if not pattern.isascii():
        raise ValueError("only ASCII regular expressions are supported")
    parser = _Parser(pattern)
    raw = parser.parse_alt()
    if parser.pos != len(pattern):
        raise ValueError(f"unexpected trailing character at byte {parser.pos}")
    return _assign_positions(raw, [1])


class Nfa:
    def __init__(self) -> None:
        self.transitions: list[list[tuple[str | None, int]]] = []
        self.start = 0
        self.accept = 0

    def new_state(self) -> int:
        idx = len(self.transitions)
        self.transitions.append([])
        return idx

    def build(self, node: RegexNode) -> tuple[int, int]:
        if isinstance(node, Empty):
            s, t = self.new_state(), self.new_state()
            self.transitions[s].append((None, t))
            return s, t
        if isinstance(node, Literal):
            s, t = self.new_state(), self.new_state()
            self.transitions[s].append((node.ch, t))
            return s, t
        if isinstance(node, Cat):
            s1, t1 = self.build(node.left)
            s2, t2 = self.build(node.right)
            self.transitions[t1].append((None, s2))
            return s1, t2
        if isinstance(node, Alt):
            s = self.new_state()
            s1, t1 = self.build(node.left)
            s2, t2 = self.build(node.right)
            t = self.new_state()
            self.transitions[s].extend([(None, s1), (None, s2)])
            self.transitions[t1].append((None, t))
            self.transitions[t2].append((None, t))
            return s, t
        assert isinstance(node, Star)
        s = self.new_state()
        s1, t1 = self.build(node.inner)
        t = self.new_state()
        self.transitions[s].extend([(None, s1), (None, t)])
        self.transitions[t1].extend([(None, s1), (None, t)])
        return s, t

    def eps_closure(self, seeds: set[int]) -> set[int]:
        closure = set(seeds)
        stack = list(seeds)
        while stack:
            state = stack.pop()
            for sym, dst in self.transitions[state]:
                if sym is None and dst not in closure:
                    closure.add(dst)
                    stack.append(dst)
        return closure

    def accepts(self, text: str) -> bool:
        current = self.eps_closure({self.start})
        for ch in text:
            moved = {
                dst
                for state in current
                for sym, dst in self.transitions[state]
                if sym == ch
            }
            current = self.eps_closure(moved)
        return self.accept in current


def to_thompson_nfa(node: RegexNode) -> Nfa:
    nfa = Nfa()
    nfa.start, nfa.accept = nfa.build(node)
    return nfa


@dataclass(frozen=True)
class Dfa:
    start: int
    alphabet: list[str]
    accepting: frozenset[int]
    trans: list[list[int]]

    def accepts(self, text: str) -> bool:
        state = self.start
        for ch in text:
            if ch not in self.alphabet:
                return False
            state = self.trans[state][self.alphabet.index(ch)]
        return state in self.accepting


def _max_pos(node: RegexNode) -> int:
    if isinstance(node, Empty):
        return 0
    if isinstance(node, (Literal, EndMarker)):
        return node.pos
    if isinstance(node, (Cat, Alt)):
        return max(_max_pos(node.left), _max_pos(node.right))
    return _max_pos(node.inner)


def _collect_positions(
    node: RegexNode,
    pos_char: dict[int, str],
    followpos: dict[int, set[int]],
) -> tuple[bool, set[int], set[int]]:
    if isinstance(node, Empty):
        return True, set(), set()
    if isinstance(node, Literal):
        pos_char[node.pos] = node.ch
        followpos.setdefault(node.pos, set())
        return False, {node.pos}, {node.pos}
    if isinstance(node, EndMarker):
        followpos.setdefault(node.pos, set())
        return False, {node.pos}, {node.pos}
    if isinstance(node, Alt):
        ln, lf, ll = _collect_positions(node.left, pos_char, followpos)
        rn, rf, rl = _collect_positions(node.right, pos_char, followpos)
        return ln or rn, lf | rf, ll | rl
    if isinstance(node, Cat):
        ln, lf, ll = _collect_positions(node.left, pos_char, followpos)
        rn, rf, rl = _collect_positions(node.right, pos_char, followpos)
        for p in ll:
            followpos.setdefault(p, set()).update(rf)
        firstpos = set(lf) | (rf if ln else set())
        lastpos = set(rl) | (ll if rn else set())
        return ln and rn, firstpos, lastpos
    assert isinstance(node, Star)
    _, inner_f, inner_l = _collect_positions(node.inner, pos_char, followpos)
    for p in inner_l:
        followpos.setdefault(p, set()).update(inner_f)
    return True, inner_f, inner_l


def compile_direct_dfa(node: RegexNode) -> Dfa:
    end_pos = _max_pos(node) + 1
    augmented = Cat(node, EndMarker(end_pos))
    pos_char: dict[int, str] = {}
    followpos: dict[int, set[int]] = {}
    _, root_first, _ = _collect_positions(augmented, pos_char, followpos)

    alphabet = sorted(set(pos_char.values()))
    state_sets = [frozenset(root_first)]
    state_index = {state_sets[0]: 0}
    accepting: set[int] = set()
    trans: list[list[int]] = []

    cursor = 0
    while cursor < len(state_sets):
        current = state_sets[cursor]
        if end_pos in current:
            accepting.add(cursor)
        row: list[int] = []
        for sym in alphabet:
            next_set: set[int] = set()
            for p in current:
                if pos_char.get(p) == sym:
                    next_set |= followpos.get(p, set())
            frozen = frozenset(next_set)
            if frozen not in state_index:
                state_index[frozen] = len(state_sets)
                state_sets.append(frozen)
            row.append(state_index[frozen])
        trans.append(row)
        cursor += 1

    return Dfa(start=0, alphabet=alphabet, accepting=frozenset(accepting), trans=trans)


def main() -> None:
    pattern = "(a|b)*abb"
    ast = parse_regex(pattern)
    nfa = to_thompson_nfa(ast)
    dfa = compile_direct_dfa(ast)
    print(
        f"Pattern {pattern}: Thompson NFA has {len(nfa.transitions)} states, "
        f"direct DFA has {len(dfa.trans)} states"
    )
    for word in ["abb", "aabb", "babb", "ab", "abbc", ""]:
        print(f"  {word!r}: NFA={nfa.accepts(word)}, DFA={dfa.accepts(word)}")


class RegexAutomataTests(unittest.TestCase):
    def test_thompson_nfa_and_direct_dfa_agree_across_patterns(self) -> None:
        cases = [
            ("(a|b)*abb", ["a", "b"]),
            ("a(b|c)*", ["a", "b", "c"]),
            ("a+b?", ["a", "b"]),
            ("()", ["a"]),
            ("", ["a"]),
        ]
        for pattern, alpha in cases:
            ast = parse_regex(pattern)
            nfa = to_thompson_nfa(ast)
            dfa = compile_direct_dfa(ast)
            words = [""]
            for _ in range(5):
                for w in words:
                    self.assertEqual(nfa.accepts(w), dfa.accepts(w), f"{pattern}: {w}")
                words = [w + ch for w in words for ch in alpha]
        dfa = compile_direct_dfa(parse_regex("(a|b)*abb"))
        self.assertTrue(dfa.accepts("abb"))
        self.assertTrue(dfa.accepts("ababb"))
        self.assertFalse(dfa.accepts("aba"))
        self.assertFalse(dfa.accepts("abbc"))

    def test_rejects_malformed_regexes(self) -> None:
        for bad in ["(ab", "*a", "a||b", "a)", "a|", "|a", "a\\"]:
            with self.assertRaises(ValueError, msg=bad):
                parse_regex(bad)


if __name__ == "__main__":
    main()
