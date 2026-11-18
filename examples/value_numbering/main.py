"""Replace repeated pure calculations within one basic block with copies."""

from dataclasses import dataclass
from enum import Enum
import unittest

I64_MASK = (1 << 64) - 1


def wrap_i64(value: int) -> int:
    """Simulate signed 64-bit two's-complement wraparound (matching Rust `wrapping_add`/`sub`)."""
    masked = value & I64_MASK
    return masked - (1 << 64) if masked >= (1 << 63) else masked


class Op(Enum):
    """Pure binary arithmetic operators supported inside the basic block."""

    ADD = "Add"
    SUB = "Sub"


@dataclass(frozen=True)
class Compute:
    """Binary operation whose implicit destination index is `inputs + instruction_index`."""

    op: Op
    left: int
    right: int


@dataclass(frozen=True)
class Copy:
    """Copy from an earlier temporary index."""

    source: int


Expr = Compute | Copy


def number(inputs: int, code: list[Expr]) -> list[Expr]:
    """Run Local Value Numbering (LVN) over a straight-line basic block.

    `values[i]` maps temporary `i` to its canonical representative (the earliest
    temporary guaranteed to hold the same runtime value). `known` indexes each
    canonical `(op, left_rep, right_rep)` key to its first computing temporary.
    """
    values = list(range(inputs))
    known: dict[tuple[Op, int, int], int] = {}
    output: list[Expr] = []
    for expr in code:
        if isinstance(expr, Compute):
            # Resolve operands to their canonical value numbers *before* lookup so
            # copies like `t3 = Copy(2)` expose `t3 - t0` == `t2 - t0` downstream.
            left = values[expr.left]
            right = values[expr.right]
            # Soundness: wrapping addition is commutative even on overflow, so sorting
            # operands merges `a + b` and `b + a`. Subtraction is NOT commutative.
            if expr.op == Op.ADD and left > right:
                left, right = right, left
            fresh = len(values)
            key = (expr.op, left, right)
            # Claim `fresh` if unseen; otherwise reuse the earlier representative.
            previous = known.setdefault(key, fresh)
            if previous == fresh:
                result: Expr = Compute(expr.op, left, right)
            else:
                result = Copy(previous)
            representative = previous
        else:
            # Propagate the copied source's representative directly.
            rep = values[expr.source]
            result = Copy(rep)
            representative = rep
        output.append(result)
        # Preserve original destination slot numbering while tracking its value class.
        values.append(representative)
    return output


def execute(inputs: list[int], code: list[Expr]) -> list[int]:
    """Reference interpreter returning the value produced at every temporary index."""
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


# Straight-line sample:
# t2 = t0 + t1; t3 = t1 + t0 (redundant); t4 = t3 - t0; t5 = t0 - t2 (distinct); t6 = t2 - t0 (matches t4).
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
        # Commutative canonicalization folds `1 + 0` into `Copy(2)`.
        self.assertEqual(optimized[1], Copy(2))
        # Non-commutative `0 - 2` remains distinct from `2 - 0` (`4`).
        self.assertEqual(optimized[3], Compute(Op.SUB, 0, 2))
        # `2 - 0` hits `known[(SUB, 2, 0)] = 4` because `values[3] == values[2] == 2`.
        self.assertEqual(optimized[4], Copy(4))
        # Check semantic equivalence across normal and 64-bit signed overflow inputs.
        for inputs in ([9, 4], [4, -9], [(1 << 63) - 1, 1]):
            self.assertEqual(execute(inputs, optimized), execute(inputs, CODE))


if __name__ == "__main__":
    main()

