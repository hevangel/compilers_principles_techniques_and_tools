"""Bottom-up DP tree tiling with multi-register cost vectors and sliding-window peephole optimization."""

from dataclasses import dataclass
from enum import Enum
import unittest


@dataclass(frozen=True)
class Mem:
    name: str


@dataclass(frozen=True)
class Imm:
    val: int


@dataclass(frozen=True)
class AddExpr:
    left: "Expr"
    right: "Expr"


@dataclass(frozen=True)
class MulExpr:
    left: "Expr"
    right: "Expr"


Expr = Mem | Imm | AddExpr | MulExpr


class SelectedTile(Enum):
    LD_MEM = "LdMem"
    LD_IMM = "LdImm"
    ADD_IMM = "AddImmTile"
    ADD_REG = "AddRegTile"
    MUL_REG = "MulRegTile"
    LEA_INDEXED = "LeaIndexedTile"


@dataclass(frozen=True)
class DpSolution:
    cost: int
    left_first: bool
    tile: SelectedTile


@dataclass
class DpNode:
    expr: Expr
    left: "DpNode | None"
    right: "DpNode | None"
    best: list[DpSolution | None]


@dataclass(frozen=True)
class Ld:
    dst: int
    mem: str


@dataclass(frozen=True)
class LdImm:
    dst: int
    val: int


@dataclass(frozen=True)
class LdIndexed:
    dst: int
    base: int
    scale: int
    idx: int


@dataclass(frozen=True)
class AddReg:
    dst: int
    lhs: int
    rhs: int


@dataclass(frozen=True)
class AddImm:
    dst: int
    lhs: int
    imm: int


@dataclass(frozen=True)
class MulReg:
    dst: int
    lhs: int
    rhs: int


@dataclass(frozen=True)
class MulImm:
    dst: int
    lhs: int
    imm: int


@dataclass(frozen=True)
class ShlImm:
    dst: int
    lhs: int
    shift: int


@dataclass(frozen=True)
class St:
    src: int
    mem: str


@dataclass(frozen=True)
class Jmp:
    target: str


@dataclass(frozen=True)
class Label:
    name: str


TileCode = Ld | LdImm | LdIndexed | AddReg | AddImm | MulReg | MulImm | ShlImm | St | Jmp | Label
INF = 1_000_000


def solve_dp(expr: Expr, max_regs: int) -> DpNode:
    left = solve_dp(expr.left, max_regs) if isinstance(expr, (AddExpr, MulExpr)) else None
    right = solve_dp(expr.right, max_regs) if isinstance(expr, (AddExpr, MulExpr)) else None
    best: list[DpSolution | None] = [None] * (max_regs + 1)

    def update(k: int, cand: DpSolution) -> None:
        cur = best[k]
        if cur is None or cand.cost < cur.cost:
            best[k] = cand

    for k in range(1, max_regs + 1):
        if isinstance(expr, Mem):
            update(k, DpSolution(2, True, SelectedTile.LD_MEM))
        elif isinstance(expr, Imm):
            update(k, DpSolution(1, True, SelectedTile.LD_IMM))
        elif isinstance(expr, AddExpr):
            assert left is not None and right is not None
            if isinstance(expr.right, Imm):
                ls = left.best[k]
                if ls is not None:
                    update(k, DpSolution(ls.cost + 1, True, SelectedTile.ADD_IMM))
            if (
                isinstance(expr.right, MulExpr)
                and isinstance(expr.right.right, Imm)
                and expr.right.right.val in (2, 4, 8)
                and k >= 2
            ):
                idx_node = right.left
                assert idx_node is not None
                if left.best[k] is not None and idx_node.best[k - 1] is not None:
                    update(
                        k,
                        DpSolution(
                            left.best[k].cost + idx_node.best[k - 1].cost + 1,
                            True,
                            SelectedTile.LEA_INDEXED,
                        ),
                    )
                if idx_node.best[k] is not None and left.best[k - 1] is not None:
                    update(
                        k,
                        DpSolution(
                            idx_node.best[k].cost + left.best[k - 1].cost + 1,
                            False,
                            SelectedTile.LEA_INDEXED,
                        ),
                    )
            if k >= 2:
                if left.best[k] is not None and right.best[k - 1] is not None:
                    update(
                        k,
                        DpSolution(
                            left.best[k].cost + right.best[k - 1].cost + 1,
                            True,
                            SelectedTile.ADD_REG,
                        ),
                    )
                if right.best[k] is not None and left.best[k - 1] is not None:
                    update(
                        k,
                        DpSolution(
                            right.best[k].cost + left.best[k - 1].cost + 1,
                            False,
                            SelectedTile.ADD_REG,
                        ),
                    )
            if left.best[k] is not None and right.best[k] is not None:
                update(
                    k,
                    DpSolution(left.best[k].cost + right.best[k].cost + 5, True, SelectedTile.ADD_REG),
                )
        else:
            assert left is not None and right is not None
            if k >= 2:
                if left.best[k] is not None and right.best[k - 1] is not None:
                    update(
                        k,
                        DpSolution(
                            left.best[k].cost + right.best[k - 1].cost + 2,
                            True,
                            SelectedTile.MUL_REG,
                        ),
                    )
                if right.best[k] is not None and left.best[k - 1] is not None:
                    update(
                        k,
                        DpSolution(
                            right.best[k].cost + left.best[k - 1].cost + 2,
                            False,
                            SelectedTile.MUL_REG,
                        ),
                    )
            if best[k] is None:
                lc = left.best[k].cost if left.best[k] is not None else INF
                rc = right.best[k].cost if right.best[k] is not None else INF
                if lc + rc + 5 < INF:
                    update(k, DpSolution(lc + rc + 5, True, SelectedTile.MUL_REG))
    return DpNode(expr, left, right, best)


def emit_dp(node: DpNode, base_reg: int, regs_avail: int, out: list[TileCode]) -> None:
    sol = node.best[regs_avail]
    assert sol is not None
    if sol.tile == SelectedTile.LD_MEM:
        assert isinstance(node.expr, Mem)
        out.append(Ld(base_reg, node.expr.name))
    elif sol.tile == SelectedTile.LD_IMM:
        assert isinstance(node.expr, Imm)
        out.append(LdImm(base_reg, node.expr.val))
    elif sol.tile == SelectedTile.ADD_IMM:
        assert node.left is not None and isinstance(node.expr, AddExpr) and isinstance(node.expr.right, Imm)
        emit_dp(node.left, base_reg, regs_avail, out)
        out.append(AddImm(base_reg, base_reg, node.expr.right.val))
    elif sol.tile == SelectedTile.LEA_INDEXED:
        assert node.left is not None and node.right is not None and node.right.left is not None
        assert isinstance(node.right.right, DpNode) and isinstance(node.right.right.expr, Imm)
        idx_node = node.right.left
        scale = node.right.right.expr.val
        if sol.left_first:
            emit_dp(node.left, base_reg, regs_avail, out)
            emit_dp(idx_node, base_reg + 1, regs_avail - 1, out)
        else:
            emit_dp(idx_node, base_reg + 1, regs_avail, out)
            emit_dp(node.left, base_reg, regs_avail - 1, out)
        out.append(LdIndexed(base_reg, base_reg, scale, base_reg + 1))
    else:
        assert node.left is not None and node.right is not None
        if sol.left_first:
            emit_dp(node.left, base_reg, regs_avail, out)
            emit_dp(node.right, base_reg + 1, max(regs_avail - 1, 1), out)
        else:
            emit_dp(node.right, base_reg + 1, regs_avail, out)
            emit_dp(node.left, base_reg, max(regs_avail - 1, 1), out)
        if sol.tile == SelectedTile.ADD_REG:
            out.append(AddReg(base_reg, base_reg, base_reg + 1))
        else:
            out.append(MulReg(base_reg, base_reg, base_reg + 1))


def naive_postorder_emit(expr: Expr, reg: int, out: list[TileCode]) -> int:
    if isinstance(expr, Mem):
        out.append(Ld(reg, expr.name))
        return 2
    if isinstance(expr, Imm):
        out.append(LdImm(reg, expr.val))
        return 1
    cl = naive_postorder_emit(expr.left, reg, out)
    cr = naive_postorder_emit(expr.right, reg + 1, out)
    if isinstance(expr, AddExpr):
        out.append(AddReg(reg, reg, reg + 1))
        return cl + cr + 1
    out.append(MulReg(reg, reg, reg + 1))
    return cl + cr + 2


def peephole_optimize(code: list[TileCode]) -> list[TileCode]:
    cur = list(code)
    while True:
        nxt: list[TileCode] = []
        i = 0
        changed = False
        while i < len(cur):
            if i + 1 < len(cur):
                a, b = cur[i], cur[i + 1]
                if isinstance(a, St) and isinstance(b, Ld) and a.src == b.dst and a.mem == b.mem:
                    nxt.append(a)
                    i += 2
                    changed = True
                    continue
                if isinstance(a, Jmp) and isinstance(b, Label) and a.target == b.name:
                    i += 1
                    changed = True
                    continue
            ins = cur[i]
            if isinstance(ins, AddImm) and ins.dst == ins.lhs and ins.imm == 0:
                i += 1
                changed = True
                continue
            if isinstance(ins, MulImm) and ins.imm == 2:
                nxt.append(ShlImm(ins.dst, ins.lhs, 1))
                i += 1
                changed = True
                continue
            nxt.append(ins)
            i += 1
        cur = nxt
        if not changed:
            break
    return cur


def eval_expr(expr: Expr, mem: dict[str, int]) -> int:
    if isinstance(expr, Mem):
        return mem[expr.name]
    if isinstance(expr, Imm):
        return expr.val
    if isinstance(expr, AddExpr):
        return eval_expr(expr.left, mem) + eval_expr(expr.right, mem)
    return eval_expr(expr.left, mem) * eval_expr(expr.right, mem)


def eval_tiles(code: list[TileCode], initial: dict[str, int]) -> int:
    mem = dict(initial)
    regs = [0] * 16
    labels = {ins.name: i for i, ins in enumerate(code) if isinstance(ins, Label)}
    pc = 0
    while pc < len(code):
        ins = code[pc]
        if isinstance(ins, Ld):
            regs[ins.dst] = mem[ins.mem]
        elif isinstance(ins, LdImm):
            regs[ins.dst] = ins.val
        elif isinstance(ins, LdIndexed):
            regs[ins.dst] = regs[ins.base] + ins.scale * regs[ins.idx]
        elif isinstance(ins, AddReg):
            regs[ins.dst] = regs[ins.lhs] + regs[ins.rhs]
        elif isinstance(ins, AddImm):
            regs[ins.dst] = regs[ins.lhs] + ins.imm
        elif isinstance(ins, MulReg):
            regs[ins.dst] = regs[ins.lhs] * regs[ins.rhs]
        elif isinstance(ins, MulImm):
            regs[ins.dst] = regs[ins.lhs] * ins.imm
        elif isinstance(ins, ShlImm):
            regs[ins.dst] = regs[ins.lhs] << ins.shift
        elif isinstance(ins, St):
            mem[ins.mem] = regs[ins.src]
        elif isinstance(ins, Jmp):
            pc = labels[ins.target]
            continue
        pc += 1
    return regs[0]


def indexed_sample_expr() -> Expr:
    return AddExpr(AddExpr(Mem("base"), MulExpr(Mem("idx"), Imm(4))), Imm(8))


def main() -> None:
    expr = indexed_sample_expr()
    dp = solve_dp(expr, 4)
    dp_code: list[TileCode] = []
    emit_dp(dp, 0, 4, dp_code)
    naive_code: list[TileCode] = []
    naive_cost = naive_postorder_emit(expr, 0, naive_code)
    assert dp.best[4] is not None
    print(f"DP cost={dp.best[4].cost} ({len(dp_code)} instrs) vs naive={naive_cost} ({len(naive_code)} instrs)")

    raw: list[TileCode] = [
        Ld(0, "idx"),
        MulImm(0, 0, 2),
        AddImm(0, 0, 0),
        St(0, "out"),
        Ld(0, "out"),
        Jmp("L_done"),
        Label("L_done"),
    ]
    opt = peephole_optimize(raw)
    mem = {"base": 1000, "idx": 7}
    print(
        f"Eval expr={eval_expr(expr, mem)}, dp={eval_tiles(dp_code, mem)}, "
        f"peephole {len(raw)} -> {len(opt)} instrs (val={eval_tiles(opt, mem)})"
    )


class DpTilingTests(unittest.TestCase):
    def test_dp_selects_cheaper_fused_indexed_and_imm_tiles(self) -> None:
        expr = indexed_sample_expr()
        dp = solve_dp(expr, 4)
        dp_code: list[TileCode] = []
        emit_dp(dp, 0, 4, dp_code)
        naive_code: list[TileCode] = []
        naive_cost = naive_postorder_emit(expr, 0, naive_code)
        assert dp.best[4] is not None
        self.assertLess(dp.best[4].cost, naive_cost)
        self.assertLess(len(dp_code), len(naive_code))
        self.assertTrue(any(isinstance(i, LdIndexed) and i.scale == 4 for i in dp_code))
        mem = {"base": 500, "idx": 12}
        self.assertEqual(eval_tiles(dp_code, mem), eval_expr(expr, mem))
        self.assertEqual(eval_tiles(naive_code, mem), eval_expr(expr, mem))

    def test_peephole_shrinks_redundant_instructions_preserving_value(self) -> None:
        raw: list[TileCode] = [
            Ld(0, "x"),
            MulImm(0, 0, 2),
            AddImm(0, 0, 0),
            St(0, "tmp"),
            Ld(0, "tmp"),
            Jmp("L_next"),
            Label("L_next"),
        ]
        opt = peephole_optimize(raw)
        self.assertLessEqual(len(opt), 4)
        self.assertTrue(any(isinstance(i, ShlImm) and i.shift == 1 for i in opt))
        mem = {"x": 9}
        self.assertEqual(eval_tiles(opt, mem), eval_tiles(raw, mem))


if __name__ == "__main__":
    main()
