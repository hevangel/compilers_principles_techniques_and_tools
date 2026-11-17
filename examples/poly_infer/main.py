"""Infer let-polymorphic Hindley-Milner types and compare nominal vs structural records."""

from dataclasses import dataclass
import unittest


@dataclass(frozen=True)
class TVar:
    id: int

    def __str__(self) -> str:
        return f"t{self.id}"


@dataclass(frozen=True)
class TInt:
    def __str__(self) -> str:
        return "Int"


@dataclass(frozen=True)
class TBool:
    def __str__(self) -> str:
        return "Bool"


@dataclass(frozen=True)
class TPair:
    first: "Type"
    second: "Type"

    def __str__(self) -> str:
        return f"({self.first}, {self.second})"


@dataclass(frozen=True)
class TFun:
    src: "Type"
    dst: "Type"

    def __str__(self) -> str:
        return f"({self.src} -> {self.dst})"


@dataclass(frozen=True)
class TRecord:
    tag: str
    fields: tuple[tuple[str, "Type"], ...]

    def __str__(self) -> str:
        fs = ", ".join(f"{n}: {t}" for n, t in self.fields)
        return f"{self.tag}{{{fs}}}"


Type = TVar | TInt | TBool | TPair | TFun | TRecord


@dataclass(frozen=True)
class TypeScheme:
    quantified: frozenset[int]
    ty: Type


@dataclass(frozen=True)
class VarExpr:
    name: str


@dataclass(frozen=True)
class IntLitExpr:
    pass


@dataclass(frozen=True)
class BoolLitExpr:
    pass


@dataclass(frozen=True)
class PairExpr:
    first: "Expr"
    second: "Expr"


@dataclass(frozen=True)
class LamExpr:
    param: str
    body: "Expr"


@dataclass(frozen=True)
class AppExpr:
    fun: "Expr"
    arg: "Expr"


@dataclass(frozen=True)
class LetExpr:
    name: str
    rhs: "Expr"
    body: "Expr"


@dataclass(frozen=True)
class RecordLitExpr:
    tag: str
    fields: list[tuple[str, "Expr"]]


Expr = (
    VarExpr
    | IntLitExpr
    | BoolLitExpr
    | PairExpr
    | LamExpr
    | AppExpr
    | LetExpr
    | RecordLitExpr
)


class InfiniteTypeError(TypeError):
    pass


class TypeClashError(TypeError):
    pass


class UnboundVarError(NameError):
    pass


def apply_subst(ty: Type, subst: dict[int, Type]) -> Type:
    if isinstance(ty, TVar):
        return apply_subst(subst[ty.id], subst) if ty.id in subst else ty
    if isinstance(ty, TPair):
        return TPair(apply_subst(ty.first, subst), apply_subst(ty.second, subst))
    if isinstance(ty, TFun):
        return TFun(apply_subst(ty.src, subst), apply_subst(ty.dst, subst))
    if isinstance(ty, TRecord):
        return TRecord(
            ty.tag,
            tuple((n, apply_subst(t, subst)) for n, t in ty.fields),
        )
    return ty


def free_vars_type(ty: Type) -> set[int]:
    if isinstance(ty, TVar):
        return {ty.id}
    if isinstance(ty, (TPair, TFun)):
        a, b = (ty.first, ty.second) if isinstance(ty, TPair) else (ty.src, ty.dst)
        return free_vars_type(a) | free_vars_type(b)
    if isinstance(ty, TRecord):
        out: set[int] = set()
        for _, ft in ty.fields:
            out |= free_vars_type(ft)
        return out
    return set()


def free_vars_env(env: dict[str, TypeScheme]) -> set[int]:
    out: set[int] = set()
    for scheme in env.values():
        out |= free_vars_type(scheme.ty) - scheme.quantified
    return out


def apply_subst_env(env: dict[str, TypeScheme], subst: dict[int, Type]) -> dict[str, TypeScheme]:
    res: dict[str, TypeScheme] = {}
    for k, scheme in env.items():
        filtered = {vid: t for vid, t in subst.items() if vid not in scheme.quantified}
        res[k] = TypeScheme(scheme.quantified, apply_subst(scheme.ty, filtered))
    return res


def occurs(vid: int, ty: Type) -> bool:
    return vid in free_vars_type(ty)


def unify(t1: Type, t2: Type, subst: dict[int, Type]) -> None:
    a = apply_subst(t1, subst)
    b = apply_subst(t2, subst)
    if a == b:
        return
    if isinstance(a, TVar):
        if occurs(a.id, b):
            raise InfiniteTypeError(f"infinite type involving t{a.id}")
        subst[a.id] = b
    elif isinstance(b, TVar):
        if occurs(b.id, a):
            raise InfiniteTypeError(f"infinite type involving t{b.id}")
        subst[b.id] = a
    elif isinstance(a, TPair) and isinstance(b, TPair):
        unify(a.first, b.first, subst)
        unify(a.second, b.second, subst)
    elif isinstance(a, TFun) and isinstance(b, TFun):
        unify(a.src, b.src, subst)
        unify(a.dst, b.dst, subst)
    elif (
        isinstance(a, TRecord)
        and isinstance(b, TRecord)
        and a.tag == b.tag
        and len(a.fields) == len(b.fields)
    ):
        for (n1, ty1), (n2, ty2) in zip(a.fields, b.fields):
            if n1 != n2:
                raise TypeClashError(f"field mismatch: {n1} vs {n2}")
            unify(ty1, ty2, subst)
    else:
        raise TypeClashError(f"cannot unify {a} with {b}")


class InferCtx:
    def __init__(self) -> None:
        self.next_id = 0
        self.subst: dict[int, Type] = {}

    def fresh(self) -> TVar:
        vid = self.next_id
        self.next_id += 1
        return TVar(vid)

    def instantiate(self, scheme: TypeScheme) -> Type:
        rename: dict[int, Type] = {q: self.fresh() for q in sorted(scheme.quantified)}
        return apply_subst(scheme.ty, rename)

    def infer(self, env: dict[str, TypeScheme], expr: Expr) -> Type:
        if isinstance(expr, VarExpr):
            if expr.name not in env:
                raise UnboundVarError(expr.name)
            return self.instantiate(env[expr.name])
        if isinstance(expr, IntLitExpr):
            return TInt()
        if isinstance(expr, BoolLitExpr):
            return TBool()
        if isinstance(expr, PairExpr):
            ta = self.infer(env, expr.first)
            tb = self.infer(env, expr.second)
            return TPair(apply_subst(ta, self.subst), apply_subst(tb, self.subst))
        if isinstance(expr, LamExpr):
            pty = self.fresh()
            inner = dict(env)
            inner[expr.param] = TypeScheme(frozenset(), pty)
            bty = self.infer(inner, expr.body)
            return TFun(apply_subst(pty, self.subst), apply_subst(bty, self.subst))
        if isinstance(expr, AppExpr):
            fty = self.infer(env, expr.fun)
            aty = self.infer(apply_subst_env(env, self.subst), expr.arg)
            rty = self.fresh()
            unify(fty, TFun(aty, rty), self.subst)
            return apply_subst(rty, self.subst)
        if isinstance(expr, LetExpr):
            rty = self.infer(env, expr.rhs)
            env_after = apply_subst_env(env, self.subst)
            resolved = apply_subst(rty, self.subst)
            quant = frozenset(free_vars_type(resolved) - free_vars_env(env_after))
            inner = dict(env_after)
            inner[expr.name] = TypeScheme(quant, resolved)
            return self.infer(inner, expr.body)
        assert isinstance(expr, RecordLitExpr)
        typed = tuple((n, apply_subst(self.infer(env, fe), self.subst)) for n, fe in expr.fields)
        return TRecord(expr.tag, typed)


def infer_type(expr: Expr) -> Type:
    ctx = InferCtx()
    ty = ctx.infer({}, expr)
    return apply_subst(ty, ctx.subst)


def structural_eq(a: Type, b: Type) -> bool:
    if isinstance(a, TRecord) and isinstance(b, TRecord):
        return len(a.fields) == len(b.fields) and all(
            n1 == n2 and structural_eq(t1, t2)
            for (n1, t1), (n2, t2) in zip(a.fields, b.fields)
        )
    if isinstance(a, TPair) and isinstance(b, TPair):
        return structural_eq(a.first, b.first) and structural_eq(a.second, b.second)
    if isinstance(a, TFun) and isinstance(b, TFun):
        return structural_eq(a.src, b.src) and structural_eq(a.dst, b.dst)
    return a == b


def nominal_eq(a: Type, b: Type) -> bool:
    return a == b


def main() -> None:
    poly = LetExpr(
        "id",
        LamExpr("x", VarExpr("x")),
        PairExpr(AppExpr(VarExpr("id"), IntLitExpr()), AppExpr(VarExpr("id"), BoolLitExpr())),
    )
    print(f"let id = \\x. x in (id 1, id true) : {infer_type(poly)}")
    p1 = infer_type(RecordLitExpr("Point", [("x", IntLitExpr()), ("y", IntLitExpr())]))
    p2 = infer_type(RecordLitExpr("Vec2", [("x", IntLitExpr()), ("y", IntLitExpr())]))
    print(f"{p1} vs {p2} => structural_eq={structural_eq(p1, p2)}, nominal_eq={nominal_eq(p1, p2)}")


class PolyInferTests(unittest.TestCase):
    def test_let_polymorphism_allows_multiple_instantiations_unlike_lambda(self) -> None:
        poly = LetExpr(
            "id",
            LamExpr("x", VarExpr("x")),
            PairExpr(AppExpr(VarExpr("id"), IntLitExpr()), AppExpr(VarExpr("id"), BoolLitExpr())),
        )
        self.assertEqual(infer_type(poly), TPair(TInt(), TBool()))

        mono_fail = AppExpr(
            LamExpr(
                "id",
                PairExpr(
                    AppExpr(VarExpr("id"), IntLitExpr()),
                    AppExpr(VarExpr("id"), BoolLitExpr()),
                ),
            ),
            LamExpr("x", VarExpr("x")),
        )
        with self.assertRaises(TypeClashError):
            infer_type(mono_fail)

    def test_detects_occurs_check_and_distinguishes_nominal_vs_structural_records(self) -> None:
        self_app = LamExpr("x", AppExpr(VarExpr("x"), VarExpr("x")))
        with self.assertRaises(InfiniteTypeError):
            infer_type(self_app)
        with self.assertRaises(UnboundVarError):
            infer_type(VarExpr("missing"))

        p1 = infer_type(RecordLitExpr("Point", [("x", IntLitExpr()), ("y", IntLitExpr())]))
        p2 = infer_type(RecordLitExpr("Vec2", [("x", IntLitExpr()), ("y", IntLitExpr())]))
        self.assertTrue(structural_eq(p1, p2))
        self.assertFalse(nominal_eq(p1, p2))


if __name__ == "__main__":
    main()
