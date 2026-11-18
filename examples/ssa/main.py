"""Construct minimal SSA form via dominance frontiers, rename variables, fold constants, and de-SSA."""

from dataclasses import dataclass
from enum import Enum
import unittest


class BinOp(Enum):
    ADD = "Add"
    SUB = "Sub"
    LT = "Lt"


# Pre-SSA CFG IR: mutable variable names can be reassigned across blocks.
@dataclass(frozen=True)
class RhsConst:
    val: int


@dataclass(frozen=True)
class RhsVar:
    name: str


@dataclass(frozen=True)
class RhsBin:
    op: BinOp
    left: str
    right: str


Rhs = RhsConst | RhsVar | RhsBin


@dataclass(frozen=True)
class Stmt:
    dst: str
    rhs: Rhs


@dataclass(frozen=True)
class TermJump:
    target: int


@dataclass(frozen=True)
class TermBranch:
    cond: str
    then_bb: int
    else_bb: int


@dataclass(frozen=True)
class TermReturn:
    var: str


Term = TermJump | TermBranch | TermReturn


@dataclass
class BasicBlock:
    stmts: list[Stmt]
    term: Term


# SSA variable identifier: `(base_name, version)`. Version 0 denotes an initial/parameter value.
@dataclass(frozen=True)
class SsaVar:
    name: str
    ver: int


@dataclass(frozen=True)
class SsaRhsConst:
    val: int


@dataclass(frozen=True)
class SsaRhsVar:
    var: SsaVar


@dataclass(frozen=True)
class SsaRhsBin:
    op: BinOp
    left: SsaVar
    right: SsaVar


SsaRhs = SsaRhsConst | SsaRhsVar | SsaRhsBin


# Block-entry merge node: picks `incoming[pred_bb]` based on which predecessor jumped here.
@dataclass
class Phi:
    dst: SsaVar
    incoming: dict[int, SsaVar]


@dataclass
class SsaStmt:
    dst: SsaVar
    rhs: SsaRhs


@dataclass(frozen=True)
class SsaTermJump:
    target: int


@dataclass(frozen=True)
class SsaTermBranch:
    cond: SsaVar
    then_bb: int
    else_bb: int


@dataclass(frozen=True)
class SsaTermReturn:
    var: SsaVar


SsaTerm = SsaTermJump | SsaTermBranch | SsaTermReturn


@dataclass
class SsaBlock:
    phis: list[Phi]
    stmts: list[SsaStmt]
    term: SsaTerm


@dataclass
class SsaProgram:
    """SSA CFG bundled with dominance sets, idom links, and dominance frontiers."""

    blocks: list[SsaBlock]
    preds: list[list[int]]
    dom: list[set[int]]
    idom: list[int | None]
    dom_frontiers: list[set[int]]


def successors_of(term: Term) -> list[int]:
    if isinstance(term, TermJump):
        return [term.target]
    if isinstance(term, TermBranch):
        return [term.then_bb, term.else_bb]
    return []


def compute_dominators(
    num_blocks: int, preds: list[list[int]], reachable: set[int]
) -> tuple[list[set[int]], list[int | None], list[list[int]]]:
    """Iteratively compute dominator sets, immediate dominators, and the dominator tree."""
    dom: list[set[int]] = [set() for _ in range(num_blocks)]
    for b in reachable:
        dom[b] = {0} if b == 0 else set(reachable)
    while True:
        changed = False
        for b in sorted(reachable):
            if b == 0:
                continue
            rpreds = [p for p in preds[b] if p in reachable]
            if not rpreds:
                continue
            inter = set(dom[rpreds[0]])
            for p in rpreds[1:]:
                inter &= dom[p]
            inter.add(b)
            if inter != dom[b]:
                dom[b] = inter
                changed = True
        if not changed:
            break
    idom: list[int | None] = [None] * num_blocks
    dom_children: list[list[int]] = [[] for _ in range(num_blocks)]
    for b in sorted(reachable):
        if b == 0:
            continue
        strict = sorted(d for d in dom[b] if d != b)
        # Immediate dominator is the strict dominator dominated by all other strict dominators.
        parent = next(
            (cand for cand in strict if all(other in dom[cand] for other in strict)),
            None,
        )
        idom[b] = parent
        if parent is not None:
            dom_children[parent].append(b)
    return dom, idom, dom_children


def compute_dominance_frontiers(
    preds: list[list[int]], idom: list[int | None], reachable: set[int]
) -> list[set[int]]:
    """Compute dominance frontiers via the Cooper-Harvey-Kennedy predecessor walk."""
    df: list[set[int]] = [set() for _ in preds]
    for b in sorted(reachable):
        rpreds = [p for p in preds[b] if p in reachable]
        if len(rpreds) >= 2:
            for p in rpreds:
                runner: int | None = p
                # Each visited node dominates `p` (a pred of `b`) without strictly dominating `b`.
                while runner is not None and runner != idom[b]:
                    df[runner].add(b)
                    runner = idom[runner]
    return df


def to_ssa(cfg: list[BasicBlock]) -> SsaProgram:
    """Convert a CFG to minimal pruned SSA form (dominance -> liveness -> phis -> DFS rename)."""
    n = len(cfg)
    preds: list[list[int]] = [[] for _ in range(n)]
    for b, blk in enumerate(cfg):
        for s in successors_of(blk.term):
            preds[s].append(b)

    # Filter to blocks reachable from entry 0 so dead blocks never pollute phis.
    reachable = {0}
    stack = [0]
    while stack:
        b = stack.pop()
        for s in successors_of(cfg[b].term):
            if s not in reachable:
                reachable.add(s)
                stack.append(s)

    dom, idom, dom_children = compute_dominators(n, preds, reachable)
    dom_frontiers = compute_dominance_frontiers(preds, idom, reachable)

    defs: dict[str, set[int]] = {}
    for b in sorted(reachable):
        for stmt in cfg[b].stmts:
            defs.setdefault(stmt.dst, set()).add(b)

    # Backward liveness analysis for pruned SSA: suppress phis when a variable is dead at entry.
    use_blk: list[set[str]] = [set() for _ in range(n)]
    def_blk: list[set[str]] = [set() for _ in range(n)]
    for b in sorted(reachable):
        for stmt in cfg[b].stmts:
            if isinstance(stmt.rhs, RhsVar) and stmt.rhs.name not in def_blk[b]:
                use_blk[b].add(stmt.rhs.name)
            elif isinstance(stmt.rhs, RhsBin):
                if stmt.rhs.left not in def_blk[b]:
                    use_blk[b].add(stmt.rhs.left)
                if stmt.rhs.right not in def_blk[b]:
                    use_blk[b].add(stmt.rhs.right)
            def_blk[b].add(stmt.dst)
        t = cfg[b].term
        if isinstance(t, TermBranch) and t.cond not in def_blk[b]:
            use_blk[b].add(t.cond)
        elif isinstance(t, TermReturn) and t.var not in def_blk[b]:
            use_blk[b].add(t.var)

    live_in: list[set[str]] = [set() for _ in range(n)]
    live_out: list[set[str]] = [set() for _ in range(n)]
    while True:
        changed = False
        for b in sorted(reachable, reverse=True):
            new_out: set[str] = set()
            for s in successors_of(cfg[b].term):
                new_out |= live_in[s]
            new_in = use_blk[b] | (new_out - def_blk[b])
            if new_out != live_out[b] or new_in != live_in[b]:
                live_out[b] = new_out
                live_in[b] = new_in
                changed = True
        if not changed:
            break

    # Iterated dominance-frontier worklist for minimal phi placement.
    phi_vars: list[set[str]] = [set() for _ in range(n)]
    for var, def_blocks in sorted(defs.items()):
        has_phi: set[int] = set()
        worklist = sorted(def_blocks)
        queued = set(def_blocks)
        while worklist:
            b = worklist.pop()
            for y in sorted(dom_frontiers[b]):
                if y not in has_phi:
                    has_phi.add(y)
                    if var in live_in[y]:
                        phi_vars[y].add(var)
                    if y not in queued:
                        queued.add(y)
                        worklist.append(y)

    ssa_blocks: list[SsaBlock] = []
    for b, blk in enumerate(cfg):
        phis = [Phi(SsaVar(v, 0), {}) for v in sorted(phi_vars[b])]
        stmts: list[SsaStmt] = []
        if b in reachable:
            for s in blk.stmts:
                if isinstance(s.rhs, RhsConst):
                    rhs: SsaRhs = SsaRhsConst(s.rhs.val)
                elif isinstance(s.rhs, RhsVar):
                    rhs = SsaRhsVar(SsaVar(s.rhs.name, 0))
                else:
                    rhs = SsaRhsBin(s.rhs.op, SsaVar(s.rhs.left, 0), SsaVar(s.rhs.right, 0))
                stmts.append(SsaStmt(SsaVar(s.dst, 0), rhs))
        if isinstance(blk.term, TermJump):
            term: SsaTerm = SsaTermJump(blk.term.target)
        elif isinstance(blk.term, TermBranch):
            term = SsaTermBranch(SsaVar(blk.term.cond, 0), blk.term.then_bb, blk.term.else_bb)
        else:
            term = SsaTermReturn(SsaVar(blk.term.var, 0))
        ssa_blocks.append(SsaBlock(phis, stmts, term))

    # Dominator-tree DFS renaming: `stacks[name][-1]` tracks the reaching definition in scope.
    counters: dict[str, int] = {}
    stacks: dict[str, list[int]] = {}

    def new_ver(name: str, pushed: list[str]) -> int:
        counters[name] = counters.get(name, 0) + 1
        ver = counters[name]
        stacks.setdefault(name, []).append(ver)
        pushed.append(name)
        return ver

    def cur_ver(name: str) -> int:
        s = stacks.get(name)
        return s[-1] if s else 0

    def rename(b: int) -> None:
        pushed: list[str] = []
        for phi in ssa_blocks[b].phis:
            phi.dst = SsaVar(phi.dst.name, new_ver(phi.dst.name, pushed))
        for stmt in ssa_blocks[b].stmts:
            if isinstance(stmt.rhs, SsaRhsVar):
                stmt.rhs = SsaRhsVar(SsaVar(stmt.rhs.var.name, cur_ver(stmt.rhs.var.name)))
            elif isinstance(stmt.rhs, SsaRhsBin):
                stmt.rhs = SsaRhsBin(
                    stmt.rhs.op,
                    SsaVar(stmt.rhs.left.name, cur_ver(stmt.rhs.left.name)),
                    SsaVar(stmt.rhs.right.name, cur_ver(stmt.rhs.right.name)),
                )
            stmt.dst = SsaVar(stmt.dst.name, new_ver(stmt.dst.name, pushed))
        t = ssa_blocks[b].term
        if isinstance(t, SsaTermBranch):
            ssa_blocks[b].term = SsaTermBranch(
                SsaVar(t.cond.name, cur_ver(t.cond.name)), t.then_bb, t.else_bb
            )
        elif isinstance(t, SsaTermReturn):
            ssa_blocks[b].term = SsaTermReturn(SsaVar(t.var.name, cur_ver(t.var.name)))
        # Fill outgoing edge phi incoming operands with versions live at the end of block `b`.
        for succ in successors_of(cfg[b].term):
            for phi in ssa_blocks[succ].phis:
                phi.incoming[b] = SsaVar(phi.dst.name, cur_ver(phi.dst.name))
        for child in dom_children[b]:
            rename(child)
        # Pop versions defined in `b` so sibling dominator subtrees see the outer version.
        for name in pushed:
            stacks[name].pop()

    rename(0)
    return SsaProgram(ssa_blocks, preds, dom, idom, dom_frontiers)


def verify_ssa(ssa: SsaProgram) -> None:
    """Verify unique definitions and strict dominance of every use (phi uses checked at pred)."""
    def_block: dict[SsaVar, int] = {}
    for b, blk in enumerate(ssa.blocks):
        for phi in blk.phis:
            if phi.dst in def_block:
                raise ValueError(f"duplicate SSA definition {phi.dst}")
            def_block[phi.dst] = b
        for stmt in blk.stmts:
            if stmt.dst in def_block:
                raise ValueError(f"duplicate SSA definition {stmt.dst}")
            def_block[stmt.dst] = b

    def check_use(var: SsaVar, use_bb: int) -> None:
        if var.ver == 0:
            return
        if var not in def_block:
            raise ValueError(f"use of undefined SSA var {var}")
        d_bb = def_block[var]
        if d_bb not in ssa.dom[use_bb]:
            raise ValueError(f"SSA def {var} in B{d_bb} does not dominate use in B{use_bb}")

    for b, blk in enumerate(ssa.blocks):
        if not ssa.dom[b]:
            continue
        for phi in blk.phis:
            for pred, var in phi.incoming.items():
                check_use(var, pred)
        for stmt in blk.stmts:
            if isinstance(stmt.rhs, SsaRhsVar):
                check_use(stmt.rhs.var, b)
            elif isinstance(stmt.rhs, SsaRhsBin):
                check_use(stmt.rhs.left, b)
                check_use(stmt.rhs.right, b)
        if isinstance(blk.term, SsaTermBranch):
            check_use(blk.term.cond, b)
        elif isinstance(blk.term, SsaTermReturn):
            check_use(blk.term.var, b)


def fold_ssa_constants(ssa: SsaProgram) -> None:
    """Propagate and fold SSA constants globally until a fixed point is reached."""
    while True:
        consts: dict[SsaVar, int] = {}
        for blk in ssa.blocks:
            for stmt in blk.stmts:
                if isinstance(stmt.rhs, SsaRhsConst):
                    consts[stmt.dst] = stmt.rhs.val
        changed = False
        for blk in ssa.blocks:
            for phi in blk.phis:
                if phi.incoming and all(v in consts for v in phi.incoming.values()):
                    vals = {consts[v] for v in phi.incoming.values()}
                    if len(vals) == 1:
                        consts[phi.dst] = next(iter(vals))
            for stmt in blk.stmts:
                if isinstance(stmt.rhs, SsaRhsVar) and stmt.rhs.var in consts:
                    stmt.rhs = SsaRhsConst(consts[stmt.rhs.var])
                    changed = True
                elif (
                    isinstance(stmt.rhs, SsaRhsBin)
                    and stmt.rhs.left in consts
                    and stmt.rhs.right in consts
                ):
                    a, b_val = consts[stmt.rhs.left], consts[stmt.rhs.right]
                    if stmt.rhs.op == BinOp.ADD:
                        c = a + b_val
                    elif stmt.rhs.op == BinOp.SUB:
                        c = a - b_val
                    else:
                        c = 1 if a < b_val else 0
                    stmt.rhs = SsaRhsConst(c)
                    changed = True
        if not changed:
            break


def ssa_lowered_name(v: SsaVar) -> str:
    return v.name if v.ver == 0 else f"{v.name}_{v.ver}"


def de_ssa(ssa: SsaProgram) -> list[BasicBlock]:
    """Lower SSA phis to sequential edge copies using temporaries and critical-edge splitting."""
    blocks: list[BasicBlock] = []
    for blk in ssa.blocks:
        stmts = []
        for s in blk.stmts:
            dst = ssa_lowered_name(s.dst)
            if isinstance(s.rhs, SsaRhsConst):
                rhs: Rhs = RhsConst(s.rhs.val)
            elif isinstance(s.rhs, SsaRhsVar):
                rhs = RhsVar(ssa_lowered_name(s.rhs.var))
            else:
                rhs = RhsBin(
                    s.rhs.op,
                    ssa_lowered_name(s.rhs.left),
                    ssa_lowered_name(s.rhs.right),
                )
            stmts.append(Stmt(dst, rhs))
        if isinstance(blk.term, SsaTermJump):
            term: Term = TermJump(blk.term.target)
        elif isinstance(blk.term, SsaTermBranch):
            term = TermBranch(
                ssa_lowered_name(blk.term.cond),
                blk.term.then_bb,
                blk.term.else_bb,
            )
        else:
            term = TermReturn(ssa_lowered_name(blk.term.var))
        blocks.append(BasicBlock(stmts, term))

    for succ_idx, blk in enumerate(ssa.blocks):
        if not blk.phis:
            continue
        by_pred: dict[int, list[tuple[str, str]]] = {}
        for phi in blk.phis:
            dst = ssa_lowered_name(phi.dst)
            for pred, src in sorted(phi.incoming.items()):
                by_pred.setdefault(pred, []).append((dst, ssa_lowered_name(src)))
        for pred, copies in sorted(by_pred.items()):
            # Two-phase copy via __phi_tmp_* preserves simultaneous phi evaluation semantics.
            copy_stmts = [
                Stmt(f"__phi_tmp_{succ_idx}_{idx}", RhsVar(src))
                for idx, (_, src) in enumerate(copies)
            ] + [
                Stmt(dst, RhsVar(f"__phi_tmp_{succ_idx}_{idx}"))
                for idx, (dst, _) in enumerate(copies)
            ]
            if len(successors_of(blocks[pred].term)) == 1:
                blocks[pred].stmts.extend(copy_stmts)
            else:
                # Split critical edge pred -> succ_idx with a dedicated trampoline block.
                tramp = len(blocks)
                blocks.append(BasicBlock(copy_stmts, TermJump(succ_idx)))
                t = blocks[pred].term
                assert isinstance(t, TermBranch)
                blocks[pred].term = TermBranch(
                    t.cond,
                    tramp if t.then_bb == succ_idx else t.then_bb,
                    tramp if t.else_bb == succ_idx else t.else_bb,
                )
    return blocks


def eval_cfg(blocks: list[BasicBlock], initial: dict[str, int]) -> int:
    env = dict(initial)
    bb = 0
    for _ in range(10_000):
        for stmt in blocks[bb].stmts:
            if isinstance(stmt.rhs, RhsConst):
                env[stmt.dst] = stmt.rhs.val
            elif isinstance(stmt.rhs, RhsVar):
                env[stmt.dst] = env.get(stmt.rhs.name, 0)
            else:
                lv = env.get(stmt.rhs.left, 0)
                rv = env.get(stmt.rhs.right, 0)
                if stmt.rhs.op == BinOp.ADD:
                    env[stmt.dst] = lv + rv
                elif stmt.rhs.op == BinOp.SUB:
                    env[stmt.dst] = lv - rv
                else:
                    env[stmt.dst] = 1 if lv < rv else 0
        t = blocks[bb].term
        if isinstance(t, TermJump):
            bb = t.target
        elif isinstance(t, TermBranch):
            bb = t.then_bb if env.get(t.cond, 0) != 0 else t.else_bb
        else:
            return env.get(t.var, 0)
    raise RuntimeError("infinite loop")


def eval_ssa(ssa: SsaProgram, initial: dict[str, int]) -> int:
    env: dict[SsaVar, int] = {SsaVar(k, 0): v for k, v in initial.items()}
    bb = 0
    prev_bb: int | None = None
    for _ in range(10_000):
        if prev_bb is not None:
            updates = [(phi.dst, env.get(phi.incoming[prev_bb], 0)) for phi in ssa.blocks[bb].phis]
            for dst, val in updates:
                env[dst] = val
        for stmt in ssa.blocks[bb].stmts:
            if isinstance(stmt.rhs, SsaRhsConst):
                env[stmt.dst] = stmt.rhs.val
            elif isinstance(stmt.rhs, SsaRhsVar):
                env[stmt.dst] = env.get(stmt.rhs.var, 0)
            else:
                lv = env.get(stmt.rhs.left, 0)
                rv = env.get(stmt.rhs.right, 0)
                if stmt.rhs.op == BinOp.ADD:
                    env[stmt.dst] = lv + rv
                elif stmt.rhs.op == BinOp.SUB:
                    env[stmt.dst] = lv - rv
                else:
                    env[stmt.dst] = 1 if lv < rv else 0
        prev_bb = bb
        t = ssa.blocks[bb].term
        if isinstance(t, SsaTermJump):
            bb = t.target
        elif isinstance(t, SsaTermBranch):
            bb = t.then_bb if env.get(t.cond, 0) != 0 else t.else_bb
        else:
            return env.get(t.var, 0)
    raise RuntimeError("infinite loop")


def loop_sample() -> list[BasicBlock]:
    return [
        BasicBlock([Stmt("i", RhsConst(0)), Stmt("s", RhsConst(0))], TermJump(1)),
        BasicBlock(
            [Stmt("cond", RhsBin(BinOp.LT, "i", "n"))],
            TermBranch("cond", 2, 3),
        ),
        BasicBlock(
            [
                Stmt("s", RhsBin(BinOp.ADD, "s", "i")),
                Stmt("one", RhsConst(1)),
                Stmt("i", RhsBin(BinOp.ADD, "i", "one")),
            ],
            TermJump(1),
        ),
        BasicBlock([], TermReturn("s")),
    ]


def main() -> None:
    cfg = loop_sample()
    ssa = to_ssa(cfg)
    verify_ssa(ssa)
    print(f"Dominance frontiers: {[sorted(s) for s in ssa.dom_frontiers]}")
    print(f"Immediate dominators: {ssa.idom}")
    for b, blk in enumerate(ssa.blocks):
        print(f"B{b}:")
        for phi in blk.phis:
            print(f"  phi {phi.dst} <- {phi.incoming}")
        for stmt in blk.stmts:
            print(f"  {stmt.dst} = {stmt.rhs}")
        print(f"  term {blk.term}")
    fold_ssa_constants(ssa)
    lowered = de_ssa(ssa)
    print(
        f"Sum 0..5: original={eval_cfg(cfg, {'n': 5})}, "
        f"ssa={eval_ssa(ssa, {'n': 5})}, de-ssa={eval_cfg(lowered, {'n': 5})}"
    )


class SsaTests(unittest.TestCase):
    def test_diamond_places_phi_only_at_join_and_preserves_semantics(self) -> None:
        diamond = [
            BasicBlock([Stmt("x", RhsConst(10))], TermBranch("flag", 1, 2)),
            BasicBlock([Stmt("x", RhsConst(20))], TermJump(3)),
            BasicBlock([Stmt("x", RhsConst(20))], TermJump(3)),
            BasicBlock([Stmt("y", RhsBin(BinOp.SUB, "x", "z"))], TermReturn("y")),
            BasicBlock([Stmt("dead", RhsConst(99))], TermJump(3)),
        ]
        ssa = to_ssa(diamond)
        verify_ssa(ssa)
        self.assertEqual(len(ssa.blocks[0].phis), 0)
        self.assertEqual(len(ssa.blocks[3].phis), 1)
        self.assertEqual(ssa.blocks[3].phis[0].dst.name, "x")
        self.assertEqual(len(ssa.blocks[4].stmts), 0)
        fold_ssa_constants(ssa)
        lowered = de_ssa(ssa)
        for flag in (0, 1):
            orig = eval_cfg(diamond, {"flag": flag, "z": 3})
            self.assertEqual(orig, 17)
            self.assertEqual(eval_ssa(ssa, {"flag": flag, "z": 3}), orig)
            self.assertEqual(eval_cfg(lowered, {"flag": flag, "z": 3}), orig)

    def test_loop_places_header_phi_with_distinct_preheader_and_latch_versions(self) -> None:
        cfg = loop_sample()
        ssa = to_ssa(cfg)
        verify_ssa(ssa)
        self.assertEqual(len(ssa.blocks[1].phis), 2)
        for phi in ssa.blocks[1].phis:
            self.assertNotEqual(phi.incoming[0], phi.incoming[2])
        fold_ssa_constants(ssa)
        lowered = de_ssa(ssa)
        for n in (0, 1, 5, 10):
            expected = sum(range(n))
            self.assertEqual(eval_cfg(cfg, {"n": n}), expected)
            self.assertEqual(eval_ssa(ssa, {"n": n}), expected)
            self.assertEqual(eval_cfg(lowered, {"n": n}), expected)


if __name__ == "__main__":
    main()
