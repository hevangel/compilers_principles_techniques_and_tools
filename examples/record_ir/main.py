"""Compute aligned record/union field offsets and compare quadruple, triple, and indirect-triple IR."""

from dataclasses import dataclass
from enum import Enum
import unittest


@dataclass(frozen=True)
class PrimType:
    """Scalar primitive type with fixed ABI byte size and alignment requirement."""

    name: str
    size: int
    align: int


@dataclass(frozen=True)
class StructType:
    """Product type whose fields are laid out sequentially with alignment padding."""

    members: list[tuple[str, "Type"]]


@dataclass(frozen=True)
class UnionType:
    """Sum/overlay type whose fields all begin at offset 0."""

    members: list[tuple[str, "Type"]]


Type = PrimType | StructType | UnionType
U8 = PrimType("U8", 1, 1)
U32 = PrimType("U32", 4, 4)
U64 = PrimType("U64", 8, 8)


@dataclass(frozen=True)
class FieldLayout:
    """Computed byte offset, size, and alignment of a single record member."""

    name: str
    offset: int
    size: int
    align: int


@dataclass(frozen=True)
class RecordLayout:
    """Total padded size, strictest alignment, and padding waste of a `Type`."""

    size: int
    align: int
    padding_bytes: int
    fields: list[FieldLayout]


def align_up(offset: int, align: int) -> int:
    """Round `offset` up to the smallest multiple of `align` >= `offset`."""
    if align <= 1:
        return offset
    return ((offset + align - 1) // align) * align


def layout_type(ty: Type) -> RecordLayout:
    """Compute natural C-style field offsets, alignment, and padding for `ty`."""
    if isinstance(ty, PrimType):
        return RecordLayout(ty.size, ty.align, 0, [])
    if isinstance(ty, StructType):
        cursor = 0
        # Struct alignment equals the max member alignment so every field stays aligned.
        max_align = 1
        payload = 0
        fields: list[FieldLayout] = []
        for name, mty in ty.members:
            m = layout_type(mty)
            # Insert internal padding before `name` if `cursor` is not aligned to `m.align`.
            off = align_up(cursor, m.align)
            max_align = max(max_align, m.align)
            payload += m.size
            fields.append(FieldLayout(name, off, m.size, m.align))
            cursor = off + m.size
        # Add tail padding so arrays of this struct align every element (`max(..., 1)` avoids size 0).
        total = max(align_up(cursor, max_align), 1)
        return RecordLayout(total, max_align, max(0, total - payload), fields)
    max_size = 1
    max_align = 1
    fields = []
    for name, mty in ty.members:
        m = layout_type(mty)
        max_size = max(max_size, m.size)
        max_align = max(max_align, m.align)
        # All union members overlap at offset 0.
        fields.append(FieldLayout(name, 0, m.size, m.align))
    # Union size must fit the largest member and be a multiple of the strictest member alignment.
    total = align_up(max_size, max_align)
    max_member = max((f.size for f in fields), default=0)
    return RecordLayout(total, max_align, total - max_member, fields)


class BinOp(Enum):
    ADD = "+"
    MUL = "*"


@dataclass(frozen=True)
class VarExpr:
    name: str


@dataclass(frozen=True)
class FieldAddrExpr:
    """Computes `base + offset` for a struct/union field address."""

    base: str
    offset: int


@dataclass(frozen=True)
class BinExpr:
    op: BinOp
    left: "Expr"
    right: "Expr"


Expr = VarExpr | FieldAddrExpr | BinExpr


@dataclass(frozen=True)
class Quadruple:
    """Four-field three-address instruction using explicit named temporaries (`t0`, `t1`, ...)."""

    op: str
    arg1: str
    arg2: str
    result: str


@dataclass(frozen=True)
class SymRef:
    """Variable/symbol operand reference."""

    name: str


@dataclass(frozen=True)
class ConstRef:
    """Constant integer operand reference."""

    val: int


@dataclass(frozen=True)
class TripleIdxRef:
    """Positional reference to the value produced by triple slot `idx`."""

    idx: int


ArgRef = SymRef | ConstRef | TripleIdxRef


@dataclass(frozen=True)
class Triple:
    """Three-field instruction whose result is identified by its slot index `(idx)`."""

    op: str
    arg1: ArgRef
    arg2: ArgRef


@dataclass
class IndirectTriples:
    """Pairs an immutable `triples` table with a reorderable `order` schedule list."""

    triples: list[Triple]
    order: list[int]


def lower_to_quads(stmts: list[tuple[str, Expr]]) -> list[Quadruple]:
    """Lower assignments into `Quadruple` form with explicit fresh temporaries."""
    next_t = 0
    out: list[Quadruple] = []

    def emit(expr: Expr) -> str:
        nonlocal next_t
        if isinstance(expr, VarExpr):
            return expr.name
        if isinstance(expr, FieldAddrExpr):
            res = f"t{next_t}"
            next_t += 1
            out.append(Quadruple("addr+", expr.base, str(expr.offset), res))
            return res
        lv = emit(expr.left)
        rv = emit(expr.right)
        res = f"t{next_t}"
        next_t += 1
        out.append(Quadruple(expr.op.value, lv, rv, res))
        return res

    for dst, expr in stmts:
        src = emit(expr)
        out.append(Quadruple("=", src, "", dst))
    return out


def lower_to_triples(stmts: list[tuple[str, Expr]]) -> IndirectTriples:
    """Lower assignments into `IndirectTriples` (sequential initial `order`)."""
    triples: list[Triple] = []

    def emit(expr: Expr) -> ArgRef:
        if isinstance(expr, VarExpr):
            return SymRef(expr.name)
        if isinstance(expr, FieldAddrExpr):
            idx = len(triples)
            triples.append(Triple("addr+", SymRef(expr.base), ConstRef(expr.offset)))
            return TripleIdxRef(idx)
        la = emit(expr.left)
        ra = emit(expr.right)
        idx = len(triples)
        triples.append(Triple(expr.op.value, la, ra))
        return TripleIdxRef(idx)

    for dst, expr in stmts:
        rhs = emit(expr)
        triples.append(Triple("=", SymRef(dst), rhs))
    return IndirectTriples(triples, list(range(len(triples))))


def eval_quads(quads: list[Quadruple], initial: dict[str, int]) -> dict[str, int]:
    """Evaluate `Quadruple` instructions sequentially from `initial` bindings."""
    env = dict(initial)

    def get(s: str) -> int:
        try:
            return int(s)
        except ValueError:
            return env.get(s, 0)

    for q in quads:
        if q.op in {"+", "addr+"}:
            env[q.result] = get(q.arg1) + get(q.arg2)
        elif q.op == "*":
            env[q.result] = get(q.arg1) * get(q.arg2)
        elif q.op == "=":
            env[q.result] = get(q.arg1)
        else:
            raise ValueError(q.op)
    return env


def eval_indirect_triples(it: IndirectTriples, initial: dict[str, int]) -> dict[str, int]:
    """Evaluate `IndirectTriples` in `it.order`, storing each result at its fixed triple slot."""
    env = dict(initial)
    # Keyed by fixed table slot `idx`, so permuting `it.order` preserves `TripleIdxRef(idx)`.
    results: dict[int, int] = {}

    def resolve(a: ArgRef) -> int:
        if isinstance(a, SymRef):
            return env.get(a.name, 0)
        if isinstance(a, ConstRef):
            return a.val
        return results[a.idx]

    for idx in it.order:
        t = it.triples[idx]
        if t.op in {"+", "addr+"}:
            results[idx] = resolve(t.arg1) + resolve(t.arg2)
        elif t.op == "*":
            results[idx] = resolve(t.arg1) * resolve(t.arg2)
        elif t.op == "=":
            assert isinstance(t.arg1, SymRef)
            val = resolve(t.arg2)
            env[t.arg1.name] = val
            results[idx] = val
        else:
            raise ValueError(t.op)
    return env


def main() -> None:
    unpadded = StructType([("a", U8), ("b", U32), ("c", U8)])
    packed = StructType([("b", U32), ("a", U8), ("c", U8)])
    nested_union = StructType(
        [("tag", U8), ("payload", UnionType([("word", U64), ("pair", unpadded)]))]
    )
    lu = layout_type(unpadded)
    lp = layout_type(packed)
    lnu = layout_type(nested_union)
    print(f"{{u8, u32, u8}} => size={lu.size}, padding={lu.padding_bytes}, fields={lu.fields}")
    print(f"{{u32, u8, u8}} => size={lp.size}, padding={lp.padding_bytes}, fields={lp.fields}")
    print(f"nested union struct => size={lnu.size}, align={lnu.align}, padding={lnu.padding_bytes}")

    stmts = [
        ("p", FieldAddrExpr("base", lnu.fields[1].offset)),
        (
            "x",
            BinExpr(BinOp.ADD, BinExpr(BinOp.MUL, VarExpr("a"), VarExpr("b")), VarExpr("c")),
        ),
    ]
    quads = lower_to_quads(stmts)
    it = lower_to_triples(stmts)
    it.order = [2, 0, 1, 3, 4]
    init = {"base": 1000, "a": 3, "b": 4, "c": 5}
    env_q = eval_quads(quads, init)
    env_t = eval_indirect_triples(it, init)
    print(f"Quad x={env_q['x']}, IndirectTriple x={env_t['x']}, p={env_t['p']}")


class RecordIrTests(unittest.TestCase):
    def test_computes_struct_and_union_alignment_padding(self) -> None:
        unpadded = StructType([("a", U8), ("b", U32), ("c", U8)])
        lu = layout_type(unpadded)
        self.assertEqual((lu.size, lu.align, lu.padding_bytes), (12, 4, 6))
        self.assertEqual([f.offset for f in lu.fields], [0, 4, 8])

        packed = StructType([("b", U32), ("a", U8), ("c", U8)])
        lp = layout_type(packed)
        self.assertEqual((lp.size, lp.padding_bytes), (8, 2))

        u = UnionType([("x", U8), ("y", U64)])
        lu_u = layout_type(u)
        self.assertEqual((lu_u.size, lu_u.align), (8, 8))
        self.assertTrue(all(f.offset == 0 for f in lu_u.fields))

    def test_indirect_triple_reordering_preserves_operand_references_and_values(self) -> None:
        stmts = [
            ("p", FieldAddrExpr("base", 8)),
            (
                "res",
                BinExpr(
                    BinOp.ADD,
                    BinExpr(BinOp.MUL, VarExpr("x"), VarExpr("y")),
                    VarExpr("p"),
                ),
            ),
        ]
        quads = lower_to_quads(stmts)
        it = lower_to_triples(stmts)
        initial = {"base": 100, "x": 6, "y": 7}
        before = eval_indirect_triples(it, initial)
        it.order = [2, 0, 1, 3, 4]
        after = eval_indirect_triples(it, initial)
        q_env = eval_quads(quads, initial)
        self.assertEqual((before["p"], before["res"]), (108, 150))
        self.assertEqual((after["p"], after["res"]), (108, 150))
        self.assertEqual(q_env["res"], 150)
        self.assertEqual(it.triples[3].arg1, TripleIdxRef(2))


if __name__ == "__main__":
    main()
