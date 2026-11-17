"""Backward next-use/liveness pass, register/address descriptors, getReg codegen, and block-exit flush."""

from dataclasses import dataclass
from enum import Enum
import unittest


class BinOp(Enum):
    ADD = "Add"
    SUB = "Sub"


@dataclass(frozen=True)
class Quad:
    dst: str
    op: BinOp | None
    arg1: str
    arg2: str | None = None


@dataclass(frozen=True)
class VarUseInfo:
    live: bool
    next_use: int | None


@dataclass(frozen=True)
class InstrLiveness:
    dst_info: VarUseInfo
    arg1_info: VarUseInfo
    arg2_info: VarUseInfo | None


@dataclass(frozen=True)
class Ld:
    dst: int
    mem: str


@dataclass(frozen=True)
class St:
    src: int
    mem: str


@dataclass(frozen=True)
class Mov:
    dst: int
    src: int


@dataclass(frozen=True)
class Add:
    dst: int
    lhs: int
    rhs: int


@dataclass(frozen=True)
class Sub:
    dst: int
    lhs: int
    rhs: int


RiscInstr = Ld | St | Mov | Add | Sub


def compute_next_use(quads: list[Quad], live_on_exit: set[str]) -> list[InstrLiveness]:
    table: dict[str, VarUseInfo] = {v: VarUseInfo(True, None) for v in live_on_exit}
    dead = VarUseInfo(False, None)
    out = [InstrLiveness(dead, dead, None)] * len(quads)
    for idx in range(len(quads) - 1, -1, -1):
        q = quads[idx]
        dst_info = table.get(q.dst, dead)
        arg1_info = table.get(q.arg1, dead)
        arg2_info = table.get(q.arg2, dead) if q.arg2 is not None else None
        out[idx] = InstrLiveness(dst_info, arg1_info, arg2_info)
        table[q.dst] = dead
        table[q.arg1] = VarUseInfo(True, idx)
        if q.arg2 is not None:
            table[q.arg2] = VarUseInfo(True, idx)
    return out


class CodegenState:
    def __init__(self, num_regs: int, all_vars: set[str]) -> None:
        self.reg_desc: list[set[str]] = [set() for _ in range(num_regs)]
        self.addr_desc: dict[str, set[tuple[str, str | int]]] = {
            v: {("mem", v)} for v in all_vars
        }
        self.code: list[RiscInstr] = []

    def var_in_reg(self, var: str) -> int | None:
        for r, s in enumerate(self.reg_desc):
            if var in s:
                return r
        return None

    def clear_reg(self, r: int) -> None:
        for v in list(self.reg_desc[r]):
            self.addr_desc.get(v, set()).discard(("reg", r))
        self.reg_desc[r].clear()

    def spill_reg(self, r: int, future_live: dict[str, int]) -> None:
        for v in sorted(self.reg_desc[r]):
            locs = self.addr_desc.get(v, set())
            has_other = any(loc != ("reg", r) for loc in locs)
            if not has_other and v in future_live:
                self.code.append(St(r, v))
                self.addr_desc.setdefault(v, set()).add(("mem", v))
        self.clear_reg(r)

    def pick_victim(self, exclude: int | None, future_live: dict[str, int]) -> int:
        for r, s in enumerate(self.reg_desc):
            if r != exclude and not s:
                return r
        for r, s in enumerate(self.reg_desc):
            if r == exclude:
                continue
            if all(
                v not in future_live or any(l != ("reg", r) for l in self.addr_desc[v])
                for v in s
            ):
                self.clear_reg(r)
                return r
        candidates = [r for r in range(len(self.reg_desc)) if r != exclude]
        victim = max(
            candidates,
            key=lambda r: min(
                (future_live.get(v, 10**9) for v in self.reg_desc[r]), default=10**9
            ),
        )
        self.spill_reg(victim, future_live)
        return victim

    def ensure_in_reg(self, var: str, exclude: int | None, future_live: dict[str, int]) -> int:
        cur = self.var_in_reg(var)
        if cur is not None:
            return cur
        r = self.pick_victim(exclude, future_live)
        self.code.append(Ld(r, var))
        self.reg_desc[r].add(var)
        self.addr_desc.setdefault(var, set()).add(("reg", r))
        return r

    def get_reg_for_dst(
        self, q: Quad, info: InstrLiveness, rx: int, future_live: dict[str, int]
    ) -> int:
        if (q.arg1 == q.dst or not info.arg1_info.live) and self.reg_desc[rx] == {q.arg1}:
            return rx
        return self.pick_victim(None, future_live)


def generate_block(quads: list[Quad], num_regs: int, live_on_exit: set[str]) -> list[RiscInstr]:
    liveness = compute_next_use(quads, live_on_exit)
    all_vars = set(live_on_exit)
    for q in quads:
        all_vars.add(q.dst)
        all_vars.add(q.arg1)
        if q.arg2 is not None:
            all_vars.add(q.arg2)

    state = CodegenState(num_regs, all_vars)
    for idx, q in enumerate(quads):
        live_after: dict[str, int] = {v: 10**9 for v in live_on_exit}
        for later in range(idx + 1, len(quads)):
            for name in [quads[later].arg1] + ([quads[later].arg2] if quads[later].arg2 else []):
                if not any(quads[mid].dst == name for mid in range(idx + 1, later)):
                    live_after.setdefault(name, later)
        for mid in range(idx + 1, len(quads)):
            d = quads[mid].dst
            if live_after.get(d, 10**9) >= mid and d in live_on_exit:
                used_before = any(
                    quads[j].arg1 == d or quads[j].arg2 == d for j in range(idx + 1, mid)
                )
                if not used_before:
                    live_after.pop(d, None)

        live_during = dict(live_after)
        live_during[q.arg1] = idx
        if q.arg2 is not None:
            live_during[q.arg2] = idx

        rx = state.ensure_in_reg(q.arg1, None, live_during)
        if q.op is not None and q.arg2 is not None:
            ry = state.ensure_in_reg(q.arg2, rx, live_during)
            rz = state.get_reg_for_dst(q, liveness[idx], rx, live_after)
            state.code.append(Add(rz, rx, ry) if q.op == BinOp.ADD else Sub(rz, rx, ry))
            state.clear_reg(rz)
            state.reg_desc[rz].add(q.dst)
            for r, s in enumerate(state.reg_desc):
                if r != rz:
                    s.discard(q.dst)
            state.addr_desc[q.dst] = {("reg", rz)}
        else:
            rz = state.get_reg_for_dst(q, liveness[idx], rx, live_after)
            if rz != rx:
                state.code.append(Mov(rz, rx))
            for r, s in enumerate(state.reg_desc):
                if r != rz:
                    s.discard(q.dst)
            if q.dst not in state.reg_desc[rz]:
                if rz == rx and liveness[idx].arg1_info.live:
                    state.reg_desc[rz].add(q.dst)
                else:
                    state.clear_reg(rz)
                    state.reg_desc[rz].add(q.dst)
            state.addr_desc[q.dst] = {("reg", rz)}

    for v in sorted(live_on_exit):
        locs = state.addr_desc[v]
        if ("mem", v) not in locs:
            regs = sorted(l[1] for l in locs if l[0] == "reg")
            if regs:
                state.code.append(St(int(regs[0]), v))
    return state.code


def eval_quads(quads: list[Quad], initial: dict[str, int]) -> dict[str, int]:
    mem = dict(initial)
    for q in quads:
        v1 = mem.get(q.arg1, 0)
        if q.op == BinOp.ADD and q.arg2 is not None:
            res = v1 + mem.get(q.arg2, 0)
        elif q.op == BinOp.SUB and q.arg2 is not None:
            res = v1 - mem.get(q.arg2, 0)
        else:
            res = v1
        mem[q.dst] = res
    return mem


def eval_risc(code: list[RiscInstr], num_regs: int, initial: dict[str, int]) -> dict[str, int]:
    mem = dict(initial)
    regs = [0] * num_regs
    for ins in code:
        if isinstance(ins, Ld):
            regs[ins.dst] = mem.get(ins.mem, 0)
        elif isinstance(ins, St):
            mem[ins.mem] = regs[ins.src]
        elif isinstance(ins, Mov):
            regs[ins.dst] = regs[ins.src]
        elif isinstance(ins, Add):
            regs[ins.dst] = regs[ins.lhs] + regs[ins.rhs]
        elif isinstance(ins, Sub):
            regs[ins.dst] = regs[ins.lhs] - regs[ins.rhs]
    return mem


def textbook_block() -> tuple[list[Quad], set[str]]:
    quads = [
        Quad("t", BinOp.SUB, "a", "b"),
        Quad("u", BinOp.SUB, "a", "c"),
        Quad("v", BinOp.ADD, "t", "u"),
        Quad("a", None, "d", None),
        Quad("d", BinOp.ADD, "v", "u"),
    ]
    return quads, {"a", "d"}


def main() -> None:
    quads, live_out = textbook_block()
    code3 = generate_block(quads, 3, live_out)
    code2 = generate_block(quads, 2, live_out)
    init = {"a": 10, "b": 3, "c": 2, "d": 7}
    ref = eval_quads(quads, init)
    m3 = eval_risc(code3, 3, init)
    m2 = eval_risc(code2, 2, init)
    print(f"3-reg ({len(code3)} instrs): a={m3['a']}, d={m3['d']} (ref a={ref['a']}, d={ref['d']})")
    print(f"2-reg ({len(code2)} instrs): a={m2['a']}, d={m2['d']}")


class RegDescCodegenTests(unittest.TestCase):
    def test_three_and_two_reg_match_three_address_semantics(self) -> None:
        quads, live_out = textbook_block()
        init = {"a": 25, "b": 9, "c": 4, "d": 11}
        expected = eval_quads(quads, init)
        for k in (2, 3):
            code = generate_block(quads, k, live_out)
            mem = eval_risc(code, k, init)
            for v in live_out:
                self.assertEqual(mem[v], expected[v])

    def test_forces_spill_and_reload_with_two_registers(self) -> None:
        quads = [
            Quad("t1", BinOp.ADD, "a", "b"),
            Quad("t2", BinOp.ADD, "c", "d"),
            Quad("t3", BinOp.ADD, "e", "f"),
            Quad("out1", BinOp.SUB, "t1", "t2"),
            Quad("out", BinOp.ADD, "out1", "t3"),
        ]
        code = generate_block(quads, 2, {"out"})
        self.assertTrue(any(isinstance(i, St) and i.mem in {"t1", "t2"} for i in code))
        init = {"a": 10, "b": 20, "c": 3, "d": 4, "e": 5, "f": 6}
        self.assertEqual(eval_risc(code, 2, init)["out"], eval_quads(quads, init)["out"])


if __name__ == "__main__":
    main()
