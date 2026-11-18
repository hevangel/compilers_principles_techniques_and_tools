"""Forward must-analysis for Available Expressions, Global CSE, and Copy Propagation."""

from dataclasses import dataclass
from enum import Enum
import unittest


class Op(Enum):
    ADD = "+"
    SUB = "-"
    MUL = "*"


def canonical_expr(op: Op, a: str, b: str) -> tuple[Op, str, str]:
    """Normalize operand order for commutative `+` and `*`; preserve order for `-`."""
    if op in (Op.ADD, Op.MUL) and b < a:
        return (op, b, a)
    return (op, a, b)


@dataclass(frozen=True)
class ConstStmt:
    dst: str
    val: int


@dataclass(frozen=True)
class CopyStmt:
    dst: str
    src: str


@dataclass(frozen=True)
class BinStmt:
    dst: str
    op: Op
    lhs: str
    rhs: str


@dataclass(frozen=True)
class ReturnStmt:
    var: str


Stmt = ConstStmt | CopyStmt | BinStmt | ReturnStmt


@dataclass
class BasicBlock:
    stmts: list[Stmt]
    succs: list[int]


ExprKey = tuple[Op, str, str]


def available_expressions(
    blocks: list[BasicBlock],
) -> tuple[list[set[ExprKey]], list[set[ExprKey]]]:
    """Forward must-analysis: an expression is available iff *all* paths evaluate it without kill.

    Non-entry `out_avail` sets start at `universe` (top of the intersection lattice) so
    loop back-edges do not falsely kill loop-invariant expressions; `in_avail[0]` stays empty.
    """
    n = len(blocks)
    preds: list[list[int]] = [[] for _ in range(n)]
    universe: set[ExprKey] = set()
    for b, blk in enumerate(blocks):
        for s in blk.succs:
            preds[s].append(b)
        for stmt in blk.stmts:
            if isinstance(stmt, BinStmt):
                universe.add(canonical_expr(stmt.op, stmt.lhs, stmt.rhs))

    # Build local `e_gen` (evaluated and not later killed) and `e_kill` (operand overwritten).
    e_gen: list[set[ExprKey]] = [set() for _ in range(n)]
    e_kill: list[set[ExprKey]] = [set() for _ in range(n)]
    for b, blk in enumerate(blocks):
        for stmt in blk.stmts:
            if isinstance(stmt, BinStmt):
                key = canonical_expr(stmt.op, stmt.lhs, stmt.rhs)
                for e in universe:
                    if e[1] == stmt.dst or e[2] == stmt.dst:
                        e_gen[b].discard(e)
                        e_kill[b].add(e)
                # Only generate `key` if the assignment doesn't overwrite one of its own operands.
                if stmt.dst != stmt.lhs and stmt.dst != stmt.rhs:
                    e_gen[b].add(key)
            elif isinstance(stmt, (ConstStmt, CopyStmt)):
                for e in universe:
                    if e[1] == stmt.dst or e[2] == stmt.dst:
                        e_gen[b].discard(e)
                        e_kill[b].add(e)

    in_avail: list[set[ExprKey]] = [set() for _ in range(n)]
    out_avail: list[set[ExprKey]] = [set() if b == 0 else set(universe) for b in range(n)]
    while True:
        changed = False
        for b in range(n):
            if b == 0:
                new_out = e_gen[0] | (in_avail[0] - e_kill[0])
                if new_out != out_avail[0]:
                    out_avail[0] = new_out
                    changed = True
                continue
            # Must confluence: intersect predecessor outgoing sets.
            if not preds[b]:
                new_in: set[ExprKey] = set()
            else:
                new_in = set.intersection(*(out_avail[p] for p in preds[b]))
            new_out = e_gen[b] | (new_in - e_kill[b])
            if new_in != in_avail[b] or new_out != out_avail[b]:
                in_avail[b] = new_in
                out_avail[b] = new_out
                changed = True
        if not changed:
            break
    return in_avail, out_avail


def global_cse(blocks: list[BasicBlock]) -> tuple[list[BasicBlock], int]:
    """Replace available computations with copies from materialized `_cse_k` temporaries."""
    in_avail, _ = available_expressions(blocks)
    avail_uses: set[ExprKey] = set()
    # Pass 1: find expressions that are actually read while available.
    for b, blk in enumerate(blocks):
        cur = set(in_avail[b])
        for stmt in blk.stmts:
            if isinstance(stmt, BinStmt):
                key = canonical_expr(stmt.op, stmt.lhs, stmt.rhs)
                if key in cur:
                    avail_uses.add(key)
                cur = {e for e in cur if e[1] != stmt.dst and e[2] != stmt.dst}
                if stmt.dst != stmt.lhs and stmt.dst != stmt.rhs:
                    cur.add(key)
            elif isinstance(stmt, (ConstStmt, CopyStmt)):
                cur = {e for e in cur if e[1] != stmt.dst and e[2] != stmt.dst}

    temps = {k: f"_cse_{i}" for i, k in enumerate(sorted(avail_uses, key=lambda x: (x[0].value, x[1], x[2])))}
    out_blocks: list[BasicBlock] = []
    replaced = 0
    # Pass 2: materialize `_cse_k` at non-redundant definitions and reuse it at available sites.
    for b, blk in enumerate(blocks):
        cur = set(in_avail[b])
        new_stmts: list[Stmt] = []
        for stmt in blk.stmts:
            if isinstance(stmt, BinStmt):
                key = canonical_expr(stmt.op, stmt.lhs, stmt.rhs)
                if key in temps:
                    tmp = temps[key]
                    if key in cur:
                        new_stmts.append(CopyStmt(stmt.dst, tmp))
                        replaced += 1
                    else:
                        new_stmts.append(stmt)
                        new_stmts.append(CopyStmt(tmp, stmt.dst))
                else:
                    new_stmts.append(stmt)
                cur = {e for e in cur if e[1] != stmt.dst and e[2] != stmt.dst}
                if stmt.dst != stmt.lhs and stmt.dst != stmt.rhs:
                    cur.add(key)
            elif isinstance(stmt, (ConstStmt, CopyStmt)):
                new_stmts.append(stmt)
                cur = {e for e in cur if e[1] != stmt.dst and e[2] != stmt.dst}
            else:
                new_stmts.append(stmt)
        out_blocks.append(BasicBlock(new_stmts, list(blk.succs)))
    return out_blocks, replaced


def copy_propagation(blocks: list[BasicBlock]) -> tuple[list[BasicBlock], int]:
    """Forward must-analysis for `(dst, src)` copies; rewrites uses of `dst` to `src`."""
    n = len(blocks)
    preds: list[list[int]] = [[] for _ in range(n)]
    all_copies: set[tuple[str, str]] = set()
    for b, blk in enumerate(blocks):
        for s in blk.succs:
            preds[s].append(b)
        for stmt in blk.stmts:
            if isinstance(stmt, CopyStmt) and stmt.dst != stmt.src:
                all_copies.add((stmt.dst, stmt.src))

    # Any write to `d` kills every copy pair `(dst, src)` mentioning `d` on either side.
    c_gen: list[set[tuple[str, str]]] = [set() for _ in range(n)]
    c_kill: list[set[tuple[str, str]]] = [set() for _ in range(n)]
    for b, blk in enumerate(blocks):
        for stmt in blk.stmts:
            d = None if isinstance(stmt, ReturnStmt) else stmt.dst
            if d is not None:
                for cp in all_copies:
                    if cp[0] == d or cp[1] == d:
                        c_gen[b].discard(cp)
                        c_kill[b].add(cp)
            if isinstance(stmt, CopyStmt) and stmt.dst != stmt.src:
                c_gen[b].add((stmt.dst, stmt.src))

    # Must-analysis: `out_c[b != 0]` starts at `all_copies`; confluence is intersection.
    in_c: list[set[tuple[str, str]]] = [set() for _ in range(n)]
    out_c: list[set[tuple[str, str]]] = [set() if b == 0 else set(all_copies) for b in range(n)]
    while True:
        changed = False
        for b in range(n):
            new_in = set() if b == 0 or not preds[b] else set.intersection(*(out_c[p] for p in preds[b]))
            new_out = c_gen[b] | (new_in - c_kill[b])
            if new_in != in_c[b] or new_out != out_c[b]:
                in_c[b] = new_in
                out_c[b] = new_out
                changed = True
        if not changed:
            break

    def subst(name: str, active: set[tuple[str, str]]) -> str | None:
        srcs = [src for dst, src in active if dst == name]
        return srcs[0] if len(srcs) == 1 else None

    # Rewrite downstream uses while stepping through each block's statements in order.
    out_blocks: list[BasicBlock] = []
    propagated = 0
    for b, blk in enumerate(blocks):
        active = set(in_c[b])
        new_stmts: list[Stmt] = []
        for stmt in blk.stmts:
            if isinstance(stmt, ConstStmt):
                new_stmts.append(stmt)
                active = {c for c in active if c[0] != stmt.dst and c[1] != stmt.dst}
            elif isinstance(stmt, CopyStmt):
                r = subst(stmt.src, active)
                src = r if r is not None else stmt.src
                if r is not None:
                    propagated += 1
                new_stmts.append(CopyStmt(stmt.dst, src))
                active = {c for c in active if c[0] != stmt.dst and c[1] != stmt.dst}
                if stmt.dst != src:
                    active.add((stmt.dst, src))
            elif isinstance(stmt, BinStmt):
                rl = subst(stmt.lhs, active)
                rr = subst(stmt.rhs, active)
                lhs = rl if rl is not None else stmt.lhs
                rhs = rr if rr is not None else stmt.rhs
                if rl is not None:
                    propagated += 1
                if rr is not None:
                    propagated += 1
                new_stmts.append(BinStmt(stmt.dst, stmt.op, lhs, rhs))
                active = {c for c in active if c[0] != stmt.dst and c[1] != stmt.dst}
            else:
                rv = subst(stmt.var, active)
                v = rv if rv is not None else stmt.var
                if rv is not None:
                    propagated += 1
                new_stmts.append(ReturnStmt(v))
        out_blocks.append(BasicBlock(new_stmts, list(blk.succs)))
    return out_blocks, propagated


def eval_cfg(blocks: list[BasicBlock], choices: list[int], initial: dict[str, int]) -> int:
    env = dict(initial)
    bb = 0
    ci = 0
    for _ in range(10_000):
        for stmt in blocks[bb].stmts:
            if isinstance(stmt, ConstStmt):
                env[stmt.dst] = stmt.val
            elif isinstance(stmt, CopyStmt):
                env[stmt.dst] = env.get(stmt.src, 0)
            elif isinstance(stmt, BinStmt):
                l = env.get(stmt.lhs, 0)
                r = env.get(stmt.rhs, 0)
                if stmt.op == Op.ADD:
                    env[stmt.dst] = l + r
                elif stmt.op == Op.SUB:
                    env[stmt.dst] = l - r
                else:
                    env[stmt.dst] = l * r
            else:
                return env.get(stmt.var, 0)
        succs = blocks[bb].succs
        if len(succs) == 1:
            bb = succs[0]
        else:
            bb = succs[choices[ci]]
            ci += 1
    raise RuntimeError("infinite loop")


def sample_diamond() -> list[BasicBlock]:
    return [
        BasicBlock([], [1, 2]),
        BasicBlock(
            [BinStmt("s1", Op.ADD, "x", "y"), BinStmt("m1", Op.MUL, "a", "b")],
            [3],
        ),
        BasicBlock([BinStmt("s2", Op.ADD, "y", "x")], [3]),
        BasicBlock(
            [
                BinStmt("s3", Op.ADD, "x", "y"),
                BinStmt("m2", Op.MUL, "a", "b"),
                BinStmt("res", Op.SUB, "s3", "m2"),
                ConstStmt("c", 1),
                BinStmt("out", Op.ADD, "res", "c"),
                ReturnStmt("out"),
            ],
            [],
        ),
    ]


def main() -> None:
    cfg = sample_diamond()
    in_avail, _ = available_expressions(cfg)
    print(f"Available in B3 (after merge): {sorted(in_avail[3], key=lambda x: x[0].value)}")
    after_cse, cse_hits = global_cse(cfg)
    after_cp, cp_hits = copy_propagation(after_cse)
    init = {"x": 10, "y": 5, "a": 2, "b": 3}
    print(
        f"CSE replacements={cse_hits}, Copy propagations={cp_hits}, "
        f"orig={eval_cfg(cfg, [0], init)}, opt={eval_cfg(after_cp, [0], init)}"
    )


class AvailableExprsTests(unittest.TestCase):
    def test_must_intersection_requires_both_branches_and_propagates_copies(self) -> None:
        cfg = sample_diamond()
        in_avail, out_avail = available_expressions(cfg)
        xy = canonical_expr(Op.ADD, "x", "y")
        ab = canonical_expr(Op.MUL, "a", "b")
        self.assertIn(xy, in_avail[3])
        self.assertNotIn(ab, in_avail[3])
        self.assertIn(ab, out_avail[1])
        after_cse, cse_hits = global_cse(cfg)
        self.assertEqual(cse_hits, 1)
        after_cp, cp_hits = copy_propagation(after_cse)
        self.assertGreaterEqual(cp_hits, 1)
        init = {"x": 10, "y": 5, "a": 2, "b": 3}
        for path in (0, 1):
            self.assertEqual(eval_cfg(cfg, [path], init), eval_cfg(after_cp, [path], init))

    def test_loop_header_converges_from_universal_set(self) -> None:
        cfg = [
            BasicBlock([BinStmt("t1", Op.ADD, "a", "b"), BinStmt("t2", Op.ADD, "b", "c")], [1]),
            BasicBlock([ReturnStmt("t2")], [2, 3]),
            BasicBlock([ConstStmt("a", 99)], [1]),
            BasicBlock([ReturnStmt("t2")], []),
        ]
        in_avail, _ = available_expressions(cfg)
        self.assertNotIn(canonical_expr(Op.ADD, "a", "b"), in_avail[1])
        self.assertIn(canonical_expr(Op.ADD, "b", "c"), in_avail[1])


if __name__ == "__main__":
    main()
