"""Replace repeated pure calculations within one basic block with copies."""

from dataclasses import dataclass
from enum import Enum
import unittest

I64_MASK = (1 << 64) - 1


def wrap_i64(value: int) -> int:
    masked = value & I64_MASK
    return masked - (1 << 64) if masked >= (1 << 63) else masked


class Op(Enum):
    ADD = "Add"
    SUB = "Sub"


@dataclass(frozen=True)
class Compute:
    op: Op
    left: int
    right: int


@dataclass(frozen=True)
class Copy:
    source: int


Expr = Compute | Copy


def number(inputs: int, code: list[Expr]) -> list[Expr]:
    values = list(range(inputs))
    known: dict[tuple[Op, int, int], int] = {}
    output: list[Expr] = []
    for expr in code:
        if isinstance(expr, Compute):
            left = values[expr.left]
            right = values[expr.right]
            if expr.op == Op.ADD and left > right:
                left, right = right, left
            fresh = len(values)
            key = (expr.op, left, right)
            previous = known.setdefault(key, fresh)
            if previous == fresh:
                result: Expr = Compute(expr.op, left, right)
            else:
                result = Copy(previous)
            representative = previous
        else:
            rep = values[expr.source]
            result = Copy(rep)
            representative = rep
        output.append(result)
        values.append(representative)
    return output


def execute(inputs: list[int], code: list[Expr]) -> list[int]:
    values = list(inputs)
    for expr in code:
        if isinstance(expr, Compute):
            if expr.op == Op.ADD:
                res = wrap_i64(values[expr.left] + values[expr.right])
            else:
                res = wrap_i64(values[expr.left] - values[expr.right])
        else:
            res = values[expr.source]
        values.append(res)
    return values


CODE: list[Expr] = [
    Compute(Op.ADD, 0, 1),
    Compute(Op.ADD, 1, 0),
    Compute(Op.SUB, 3, 0),
    Compute(Op.SUB, 0, 2),
    Compute(Op.SUB, 2, 0),
]


def main() -> None:
    optimized = number(2, CODE)
    print(f"Original: {CODE}")
    print(f"Optimized: {optimized}")
    print(f"Values: {execute([9, 4], optimized)}")


class ValueNumberingTests(unittest.TestCase):
    def test_combines_redundant_values_and_preserves_subtraction_order(self) -> None:
        optimized = number(2, CODE)
        self.assertEqual(optimized[1], Copy(2))
        self.assertEqual(optimized[3], Compute(Op.SUB, 0, 2))
        self.assertEqual(optimized[4], Copy(4))
        for inputs in ([9, 4], [4, -9], [(1 << 63) - 1, 1]):
            self.assertEqual(execute(inputs, optimized), execute(inputs, CODE))


if __name__ == "__main__":
    main()
