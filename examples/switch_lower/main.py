"""Lower multi-way switch statements by density and backpatch nested loop/switch break and continue jumps."""

from dataclasses import dataclass
from enum import Enum
import unittest


class SwitchStrategy(Enum):
    """Dispatch strategy chosen from case count and key density."""

    LINEAR_IF_ELSE = "LinearIfElse"
    JUMP_TABLE = "JumpTable"
    BINARY_SEARCH_TREE = "BinarySearchTree"


@dataclass(frozen=True)
class IfEqJump:
    """Branch to `target` if the switch key / loop variable equals `val`."""

    val: int
    target: str


@dataclass(frozen=True)
class IfLtJump:
    """Branch to `target` if the switch key is strictly less than `val` (for BST dispatch)."""

    val: int
    target: str


@dataclass(frozen=True)
class JumpTable:
    """Bounds-checked O(1) dispatch table covering keys `min_val .. min_val + len(table)`."""

    min_val: int
    default_label: str
    table: list[str]


@dataclass(frozen=True)
class Jump:
    """Unconditional jump emitted directly during switch lowering."""

    target: str


@dataclass(frozen=True)
class Label:
    """Named jump target placed in the flat instruction stream."""

    name: str


@dataclass(frozen=True)
class Emit:
    """Observable trace output instruction."""

    val: int


@dataclass(frozen=True)
class IncrVar:
    """Increment the loop induction variable `var` by 1."""

    pass


@dataclass(frozen=True)
class IfVarLtJump:
    """Loop back-edge test: jump to `target` while `var < limit`."""

    limit: int
    target: str


@dataclass(frozen=True)
class BreakPlaceholder:
    """Unresolved `break` jump site waiting to be backpatched when its construct finishes."""

    pass


@dataclass(frozen=True)
class ContinuePlaceholder:
    """Unresolved `continue` jump site waiting to be backpatched to the enclosing loop step."""

    pass


@dataclass(frozen=True)
class PatchedJump:
    """Resolved `break` or `continue` jump created by `backpatch`."""

    target: str


Instr = (
    IfEqJump
    | IfLtJump
    | JumpTable
    | Jump
    | Label
    | Emit
    | IncrVar
    | IfVarLtJump
    | BreakPlaceholder
    | ContinuePlaceholder
    | PatchedJump
)


def choose_strategy(arms: list[tuple[int, str]]) -> SwitchStrategy:
    """Select linear checks for <=4 arms, jump table when >=50% dense, else BST."""
    if len(arms) <= 4:
        return SwitchStrategy.LINEAR_IF_ELSE
    min_v = min(v for v, _ in arms)
    max_v = max(v for v, _ in arms)
    span = max_v - min_v + 1
    # Use integer math `2 * count >= span` to test density >= 0.5 without floating-point rounding.
    if len(arms) * 2 >= span:
        return SwitchStrategy.JUMP_TABLE
    return SwitchStrategy.BINARY_SEARCH_TREE


def lower_switch(arms: list[tuple[int, str]], default_label: str) -> tuple[SwitchStrategy, list[Instr]]:
    """Sort/dedup case arms by value and lower dispatch to linear, jump-table, or BST code."""
    dedup: dict[int, str] = {}
    for v, lbl in sorted(arms, key=lambda pair: pair[0]):
        dedup.setdefault(v, lbl)
    sorted_arms = sorted(dedup.items())

    strategy = choose_strategy(sorted_arms)
    instrs: list[Instr] = []
    if strategy == SwitchStrategy.LINEAR_IF_ELSE:
        for val, lbl in sorted_arms:
            instrs.append(IfEqJump(val, lbl))
        instrs.append(Jump(default_label))
    elif strategy == SwitchStrategy.JUMP_TABLE:
        min_v = sorted_arms[0][0]
        max_v = sorted_arms[-1][0]
        # Default-initialize all slots (including gaps between case values) to `default_label`.
        table = [default_label] * (max_v - min_v + 1)
        for val, lbl in sorted_arms:
            table[val - min_v] = lbl
        instrs.append(JumpTable(min_v, default_label, table))
    else:
        def emit_bst(sl: list[tuple[int, str]], prefix: str) -> None:
            # Partition sorted keys at `mid`: branch to `left_lbl` if `key < mid_val`,
            # match `mid_val`, or fall through into the right subrange.
            if not sl:
                instrs.append(Jump(default_label))
                return
            if len(sl) == 1:
                instrs.append(IfEqJump(sl[0][0], sl[0][1]))
                instrs.append(Jump(default_label))
                return
            mid = len(sl) // 2
            mid_val, mid_lbl = sl[mid]
            left_lbl = f"{prefix}L"
            instrs.append(IfLtJump(mid_val, left_lbl))
            instrs.append(IfEqJump(mid_val, mid_lbl))
            emit_bst(sl[mid + 1 :], f"{prefix}R")
            instrs.append(Label(left_lbl))
            emit_bst(sl[:mid], f"{prefix}L")

        emit_bst(sorted_arms, "bst_")
    return strategy, instrs


def eval_switch_dispatch(instrs: list[Instr], key: int) -> str:
    """Execute dispatch-only instructions for `key` and return the target arm or default label."""
    labels = {ins.name: i for i, ins in enumerate(instrs) if isinstance(ins, Label)}
    pc = 0
    for _ in range(10_000):
        ins = instrs[pc]
        if isinstance(ins, IfEqJump):
            if key == ins.val:
                if ins.target in labels:
                    pc = labels[ins.target]
                else:
                    return ins.target
            else:
                pc += 1
        elif isinstance(ins, IfLtJump):
            if key < ins.val:
                if ins.target in labels:
                    pc = labels[ins.target]
                else:
                    return ins.target
            else:
                pc += 1
        elif isinstance(ins, JumpTable):
            # Bounds check both below `min_val` and above `max_val` before table indexing.
            if key < ins.min_val or key - ins.min_val >= len(ins.table):
                target = ins.default_label
            else:
                target = ins.table[key - ins.min_val]
            if target in labels:
                pc = labels[target]
            else:
                return target
        elif isinstance(ins, Jump):
            if ins.target in labels:
                pc = labels[ins.target]
            else:
                return ins.target
        elif isinstance(ins, Label):
            pc += 1
        else:
            raise RuntimeError(f"unexpected instruction {ins}")
    raise RuntimeError("infinite dispatch")


@dataclass(frozen=True)
class EmitCaseValue:
    """Marker statement that lowers to `Emit(1)`."""

    pass


@dataclass(frozen=True)
class BreakStmt:
    """Structured break exiting the innermost `SwitchStmt` or `LoopStmt`."""

    pass


@dataclass(frozen=True)
class ContinueStmt:
    """Structured continue jumping to the innermost enclosing `LoopStmt`'s step."""

    pass


@dataclass(frozen=True)
class SwitchStmt:
    cases: list[tuple[int, list["StmtAst"]]]
    default: list["StmtAst"]


@dataclass(frozen=True)
class LoopStmt:
    limit: int
    body: list["StmtAst"]


StmtAst = EmitCaseValue | BreakStmt | ContinueStmt | SwitchStmt | LoopStmt


class PatchContext:
    """Tracks separate nested stacks of pending `break` and `continue` jump indices."""

    def __init__(self) -> None:
        # Invariant: `SwitchStmt` pushes onto `break_lists` only; `LoopStmt` pushes onto both.
        self.break_lists: list[list[int]] = []
        self.continue_lists: list[list[int]] = []
        self.next_id = 0

    def fresh_label(self, prefix: str) -> str:
        lbl = f"{prefix}_{self.next_id}"
        self.next_id += 1
        return lbl


def backpatch(instrs: list[Instr], sites: list[int], target: str) -> None:
    """Replace each placeholder index in `sites` with a `PatchedJump(target)`."""
    for idx in sites:
        instrs[idx] = PatchedJump(target)


def lower_stmt_list(stmts: list[StmtAst], ctx: PatchContext, out: list[Instr]) -> None:
    """Lower structured statements to flat instructions, backpatching `break` and `continue`."""
    for stmt in stmts:
        if isinstance(stmt, EmitCaseValue):
            out.append(Emit(1))
        elif isinstance(stmt, BreakStmt):
            ctx.break_lists[-1].append(len(out))
            out.append(BreakPlaceholder())
        elif isinstance(stmt, ContinueStmt):
            ctx.continue_lists[-1].append(len(out))
            out.append(ContinuePlaceholder())
        elif isinstance(stmt, SwitchStmt):
            switch_end = ctx.fresh_label("sw_end")
            def_lbl = ctx.fresh_label("sw_def")
            arms: list[tuple[int, str]] = []
            case_labels: list[str] = []
            for val, _ in stmt.cases:
                lbl = ctx.fresh_label(f"case_{val}")
                arms.append((val, lbl))
                case_labels.append(lbl)
            _, disp = lower_switch(arms, def_lbl)
            out.extend(disp)
            # Push a new break scope for this switch, but leave `continue_lists` untouched
            # so `continue` inside a case targets the enclosing loop's increment step.
            ctx.break_lists.append([])
            for (val, body), lbl in zip(stmt.cases, case_labels):
                out.append(Label(lbl))
                out.append(Emit(val * 10))
                lower_stmt_list(body, ctx, out)
            out.append(Label(def_lbl))
            lower_stmt_list(stmt.default, ctx, out)
            out.append(Label(switch_end))
            backpatch(out, ctx.break_lists.pop(), switch_end)
        elif isinstance(stmt, LoopStmt):
            loop_head = ctx.fresh_label("loop_head")
            loop_step = ctx.fresh_label("loop_step")
            loop_exit = ctx.fresh_label("loop_exit")
            ctx.break_lists.append([])
            ctx.continue_lists.append([])
            out.append(Label(loop_head))
            lower_stmt_list(stmt.body, ctx, out)
            # `continue` lands at `loop_step` (before `IncrVar`), not `loop_head`, so `var` still advances.
            out.append(Label(loop_step))
            out.append(IncrVar())
            out.append(IfVarLtJump(stmt.limit, loop_head))
            out.append(Label(loop_exit))
            backpatch(out, ctx.continue_lists.pop(), loop_step)
            backpatch(out, ctx.break_lists.pop(), loop_exit)


def lower_program(stmts: list[StmtAst]) -> list[Instr]:
    """Lower a top-level statement list with an empty `PatchContext`."""
    ctx = PatchContext()
    out: list[Instr] = []
    lower_stmt_list(stmts, ctx, out)
    return out


def run_lowered_program(instrs: list[Instr]) -> tuple[int, list[int]]:
    """Execute a complete lowered instruction stream and return `(final_var, trace)`."""
    labels = {ins.name: i for i, ins in enumerate(instrs) if isinstance(ins, Label)}
    pc = 0
    var = 0
    trace: list[int] = []
    for _ in range(50_000):
        if pc >= len(instrs):
            return var, trace
        ins = instrs[pc]
        if isinstance(ins, IfEqJump):
            pc = labels[ins.target] if var == ins.val else pc + 1
        elif isinstance(ins, IfLtJump):
            pc = labels[ins.target] if var < ins.val else pc + 1
        elif isinstance(ins, JumpTable):
            if var < ins.min_val or var - ins.min_val >= len(ins.table):
                target = ins.default_label
            else:
                target = ins.table[var - ins.min_val]
            pc = labels[target]
        elif isinstance(ins, (Jump, PatchedJump)):
            pc = labels[ins.target]
        elif isinstance(ins, Label):
            pc += 1
        elif isinstance(ins, Emit):
            trace.append(ins.val)
            pc += 1
        elif isinstance(ins, IncrVar):
            var += 1
            pc += 1
        elif isinstance(ins, IfVarLtJump):
            pc = labels[ins.target] if var < ins.limit else pc + 1
        else:
            raise RuntimeError(f"unpatched placeholder {ins}")
    raise RuntimeError("infinite loop")


def main() -> None:
    dense = [(i, f"L{i}") for i in range(10)]
    sparse = [(i * 100, f"S{i}") for i in range(10)]
    tiny = [(1, "T1"), (2, "T2")]

    d_strat, d_ins = lower_switch(dense, "DEF")
    s_strat, s_ins = lower_switch(sparse, "DEF")
    t_strat, t_ins = lower_switch(tiny, "DEF")
    print(f"Dense strategy: {d_strat.value}, instrs={len(d_ins)}")
    print(f"Sparse strategy: {s_strat.value}, instrs={len(s_ins)}")
    print(f"Tiny strategy: {t_strat.value}, instrs={len(t_ins)}")
    print(
        f"Sparse dispatch(500)={eval_switch_dispatch(s_ins, 500)}, "
        f"dispatch(501)={eval_switch_dispatch(s_ins, 501)}"
    )

    prog: list[StmtAst] = [
        LoopStmt(
            4,
            [
                SwitchStmt(
                    [
                        (0, [BreakStmt()]),
                        (1, [ContinueStmt()]),
                        (2, [EmitCaseValue(), BreakStmt()]),
                    ],
                    [EmitCaseValue()],
                ),
                EmitCaseValue(),
            ],
        )
    ]
    lowered = lower_program(prog)
    final_var, trace = run_lowered_program(lowered)
    print(f"Nested loop+switch: final_var={final_var}, trace={trace}")


class SwitchLowerTests(unittest.TestCase):
    def test_selects_switch_strategies_and_dispatches_all_keys(self) -> None:
        s_tiny, ins_tiny = lower_switch([(1, "A"), (5, "B")], "DEF")
        self.assertEqual(s_tiny, SwitchStrategy.LINEAR_IF_ELSE)
        self.assertEqual(eval_switch_dispatch(ins_tiny, 1), "A")
        self.assertEqual(eval_switch_dispatch(ins_tiny, 3), "DEF")

        dense = [(10, "D10"), (11, "D11"), (12, "D12"), (14, "D14"), (15, "D15"), (16, "D16")]
        s_dense, ins_dense = lower_switch(dense, "DEF")
        self.assertEqual(s_dense, SwitchStrategy.JUMP_TABLE)
        self.assertEqual(eval_switch_dispatch(ins_dense, 9), "DEF")
        self.assertEqual(eval_switch_dispatch(ins_dense, 12), "D12")
        self.assertEqual(eval_switch_dispatch(ins_dense, 13), "DEF")
        self.assertEqual(eval_switch_dispatch(ins_dense, 20), "DEF")

        sparse = [(i * 1000, f"S{i}") for i in range(10)]
        s_sparse, ins_sparse = lower_switch(sparse, "DEF")
        self.assertEqual(s_sparse, SwitchStrategy.BINARY_SEARCH_TREE)
        self.assertEqual(eval_switch_dispatch(ins_sparse, -5), "DEF")
        self.assertEqual(eval_switch_dispatch(ins_sparse, 0), "S0")
        self.assertEqual(eval_switch_dispatch(ins_sparse, 4500), "DEF")
        self.assertEqual(eval_switch_dispatch(ins_sparse, 9000), "S9")
        self.assertEqual(eval_switch_dispatch(ins_sparse, 9001), "DEF")

    def test_nested_loop_and_switch_backpatches_break_and_continue_distinctly(self) -> None:
        prog: list[StmtAst] = [
            LoopStmt(
                4,
                [
                    SwitchStmt(
                        [
                            (0, [BreakStmt()]),
                            (1, [ContinueStmt()]),
                            (2, [EmitCaseValue(), BreakStmt()]),
                        ],
                        [EmitCaseValue()],
                    ),
                    EmitCaseValue(),
                ],
            )
        ]
        final_i, trace = run_lowered_program(lower_program(prog))
        self.assertEqual(final_i, 4)
        self.assertEqual(trace, [0, 1, 10, 20, 1, 1, 1, 1])

        loop_break: list[StmtAst] = [
            LoopStmt(
                5,
                [
                    SwitchStmt([(0, [BreakStmt()])], [EmitCaseValue()]),
                    BreakStmt(),
                ],
            )
        ]
        final_lb, trace_lb = run_lowered_program(lower_program(loop_break))
        self.assertEqual(final_lb, 0)
        self.assertEqual(trace_lb, [0])


if __name__ == "__main__":
    main()
