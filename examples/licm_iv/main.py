"""Loop-Invariant Code Motion (with dominance/exit-safety checks) and Induction-Variable Strength Reduction."""

from dataclasses import dataclass
from enum import Enum
import unittest


@dataclass(frozen=True)
class ConstOp:
    val: int


@dataclass(frozen=True)
class VarOp:
    name: str


Operand = ConstOp | VarOp


class Op(Enum):
    ADD = "+"
    MUL = "*"
    LT = "<"


@dataclass(frozen=True)
class AssignStmt:
    dst: str
    op: Op
    lhs: Operand
    rhs: Operand


@dataclass(frozen=True)
class CopyStmt:
    dst: str
    src: Operand


Stmt = AssignStmt | CopyStmt


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


def succs(t: Term) -> list[int]:
    if isinstance(t, TermJump):
        return [t.target]
    if isinstance(t, TermBranch):
        return [t.then_bb, t.else_bb]
    return []


def compute_doms(blocks: list[BasicBlock]) -> list[set[int]]:
    n = len(blocks)
    preds: list[list[int]] = [[] for _ in range(n)]
    for b, blk in enumerate(blocks):
        for s in succs(blk.term):
            preds[s].append(b)
    dom: list[set[int]] = [set(range(n)) for _ in range(n)]
    dom[0] = {0}
    while True:
        changed = False
        for b in range(1, n):
            if not preds[b]:
                continue
            inter = set.intersection(*(dom[p] for p in preds[b])) | {b}
            if inter != dom[b]:
                dom[b] = inter
                changed = True
        if not changed:
            break
    return dom


def natural_loop(blocks: list[BasicBlock], header: int, latch: int) -> set[int]:
    preds: list[list[int]] = [[] for _ in range(len(blocks))]
    for b, blk in enumerate(blocks):
        for s in succs(blk.term):
            preds[s].append(b)
    loop_set = {header, latch}
    stack = [] if latch == header else [latch]
    while stack:
        m = stack.pop()
        for p in preds[m]:
            if p not in loop_set:
                loop_set.add(p)
                stack.append(p)
    return loop_set


def stmt_uses(s: Stmt) -> list[str]:
    if isinstance(s, AssignStmt):
        return [o.name for o in (s.lhs, s.rhs) if isinstance(o, VarOp)]
    return [s.src.name] if isinstance(s.src, VarOp) else []


def compute_live_in(blocks: list[BasicBlock]) -> list[set[str]]:
    n = len(blocks)
    use_b: list[set[str]] = [set() for _ in range(n)]
    def_b: list[set[str]] = [set() for _ in range(n)]
    for b, blk in enumerate(blocks):
        for s in blk.stmts:
            for u in stmt_uses(s):
                if u not in def_b[b]:
                    use_b[b].add(u)
            def_b[b].add(s.dst)
        if isinstance(blk.term, TermBranch) and blk.term.cond not in def_b[b]:
            use_b[b].add(blk.term.cond)
        elif isinstance(blk.term, TermReturn) and blk.term.var not in def_b[b]:
            use_b[b].add(blk.term.var)
    live_in: list[set[str]] = [set() for _ in range(n)]
    while True:
        changed = False
        for b in range(n - 1, -1, -1):
            out = set().union(*(live_in[s] for s in succs(blocks[b].term))) if succs(blocks[b].term) else set()
            inn = use_b[b] | (out - def_b[b])
            if inn != live_in[b]:
                live_in[b] = inn
                changed = True
        if not changed:
            break
    return live_in


def licm_hoist(
    blocks: list[BasicBlock], preheader: int, header: int, latch: int
) -> tuple[list[BasicBlock], list[str], list[str]]:
    dom = compute_doms(blocks)
    loop_nodes = natural_loop(blocks, header, latch)
    live_in = compute_live_in(blocks)
    def_count: dict[str, int] = {}
    for b in loop_nodes:
        for s in blocks[b].stmts:
            def_count[s.dst] = def_count.get(s.dst, 0) + 1

    exits: list[int] = []
    exit_targets: set[int] = set()
    for b in loop_nodes:
        for s in succs(blocks[b].term):
            if s not in loop_nodes:
                exits.append(b)
                exit_targets.add(s)
    live_after_loop = set().union(*(live_in[t] for t in exit_targets)) if exit_targets else set()

    invariant: set[tuple[int, int]] = set()
    inv_vars: set[str] = set()
    while True:
        changed = False
        for b in sorted(loop_nodes):
            for si, s in enumerate(blocks[b].stmts):
                if (b, si) in invariant:
                    continue
                if all(def_count.get(u, 0) == 0 or u in inv_vars for u in stmt_uses(s)) and def_count.get(s.dst, 0) == 1:
                    invariant.add((b, si))
                    inv_vars.add(s.dst)
                    changed = True
        if not changed:
            break

    out = [BasicBlock(list(blk.stmts), blk.term) for blk in blocks]
    hoisted: list[str] = []
    skipped: list[str] = []
    for b in sorted(loop_nodes):
        kept = []
        for si, s in enumerate(out[b].stmts):
            if (b, si) in invariant:
                dom_all_exits = all(b in dom[ex] for ex in exits)
                dead_after = s.dst not in live_after_loop
                if dom_all_exits or dead_after:
                    out[preheader].stmts.append(s)
                    hoisted.append(s.dst)
                else:
                    skipped.append(s.dst)
                    kept.append(s)
            else:
                kept.append(s)
        out[b].stmts = kept
    return out, hoisted, skipped


def strength_reduce_iv(
    blocks: list[BasicBlock], preheader: int, header: int, latch: int
) -> tuple[list[BasicBlock], list[tuple[str, str, int]], bool]:
    loop_nodes = natural_loop(blocks, header, latch)
    basic_ivs: dict[str, int] = {}
    for b in loop_nodes:
        for s in blocks[b].stmts:
            if (
                isinstance(s, AssignStmt)
                and s.op == Op.ADD
                and isinstance(s.lhs, VarOp)
                and s.lhs.name == s.dst
                and isinstance(s.rhs, ConstOp)
            ):
                basic_ivs[s.dst] = s.rhs.val

    derived: list[tuple[str, str, int]] = []
    for b in sorted(loop_nodes):
        for s in blocks[b].stmts:
            if (
                isinstance(s, AssignStmt)
                and s.op == Op.MUL
                and isinstance(s.lhs, VarOp)
                and s.lhs.name in basic_ivs
                and isinstance(s.rhs, ConstOp)
            ):
                derived.append((s.dst, s.lhs.name, s.rhs.val))

    out = [BasicBlock(list(blk.stmts), blk.term) for blk in blocks]
    if not derived:
        return out, derived, False

    for var, base_iv, scale in derived:
        step = basic_ivs[base_iv]
        sr_var = f"{var}_sr"
        init_mul = f"{var}_init_mul"
        out[preheader].stmts.append(AssignStmt(init_mul, Op.MUL, VarOp(base_iv), ConstOp(scale)))
        out[preheader].stmts.append(AssignStmt(sr_var, Op.ADD, VarOp(init_mul), ConstOp(0)))
        for b in loop_nodes:
            new_stmts: list[Stmt] = []
            for s in out[b].stmts:
                if (
                    isinstance(s, AssignStmt)
                    and s.dst == var
                    and s.op == Op.MUL
                    and s.lhs == VarOp(base_iv)
                    and s.rhs == ConstOp(scale)
                ):
                    new_stmts.append(CopyStmt(var, VarOp(sr_var)))
                elif (
                    isinstance(s, AssignStmt)
                    and s.dst == base_iv
                    and s.op == Op.ADD
                    and s.lhs == VarOp(base_iv)
                    and s.rhs == ConstOp(step)
                ):
                    new_stmts.append(s)
                    new_stmts.append(AssignStmt(sr_var, Op.ADD, VarOp(sr_var), ConstOp(scale * step)))
                else:
                    new_stmts.append(s)
            out[b].stmts = new_stmts

    var0, base_iv0, scale0 = derived[0]
    sr_var0 = f"{var0}_sr"
    exit_rewritten = False
    for idx, s in enumerate(out[header].stmts):
        if (
            isinstance(s, AssignStmt)
            and s.op == Op.LT
            and isinstance(s.lhs, VarOp)
            and s.lhs.name == base_iv0
            and isinstance(s.rhs, VarOp)
        ):
            lim_scaled = f"{s.rhs.name}_scaled"
            out[preheader].stmts.append(AssignStmt(lim_scaled, Op.MUL, s.rhs, ConstOp(scale0)))
            out[header].stmts[idx] = AssignStmt(s.dst, Op.LT, VarOp(sr_var0), VarOp(lim_scaled))
            exit_rewritten = True

    if exit_rewritten:
        live_in = compute_live_in(out)
        used_in_loop = any(
            base_iv0 in stmt_uses(s)
            for b in loop_nodes
            for s in out[b].stmts
            if not (isinstance(s, AssignStmt) and s.dst == base_iv0 and s.op == Op.ADD and s.lhs == VarOp(base_iv0))
        )
        live_outside = any(
            base_iv0 in live_in[succ]
            for b in loop_nodes
            for succ in succs(out[b].term)
            if succ not in loop_nodes
        )
        if not used_in_loop and not live_outside:
            for b in loop_nodes:
                out[b].stmts = [
                    s
                    for s in out[b].stmts
                    if not (
                        isinstance(s, AssignStmt)
                        and s.dst == base_iv0
                        and s.op == Op.ADD
                        and s.lhs == VarOp(base_iv0)
                    )
                ]
    return out, derived, exit_rewritten


def eval_cfg(blocks: list[BasicBlock], initial: dict[str, int]) -> tuple[int, int]:
    env = dict(initial)
    bb = 0
    muls = 0

    def op_val(op: Operand) -> int:
        return op.val if isinstance(op, ConstOp) else env.get(op.name, 0)

    for _ in range(50_000):
        for s in blocks[bb].stmts:
            if isinstance(s, AssignStmt):
                l, r = op_val(s.lhs), op_val(s.rhs)
                if s.op == Op.MUL:
                    muls += 1
                    env[s.dst] = l * r
                elif s.op == Op.ADD:
                    env[s.dst] = l + r
                else:
                    env[s.dst] = 1 if l < r else 0
            else:
                env[s.dst] = op_val(s.src)
        t = blocks[bb].term
        if isinstance(t, TermJump):
            bb = t.target
        elif isinstance(t, TermBranch):
            bb = t.then_bb if env.get(t.cond, 0) != 0 else t.else_bb
        else:
            return env.get(t.var, 0), muls
    raise RuntimeError("infinite loop")


def sample_loop() -> list[BasicBlock]:
    return [
        BasicBlock([CopyStmt("i", ConstOp(0)), CopyStmt("sum", ConstOp(0))], TermJump(1)),
        BasicBlock([AssignStmt("cond", Op.LT, VarOp("i"), VarOp("n"))], TermBranch("cond", 2, 5)),
        BasicBlock(
            [AssignStmt("safe_inv", Op.MUL, VarOp("a"), VarOp("b"))],
            TermBranch("flag", 3, 4),
        ),
        BasicBlock([AssignStmt("unsafe_inv", Op.MUL, VarOp("a"), ConstOp(10))], TermJump(4)),
        BasicBlock(
            [
                AssignStmt("t", Op.MUL, VarOp("i"), ConstOp(4)),
                AssignStmt("sum1", Op.ADD, VarOp("sum"), VarOp("t")),
                AssignStmt("sum", Op.ADD, VarOp("sum1"), VarOp("safe_inv")),
                AssignStmt("i", Op.ADD, VarOp("i"), ConstOp(1)),
            ],
            TermJump(1),
        ),
        BasicBlock([AssignStmt("res", Op.ADD, VarOp("sum"), VarOp("unsafe_inv"))], TermReturn("res")),
    ]


def main() -> None:
    cfg = sample_loop()
    after_licm, hoisted, skipped = licm_hoist(cfg, 0, 1, 4)
    after_sr, derived, exit_rw = strength_reduce_iv(after_licm, 0, 1, 4)
    print(f"LICM hoisted={hoisted}, skipped_unsafe={skipped}")
    print(f"Strength reduced={derived}, exit_rewritten={exit_rw}")
    init = {"n": 6, "a": 3, "b": 5, "flag": 0, "unsafe_inv": 77}
    print(f"Orig={eval_cfg(cfg, init)}, Opt={eval_cfg(after_sr, init)}")


class LicmIvTests(unittest.TestCase):
    def test_skips_unsafe_guarded_invariant_while_hoisting_safe(self) -> None:
        cfg = sample_loop()
        after_licm, hoisted, skipped = licm_hoist(cfg, 0, 1, 4)
        self.assertEqual(hoisted, ["safe_inv"])
        self.assertEqual(skipped, ["unsafe_inv"])
        for flag in (0, 1):
            init = {"n": 5, "a": 3, "b": 5, "flag": flag, "unsafe_inv": 77}
            self.assertEqual(eval_cfg(after_licm, init)[0], eval_cfg(cfg, init)[0])

    def test_strength_reduces_derived_iv_and_cuts_muls(self) -> None:
        cfg = sample_loop()
        after_licm, _, _ = licm_hoist(cfg, 0, 1, 4)
        after_sr, derived, exit_rw = strength_reduce_iv(after_licm, 0, 1, 4)
        self.assertEqual(len(derived), 1)
        self.assertTrue(exit_rw)
        for flag in (0, 1):
            for n in (0, 1, 8):
                init = {"n": n, "a": 3, "b": 5, "flag": flag, "unsafe_inv": 77}
                v0, m0 = eval_cfg(cfg, init)
                v1, m1 = eval_cfg(after_sr, init)
                self.assertEqual(v1, v0)
                if n >= 3:
                    self.assertLess(m1, m0)


if __name__ == "__main__":
    main()
