"""Semilattice Constant Propagation (MFP) and path-by-path MOP nondistributivity witness."""

from dataclasses import dataclass
from enum import Enum
import unittest


class LatticeKind(Enum):
    """Three-level flat constant-propagation semilattice: TOP > CONST(c) > BOTTOM."""

    TOP = "Top"
    CONST = "Const"
    BOTTOM = "Bottom"


@dataclass(frozen=True)
class ConstVal:
    kind: LatticeKind
    val: int | None = None


TOP = ConstVal(LatticeKind.TOP)
BOTTOM = ConstVal(LatticeKind.BOTTOM)


def const(v: int) -> ConstVal:
    return ConstVal(LatticeKind.CONST, v)


def meet(a: ConstVal, b: ConstVal) -> ConstVal:
    """Semilattice meet (`a /\\ b`): TOP is identity, BOTTOM is absorbing, unequal constants drop to BOTTOM."""
    if a == TOP:
        return b
    if b == TOP:
        return a
    if a == BOTTOM or b == BOTTOM:
        return BOTTOM
    return a if a.val == b.val else BOTTOM


class Op(Enum):
    ADD = "+"
    MUL = "*"


@dataclass(frozen=True)
class RhsConst:
    val: int


@dataclass(frozen=True)
class RhsVar:
    name: str


@dataclass(frozen=True)
class RhsBin:
    op: Op
    left: str
    right: str


Rhs = RhsConst | RhsVar | RhsBin


@dataclass(frozen=True)
class Stmt:
    dst: str
    rhs: Rhs


@dataclass
class BasicBlock:
    stmts: list[Stmt]
    succs: list[int]


def eval_rhs(rhs: Rhs, env: dict[str, ConstVal]) -> ConstVal:
    """Monotone transfer function for one RHS expression over abstract environment `env`."""
    if isinstance(rhs, RhsConst):
        return const(rhs.val)
    if isinstance(rhs, RhsVar):
        return env.get(rhs.name, TOP)
    l = env.get(rhs.left, TOP)
    r = env.get(rhs.right, TOP)
    if rhs.op == Op.ADD:
        if l.kind == LatticeKind.CONST and r.kind == LatticeKind.CONST:
            assert l.val is not None and r.val is not None
            return const(l.val + r.val)
    else:
        # Check `0` before `BOTTOM` so `NAC * 0` safely folds to `Const(0)`.
        if (l.kind == LatticeKind.CONST and l.val == 0) or (
            r.kind == LatticeKind.CONST and r.val == 0
        ):
            return const(0)
        if l.kind == LatticeKind.CONST and r.kind == LatticeKind.CONST:
            assert l.val is not None and r.val is not None
            return const(l.val * r.val)
    if l == BOTTOM or r == BOTTOM:
        return BOTTOM
    return TOP


def transfer_block(stmts: list[Stmt], initial: dict[str, ConstVal]) -> dict[str, ConstVal]:
    """Apply block statements sequentially to produce the exit abstract environment."""
    env = dict(initial)
    for s in stmts:
        env[s.dst] = eval_rhs(s.rhs, env)
    return env


def solve_mfp(
    blocks: list[BasicBlock], initial: dict[str, ConstVal]
) -> tuple[list[dict[str, ConstVal]], list[dict[str, ConstVal]]]:
    """Iterative Maximum Fixed Point (MFP) solver initialized optimistically at TOP.

    Meets predecessor outputs pointwise per variable before each block's transfer.
    Because constant propagation is monotone but non-distributive, MFP <= MOP
    (always sound, though strictly less precise than MOP on correlated branches).
    """
    n = len(blocks)
    preds: list[list[int]] = [[] for _ in range(n)]
    all_vars = set(initial.keys())
    for b, blk in enumerate(blocks):
        for s in blk.succs:
            preds[s].append(b)
        for st in blk.stmts:
            all_vars.add(st.dst)

    top_env = {v: TOP for v in sorted(all_vars)}
    in_env = [dict(top_env) for _ in range(n)]
    in_env[0] = dict(initial)
    out_env = [dict(top_env) for _ in range(n)]
    out_env[0] = transfer_block(blocks[0].stmts, in_env[0])

    while True:
        changed = False
        for b in range(n):
            if b == 0:
                new_in = dict(initial)
            elif not preds[b]:
                new_in = dict(in_env[b])
            else:
                new_in = {}
                for v in sorted(all_vars):
                    acc = TOP
                    for p in preds[b]:
                        acc = meet(acc, out_env[p].get(v, TOP))
                    new_in[v] = acc
            new_out = transfer_block(blocks[b].stmts, new_in)
            if new_in != in_env[b] or new_out != out_env[b]:
                in_env[b] = new_in
                out_env[b] = new_out
                changed = True
        if not changed:
            break
    return in_env, out_env


def solve_mop_acyclic(
    blocks: list[BasicBlock], target: int, initial: dict[str, ConstVal]
) -> dict[str, ConstVal]:
    """Exact Meet-Over-All-Paths (MOP) solver for acyclic CFGs via full path enumeration."""
    paths: list[dict[str, ConstVal]] = []

    def dfs(b: int, env: dict[str, ConstVal]) -> None:
        out = transfer_block(blocks[b].stmts, env)
        if b == target:
            paths.append(out)
            return
        for s in blocks[b].succs:
            dfs(s, out)

    dfs(0, initial)
    keys = sorted({k for e in paths for k in e})
    joined: dict[str, ConstVal] = {}
    for k in keys:
        acc = TOP
        for e in paths:
            acc = meet(acc, e.get(k, TOP))
        joined[k] = acc
    return joined


def fold_blocks(blocks: list[BasicBlock], in_env: list[dict[str, ConstVal]]) -> list[BasicBlock]:
    """Replace any RHS whose abstract value is `Const(c)` with a constant literal."""
    out_blocks = []
    for b, blk in enumerate(blocks):
        env = dict(in_env[b])
        stmts = []
        for s in blk.stmts:
            v = eval_rhs(s.rhs, env)
            rhs = RhsConst(v.val) if v.kind == LatticeKind.CONST and v.val is not None else s.rhs
            stmts.append(Stmt(s.dst, rhs))
            env[s.dst] = v
        out_blocks.append(BasicBlock(stmts, list(blk.succs)))
    return out_blocks


def nondistributivity_witness() -> list[BasicBlock]:
    """Build `if c { x = 2; y = 3 } else { x = 3; y = 2 }; z = x + y` (§9.4.5)."""
    return [
        BasicBlock([], [1, 2]),
        BasicBlock([Stmt("x", RhsConst(2)), Stmt("y", RhsConst(3))], [3]),
        BasicBlock([Stmt("x", RhsConst(3)), Stmt("y", RhsConst(2))], [3]),
        BasicBlock([Stmt("z", RhsBin(Op.ADD, "x", "y"))], []),
    ]


def main() -> None:
    cfg = nondistributivity_witness()
    init = {"c": BOTTOM}
    _, mfp_out = solve_mfp(cfg, init)
    mop_out = solve_mop_acyclic(cfg, 3, init)
    print(f"MFP z = {mfp_out[3]['z']} vs MOP z = {mop_out['z']}")


class ConstPropTests(unittest.TestCase):
    def test_mfp_yields_nac_while_mop_yields_5(self) -> None:
        cfg = nondistributivity_witness()
        init = {"c": BOTTOM}
        _, mfp_out = solve_mfp(cfg, init)
        mop_out = solve_mop_acyclic(cfg, 3, init)
        # MFP meets x and y separately to BOTTOM before evaluating `z = x + y`.
        self.assertEqual(mfp_out[3]["x"], BOTTOM)
        self.assertEqual(mfp_out[3]["y"], BOTTOM)
        self.assertEqual(mfp_out[3]["z"], BOTTOM)
        # MOP evaluates `x + y = 5` on each path before joining, obtaining `Const(5)`.
        self.assertEqual(mop_out["z"], const(5))

    def test_folds_loop_constants_and_nac_times_zero(self) -> None:
        cfg = [
            BasicBlock([Stmt("a", RhsConst(4)), Stmt("b", RhsConst(0))], [1]),
            BasicBlock(
                [Stmt("c", RhsBin(Op.ADD, "a", "b")), Stmt("d", RhsBin(Op.MUL, "c", "b"))],
                [1, 2],
            ),
            BasicBlock([Stmt("z", RhsBin(Op.MUL, "param", "b"))], []),
        ]
        init = {"param": BOTTOM}
        in_env, out_env = solve_mfp(cfg, init)
        self.assertEqual(out_env[1]["c"], const(4))
        # `param * 0` folds to `0` even though `param` is `BOTTOM` (NAC).
        self.assertEqual(out_env[2]["z"], const(0))
        folded = fold_blocks(cfg, in_env)
        self.assertEqual(folded[2].stmts[0].rhs, RhsConst(0))


if __name__ == "__main__":
    main()

