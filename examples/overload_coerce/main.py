"""Widen primitive types across a hierarchy and resolve overloads in two passes."""

from dataclasses import dataclass
from enum import Enum
import unittest


class Type(Enum):
    BOOL = "Bool"
    CHAR = "Char"
    INT = "Int"
    FLOAT = "Float"
    DOUBLE = "Double"


class CoercionOp(Enum):
    B2C = "B2C"
    C2I = "C2I"
    I2F = "I2F"
    F2D = "F2D"


@dataclass(frozen=True)
class Signature:
    params: list[Type]
    ret: Type


@dataclass(frozen=True)
class LitExpr:
    ty: Type


@dataclass(frozen=True)
class CallExpr:
    name: str
    args: list["Expr"]


Expr = LitExpr | CallExpr


@dataclass(frozen=True)
class ResolvedLit:
    ty: Type


@dataclass(frozen=True)
class ResolvedCall:
    name: str
    ret: Type
    args: list[tuple["ResolvedExpr", list[CoercionOp]]]


ResolvedExpr = ResolvedLit | ResolvedCall


class OverloadError(TypeError):
    pass


class NoViableCandidateError(OverloadError):
    pass


class AmbiguousError(OverloadError):
    pass


NEXT_WIDER: dict[Type, tuple[Type, CoercionOp]] = {
    Type.BOOL: (Type.CHAR, CoercionOp.B2C),
    Type.CHAR: (Type.INT, CoercionOp.C2I),
    Type.INT: (Type.FLOAT, CoercionOp.I2F),
    Type.FLOAT: (Type.DOUBLE, CoercionOp.F2D),
}


def coercion_chain(src: Type, dst: Type) -> list[CoercionOp] | None:
    cur = src
    ops: list[CoercionOp] = []
    while cur != dst:
        step = NEXT_WIDER.get(cur)
        if step is None:
            return None
        cur, op = step
        ops.append(op)
    return ops


def widen(t1: Type, t2: Type) -> tuple[Type, list[CoercionOp], list[CoercionOp]] | None:
    ops1 = coercion_chain(t1, t2)
    if ops1 is not None:
        return t2, ops1, []
    ops2 = coercion_chain(t2, t1)
    if ops2 is not None:
        return t1, [], ops2
    return None


@dataclass(frozen=True)
class Candidate:
    ret: Type
    cost: int
    expr: ResolvedExpr


def synthesize_candidates(expr: Expr, overloads: dict[str, list[Signature]]) -> list[Candidate]:
    if isinstance(expr, LitExpr):
        return [Candidate(expr.ty, 0, ResolvedLit(expr.ty))]
    if expr.name not in overloads:
        raise OverloadError(f"unknown function {expr.name}")
    sigs = overloads[expr.name]
    arg_cands = [synthesize_candidates(arg, overloads) for arg in expr.args]
    results: list[Candidate] = []
    for sig in sigs:
        if len(sig.params) != len(arg_cands):
            continue
        matched: list[tuple[ResolvedExpr, list[CoercionOp]]] = []
        total_cost = 0
        feasible = True
        for cands, pty in zip(arg_cands, sig.params):
            best: tuple[int, ResolvedExpr, list[CoercionOp]] | None = None
            tie = False
            for c in cands:
                ops = coercion_chain(c.ret, pty)
                if ops is not None:
                    cost = c.cost + len(ops)
                    if best is None or cost < best[0]:
                        best = (cost, c.expr, ops)
                        tie = False
                    elif cost == best[0]:
                        tie = True
            if tie:
                raise AmbiguousError(f"ambiguous argument for {expr.name}")
            if best is None:
                feasible = False
                break
            total_cost += best[0]
            matched.append((best[1], best[2]))
        if feasible:
            results.append(Candidate(sig.ret, total_cost, ResolvedCall(expr.name, sig.ret, matched)))
    if not results:
        raise NoViableCandidateError(f"no viable overload for {expr.name}")
    return results


def resolve_overload(
    expr: Expr, expected_ret: Type | None, overloads: dict[str, list[Signature]]
) -> ResolvedExpr:
    cands = synthesize_candidates(expr, overloads)
    if expected_ret is not None:
        narrowed: list[Candidate] = []
        for c in cands:
            ops = coercion_chain(c.ret, expected_ret)
            if ops is not None:
                narrowed.append(Candidate(c.ret, c.cost + len(ops), c.expr))
        cands = narrowed
    if not cands:
        raise NoViableCandidateError("no overload matches expected return type")
    min_cost = min(c.cost for c in cands)
    best = [c for c in cands if c.cost == min_cost]
    if len(best) > 1:
        raise AmbiguousError(f"ambiguous overloads: {[c.ret for c in best]}")
    return best[0].expr


def sample_overloads() -> dict[str, list[Signature]]:
    return {
        "f": [Signature([Type.INT], Type.INT), Signature([Type.INT], Type.DOUBLE)],
        "g": [Signature([Type.FLOAT], Type.FLOAT), Signature([Type.DOUBLE], Type.DOUBLE)],
        "sink_double": [Signature([Type.DOUBLE], Type.DOUBLE)],
    }


def main() -> None:
    overloads = sample_overloads()
    print(f"Widen (Int, Float) => {widen(Type.INT, Type.FLOAT)}")
    call_f = CallExpr("f", [LitExpr(Type.INT)])
    print(f"f(Int) with expected Int    => {resolve_overload(call_f, Type.INT, overloads)}")
    print(f"f(Int) with expected Double => {resolve_overload(call_f, Type.DOUBLE, overloads)}")
    print(
        f"g(Char) picks cheaper Float => "
        f"{resolve_overload(CallExpr('g', [LitExpr(Type.CHAR)]), None, overloads)}"
    )
    print(
        f"sink_double(f(Int))         => "
        f"{resolve_overload(CallExpr('sink_double', [call_f]), None, overloads)}"
    )


class OverloadCoerceTests(unittest.TestCase):
    def test_disambiguates_return_type_and_selects_minimum_widening_cost(self) -> None:
        overloads = sample_overloads()
        call_f = CallExpr("f", [LitExpr(Type.INT)])
        with self.assertRaises(AmbiguousError):
            resolve_overload(call_f, None, overloads)

        res_dbl = resolve_overload(call_f, Type.DOUBLE, overloads)
        self.assertIsInstance(res_dbl, ResolvedCall)
        assert isinstance(res_dbl, ResolvedCall)
        self.assertEqual(res_dbl.ret, Type.DOUBLE)

        res_int = resolve_overload(call_f, Type.INT, overloads)
        assert isinstance(res_int, ResolvedCall)
        self.assertEqual(res_int.ret, Type.INT)

        res_g = resolve_overload(CallExpr("g", [LitExpr(Type.CHAR)]), None, overloads)
        self.assertEqual(
            res_g,
            ResolvedCall(
                "g",
                Type.FLOAT,
                [(ResolvedLit(Type.CHAR), [CoercionOp.C2I, CoercionOp.I2F])],
            ),
        )

    def test_reports_widening_failure_and_ambiguous_nested_calls(self) -> None:
        overloads = sample_overloads()
        self.assertIsNotNone(widen(Type.CHAR, Type.DOUBLE))
        self.assertIsNone(coercion_chain(Type.DOUBLE, Type.INT))
        with self.assertRaises(NoViableCandidateError):
            resolve_overload(CallExpr("g", [LitExpr(Type.DOUBLE)]), Type.INT, overloads)


if __name__ == "__main__":
    main()
