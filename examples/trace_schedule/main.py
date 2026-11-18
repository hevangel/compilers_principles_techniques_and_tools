"""Chapter 10 (§10.4): Global Trace Scheduling & Speculative Code Motion.

Implements profile-guided hot-trace selection, backward CFG liveness analysis,
control-dependence safety barriers for trapping (`DivChecked`) and side-effecting
(`Store`) operations, global list scheduling across branch splits and joins,
off-trace compensation insertion, and multi-path CFG simulation proving semantic
equivalence across both hot and cold paths.
"""

from __future__ import annotations

from dataclasses import dataclass, field
import unittest


# ---------------------------------------------------------------------------
# 1. CFG Intermediate Representation & Speculative Safety Classification
# ---------------------------------------------------------------------------


@dataclass(frozen=True)
class Op:
    kind: str  # "const", "load_safe", "add_imm", "mul_imm", "add", "div_checked", "store"
    dst: str | None = None
    src: str | None = None
    lhs: str | None = None
    rhs: str | None = None
    den: str | None = None
    val: int = 0
    imm: int = 0
    num: int = 0
    slot: int = 0

    def uses(self) -> list[str]:
        if self.kind in ("add_imm", "mul_imm") and self.src is not None:
            return [self.src]
        if self.kind == "add" and self.lhs is not None and self.rhs is not None:
            return [self.lhs, self.rhs]
        if self.kind == "div_checked" and self.den is not None:
            return [self.den]
        if self.kind == "store" and self.src is not None:
            return [self.src]
        return []

    def is_speculative_safe(self) -> bool:
        """Only pure, non-trapping ops may execute above a controlling branch."""
        return self.kind not in ("div_checked", "store")

    def reads_mem(self) -> bool:
        return self.kind == "load_safe"

    def writes_mem(self) -> bool:
        return self.kind == "store"

    def __str__(self) -> str:
        if self.kind == "const":
            return f"{self.dst} = {self.val}"
        if self.kind == "load_safe":
            return f"{self.dst} = load_safe(mem[{self.slot}])"
        if self.kind == "add_imm":
            return f"{self.dst} = {self.src} + {self.imm}"
        if self.kind == "mul_imm":
            return f"{self.dst} = {self.src} * {self.imm}"
        if self.kind == "add":
            return f"{self.dst} = {self.lhs} + {self.rhs}"
        if self.kind == "div_checked":
            return f"{self.dst} = {self.num} / {self.den} [trap]"
        return f"mem[{self.slot}] = {self.src} [effect]"


@dataclass(frozen=True)
class Inst:
    op: Op
    latency: int


@dataclass(frozen=True)
class Term:
    kind: str  # "br_zero", "jump", "ret"
    cond: str = ""
    if_zero: int = 0
    if_nonzero: int = 0
    target: int = 0
    reg: str = ""

    def succs(self) -> list[int]:
        if self.kind == "br_zero":
            return [self.if_zero, self.if_nonzero]
        if self.kind == "jump":
            return [self.target]
        return []

    def __str__(self) -> str:
        if self.kind == "br_zero":
            return f"br_zero {self.cond} ? B{self.if_zero} : B{self.if_nonzero}"
        if self.kind == "jump":
            return f"jump B{self.target}"
        return f"ret {self.reg}"


@dataclass
class Block:
    freq: int
    insts: list[Inst]
    term: Term


@dataclass
class CfgProgram:
    blocks: dict[int, Block]
    entry: int


# ---------------------------------------------------------------------------
# 2. Profile-Guided Trace Selection & Backward Liveness Analysis
# ---------------------------------------------------------------------------


def pick_hot_trace(cfg: CfgProgram) -> list[int]:
    """Selects the highest-frequency acyclic path from `entry`."""
    trace = [cfg.entry]
    seen = {cfg.entry}
    cur = cfg.entry
    while True:
        cands = [s for s in cfg.blocks[cur].term.succs() if s not in seen]
        if not cands:
            break
        nxt = max(cands, key=lambda s: (cfg.blocks[s].freq, -s))
        trace.append(nxt)
        seen.add(nxt)
        cur = nxt
    return trace


def compute_liveness(cfg: CfgProgram) -> tuple[dict[int, set[str]], dict[int, set[str]]]:
    live_in: dict[int, set[str]] = {bid: set() for bid in cfg.blocks}
    live_out: dict[int, set[str]] = {bid: set() for bid in cfg.blocks}

    changed = True
    while changed:
        changed = False
        for bid in sorted(cfg.blocks.keys(), reverse=True):
            blk = cfg.blocks[bid]
            out: set[str] = set()
            for s in blk.term.succs():
                out |= live_in[s]
            cur_in = set(out)
            if blk.term.kind == "br_zero":
                cur_in.add(blk.term.cond)
            elif blk.term.kind == "ret":
                cur_in.add(blk.term.reg)
            for inst in reversed(blk.insts):
                if inst.op.dst is not None:
                    cur_in.discard(inst.op.dst)
                for u in inst.op.uses():
                    cur_in.add(u)
            if out != live_out[bid] or cur_in != live_in[bid]:
                live_out[bid] = out
                live_in[bid] = cur_in
                changed = True
    return live_in, live_out


# ---------------------------------------------------------------------------
# 3. Basic-Block Baseline vs. Global Trace Scheduling + Compensation
# ---------------------------------------------------------------------------


def list_schedule_single_block(insts: list[Inst], term_extra_cycle: int, width: int) -> int:
    assert width > 0
    n = len(insts)
    deps: list[list[tuple[int, int]]] = [[] for _ in range(n)]
    last_def: dict[str, int] = {}
    last_reads: dict[str, list[int]] = {}
    last_store: int | None = None
    loads_since_store: list[int] = []

    for i, inst in enumerate(insts):
        for u in inst.op.uses():
            if u in last_def:
                d = last_def[u]
                deps[i].append((d, insts[d].latency))
            last_reads.setdefault(u, []).append(i)
        if inst.op.reads_mem():
            if last_store is not None:
                deps[i].append((last_store, insts[last_store].latency))
            loads_since_store.append(i)
        if inst.op.writes_mem():
            if last_store is not None:
                deps[i].append((last_store, 1))
            for ld in loads_since_store:
                deps[i].append((ld, 1))
            loads_since_store.clear()
            last_store = i
        if inst.op.dst is not None:
            d = inst.op.dst
            if d in last_def:
                deps[i].append((last_def[d], 1))
            for r in last_reads.get(d, []):
                if r != i:
                    deps[i].append((r, 0))
            last_def[d] = i

    issued: list[int | None] = [None] * n
    remaining = n
    cycle = 0
    finish = 0
    while remaining > 0:
        slots = width
        next_ready: int | None = None
        for i in range(n):
            if issued[i] is not None:
                continue
            ready_at = 0
            blocked = False
            for pred, lat in deps[i]:
                if issued[pred] is None:
                    blocked = True
                    break
                ready_at = max(ready_at, issued[pred] + lat)  # type: ignore[operator]
            if blocked:
                continue
            if ready_at <= cycle and slots > 0:
                issued[i] = cycle
                finish = max(finish, cycle + insts[i].latency)
                slots -= 1
                remaining -= 1
            elif ready_at > cycle:
                next_ready = ready_at if next_ready is None else min(next_ready, ready_at)
        if remaining > 0:
            cycle = next_ready if slots == width and next_ready is not None else cycle + 1
    return finish + term_extra_cycle


def basic_block_trace_cycles(cfg: CfgProgram, trace: list[int], width: int) -> int:
    total = 0
    for bid in trace:
        blk = cfg.blocks[bid]
        term_cyc = 1 if blk.term.kind == "br_zero" else 0
        total += list_schedule_single_block(blk.insts, term_cyc, width)
    return total


@dataclass
class TraceNode:
    orig_block: int
    orig_trace_pos: int
    kind: str  # "op" | "branch"
    latency: int
    inst: Inst | None = None
    cond: str = ""
    off_target: int = 0
    on_target: int = 0


@dataclass(frozen=True)
class ScheduledTraceEntry:
    orig_block: int
    cycle: int
    desc: str


@dataclass(frozen=True)
class MotionRecord:
    op_desc: str
    kind: str  # "speculative_split" | "join_comp" | "down_split_comp"
    from_block: int
    boundary_block: int
    comp_block: int | None = None


@dataclass
class TraceScheduleResult:
    trace: list[int]
    bb_cycles: int
    trace_cycles: int
    schedule: list[ScheduledTraceEntry]
    motions: list[MotionRecord]
    transformed_cfg: CfgProgram


def trace_schedule_cfg(cfg: CfgProgram, width: int) -> TraceScheduleResult:
    assert width > 0
    trace = pick_hot_trace(cfg)
    trace_pos = {bid: idx for idx, bid in enumerate(trace)}
    live_in, _ = compute_liveness(cfg)
    bb_cycles = basic_block_trace_cycles(cfg, trace, width)

    preds: dict[int, list[int]] = {bid: [] for bid in cfg.blocks}
    for bid, blk in cfg.blocks.items():
        for s in blk.term.succs():
            preds[s].append(bid)

    nodes: list[TraceNode] = []
    block_join_preds: dict[int, list[int]] = {}
    for pos, bid in enumerate(trace):
        blk = cfg.blocks[bid]
        for inst in blk.insts:
            nodes.append(TraceNode(bid, pos, "op", inst.latency, inst=inst))
        if pos + 1 < len(trace) and blk.term.kind == "br_zero":
            next_on_trace = trace[pos + 1]
            off_target = (
                blk.term.if_nonzero if blk.term.if_zero == next_on_trace else blk.term.if_zero
            )
            nodes.append(
                TraceNode(
                    bid,
                    pos,
                    "branch",
                    1,
                    cond=blk.term.cond,
                    off_target=off_target,
                    on_target=next_on_trace,
                )
            )
        off_preds = [p for p in preds[bid] if pos == 0 or p != trace[pos - 1]]
        if off_preds:
            block_join_preds[bid] = off_preds

    n = len(nodes)
    deps: list[list[tuple[int, int]]] = [[] for _ in range(n)]
    last_def: dict[str, int] = {}
    last_reads: dict[str, list[int]] = {}
    last_store: int | None = None
    loads_since_store: list[int] = []
    branch_nodes: list[tuple[int, int]] = []

    for i, node in enumerate(nodes):
        if node.kind == "op":
            assert node.inst is not None
            inst = node.inst
            for u in inst.op.uses():
                if u in last_def:
                    d = last_def[u]
                    deps[i].append((d, nodes[d].latency))
                last_reads.setdefault(u, []).append(i)
            if inst.op.reads_mem():
                if last_store is not None:
                    deps[i].append((last_store, nodes[last_store].latency))
                loads_since_store.append(i)
            if inst.op.writes_mem():
                if last_store is not None:
                    deps[i].append((last_store, 1))
                for ld in loads_since_store:
                    deps[i].append((ld, 1))
                loads_since_store.clear()
                last_store = i

            for br_id, off_target in branch_nodes:
                if not inst.op.is_speculative_safe():
                    deps[i].append((br_id, nodes[br_id].latency))
                elif inst.op.dst is not None and inst.op.dst in live_in[off_target]:
                    deps[i].append((br_id, nodes[br_id].latency))

            if inst.op.dst is not None:
                d = inst.op.dst
                if d in last_def:
                    deps[i].append((last_def[d], 1))
                for r in last_reads.get(d, []):
                    if r != i:
                        deps[i].append((r, 0))
                last_def[d] = i
        else:
            if node.cond in last_def:
                d = last_def[node.cond]
                deps[i].append((d, nodes[d].latency))
            last_reads.setdefault(node.cond, []).append(i)
            for reg in sorted(live_in[node.off_target]):
                if reg in last_def:
                    d = last_def[reg]
                    deps[i].append((d, nodes[d].latency))
                last_reads.setdefault(reg, []).append(i)
            if last_store is not None:
                deps[i].append((last_store, nodes[last_store].latency))
            branch_nodes.append((i, node.off_target))

    issued: list[int | None] = [None] * n
    order: list[int] = []
    remaining = n
    cycle = 0
    total_finish = 0
    while remaining > 0:
        slots = width
        next_ready: int | None = None
        for i in range(n):
            if issued[i] is not None:
                continue
            ready_at = 0
            blocked = False
            for pred, lat in deps[i]:
                if issued[pred] is None:
                    blocked = True
                    break
                ready_at = max(ready_at, issued[pred] + lat)  # type: ignore[operator]
            if blocked:
                continue
            if ready_at <= cycle and slots > 0:
                issued[i] = cycle
                order.append(i)
                total_finish = max(total_finish, cycle + nodes[i].latency)
                slots -= 1
                remaining -= 1
            elif ready_at > cycle:
                next_ready = ready_at if next_ready is None else min(next_ready, ready_at)
        if remaining > 0:
            if slots == width:
                if next_ready is None:
                    raise RuntimeError("cycle detected in trace DAG")
                cycle = next_ready
            else:
                cycle += 1

    new_blocks: dict[int, Block] = {
        bid: Block(blk.freq, list(blk.insts), blk.term) for bid, blk in cfg.blocks.items()
    }
    for bid in trace:
        new_blocks[bid].insts.clear()

    current_seg = 0
    branch_sched_seg: dict[int, int] = {}
    node_sched_seg = [0] * n
    schedule_log: list[ScheduledTraceEntry] = []

    for nid in order:
        cyc = issued[nid]
        assert cyc is not None
        node = nodes[nid]
        if node.kind == "op":
            assert node.inst is not None
            node_sched_seg[nid] = current_seg
            new_blocks[trace[current_seg]].insts.append(node.inst)
            schedule_log.append(ScheduledTraceEntry(node.orig_block, cyc, str(node.inst.op)))
        else:
            branch_sched_seg[node.orig_block] = current_seg
            node_sched_seg[nid] = current_seg
            schedule_log.append(
                ScheduledTraceEntry(
                    node.orig_block,
                    cyc,
                    f"br_zero {node.cond} ? B{node.off_target} : B{node.on_target}",
                )
            )
            current_seg += 1

    motions: list[MotionRecord] = []
    for nid in order:
        node = nodes[nid]
        if node.kind != "op" or node.inst is None:
            continue
        inst = node.inst
        orig_seg = node.orig_trace_pos
        sched_seg = node_sched_seg[nid]
        if sched_seg < orig_seg:
            for pos in range(sched_seg, orig_seg):
                boundary_block = trace[pos]
                next_block = trace[pos + 1]
                if cfg.blocks[boundary_block].term.kind == "br_zero":
                    motions.append(
                        MotionRecord(str(inst.op), "speculative_split", node.orig_block, boundary_block)
                    )
                for comp_blk in block_join_preds.get(next_block, []):
                    if comp_blk not in trace_pos:
                        new_blocks[comp_blk].insts.append(inst)
                        motions.append(
                            MotionRecord(
                                str(inst.op), "join_comp", node.orig_block, next_block, comp_blk
                            )
                        )
        elif sched_seg > orig_seg:
            for pos in range(orig_seg, sched_seg):
                split_blk = trace[pos]
                term = cfg.blocks[split_blk].term
                if term.kind == "br_zero":
                    off_target = (
                        term.if_nonzero if term.if_zero == trace[pos + 1] else term.if_zero
                    )
                    new_blocks[off_target].insts.insert(0, inst)
                    motions.append(
                        MotionRecord(
                            str(inst.op), "down_split_comp", node.orig_block, split_blk, off_target
                        )
                    )

    for idx, bid in enumerate(trace):
        if bid not in branch_sched_seg and idx + 1 < len(trace):
            new_blocks[bid].term = cfg.blocks[trace[idx + 1]].term

    return TraceScheduleResult(
        trace=trace,
        bb_cycles=bb_cycles,
        trace_cycles=total_finish,
        schedule=schedule_log,
        motions=motions,
        transformed_cfg=CfgProgram(new_blocks, cfg.entry),
    )


# ---------------------------------------------------------------------------
# 4. Multi-Path CFG VM Simulator
# ---------------------------------------------------------------------------


@dataclass(frozen=True)
class SimState:
    ret: int
    regs: dict[str, int]
    mem: list[int]
    visited_blocks: list[int]


def simulate_cfg(cfg: CfgProgram, init_regs: dict[str, int], init_mem: list[int]) -> SimState:
    regs = dict(init_regs)
    mem = list(init_mem)
    pc = cfg.entry
    visited: list[int] = []

    for _ in range(256):
        visited.append(pc)
        blk = cfg.blocks[pc]
        for inst in blk.insts:
            op = inst.op
            if op.kind == "const":
                assert op.dst is not None
                regs[op.dst] = op.val
            elif op.kind == "load_safe":
                assert op.dst is not None
                regs[op.dst] = mem[op.slot]
            elif op.kind == "add_imm":
                assert op.dst is not None and op.src is not None
                regs[op.dst] = regs[op.src] + op.imm
            elif op.kind == "mul_imm":
                assert op.dst is not None and op.src is not None
                regs[op.dst] = regs[op.src] * op.imm
            elif op.kind == "add":
                assert op.dst is not None and op.lhs is not None and op.rhs is not None
                regs[op.dst] = regs[op.lhs] + regs[op.rhs]
            elif op.kind == "div_checked":
                assert op.dst is not None and op.den is not None
                d = regs[op.den]
                if d == 0:
                    raise ZeroDivisionError("TRAP: ZeroDivisionError during DivChecked")
                regs[op.dst] = int(op.num / d)
            else:
                assert op.src is not None
                mem[op.slot] = regs[op.src]
        if blk.term.kind == "br_zero":
            pc = blk.term.if_zero if regs[blk.term.cond] == 0 else blk.term.if_nonzero
        elif blk.term.kind == "jump":
            pc = blk.term.target
        else:
            return SimState(regs[blk.term.reg], regs, mem, visited)
    raise RuntimeError("exceeded CFG step budget")


def sample_diamond_cfg() -> CfgProgram:
    b0 = Block(
        freq=100,
        insts=[
            Inst(Op("load_safe", dst="r1", slot=0), 3),
            Inst(Op("add_imm", dst="r2", src="r1", imm=5), 1),
        ],
        term=Term("br_zero", cond="r_guard", if_zero=2, if_nonzero=1),
    )
    b1 = Block(
        freq=90,
        insts=[
            Inst(Op("mul_imm", dst="r3", src="r_in", imm=3), 2),
            Inst(Op("div_checked", dst="r4", num=100, den="r_guard"), 2),
            Inst(Op("add", dst="r5", lhs="r2", rhs="r3"), 1),
        ],
        term=Term("jump", target=3),
    )
    b2 = Block(
        freq=10,
        insts=[
            Inst(Op("add_imm", dst="r5", src="r2", imm=15), 1),
            Inst(Op("const", dst="r4", val=0), 1),
        ],
        term=Term("jump", target=3),
    )
    b3 = Block(
        freq=100,
        insts=[
            Inst(Op("add_imm", dst="r6", src="r_in", imm=10), 2),
            Inst(Op("add", dst="r7", lhs="r5", rhs="r6"), 1),
            Inst(Op("add", dst="r8", lhs="r7", rhs="r4"), 1),
            Inst(Op("store", slot=1, src="r8"), 1),
        ],
        term=Term("ret", reg="r8"),
    )
    return CfgProgram({0: b0, 1: b1, 2: b2, 3: b3}, entry=0)


def main() -> None:
    cfg = sample_diamond_cfg()
    res = trace_schedule_cfg(cfg, 2)
    live_in, live_out = compute_liveness(cfg)

    print(f"Selected hot trace: {[f'B{b}' for b in res.trace]}")
    print(f"Liveness: B0 live_out={sorted(live_out[0])}, cold B2 live_in={sorted(live_in[2])}")
    print(
        f"Hot-path cycles (width=2): basic-block-only = {res.bb_cycles} vs global-trace = {res.trace_cycles}"
    )

    print("\nGlobal trace schedule:")
    for entry in res.schedule:
        print(f"  cycle {entry.cycle:2d} | from B{entry.orig_block} | {entry.desc}")

    print("\nCode motions & §10.4 compensation:")
    for m in res.motions:
        print(f"  {m.op_desc:<30} -> {m.kind} (from B{m.from_block}, boundary B{m.boundary_block}, comp={m.comp_block})")

    hot_orig = simulate_cfg(cfg, {"r_in": 4, "r_guard": 2}, [20, 0])
    hot_opt = simulate_cfg(res.transformed_cfg, {"r_in": 4, "r_guard": 2}, [20, 0])
    cold_orig = simulate_cfg(cfg, {"r_in": 4, "r_guard": 0}, [20, 0])
    cold_opt = simulate_cfg(res.transformed_cfg, {"r_in": 4, "r_guard": 0}, [20, 0])

    print(f"\nVerification (hot r_guard=2): orig={hot_orig.ret} opt={hot_opt.ret}, mem={hot_opt.mem}")
    print(
        f"Verification (cold r_guard=0): orig={cold_orig.ret} opt={cold_opt.ret}, mem={cold_opt.mem} (comp ops in B2: {len(res.transformed_cfg.blocks[2].insts)})"
    )


class TraceScheduleTest(unittest.TestCase):
    def test_trace_scheduling_hoists_speculative_ops_adds_compensation_and_saves_cycles(self) -> None:
        cfg = sample_diamond_cfg()
        res = trace_schedule_cfg(cfg, 2)
        self.assertEqual(res.trace, [0, 1, 3])
        self.assertEqual(res.bb_cycles, 13)
        self.assertEqual(res.trace_cycles, 9)
        self.assertLess(res.trace_cycles, res.bb_cycles)
        self.assertTrue(
            any("r6 = r_in + 10" in m.op_desc and m.kind == "join_comp" and m.comp_block == 2 for m in res.motions)
        )
        br_cyc = next(e.cycle for e in res.schedule if e.desc.startswith("br_zero"))
        div_cyc = next(e.cycle for e in res.schedule if "[trap]" in e.desc)
        self.assertGreater(div_cyc, br_cyc)

        hot_orig = simulate_cfg(cfg, {"r_in": 4, "r_guard": 2}, [20, 0])
        hot_opt = simulate_cfg(res.transformed_cfg, {"r_in": 4, "r_guard": 2}, [20, 0])
        self.assertEqual((hot_orig.ret, hot_opt.ret, hot_opt.mem), (101, 101, [20, 101]))

        cold_orig = simulate_cfg(cfg, {"r_in": 4, "r_guard": 0}, [20, 0])
        cold_opt = simulate_cfg(res.transformed_cfg, {"r_in": 4, "r_guard": 0}, [20, 0])
        self.assertEqual((cold_orig.ret, cold_opt.ret, cold_opt.mem), (54, 54, [20, 54]))

    def test_blocks_speculative_clobber_when_dst_is_live_in_cold_branch(self) -> None:
        b0 = Block(100, [Inst(Op("load_safe", dst="r1", slot=0), 3)], Term("br_zero", cond="r_guard", if_zero=2, if_nonzero=1))
        b1 = Block(90, [Inst(Op("add_imm", dst="r_live", src="r_in", imm=99), 1), Inst(Op("add", dst="r_out", lhs="r1", rhs="r_live"), 1)], Term("ret", reg="r_out"))
        b2 = Block(10, [Inst(Op("add", dst="r_out", lhs="r1", rhs="r_live"), 1)], Term("ret", reg="r_out"))
        cfg = CfgProgram({0: b0, 1: b1, 2: b2}, entry=0)
        res = trace_schedule_cfg(cfg, 2)
        cold_orig = simulate_cfg(cfg, {"r_in": 5, "r_live": 7, "r_guard": 0}, [20])
        cold_opt = simulate_cfg(res.transformed_cfg, {"r_in": 5, "r_live": 7, "r_guard": 0}, [20])
        self.assertEqual((cold_orig.ret, cold_opt.ret), (27, 27))

    def test_blocks_speculative_memory_store_above_controlling_branch(self) -> None:
        b0 = Block(100, [Inst(Op("load_safe", dst="r1", slot=0), 3)], Term("br_zero", cond="r_guard", if_zero=2, if_nonzero=1))
        b1 = Block(90, [Inst(Op("store", slot=1, src="r_in"), 1), Inst(Op("add_imm", dst="r_out", src="r1", imm=1), 1)], Term("ret", reg="r_out"))
        b2 = Block(10, [Inst(Op("load_safe", dst="r_out", slot=1), 1)], Term("ret", reg="r_out"))
        cfg = CfgProgram({0: b0, 1: b1, 2: b2}, entry=0)
        res = trace_schedule_cfg(cfg, 2)
        cold_orig = simulate_cfg(cfg, {"r_in": 999, "r_guard": 0}, [20, 42])
        cold_opt = simulate_cfg(res.transformed_cfg, {"r_in": 999, "r_guard": 0}, [20, 42])
        self.assertEqual((cold_orig.ret, cold_opt.ret, cold_opt.mem), (42, 42, [20, 42]))


if __name__ == "__main__":
    main()
