"""Chapter 10 (§10.4): Hardware-Predicated Execution & Full If-Converted Hyperblocks.

Demonstrates:
- Hyperblock region selection (merging balanced conditional arms while leaving
  ultra-cold error exits outside as side exits),
- Full control-dependence if-conversion using unconditional partition compares
  (`cmp.zero.un pT, pF = cond if p_guard`) and OR-type predicate merges
  (`PredClear` + `PredOrTrue`),
- Exact predicate disjointness proving (`PredTree.are_disjoint`) to eliminate
  false WAW/WAR register and memory dependences across mutually exclusive paths,
- Speculative predicate promotion (`p_child -> p0`) for pure non-trapping ops
  while proving trapping (`DivChecked`) and `Store` ops must keep their guards,
- Width-2 predicated hyperblock list scheduling vs. expected CFG cycles, and
- Cycle-accurate predicated VLIW hardware VM proving exact register, memory,
  and trap-squash equivalence across all hot, balanced, and cold-exit paths.
"""

from __future__ import annotations

from dataclasses import dataclass, field
import unittest


# ---------------------------------------------------------------------------
# 1. CFG Intermediate Representation
# ---------------------------------------------------------------------------


@dataclass(frozen=True)
class Op:
    kind: str
    dst: str | None = None
    src: str | None = None
    lhs: str | None = None
    rhs: str | None = None
    val: int | None = None
    imm: int | None = None
    num: int | None = None
    den: str | None = None
    slot: int | None = None

    def uses(self) -> list[str]:
        if self.kind in ("Const", "LoadSafe"):
            return []
        if self.kind in ("AddImm", "MulImm", "Store"):
            assert self.src is not None
            return [self.src]
        if self.kind == "Add":
            assert self.lhs is not None and self.rhs is not None
            return [self.lhs, self.rhs]
        if self.kind == "DivChecked":
            assert self.den is not None
            return [self.den]
        raise ValueError(f"unknown op kind: {self.kind}")

    def is_promotion_safe(self) -> bool:
        # Only pure, non-trapping, non-memory ops may be predicate-promoted
        # above a branch/compare without hardware support for speculative traps.
        return self.kind not in ("DivChecked", "Store")

    def writes_mem(self) -> bool:
        return self.kind == "Store"

    def reads_mem(self) -> bool:
        return self.kind == "LoadSafe"

    def __str__(self) -> str:
        if self.kind == "Const":
            return f"{self.dst} = {self.val}"
        if self.kind == "LoadSafe":
            return f"{self.dst} = load_safe(mem[{self.slot}])"
        if self.kind == "AddImm":
            return f"{self.dst} = {self.src} + {self.imm}"
        if self.kind == "MulImm":
            return f"{self.dst} = {self.src} * {self.imm}"
        if self.kind == "Add":
            return f"{self.dst} = {self.lhs} + {self.rhs}"
        if self.kind == "DivChecked":
            return f"{self.dst} = {self.num} / {self.den} [trap]"
        if self.kind == "Store":
            return f"mem[{self.slot}] = {self.src} [effect]"
        raise ValueError(f"unknown op kind: {self.kind}")


@dataclass(frozen=True)
class Inst:
    op: Op
    latency: int


@dataclass(frozen=True)
class Term:
    kind: str  # "BrZero" | "Jump" | "Ret"
    cond: str | None = None
    if_zero: int | None = None
    if_nonzero: int | None = None
    target: int | None = None
    ret_reg: str | None = None


@dataclass
class Block:
    freq: int
    insts: list[Inst]
    term: Term

    def succs(self) -> list[int]:
        if self.term.kind == "BrZero":
            assert self.term.if_zero is not None and self.term.if_nonzero is not None
            return [self.term.if_zero, self.term.if_nonzero]
        if self.term.kind == "Jump":
            assert self.term.target is not None
            return [self.term.target]
        return []


@dataclass
class CfgProgram:
    blocks: dict[int, Block]
    entry: int
    params: list[str]


# ---------------------------------------------------------------------------
# 2. CFG Liveness Analysis & Hyperblock Region Selection
# ---------------------------------------------------------------------------


def compute_liveness(cfg: CfgProgram) -> dict[int, set[str]]:
    live_in: dict[int, set[str]] = {bid: set() for bid in cfg.blocks}
    while True:
        changed = False
        for bid in sorted(cfg.blocks, reverse=True):
            blk = cfg.blocks[bid]
            cur_in: set[str] = set()
            for s in blk.succs():
                cur_in |= live_in[s]
            if blk.term.kind == "BrZero" and blk.term.cond is not None:
                cur_in.add(blk.term.cond)
            elif blk.term.kind == "Ret" and blk.term.ret_reg is not None:
                cur_in.add(blk.term.ret_reg)
            for inst in reversed(blk.insts):
                if inst.op.dst is not None:
                    cur_in.discard(inst.op.dst)
                for u in inst.op.uses():
                    cur_in.add(u)
            if cur_in != live_in[bid]:
                live_in[bid] = cur_in
                changed = True
        if not changed:
            break
    return live_in


def topo_sort_cfg(cfg: CfgProgram) -> list[int]:
    indeg: dict[int, int] = {bid: 0 for bid in cfg.blocks}
    for blk in cfg.blocks.values():
        for s in blk.succs():
            indeg[s] = indeg.get(s, 0) + 1
    ready = sorted(bid for bid, d in indeg.items() if d == 0)
    order: list[int] = []
    while ready:
        u = ready.pop(0)
        order.append(u)
        for s in cfg.blocks[u].succs():
            indeg[s] -= 1
            if indeg[s] == 0:
                ready.append(s)
                ready.sort()
    return order


def select_hyperblock(cfg: CfgProgram, min_ratio_pct: int) -> set[int]:
    """Hyperblock selection (§10.4): unlike trace selection (one linear path),
    hyperblocks include BOTH arms of frequent/balanced branches while excluding
    low-frequency trap/error handlers (< `min_ratio_pct`% of entry)."""
    entry_freq = cfg.blocks[cfg.entry].freq
    selected: set[int] = set()
    for bid in topo_sort_cfg(cfg):
        blk = cfg.blocks[bid]
        reachable = bid == cfg.entry or any(
            bid in cfg.blocks[p].succs() for p in selected
        )
        if reachable and blk.freq * 100 >= entry_freq * min_ratio_pct:
            selected.add(bid)
    return selected


# ---------------------------------------------------------------------------
# 3. Predicate Hierarchy Tree & Exact Disjointness Solver
# ---------------------------------------------------------------------------


@dataclass(frozen=True)
class PredDef:
    kind: str  # "TrueRoot" | "PartitionChild" | "JoinOr"
    parent: int | None = None
    sibling: int | None = None
    inputs: tuple[int, ...] = ()


class PredTree:
    def __init__(self) -> None:
        self.defs: list[PredDef] = [PredDef(kind="TrueRoot")]

    def alloc_partition(self, parent: int) -> tuple[int, int]:
        p_left = len(self.defs)
        p_right = p_left + 1
        self.defs.append(PredDef(kind="PartitionChild", parent=parent, sibling=p_right))
        self.defs.append(PredDef(kind="PartitionChild", parent=parent, sibling=p_left))
        return p_left, p_right

    def alloc_join(self, inputs: list[int]) -> int:
        p = len(self.defs)
        self.defs.append(PredDef(kind="JoinOr", inputs=tuple(inputs)))
        return p

    def atomic_ancestors(self, p: int) -> set[int]:
        ancs: set[int] = set()
        cur = p
        while True:
            ancs.add(cur)
            d = self.defs[cur]
            if d.kind == "PartitionChild":
                assert d.parent is not None
                cur = d.parent
            else:
                break
        return ancs

    def are_disjoint(self, a: int, b: int) -> bool:
        if a == b:
            return False
        da = self.defs[a]
        if da.kind == "JoinOr":
            return all(self.are_disjoint(in_p, b) for in_p in da.inputs)
        db = self.defs[b]
        if db.kind == "JoinOr":
            return all(self.are_disjoint(a, in_p) for in_p in db.inputs)
        anc_a = self.atomic_ancestors(a)
        anc_b = self.atomic_ancestors(b)
        for node in anc_a:
            dn = self.defs[node]
            if dn.kind == "PartitionChild" and dn.sibling in anc_b:
                return True
        return False

    def nearest_atomic_parent(self, p: int) -> int | None:
        d = self.defs[p]
        return d.parent if d.kind == "PartitionChild" else None


# ---------------------------------------------------------------------------
# 4. Predicated Hyperblock IR & Control-Dependence If-Conversion (§10.4)
# ---------------------------------------------------------------------------


@dataclass(frozen=True)
class HOp:
    kind: str  # "Core" | "CmpZeroUn" | "PredClear" | "PredOrTrue" | "SideExit" | "Ret"
    core: Op | None = None
    p_true: int | None = None
    p_false: int | None = None
    src: str | None = None
    p_dst: int | None = None
    target: int | None = None
    ret_reg: str | None = None

    def dst_reg(self) -> str | None:
        return self.core.dst if self.kind == "Core" and self.core is not None else None

    def uses_regs(self) -> list[str]:
        if self.kind == "Core" and self.core is not None:
            return self.core.uses()
        if self.kind == "CmpZeroUn":
            assert self.src is not None
            return [self.src]
        if self.kind == "Ret":
            assert self.ret_reg is not None
            return [self.ret_reg]
        return []

    def defines_preds(self) -> list[int]:
        if self.kind == "CmpZeroUn":
            assert self.p_true is not None and self.p_false is not None
            return [self.p_true, self.p_false]
        if self.kind in ("PredClear", "PredOrTrue"):
            assert self.p_dst is not None
            return [self.p_dst]
        return []

    def __str__(self) -> str:
        if self.kind == "Core":
            return str(self.core)
        if self.kind == "CmpZeroUn":
            return f"cmp.zero.un p{self.p_true}, p{self.p_false} = {self.src}"
        if self.kind == "PredClear":
            return f"p{self.p_dst} = false"
        if self.kind == "PredOrTrue":
            return f"p{self.p_dst} |= true"
        if self.kind == "SideExit":
            return f"exit B{self.target}"
        if self.kind == "Ret":
            return f"ret {self.ret_reg}"
        raise ValueError(f"unknown HOp: {self.kind}")


@dataclass
class HInst:
    pred: int
    orig_pred: int
    op: HOp
    latency: int
    promoted: bool = False

    def __str__(self) -> str:
        if self.promoted:
            return f"(p{self.pred}) {self.op} [from p{self.orig_pred}]"
        return f"(p{self.pred}) {self.op}"


@dataclass
class Hyperblock:
    blocks: set[int]
    pred_tree: PredTree
    insts: list[HInst]
    block_pred: dict[int, int]
    split_sibling_target: dict[int, int]
    side_exits: set[int]


def if_convert(cfg: CfgProgram, hb_blocks: set[int]) -> Hyperblock:
    pred_tree = PredTree()
    incoming: dict[int, list[int]] = {}
    block_pred: dict[int, int] = {}
    split_sibling_target: dict[int, int] = {}
    side_exits: set[int] = set()
    insts: list[HInst] = []

    for b in topo_sort_cfg(cfg):
        if b not in hb_blocks:
            continue
        if b == cfg.entry:
            p_b = 0
        else:
            inc = sorted(set(incoming.get(b, [])))
            assert inc, f"Hyperblock block B{b} has no incoming pred"
            if len(inc) == 1:
                p_b = inc[0]
            else:
                merged_parent: int | None = None
                if len(inc) == 2:
                    par0 = pred_tree.nearest_atomic_parent(inc[0])
                    par1 = pred_tree.nearest_atomic_parent(inc[1])
                    d0 = pred_tree.defs[inc[0]]
                    if (
                        par0 is not None
                        and par0 == par1
                        and d0.kind == "PartitionChild"
                        and d0.sibling == inc[1]
                    ):
                        merged_parent = par0

                if merged_parent is not None:
                    p_b = merged_parent
                else:
                    p_join = pred_tree.alloc_join(inc)
                    insts.append(
                        HInst(
                            pred=0,
                            orig_pred=0,
                            op=HOp(kind="PredClear", p_dst=p_join),
                            latency=1,
                        )
                    )
                    for p_in in inc:
                        insts.append(
                            HInst(
                                pred=p_in,
                                orig_pred=p_in,
                                op=HOp(kind="PredOrTrue", p_dst=p_join),
                                latency=1,
                            )
                        )
                    p_b = p_join
        block_pred[b] = p_b

        blk = cfg.blocks[b]
        for inst in blk.insts:
            insts.append(
                HInst(
                    pred=p_b,
                    orig_pred=p_b,
                    op=HOp(kind="Core", core=inst.op),
                    latency=inst.latency,
                )
            )

        if blk.term.kind == "BrZero":
            assert blk.term.if_zero is not None and blk.term.if_nonzero is not None
            p_zero, p_nz = pred_tree.alloc_partition(p_b)
            split_sibling_target[p_zero] = blk.term.if_nonzero
            split_sibling_target[p_nz] = blk.term.if_zero

            insts.append(
                HInst(
                    pred=p_b,
                    orig_pred=p_b,
                    op=HOp(
                        kind="CmpZeroUn",
                        p_true=p_zero,
                        p_false=p_nz,
                        src=blk.term.cond,
                    ),
                    latency=1,
                )
            )
            for p_edge, target in [
                (p_zero, blk.term.if_zero),
                (p_nz, blk.term.if_nonzero),
            ]:
                if target in hb_blocks:
                    incoming.setdefault(target, []).append(p_edge)
                else:
                    side_exits.add(target)
                    insts.append(
                        HInst(
                            pred=p_edge,
                            orig_pred=p_edge,
                            op=HOp(kind="SideExit", target=target),
                            latency=1,
                        )
                    )
        elif blk.term.kind == "Jump":
            assert blk.term.target is not None
            t = blk.term.target
            if t in hb_blocks:
                incoming.setdefault(t, []).append(p_b)
            else:
                side_exits.add(t)
                insts.append(
                    HInst(
                        pred=p_b,
                        orig_pred=p_b,
                        op=HOp(kind="SideExit", target=t),
                        latency=1,
                    )
                )
        else:
            insts.append(
                HInst(
                    pred=p_b,
                    orig_pred=p_b,
                    op=HOp(kind="Ret", ret_reg=blk.term.ret_reg),
                    latency=1,
                )
            )

    return Hyperblock(
        blocks=set(hb_blocks),
        pred_tree=pred_tree,
        insts=insts,
        block_pred=block_pred,
        split_sibling_target=split_sibling_target,
        side_exits=side_exits,
    )


# ---------------------------------------------------------------------------
# 5. Speculative Predicate Promotion (§10.4)
# ---------------------------------------------------------------------------


def promote_predicates(
    cfg: CfgProgram, hb: Hyperblock, live_in: dict[int, set[str]]
) -> None:
    defs_by_reg: dict[str, list[int]] = {}
    for p in cfg.params:
        defs_by_reg.setdefault(p, []).append(0)
    for inst in hb.insts:
        d = inst.op.dst_reg()
        if d is not None:
            defs_by_reg.setdefault(d, []).append(inst.pred)

    for inst in hb.insts:
        if inst.pred == 0 or inst.op.kind != "Core":
            continue
        core_op = inst.op.core
        assert core_op is not None
        if not core_op.is_promotion_safe() or core_op.dst is None:
            continue
        dst = core_op.dst

        target_pred = inst.pred
        cur = inst.pred
        while hb.pred_tree.defs[cur].kind == "PartitionChild":
            parent = hb.pred_tree.defs[cur].parent
            assert parent is not None

            if cur in hb.split_sibling_target:
                sib_blk = hb.split_sibling_target[cur]
                if dst in live_in[sib_blk]:
                    break

            if any(
                hb.pred_tree.are_disjoint(p_def, inst.pred)
                for p_def in defs_by_reg.get(dst, [])
            ):
                break

            parent_ancs = hb.pred_tree.atomic_ancestors(parent)
            uses_ok = all(
                any(
                    p_def == parent or p_def in parent_ancs
                    for p_def in defs_by_reg.get(u, [])
                )
                for u in core_op.uses()
            )
            if not uses_ok:
                break

            target_pred = parent
            cur = parent

        if target_pred != inst.pred:
            inst.pred = target_pred
            inst.promoted = True


# ---------------------------------------------------------------------------
# 6. Predicate-Aware Hyperblock List Scheduler & Expected BB Baseline
# ---------------------------------------------------------------------------


@dataclass(frozen=True)
class ScheduledHOp:
    cycle: int
    inst_idx: int


@dataclass
class HyperblockSchedule:
    entries: list[ScheduledHOp]
    total_cycles: int
    removed_false_deps: int


def schedule_hyperblock(hb: Hyperblock, width: int) -> HyperblockSchedule:
    assert width > 0
    n = len(hb.insts)
    succs: list[dict[int, int]] = [{} for _ in range(n)]
    preds: list[dict[int, int]] = [{} for _ in range(n)]
    removed_false_deps = 0

    def add_edge(u: int, v: int, lat: int) -> None:
        if v in succs[u]:
            if lat > succs[u][v]:
                succs[u][v] = lat
                preds[v][u] = lat
        else:
            succs[u][v] = lat
            preds[v][u] = lat

    for u in range(n):
        for v in range(u + 1, n):
            iu = hb.insts[u]
            iv = hb.insts[v]
            disjoint = hb.pred_tree.are_disjoint(iu.pred, iv.pred)

            # 1. Predicate-register dependences.
            u_pdefs = iu.op.defines_preds()
            if u_pdefs:
                if iv.pred in u_pdefs:
                    add_edge(u, v, iu.latency)
                if (
                    iu.op.kind == "PredClear"
                    and iv.op.kind == "PredOrTrue"
                    and iu.op.p_dst == iv.op.p_dst
                ):
                    add_edge(u, v, iu.latency)
                if (
                    iu.op.kind == "PredOrTrue"
                    and iv.op.kind == "PredOrTrue"
                    and iu.op.p_dst == iv.op.p_dst
                    and not disjoint
                ):
                    add_edge(u, v, iu.latency)

            # 2. Data register RAW / WAW / WAR (filtered by disjointness).
            u_dst = iu.op.dst_reg()
            v_dst = iv.op.dst_reg()
            u_uses = iu.op.uses_regs()
            v_uses = iv.op.uses_regs()

            if u_dst is not None and u_dst in v_uses:
                if disjoint:
                    removed_false_deps += 1
                else:
                    add_edge(u, v, iu.latency)
            if u_dst is not None and u_dst == v_dst:
                if disjoint:
                    removed_false_deps += 1
                else:
                    add_edge(u, v, 1)
            if v_dst is not None and v_dst in u_uses:
                if disjoint:
                    removed_false_deps += 1
                else:
                    add_edge(u, v, 1)

            # 3. Memory dependences (filtered by disjointness).
            u_w = iu.op.kind == "Core" and iu.op.core is not None and iu.op.core.writes_mem()
            u_r = iu.op.kind == "Core" and iu.op.core is not None and iu.op.core.reads_mem()
            v_w = iv.op.kind == "Core" and iv.op.core is not None and iv.op.core.writes_mem()
            v_r = iv.op.kind == "Core" and iv.op.core is not None and iv.op.core.reads_mem()
            if (u_w and (v_w or v_r)) or (u_r and v_w):
                if disjoint:
                    removed_false_deps += 1
                else:
                    add_edge(u, v, max(1, iu.latency))

            # 4. SideExit / Ret control ordering.
            if (
                iu.op.kind == "SideExit"
                and (v_w or iv.op.kind in ("Ret", "SideExit"))
                and not disjoint
            ):
                add_edge(u, v, 1)
            if iv.op.kind == "Ret" and not disjoint:
                add_edge(u, v, max(1, iu.latency))

    height = [0] * n
    for u in range(n - 1, -1, -1):
        h = hb.insts[u].latency
        for v, lat in succs[u].items():
            h = max(h, lat + height[v])
        height[u] = h

    unscheduled_preds = [len(preds[i]) for i in range(n)]
    earliest = [0] * n
    scheduled = [False] * n
    entries: list[ScheduledHOp] = []
    cycle = 0

    while len(entries) < n:
        ready = [
            i
            for i in range(n)
            if not scheduled[i] and unscheduled_preds[i] == 0 and earliest[i] <= cycle
        ]
        ready.sort(key=lambda idx: (-height[idx], idx))
        for idx in ready[:width]:
            scheduled[idx] = True
            entries.append(ScheduledHOp(cycle=cycle, inst_idx=idx))
            for v, lat in succs[idx].items():
                unscheduled_preds[v] -= 1
                earliest[v] = max(earliest[v], cycle + lat)
        cycle += 1

    finish_cycle = max(e.cycle + hb.insts[e.inst_idx].latency for e in entries)
    return HyperblockSchedule(
        entries=entries,
        total_cycles=finish_cycle,
        removed_false_deps=removed_false_deps,
    )


def list_schedule_single_block(insts: list[Inst], width: int) -> int:
    n = len(insts)
    succs: list[dict[int, int]] = [{} for _ in range(n)]
    indeg = [0] * n
    for u in range(n):
        for v in range(u + 1, n):
            iu = insts[u]
            iv = insts[v]
            dep: int | None = None
            if iu.op.dst is not None and iu.op.dst in iv.op.uses():
                dep = iu.latency
            if iu.op.dst is not None and iu.op.dst == iv.op.dst:
                dep = max(dep or 1, 1)
            if iv.op.dst is not None and iv.op.dst in iu.op.uses():
                dep = max(dep or 1, 1)
            if (iu.op.writes_mem() and (iv.op.writes_mem() or iv.op.reads_mem())) or (
                iu.op.reads_mem() and iv.op.writes_mem()
            ):
                dep = max(dep or 1, iu.latency)
            if dep is not None:
                succs[u][v] = dep
                indeg[v] += 1

    ready_at = [0] * n
    done = [False] * n
    finish = 0
    scheduled_count = 0
    cycle = 0
    while scheduled_count < n:
        ready = [
            i for i in range(n) if not done[i] and indeg[i] == 0 and ready_at[i] <= cycle
        ]
        for idx in ready[:width]:
            done[idx] = True
            scheduled_count += 1
            finish = max(finish, cycle + insts[idx].latency)
            for v, lat in succs[idx].items():
                indeg[v] -= 1
                ready_at[v] = max(ready_at[v], cycle + lat)
        cycle += 1
    return finish + 1


def expected_bb_cycles(
    cfg: CfgProgram, width: int, branch_mispredict_penalty: int
) -> float:
    block_cycs = {
        bid: list_schedule_single_block(blk.insts, width)
        for bid, blk in cfg.blocks.items()
    }

    def walk(b: int, prob: float, acc: int) -> float:
        blk = cfg.blocks[b]
        c = acc + block_cycs[b]
        if blk.term.kind == "Ret":
            return prob * c
        if blk.term.kind == "Jump":
            assert blk.term.target is not None
            return walk(blk.term.target, prob, c)
        assert blk.term.if_zero is not None and blk.term.if_nonzero is not None
        fz = float(cfg.blocks[blk.term.if_zero].freq)
        fnz = float(cfg.blocks[blk.term.if_nonzero].freq)
        tot = fz + fnz
        hot_zero = fz >= fnz
        pen_zero = 0 if hot_zero else branch_mispredict_penalty
        pen_nz = branch_mispredict_penalty if hot_zero else 0
        return walk(blk.term.if_zero, prob * (fz / tot), c + pen_zero) + walk(
            blk.term.if_nonzero, prob * (fnz / tot), c + pen_nz
        )

    return walk(cfg.entry, 1.0, 0)


# ---------------------------------------------------------------------------
# 7. Original CFG VM vs. Predicated Hyperblock Hardware VM
# ---------------------------------------------------------------------------


@dataclass(frozen=True)
class SimResult:
    ret_val: int
    mem: list[int]
    visited: list[str]
    traps: int
    squashed_ops: int
    side_exited: bool


def eval_core_op(
    op: Op, regs: dict[str, int], mem: list[int], traps: list[int]
) -> None:
    if op.kind == "Const":
        assert op.dst is not None and op.val is not None
        regs[op.dst] = op.val
    elif op.kind == "LoadSafe":
        assert op.dst is not None and op.slot is not None
        regs[op.dst] = mem[op.slot]
    elif op.kind == "AddImm":
        assert op.dst is not None and op.src is not None and op.imm is not None
        regs[op.dst] = regs[op.src] + op.imm
    elif op.kind == "MulImm":
        assert op.dst is not None and op.src is not None and op.imm is not None
        regs[op.dst] = regs[op.src] * op.imm
    elif op.kind == "Add":
        assert op.dst is not None and op.lhs is not None and op.rhs is not None
        regs[op.dst] = regs[op.lhs] + regs[op.rhs]
    elif op.kind == "DivChecked":
        assert op.dst is not None and op.num is not None and op.den is not None
        d = regs[op.den]
        if d == 0:
            traps[0] += 1
            raise ZeroDivisionError(f"Hardware trap: division by zero in {op}")
        regs[op.dst] = int(op.num // d)
    elif op.kind == "Store":
        assert op.slot is not None and op.src is not None
        mem[op.slot] = regs[op.src]


def simulate_cfg(
    cfg: CfgProgram, init_regs: dict[str, int], init_mem: list[int]
) -> SimResult:
    regs = dict(init_regs)
    mem = list(init_mem)
    visited: list[str] = []
    traps = [0]
    cur = cfg.entry
    while True:
        visited.append(f"B{cur}")
        blk = cfg.blocks[cur]
        for inst in blk.insts:
            eval_core_op(inst.op, regs, mem, traps)
        if blk.term.kind == "BrZero":
            assert blk.term.cond is not None
            assert blk.term.if_zero is not None and blk.term.if_nonzero is not None
            cur = blk.term.if_zero if regs[blk.term.cond] == 0 else blk.term.if_nonzero
        elif blk.term.kind == "Jump":
            assert blk.term.target is not None
            cur = blk.term.target
        else:
            assert blk.term.ret_reg is not None
            return SimResult(
                ret_val=regs[blk.term.ret_reg],
                mem=mem,
                visited=visited,
                traps=traps[0],
                squashed_ops=0,
                side_exited=False,
            )


def simulate_hyperblock(
    cfg: CfgProgram,
    hb: Hyperblock,
    sched: HyperblockSchedule,
    init_regs: dict[str, int],
    init_mem: list[int],
) -> SimResult:
    regs = dict(init_regs)
    mem = list(init_mem)
    preds: dict[int, bool] = {0: True}
    traps = [0]
    squashed_ops = 0

    side_target: int | None = None
    ret_val: int | None = None

    by_cycle: dict[int, list[int]] = {}
    for e in sched.entries:
        by_cycle.setdefault(e.cycle, []).append(e.inst_idx)

    for cyc in sorted(by_cycle):
        jump_out: int | None = None
        cyc_ret: int | None = None

        for idx in by_cycle[cyc]:
            hinst = hb.insts[idx]
            active = preds.get(hinst.pred, False)

            if hinst.op.kind == "CmpZeroUn":
                assert hinst.op.src is not None
                assert hinst.op.p_true is not None and hinst.op.p_false is not None
                preds[hinst.op.p_true] = active and regs[hinst.op.src] == 0
                preds[hinst.op.p_false] = active and regs[hinst.op.src] != 0
                if not active:
                    squashed_ops += 1
            elif hinst.op.kind == "PredClear":
                assert hinst.op.p_dst is not None
                preds[hinst.op.p_dst] = False
            elif hinst.op.kind == "PredOrTrue":
                assert hinst.op.p_dst is not None
                if active:
                    preds[hinst.op.p_dst] = True
                else:
                    squashed_ops += 1
            elif hinst.op.kind == "Core":
                assert hinst.op.core is not None
                if active:
                    eval_core_op(hinst.op.core, regs, mem, traps)
                else:
                    squashed_ops += 1
            elif hinst.op.kind == "SideExit":
                if active:
                    jump_out = hinst.op.target
                else:
                    squashed_ops += 1
            elif hinst.op.kind == "Ret":
                assert hinst.op.ret_reg is not None
                if active:
                    cyc_ret = regs[hinst.op.ret_reg]
                else:
                    squashed_ops += 1

        if jump_out is not None:
            side_target = jump_out
            break
        if cyc_ret is not None:
            ret_val = cyc_ret
            break

    if side_target is not None:
        cur = side_target
        visited = ["HB"]
        while True:
            visited.append(f"B{cur}")
            blk = cfg.blocks[cur]
            for inst in blk.insts:
                eval_core_op(inst.op, regs, mem, traps)
            if blk.term.kind == "BrZero":
                assert blk.term.cond is not None
                assert blk.term.if_zero is not None and blk.term.if_nonzero is not None
                cur = (
                    blk.term.if_zero if regs[blk.term.cond] == 0 else blk.term.if_nonzero
                )
            elif blk.term.kind == "Jump":
                assert blk.term.target is not None
                cur = blk.term.target
            else:
                assert blk.term.ret_reg is not None
                return SimResult(
                    ret_val=regs[blk.term.ret_reg],
                    mem=mem,
                    visited=visited,
                    traps=traps[0],
                    squashed_ops=squashed_ops,
                    side_exited=True,
                )

    assert ret_val is not None
    return SimResult(
        ret_val=ret_val,
        mem=mem,
        visited=["HB"],
        traps=traps[0],
        squashed_ops=squashed_ops,
        side_exited=False,
    )


# ---------------------------------------------------------------------------
# 8. Sample Diamond-with-Cold-Side-Exit CFG & Driver
# ---------------------------------------------------------------------------


def sample_hyperblock_cfg() -> CfgProgram:
    blocks = {
        0: Block(
            freq=100,
            insts=[
                Inst(Op(kind="LoadSafe", dst="r1", slot=0), latency=3),
                Inst(Op(kind="AddImm", dst="r2", src="r1", imm=1), latency=1),
            ],
            term=Term(kind="BrZero", cond="r_x", if_zero=2, if_nonzero=1),
        ),
        1: Block(
            freq=52,
            insts=[],
            term=Term(kind="BrZero", cond="r_err", if_zero=3, if_nonzero=5),
        ),
        2: Block(
            freq=48,
            insts=[
                Inst(Op(kind="AddImm", dst="r6", src="r_y", imm=10), latency=1),
                Inst(Op(kind="MulImm", dst="r5", src="r6", imm=2), latency=2),
            ],
            term=Term(kind="Jump", target=4),
        ),
        3: Block(
            freq=50,
            insts=[
                Inst(Op(kind="MulImm", dst="r7", src="r_y", imm=3), latency=2),
                Inst(
                    Op(kind="DivChecked", dst="r3", num=100, den="r_x"), latency=3
                ),
                Inst(Op(kind="Add", dst="r5", lhs="r3", rhs="r7"), latency=1),
            ],
            term=Term(kind="Jump", target=4),
        ),
        4: Block(
            freq=98,
            insts=[
                Inst(Op(kind="Add", dst="r8", lhs="r5", rhs="r2"), latency=1),
                Inst(Op(kind="Store", slot=1, src="r8"), latency=1),
            ],
            term=Term(kind="Ret", ret_reg="r8"),
        ),
        5: Block(
            freq=2,
            insts=[
                Inst(Op(kind="Const", dst="r_fail", val=-1), latency=1),
                Inst(Op(kind="Store", slot=1, src="r_fail"), latency=1),
            ],
            term=Term(kind="Ret", ret_reg="r_fail"),
        ),
    }
    return CfgProgram(blocks=blocks, entry=0, params=["r_x", "r_y", "r_err"])


def main() -> None:
    cfg = sample_hyperblock_cfg()
    live_in = compute_liveness(cfg)
    hb_blocks = select_hyperblock(cfg, 20)
    hb = if_convert(cfg, hb_blocks)
    promote_predicates(cfg, hb, live_in)

    width = 2
    mispredict_penalty = 3
    sched = schedule_hyperblock(hb, width)
    bb_no_pen = expected_bb_cycles(cfg, width, 0)
    bb_with_pen = expected_bb_cycles(cfg, width, mispredict_penalty)

    print("=== Hardware-Predicated Execution & Hyperblock If-Conversion (§10.4) ===")
    print(
        f"selected hyperblock: {[f'B{b}' for b in sorted(hb.blocks)]} "
        f"(excluded cold exits: {[f'B{b}' for b in sorted(hb.side_exits)]})"
    )
    print(
        "block predicates: "
        + str({f"B{b}": f"p{p}" for b, p in sorted(hb.block_pred.items())})
    )
    print(
        f"expected BB cycles (no mispredict): {bb_no_pen:.2f}, "
        f"(3-cyc mispredict on cold arm): {bb_with_pen:.2f}"
    )
    print(
        f"predicated hyperblock schedule (width={width}): {sched.total_cycles} cycles "
        f"(removed {sched.removed_false_deps} false disjoint deps)"
    )

    print("\nScheduled VLIW hyperblock:")
    by_cyc: dict[int, list[HInst]] = {}
    for e in sched.entries:
        by_cyc.setdefault(e.cycle, []).append(hb.insts[e.inst_idx])
    for c in sorted(by_cyc):
        print("  cyc {}: {}".format(c, "  ||  ".join(str(h) for h in by_cyc[c])))

    print("\nEquivalence simulation (init mem=[50, 0]):")
    for label, rx, ry, rerr in [
        ("then_path (r_x=5, r_y=4, r_err=0)", 5, 4, 0),
        ("else_path (r_x=0, r_y=4, r_err=0)", 0, 4, 0),
        ("cold_exit (r_x=5, r_y=4, r_err=1)", 5, 4, 1),
    ]:
        regs = {"r_x": rx, "r_y": ry, "r_err": rerr}
        mem = [50, 0]
        orig = simulate_cfg(cfg, regs, mem)
        pred = simulate_hyperblock(cfg, hb, sched, regs, mem)
        assert orig.ret_val == pred.ret_val and orig.mem == pred.mem
        print(
            f"  {label}: ret={pred.ret_val}, mem={pred.mem}, traps={pred.traps}, "
            f"squashed_ops={pred.squashed_ops}, side_exited={pred.side_exited}"
        )


class HyperblockPredicationTests(unittest.TestCase):
    def test_hyperblock_if_conversion_matches_cfg_on_all_paths(self) -> None:
        cfg = sample_hyperblock_cfg()
        live_in = compute_liveness(cfg)
        hb_blocks = select_hyperblock(cfg, 20)
        self.assertEqual(hb_blocks, {0, 1, 2, 3, 4})

        hb = if_convert(cfg, hb_blocks)
        promote_predicates(cfg, hb, live_in)

        r7_inst = next(
            i for i in hb.insts if i.op.core is not None and i.op.core.dst == "r7"
        )
        r6_inst = next(
            i for i in hb.insts if i.op.core is not None and i.op.core.dst == "r6"
        )
        div_inst = next(
            i
            for i in hb.insts
            if i.op.core is not None and i.op.core.kind == "DivChecked"
        )
        store_inst = next(
            i for i in hb.insts if i.op.core is not None and i.op.core.kind == "Store"
        )
        self.assertEqual(r7_inst.pred, 0)
        self.assertTrue(r7_inst.promoted)
        self.assertEqual(r6_inst.pred, 0)
        self.assertTrue(r6_inst.promoted)
        self.assertNotEqual(div_inst.pred, 0)
        self.assertFalse(div_inst.promoted)
        self.assertNotEqual(store_inst.pred, 0)

        p_b2 = hb.block_pred[2]
        p_b3 = hb.block_pred[3]
        p_b4 = hb.block_pred[4]
        self.assertTrue(hb.pred_tree.are_disjoint(p_b2, p_b3))
        self.assertFalse(hb.pred_tree.are_disjoint(p_b3, p_b4))
        exit_inst = next(i for i in hb.insts if i.op.kind == "SideExit")
        self.assertTrue(hb.pred_tree.are_disjoint(exit_inst.pred, p_b4))

        sched = schedule_hyperblock(hb, 2)
        bb_exp = expected_bb_cycles(cfg, 2, 0)
        self.assertLess(sched.total_cycles, bb_exp)
        self.assertGreater(sched.removed_false_deps, 0)

        for rx, ry, rerr, expect_ret, expect_exit in [
            (5, 4, 0, 83, False),
            (0, 4, 0, 79, False),
            (5, 4, 1, -1, True),
            (0, 4, 1, 79, False),
        ]:
            regs = {"r_x": rx, "r_y": ry, "r_err": rerr}
            mem = [50, 0]
            orig = simulate_cfg(cfg, regs, mem)
            pred = simulate_hyperblock(cfg, hb, sched, regs, mem)
            self.assertEqual(orig.ret_val, expect_ret)
            self.assertEqual(pred.ret_val, orig.ret_val)
            self.assertEqual(pred.mem, orig.mem)
            self.assertEqual(pred.traps, 0)
            self.assertEqual(pred.side_exited, expect_exit)

    def test_predicate_promotion_blocked_when_dst_live_in_sibling_branch(self) -> None:
        cfg = CfgProgram(
            blocks={
                0: Block(
                    freq=100,
                    insts=[],
                    term=Term(kind="BrZero", cond="r_cond", if_zero=2, if_nonzero=1),
                ),
                1: Block(
                    freq=50,
                    insts=[
                        Inst(
                            Op(kind="AddImm", dst="r_val", src="r_in", imm=10),
                            latency=1,
                        )
                    ],
                    term=Term(kind="Jump", target=3),
                ),
                2: Block(freq=50, insts=[], term=Term(kind="Jump", target=3)),
                3: Block(freq=100, insts=[], term=Term(kind="Ret", ret_reg="r_val")),
            },
            entry=0,
            params=["r_cond", "r_in", "r_val"],
        )
        live_in = compute_liveness(cfg)
        self.assertIn("r_val", live_in[2])

        hb_blocks = select_hyperblock(cfg, 20)
        hb = if_convert(cfg, hb_blocks)
        promote_predicates(cfg, hb, live_in)

        add_inst = next(
            i
            for i in hb.insts
            if i.op.core is not None and i.op.core.kind == "AddImm"
        )
        self.assertNotEqual(add_inst.pred, 0)
        self.assertFalse(add_inst.promoted)

        sched = schedule_hyperblock(hb, 2)
        for rc, rv_init, expect in [(1, 99, 15), (0, 99, 99)]:
            regs = {"r_cond": rc, "r_in": 5, "r_val": rv_init}
            orig = simulate_cfg(cfg, regs, [])
            pred = simulate_hyperblock(cfg, hb, sched, regs, [])
            self.assertEqual(orig.ret_val, expect)
            self.assertEqual(pred.ret_val, orig.ret_val)

    def test_disjoint_predicates_allow_concurrent_stores_and_waw_writes(self) -> None:
        cfg = CfgProgram(
            blocks={
                0: Block(
                    freq=100,
                    insts=[],
                    term=Term(kind="BrZero", cond="r_cond", if_zero=2, if_nonzero=1),
                ),
                1: Block(
                    freq=50,
                    insts=[
                        Inst(Op(kind="Const", dst="r_out", val=111), latency=1),
                        Inst(Op(kind="Store", slot=0, src="r_out"), latency=1),
                    ],
                    term=Term(kind="Jump", target=3),
                ),
                2: Block(
                    freq=50,
                    insts=[
                        Inst(Op(kind="Const", dst="r_out", val=222), latency=1),
                        Inst(Op(kind="Store", slot=0, src="r_out"), latency=1),
                    ],
                    term=Term(kind="Jump", target=3),
                ),
                3: Block(freq=100, insts=[], term=Term(kind="Ret", ret_reg="r_out")),
            },
            entry=0,
            params=["r_cond"],
        )
        live_in = compute_liveness(cfg)
        hb_blocks = select_hyperblock(cfg, 20)
        hb = if_convert(cfg, hb_blocks)
        promote_predicates(cfg, hb, live_in)

        sched = schedule_hyperblock(hb, 2)
        self.assertGreaterEqual(sched.removed_false_deps, 2)
        const_cycles = [
            e.cycle
            for e in sched.entries
            if hb.insts[e.inst_idx].op.core is not None
            and hb.insts[e.inst_idx].op.core.kind == "Const"
        ]
        self.assertEqual(const_cycles[0], const_cycles[1])

        for rc, expect in [(1, 111), (0, 222)]:
            regs = {"r_cond": rc}
            orig = simulate_cfg(cfg, regs, [0])
            pred = simulate_hyperblock(cfg, hb, sched, regs, [0])
            self.assertEqual(orig.ret_val, expect)
            self.assertEqual(pred.ret_val, orig.ret_val)
            self.assertEqual(pred.mem, [expect])


if __name__ == "__main__":
    main()
