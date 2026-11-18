"""Label expression trees with register needs and emit code for an abstract machine."""

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
class Input:
    slot: int


@dataclass(frozen=True)
class BinaryExpr:
    op: Op
    left: "Expr"
    right: "Expr"


Expr = Input | BinaryExpr


@dataclass(frozen=True)
class PlanLoad:
    """Annotated leaf: loading one input requires 1 register."""

    slot: int
    need: int = 1


@dataclass(frozen=True)
class PlanBinary:
    """Annotated binary node storing its Sethi–Ullman (Ershov) register `need`."""

    op: Op
    left: "Plan"
    right: "Plan"
    need: int


Plan = PlanLoad | PlanBinary


def plan_for(expr: Expr) -> Plan:
    """Compute Ershov numbers bottom-up: max(a, b) if unequal, else a + 1."""
    if isinstance(expr, Input):
        return PlanLoad(expr.slot)
    left = plan_for(expr.left)
    right = plan_for(expr.right)
    # Equal needs force holding the first child's result in a register while computing the second.
    need = max(left.need, right.need) + (1 if left.need == right.need else 0)
    return PlanBinary(op=expr.op, left=left, right=right, need=need)


@dataclass(frozen=True)
class InstLoad:
    dst: int
    slot: int


@dataclass(frozen=True)
class InstBinary:
    dst: int
    op: Op
    left: int
    right: int


Inst = InstLoad | InstBinary


def generate(plan: Plan, base: int, code: list[Inst]) -> None:
    """Emit code using `base .. base + plan.need`, leaving the result in `base`."""
    if isinstance(plan, PlanLoad):
        code.append(InstLoad(dst=base, slot=plan.slot))
        return
    # Always evaluate the higher-need subtree first in `base`, freeing `base + 1..` for the other.
    if plan.left.need >= plan.right.need:
        generate(plan.left, base, code)
        generate(plan.right, base + 1, code)
        left_reg, right_reg = base, base + 1
    else:
        generate(plan.right, base, code)
        generate(plan.left, base + 1, code)
        # Evaluation order swapped, so preserve source operand order for non-commutative `Sub`.
        left_reg, right_reg = base + 1, base
    code.append(InstBinary(dst=base, op=plan.op, left=left_reg, right=right_reg))


def run(code: list[Inst], registers: int, inputs: list[int]) -> int:
    """Execute emitted instructions on `registers` slots and return `regs[0]`."""
    regs = [0] * registers
    for inst in code:
        if isinstance(inst, InstLoad):
            regs[inst.dst] = inputs[inst.slot]
        else:
            val = (
                regs[inst.left] + regs[inst.right]
                if inst.op == Op.ADD
                else regs[inst.left] - regs[inst.right]
            )
            regs[inst.dst] = wrap_i64(val)
    return regs[0]


def sample() -> Plan:
    return plan_for(BinaryExpr(Op.SUB, Input(0), BinaryExpr(Op.ADD, Input(1), Input(2))))


def main() -> None:
    plan = sample()
    code: list[Inst] = []
    generate(plan, 0, code)
    print(f"{plan.need} registers")
    for inst in code:
        print(inst)
    print(f"Result: {run(code, plan.need, [30, 4, 5])}")


class RegisterCodegenTests(unittest.TestCase):
    def test_evaluates_larger_subtree_first_but_preserves_operand_order(self) -> None:
        plan = sample()
        self.assertEqual(plan.need, 2)
        code: list[Inst] = []
        generate(plan, 0, code)
        self.assertIsInstance(code[0], InstLoad)
        assert isinstance(code[0], InstLoad)
        self.assertEqual(code[0].slot, 1)
        self.assertEqual(run(code, plan.need, [30, 4, 5]), 21)


if __name__ == "__main__":
    main()
