"""Generate short-circuit Boolean branches by filling forward targets later."""

from dataclasses import dataclass
import unittest


@dataclass(frozen=True)
class CondInput:
    """Leaf predicate reading `inputs[index]`."""

    index: int


@dataclass(frozen=True)
class CondAnd:
    """Short-circuit conjunction: evaluates `right` only when `left` is true."""

    left: "Condition"
    right: "Condition"


@dataclass(frozen=True)
class CondOr:
    """Short-circuit disjunction: evaluates `right` only when `left` is false."""

    left: "Condition"
    right: "Condition"


Condition = CondInput | CondAnd | CondOr


@dataclass
class IfTrue:
    """Conditional jump: branches to `target` if `inputs[index]` is true, else falls through."""

    index: int
    target: int | None = None  # `None` until backpatched


@dataclass
class Jump:
    """Unconditional jump (used as the false-exit companion to `IfTrue`)."""

    target: int | None = None  # `None` until backpatched


@dataclass
class Return:
    """Halts the condition evaluator with a Boolean outcome."""

    value: bool


Instruction = IfTrue | Jump | Return


@dataclass
class Exits:
    """Lists of instruction indices whose forward jump `target` is still `None`."""

    on_true: list[int]
    on_false: list[int]


def patch(code: list[Instruction], sites: list[int], target: int) -> None:
    """Fill every unresolved jump in `sites` with `target`, asserting each is patched once."""
    for site in sites:
        inst = code[site]
        assert isinstance(inst, (IfTrue, Jump))
        # Invariant: no jump site is ever patched twice.
        assert inst.target is None
        inst.target = target


def emit(condition: Condition, code: list[Instruction]) -> Exits:
    """Emit jump instructions in one pass, returning unresolved true/false exit lists."""
    if isinstance(condition, CondInput):
        # Emit a paired conditional true branch and fall-through false jump, both initially `None`.
        site = len(code)
        code.extend([IfTrue(condition.index), Jump()])
        return Exits(on_true=[site], on_false=[site + 1])
    if isinstance(condition, CondAnd):
        left = emit(condition.left, code)
        # `nxt` is the instruction index where `right`'s code starts.
        nxt = len(code)
        # Only when `left` is true do we continue to `right`; `left.on_false` short-circuits.
        patch(code, left.on_true, nxt)
        right = emit(condition.right, code)
        return Exits(on_true=right.on_true, on_false=[*left.on_false, *right.on_false])
    # `CondOr`: evaluate `right` only when `left` is false; `left.on_true` short-circuits.
    left = emit(condition.left, code)
    nxt = len(code)
    patch(code, left.on_false, nxt)
    right = emit(condition.right, code)
    return Exits(on_true=[*left.on_true, *right.on_true], on_false=right.on_false)


def compile_condition(condition: Condition) -> list[Instruction]:
    """Compile `condition` and resolve its remaining exits to `Return(True)` / `Return(False)`."""
    code: list[Instruction] = []
    exits = emit(condition, code)
    end = len(code)
    code.extend([Return(True), Return(False)])
    patch(code, exits.on_true, end)
    patch(code, exits.on_false, end + 1)
    return code


def run(code: list[Instruction], inputs: list[bool]) -> tuple[bool, list[int]]:
    """Run compiled instructions on `inputs` and return `(result, read_indices)`."""
    pc = 0
    reads: list[int] = []
    while True:
        inst = code[pc]
        if isinstance(inst, IfTrue):
            assert inst.target is not None
            # Track which inputs were actually inspected to verify short-circuiting.
            reads.append(inst.index)
            pc = inst.target if inputs[inst.index] else pc + 1
        elif isinstance(inst, Jump):
            assert inst.target is not None
            pc = inst.target
        else:
            return inst.value, reads


def sample() -> Condition:
    """Construct `(Input(0) && Input(1)) || Input(2)`."""
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
