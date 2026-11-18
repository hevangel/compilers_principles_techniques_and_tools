"""Modulo schedule inner loops with resource/recurrence bounds and modular variable expansion."""

from dataclasses import dataclass
import math
import unittest


@dataclass(frozen=True)
class LoopOp:
    """Single instruction in the inner loop body."""

    op_id: int
    name: str
    unit: str  # Functional unit required ("Alu" or "Mem").
    latency: int
    kind: str
    args: tuple[int, ...]


@dataclass(frozen=True)
class DepEdge:
    """Dependence `src -> dst` requiring `S(src) + latency <= S(dst) + dist * II`."""

    src: int
    dst: int
    latency: int
    dist: int  # 0 = intra-iteration dependence; >=1 = loop-carried across `dist` iterations.


@dataclass
class LoopKernel:
    """Loop body operations, dependence graph, and hardware unit capacities."""

    ops: list[LoopOp]
    edges: list[DepEdge]
    capacities: dict[str, int]


@dataclass
class ModuloSchedule:
    """Cyclic schedule metadata plus Modular Variable Expansion (MVE) unroll factor."""

    ii: int
    mii_res: int
    mii_rec: int
    start_cycles: list[int]
    num_stages: int
    unroll_factor: int


def compute_mii_res(ops: list[LoopOp], capacities: dict[str, int]) -> int:
    """Resource lower bound: MII_res = max_u ceil(ops_u / cap_u)."""
    counts: dict[str, int] = {}
    for op in ops:
        counts[op.unit] = counts.get(op.unit, 0) + 1
    best = 1
    for unit, cnt in counts.items():
        cap = capacities.get(unit, 1)
        best = max(best, math.ceil(cnt / cap))
    return best


def compute_mii_rec(n: int, edges: list[DepEdge]) -> int:
    """Recurrence lower bound: MII_rec = max_C ceil(sum L / sum D) over elementary cycles.

    Uses `e.dst > start` canonicalization so each simple cycle is visited once.
    """
    adj: list[list[DepEdge]] = [[] for _ in range(n)]
    for e in edges:
        adj[e.src].append(e)
    best = 1
    visited = [False] * n

    def dfs(start: int, curr: int, sum_l: int, sum_d: int) -> None:
        nonlocal best
        for e in adj[curr]:
            nl = sum_l + e.latency
            nd = sum_d + e.dist
            if e.dst == start:
                if nd > 0 and nl > 0:
                    best = max(best, math.ceil(nl / nd))
            elif e.dst > start and not visited[e.dst]:
                visited[e.dst] = True
                dfs(start, e.dst, nl, nd)
                visited[e.dst] = False

    for start in range(n):
        visited[start] = True
        dfs(start, start, 0, 0)
        visited[start] = False
    return best


def modulo_schedule(kernel: LoopKernel) -> ModuloSchedule:
    """Find minimal II >= max(MII_res, MII_rec) with a valid modulo reservation table.

    Invariants:
    - Modulo resource slot `c % II` never exceeds `capacities[unit]`.
    - Dependence inequality `S(u) + L - D * II <= S(v)` holds for all edges.
    - `unroll_factor = ceil((max_span + 1) / II)` gives enough MVE register banks so
      overlapping iterations never overwrite a live value before its consumer reads it.
    """
    mii_res = compute_mii_res(kernel.ops, kernel.capacities)
    mii_rec = compute_mii_rec(len(kernel.ops), kernel.edges)
    ii = max(mii_res, mii_rec)
    total_lat = sum(op.latency for op in kernel.ops)

    while True:
        max_cycle = total_lat + ii * 2
        schedule: list[int | None] = [None] * len(kernel.ops)
        res_table: list[dict[str, int]] = [{} for _ in range(ii)]

        def backtrack(idx: int) -> bool:
            if idx == len(kernel.ops):
                return True
            earliest = 0
            latest = max_cycle
            for e in kernel.edges:
                if e.dst == idx and schedule[e.src] is not None:
                    earliest = max(earliest, schedule[e.src] + e.latency - e.dist * ii)
                if e.src == idx and schedule[e.dst] is not None:
                    latest = min(latest, schedule[e.dst] - e.latency + e.dist * ii)
            unit = kernel.ops[idx].unit
            cap = kernel.capacities.get(unit, 1)
            for c in range(max(earliest, 0), min(latest, max_cycle) + 1):
                slot = c % ii
                used = res_table[slot].get(unit, 0)
                if used < cap:
                    schedule[idx] = c
                    res_table[slot][unit] = used + 1
                    if backtrack(idx + 1):
                        return True
                    res_table[slot][unit] -= 1
                    schedule[idx] = None
            return False

        if backtrack(0):
            starts = [s for s in schedule if s is not None]
            # Stage of op is `start // II`; total overlapping stages is `max(stage) + 1`.
            num_stages = max((c // ii + 1 for c in starts), default=1)
            max_span = 0
            for e in kernel.edges:
                span = max(starts[e.dst] + e.dist * ii - starts[e.src], 0)
                max_span = max(max_span, span)
            for op in kernel.ops:
                max_span = max(max_span, op.latency)
            unroll = max(1, math.ceil((max_span + 1) / ii))
            return ModuloSchedule(ii, mii_res, mii_rec, starts, num_stages, unroll)
        ii += 1


@dataclass(frozen=True)
class ScheduledInst:
    """One scheduled instruction copy tagged with its pipeline stage and MVE register bank."""

    op_id: int
    name: str
    iter_offset: int
    mve_bank: int
    cycle_in_ii: int


@dataclass
class PipelinedProgram:
    """Ramp-up prologue, MVE-unrolled steady-state kernel, and drain epilogue."""

    schedule: ModuloSchedule
    prologue: list[ScheduledInst]
    kernel_body: list[list[ScheduledInst]]
    epilogue: list[ScheduledInst]


def build_pipelined_program(
    kernel: LoopKernel, sched: ModuloSchedule, trip_count: int
) -> PipelinedProgram:
    stages = sched.num_stages
    ii = sched.ii
    unroll = sched.unroll_factor
    prologue: list[ScheduledInst] = []
    kernel_body: list[list[ScheduledInst]] = []
    epilogue: list[ScheduledInst] = []

    if trip_count < stages:
        by_cyc: dict[int, list[tuple[int, int]]] = {}
        for it in range(trip_count):
            for op in kernel.ops:
                cyc = it * ii + sched.start_cycles[op.op_id]
                by_cyc.setdefault(cyc, []).append((it, op.op_id))
        for cyc in sorted(by_cyc):
            for it, op_id in by_cyc[cyc]:
                prologue.append(ScheduledInst(op_id, kernel.ops[op_id].name, it, it % unroll, cyc))
        return PipelinedProgram(sched, prologue, kernel_body, epilogue)

    for beat in range(stages - 1):
        for c_off in range(ii):
            for op in kernel.ops:
                st = sched.start_cycles[op.op_id] // ii
                if sched.start_cycles[op.op_id] % ii == c_off and st <= beat:
                    it = beat - st
                    prologue.append(
                        ScheduledInst(op.op_id, op.name, it, it % unroll, beat * ii + c_off)
                    )

    for u in range(unroll):
        beat_ops: list[ScheduledInst] = []
        for c_off in range(ii):
            for op in kernel.ops:
                if sched.start_cycles[op.op_id] % ii == c_off:
                    st = sched.start_cycles[op.op_id] // ii
                    bank = (stages - 1 + u + unroll - (st % unroll)) % unroll
                    beat_ops.append(ScheduledInst(op.op_id, op.name, st, bank, c_off))
        kernel_body.append(beat_ops)

    for beat in range(stages - 1):
        for c_off in range(ii):
            for op in kernel.ops:
                st = sched.start_cycles[op.op_id] // ii
                if sched.start_cycles[op.op_id] % ii == c_off and st > beat:
                    it = trip_count - st + beat
                    epilogue.append(
                        ScheduledInst(op.op_id, op.name, it, it % unroll, beat * ii + c_off)
                    )

    return PipelinedProgram(sched, prologue, kernel_body, epilogue)


def eval_op(
    op: LoopOp,
    it: int,
    bank: int,
    mem: list[list[int]],
    regs: dict[tuple[int, int], int],
    accum: dict[int, int],
) -> None:
    if op.kind == "load":
        dst, base = op.args
        regs[(dst, bank)] = mem[base][it]
    elif op.kind == "add_const":
        dst, src, imm = op.args
        regs[(dst, bank)] = regs[(src, bank)] + imm
    elif op.kind == "mul_const":
        dst, src, imm = op.args
        regs[(dst, bank)] = regs[(src, bank)] * imm
    elif op.kind == "accum":
        acc, src = op.args
        val = accum.get(acc, 0) + regs[(src, bank)]
        accum[acc] = val
        regs[(acc, bank)] = val
    elif op.kind == "store":
        src, base = op.args
        mem[base][it] = regs[(src, bank)]


def execute_sequential(
    kernel: LoopKernel, mem: list[list[int]], trip_count: int
) -> tuple[list[list[int]], dict[int, int]]:
    mem_copy = [row[:] for row in mem]
    regs: dict[tuple[int, int], int] = {}
    accum: dict[int, int] = {}
    for it in range(trip_count):
        for op in kernel.ops:
            eval_op(op, it, 0, mem_copy, regs, accum)
    return mem_copy, accum


def execute_pipelined(
    kernel: LoopKernel, prog: PipelinedProgram, mem: list[list[int]], trip_count: int
) -> tuple[list[list[int]], dict[int, int]]:
    mem_copy = [row[:] for row in mem]
    regs: dict[tuple[int, int], int] = {}
    accum: dict[int, int] = {}
    stages = prog.schedule.num_stages
    if trip_count < stages:
        for inst in prog.prologue:
            eval_op(kernel.ops[inst.op_id], inst.iter_offset, inst.mve_bank, mem_copy, regs, accum)
        return mem_copy, accum

    for inst in prog.prologue:
        eval_op(kernel.ops[inst.op_id], inst.iter_offset, inst.mve_bank, mem_copy, regs, accum)

    unroll = prog.schedule.unroll_factor
    for k in range(trip_count - (stages - 1)):
        for inst in prog.kernel_body[k % unroll]:
            it = k + (stages - 1) - inst.iter_offset
            eval_op(kernel.ops[inst.op_id], it, it % unroll, mem_copy, regs, accum)

    for inst in prog.epilogue:
        eval_op(kernel.ops[inst.op_id], inst.iter_offset, inst.mve_bank, mem_copy, regs, accum)
    return mem_copy, accum


def resource_bound_kernel() -> LoopKernel:
    ops = [
        LoopOp(0, "ld_a", "Mem", 2, "load", (0, 0)),
        LoopOp(1, "add1", "Alu", 1, "add_const", (1, 0, 1)),
        LoopOp(2, "mul2", "Alu", 1, "mul_const", (2, 1, 2)),
        LoopOp(3, "st_b", "Mem", 1, "store", (2, 1)),
    ]
    edges = [DepEdge(0, 1, 2, 0), DepEdge(1, 2, 1, 0), DepEdge(2, 3, 1, 0)]
    return LoopKernel(ops, edges, {"Alu": 1, "Mem": 2})


def recurrence_bound_kernel() -> LoopKernel:
    ops = [
        LoopOp(0, "ld_a", "Mem", 2, "load", (0, 0)),
        LoopOp(1, "mul3", "Alu", 1, "mul_const", (1, 0, 3)),
        LoopOp(2, "accum", "Alu", 3, "accum", (2, 1)),
        LoopOp(3, "st_b", "Mem", 1, "store", (2, 1)),
    ]
    edges = [
        DepEdge(0, 1, 2, 0),
        DepEdge(1, 2, 1, 0),
        DepEdge(2, 2, 3, 1),
        DepEdge(2, 3, 3, 0),
    ]
    return LoopKernel(ops, edges, {"Alu": 2, "Mem": 2})


def main() -> None:
    k_res = resource_bound_kernel()
    s_res = modulo_schedule(k_res)
    p_res = build_pipelined_program(k_res, s_res, 8)
    mem_res = [[1, 2, 3, 4, 5, 6, 7, 8], [0] * 8]
    seq_res, _ = execute_sequential(k_res, mem_res, 8)
    pipe_res, _ = execute_pipelined(k_res, p_res, mem_res, 8)
    print(
        f"Resource-bound kernel: MII_res={s_res.mii_res}, MII_rec={s_res.mii_rec}, "
        f"II={s_res.ii}, stages={s_res.num_stages}, MVE unroll={s_res.unroll_factor}"
    )
    print(f"  cycles={s_res.start_cycles}, match_seq={seq_res == pipe_res}")

    k_rec = recurrence_bound_kernel()
    s_rec = modulo_schedule(k_rec)
    p_rec = build_pipelined_program(k_rec, s_rec, 8)
    mem_rec = [[2, 4, 1, 5, 3, 6, 2, 1], [0] * 8]
    seq_rec, seq_acc = execute_sequential(k_rec, mem_rec, 8)
    pipe_rec, pipe_acc = execute_pipelined(k_rec, p_rec, mem_rec, 8)
    banks = sorted({inst.mve_bank for beat in p_rec.kernel_body for inst in beat})
    print(
        f"Recurrence-bound kernel: MII_res={s_rec.mii_res}, MII_rec={s_rec.mii_rec}, "
        f"II={s_rec.ii}, stages={s_rec.num_stages}, MVE banks={banks}"
    )
    print(
        f"  cycles={s_rec.start_cycles}, final_out={pipe_rec[1]}, "
        f"match_seq={seq_rec == pipe_rec and seq_acc == pipe_acc}"
    )


class SoftwarePipelineTests(unittest.TestCase):
    def test_resource_bound_modulo_schedule(self) -> None:
        kernel = resource_bound_kernel()
        sched = modulo_schedule(kernel)
        self.assertEqual(sched.mii_res, 2)
        self.assertEqual(sched.ii, 2)
        mem = [[10, 20, 30, 40, 50, 60, 70], [0] * 7]
        prog = build_pipelined_program(kernel, sched, 7)
        seq, _ = execute_sequential(kernel, mem, 7)
        pipe, _ = execute_pipelined(kernel, prog, mem, 7)
        self.assertEqual(pipe, seq)
        self.assertEqual(pipe[1], [22, 42, 62, 82, 102, 122, 142])

    def test_recurrence_bound_modulo_schedule_and_mve(self) -> None:
        kernel = recurrence_bound_kernel()
        sched = modulo_schedule(kernel)
        self.assertEqual(sched.mii_rec, 3)
        self.assertEqual(sched.ii, 3)
        mem = [[1, 2, 3, 4, 5, 6, 7, 8], [0] * 8]
        prog = build_pipelined_program(kernel, sched, 8)
        seq_m, seq_a = execute_sequential(kernel, mem, 8)
        pipe_m, pipe_a = execute_pipelined(kernel, prog, mem, 8)
        self.assertEqual(pipe_m, seq_m)
        self.assertEqual(pipe_a, seq_a)
        self.assertEqual(pipe_m[1], [3, 9, 18, 30, 45, 63, 84, 108])


if __name__ == "__main__":
    main()
