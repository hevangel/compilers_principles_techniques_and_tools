"""Unify finite type expressions by solving equations between their components."""

from dataclasses import dataclass
import unittest


# Type term AST: type variables (TVar), base types (TInt, TBool), and function arrows (TFunc).
@dataclass(frozen=True)
class TVar:
    id: int


@dataclass(frozen=True)
class TInt:
    pass


@dataclass(frozen=True)
class TBool:
    pass


@dataclass(frozen=True)
class TFunc:
    src: "Type"
    dst: "Type"


Type = TVar | TInt | TBool | TFunc


class InfiniteTypeError(ValueError):
    def __init__(self, var_id: int) -> None:
        super().__init__(f"infinite type involving t{var_id}")
        self.var_id = var_id


class ClashError(TypeError):
    def __init__(self, left: Type, right: Type) -> None:
        super().__init__(f"type clash: {left} vs {right}")
        self.left = left
        self.right = right


def resolve(ty: Type, substitutions: dict[int, Type]) -> Type:
    """Apply `substitutions` transitively to variables and recursively inside `TFunc`."""
    if isinstance(ty, TVar):
        bound = substitutions.get(ty.id)
        return ty if bound is None else resolve(bound, substitutions)
    if isinstance(ty, TFunc):
        return TFunc(resolve(ty.src, substitutions), resolve(ty.dst, substitutions))
    return ty


def occurs(var_id: int, ty: Type) -> bool:
    """Return True if `var_id` appears in `ty`, preventing cyclic infinite types."""
    if isinstance(ty, TVar):
        return ty.id == var_id
    if isinstance(ty, TFunc):
        return occurs(var_id, ty.src) or occurs(var_id, ty.dst)
    return False


def unify(equations: list[tuple[Type, Type]]) -> dict[int, Type]:
    """Solve type equality constraints via a worklist, preserving acyclic substitutions."""
    work = list(equations)
    substitutions: dict[int, Type] = {}
    while work:
        raw_left, raw_right = work.pop()
        # Resolving both sides first exposes indirect chains (e.g. t0 -> t1 -> Int) and cycles.
        left = resolve(raw_left, substitutions)
        right = resolve(raw_right, substitutions)
        if left == right:
            continue
        if isinstance(left, TVar):
            if occurs(left.id, right):
                raise InfiniteTypeError(left.id)
            substitutions[left.id] = right
        elif isinstance(right, TVar):
            if occurs(right.id, left):
                raise InfiniteTypeError(right.id)
            substitutions[right.id] = left
        elif isinstance(left, TFunc) and isinstance(right, TFunc):
            # Function types unify iff both parameter and return types unify.
            work.extend([(left.src, right.src), (left.dst, right.dst)])
        else:
            raise ClashError(left, right)
    return substitutions


def main() -> None:
    generic = TFunc(TVar(0), TVar(1))
    concrete = TFunc(TInt(), TBool())
    subs = unify([(generic, concrete)])
    print(f"Constraints solved as {resolve(generic, subs)}")


class UnificationTests(unittest.TestCase):
    def test_resolves_chained_bindings_and_rejects_clashes_and_cycles(self) -> None:
        subs = unify([(TVar(1), TInt()), (TVar(0), TVar(1))])
        self.assertEqual(resolve(TVar(0), subs), TInt())
        with self.assertRaises(ClashError) as clash:
            unify([(TInt(), TBool())])
        self.assertEqual((clash.exception.left, clash.exception.right), (TInt(), TBool()))
        with self.assertRaises(InfiniteTypeError) as cycle:
            unify([(TVar(1), TFunc(TVar(0), TInt())), (TVar(0), TVar(1))])
        self.assertEqual(cycle.exception.var_id, 1)


if __name__ == "__main__":
    main()
