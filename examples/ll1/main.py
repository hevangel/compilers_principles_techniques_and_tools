"""Compute FIRST and FOLLOW, then use the resulting LL(1) prediction table."""

from dataclasses import dataclass
import unittest


@dataclass(frozen=True)
class Terminal:
    ch: str


@dataclass(frozen=True)
class Rule:
    index: int


Symbol = Terminal | Rule
Grammar = list[tuple[int, list[Symbol]]]

GRAMMAR: Grammar = [
    (0, [Terminal("["), Rule(0), Terminal("]"), Rule(0)]),
    (0, []),
]


class PredictionCollisionError(ValueError):
    def __init__(self, lhs: int, token: str) -> None:
        super().__init__(f"LL(1) collision at rule {lhs}, token {token!r}")
        self.lhs = lhs
        self.token = token


def first_of(sequence: list[Symbol], first: list[tuple[set[str], bool]]) -> tuple[set[str], bool]:
    result: set[str] = set()
    for symbol in sequence:
        if isinstance(symbol, Terminal):
            result.add(symbol.ch)
            return result, False
        tokens, nullable = first[symbol.index]
        result |= tokens
        if not nullable:
            return result, False
    return result, True


def predict(grammar: Grammar, rules: int) -> dict[tuple[int, str], int]:
    first: list[tuple[set[str], bool]] = [(set(), False) for _ in range(rules)]
    while True:
        previous = [(set(t), n) for t, n in first]
        for lhs, rhs in grammar:
            tokens, nullable = first_of(rhs, first)
            first[lhs] = (first[lhs][0] | tokens, first[lhs][1] or nullable)
        if first == previous:
            break

    follow: list[set[str]] = [set() for _ in range(rules)]
    follow[0].add("$")
    while True:
        previous_follow = [set(s) for s in follow]
        for lhs, rhs in grammar:
            for idx, symbol in enumerate(rhs):
                if isinstance(symbol, Rule):
                    tokens, nullable = first_of(rhs[idx + 1 :], first)
                    if nullable:
                        tokens = tokens | follow[lhs]
                    follow[symbol.index] |= tokens
        if follow == previous_follow:
            break

    table: dict[tuple[int, str], int] = {}
    for idx, (lhs, rhs) in enumerate(grammar):
        tokens, nullable = first_of(rhs, first)
        if nullable:
            tokens = tokens | follow[lhs]
        for token in sorted(tokens):
            key = (lhs, token)
            if key in table:
                raise PredictionCollisionError(lhs, token)
            table[key] = idx
    return table


def parse(text: str, grammar: Grammar, table: dict[tuple[int, str], int]) -> bool:
    tokens = list(text) + ["$"]
    pos = 0
    stack: list[Symbol] = [Terminal("$"), Rule(0)]
    while stack:
        symbol = stack.pop()
        if isinstance(symbol, Terminal):
            if pos < len(tokens) and tokens[pos] == symbol.ch:
                pos += 1
            else:
                return False
        else:
            if pos >= len(tokens) or (symbol.index, tokens[pos]) not in table:
                return False
            prod_idx = table[(symbol.index, tokens[pos])]
            stack.extend(reversed(grammar[prod_idx][1]))
    return pos == len(tokens)


def main() -> None:
    table = predict(GRAMMAR, 1)
    print(f"Prediction table: {table}")
    for text in ["[[]][]", "[][", ""]:
        print(f"{text!r}: {parse(text, GRAMMAR, table)}")


class Ll1Tests(unittest.TestCase):
    def test_predicts_nullable_rule_from_follow(self) -> None:
        table = predict(GRAMMAR, 1)
        self.assertEqual(table.get((0, "]")), 1)
        self.assertTrue(parse("[[]][]", GRAMMAR, table))
        self.assertFalse(parse("[]]", GRAMMAR, table))
        self.assertFalse(parse("[]$", GRAMMAR, table))

    def test_reports_prediction_collision(self) -> None:
        with self.assertRaises(PredictionCollisionError) as ctx:
            predict([(0, [Terminal("x")]), (0, [Terminal("x"), Rule(0)])], 1)
        self.assertEqual((ctx.exception.lhs, ctx.exception.token), (0, "x"))


if __name__ == "__main__":
    main()
