"""Generate short-circuit Boolean branches by filling forward targets later."""

from dataclasses import dataclass
import unittest


@dataclass(frozen=True)
class CondInput:
    index: int


@dataclass(frozen=True)
class CondAnd:
    left: "Condition"
    right: "Condition"


@dataclass(frozen=True)
class CondOr:
    left: "Condition"
    right: "Condition"


Condition = CondInput | CondAnd | CondOr


@dataclass
class IfTrue:
    index: int
    target: int | None = None


@dataclass
class Jump:
    target: int | None = None


@dataclass
class Return:
    value: bool


Instruction = IfTrue | Jump | Return


@dataclass
class Exits:
    on_true: list[int]
    on_false: list[int]


def patch(code: list[Instruction], sites: list[int], target: int) -> None:
    for site in sites:
        inst = code[site]
        assert isinstance(inst, (IfTrue, Jump))
        assert inst.target is None
        inst.target = target


def emit(condition: Condition, code: list[Instruction]) -> Exits:
    if isinstance(condition, CondInput):
        site = len(code)
        code.extend([IfTrue(condition.index), Jump()])
        return Exits(on_true=[site], on_false=[site + 1])
    if isinstance(condition, CondAnd):
        left = emit(condition.left, code)
        nxt = len(code)
        patch(code, left.on_true, nxt)
        right = emit(condition.right, code)
        return Exits(on_true=right.on_true, on_false=[*left.on_false, *right.on_false])
    left = emit(condition.left, code)
    nxt = len(code)
    patch(code, left.on_false, nxt)
    right = emit(condition.right, code)
    return Exits(on_true=[*left.on_true, *right.on_true], on_false=right.on_false)


def compile_condition(condition: Condition) -> list[Instruction]:
    code: list[Instruction] = []
    exits = emit(condition, code)
    end = len(code)
    code.extend([Return(True), Return(False)])
    patch(code, exits.on_true, end)
    patch(code, exits.on_false, end + 1)
    return code


def run(code: list[Instruction], inputs: list[bool]) -> tuple[bool, list[int]]:
    pc = 0
    reads: list[int] = []
    while True:
        inst = code[pc]
        if isinstance(inst, IfTrue):
            assert inst.target is not None
            reads.append(inst.index)
            pc = inst.target if inputs[inst.index] else pc + 1
        elif isinstance(inst, Jump):
            assert inst.target is not None
            pc = inst.target
        else:
            return inst.value, reads


def sample() -> Condition:
    return CondOr(CondAnd(CondInput(0), CondInput(1)), CondInput(2))


def main() -> None:
    code = compile_condition(sample())
    for pc, inst in enumerate(code):
        print(f"{pc}: {inst}")
    print(f"Result and inputs read: {run(code, [False, False, True])}")


class BackpatchTests(unittest.TestCase):
    def test_matches_truth_table_and_skips_unused_inputs(self) -> None:
        code = compile_condition(sample())
        for a in (False, True):
            for b in (False, True):
                for c in (False, True):
                    self.assertEqual(run(code, [a, b, c])[0], (a and b) or c)
        self.assertEqual(run(code, [False, False, True]), (True, [0, 2]))
        self.assertEqual(run(code, [True, True, False]), (True, [0, 1]))


if __name__ == "__main__":
    main()
