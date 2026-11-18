"""Resolve names in nested scopes and check the types of expressions."""

from dataclasses import dataclass
from enum import Enum
import unittest


# Static types stored in symbol-table entries and returned by the type checker.
class Type(Enum):
    INTEGER = "Integer"
    BOOLEAN = "Boolean"


# Expression AST nodes; syntax permits `Add` on any nodes, leaving type safety to `type_of`.
@dataclass(frozen=True)
class Name:
    ident: str


@dataclass(frozen=True)
class IntegerLiteral:
    pass


@dataclass(frozen=True)
class Add:
    left: "Expr"
    right: "Expr"


Expr = Name | IntegerLiteral | Add


# Distinct semantic error classes for redeclaration, missing symbol, and type mismatch.
class DuplicateError(ValueError):
    def __init__(self, name: str) -> None:
        super().__init__(f"duplicate declaration of {name}")
        self.name = name


class UnknownError(ValueError):
    def __init__(self, name: str) -> None:
        super().__init__(f"unknown name {name}")
        self.name = name


class ExpectedIntegerError(TypeError):
    def __init__(self, found: Type) -> None:
        super().__init__(f"expected Integer, found {found.value}")
        self.found = found


def declare(scope: dict[str, Type], name: str, ty: Type) -> None:
    """Bind `name` in the current scope, rejecting same-scope duplicates intact."""
    if name in scope:
        raise DuplicateError(name)
    scope[name] = ty


def type_of(expr: Expr, scopes: list[dict[str, Type]]) -> Type:
    """Resolve names from innermost to outermost scope and enforce integer addition."""
    if isinstance(expr, Name):
        # Scopes are ordered outermost-to-innermost, so reverse iteration finds the nearest binding.
        for scope in reversed(scopes):
            if expr.ident in scope:
                return scope[expr.ident]
        raise UnknownError(expr.ident)
    if isinstance(expr, IntegerLiteral):
        return Type.INTEGER
    for operand in (expr.left, expr.right):
        ty = type_of(operand, scopes)
        if ty != Type.INTEGER:
            raise ExpectedIntegerError(ty)
    return Type.INTEGER


def main() -> None:
    scopes: list[dict[str, Type]] = [{}, {}]
    declare(scopes[0], "width", Type.BOOLEAN)
    declare(scopes[1], "width", Type.INTEGER)
    expr = Add(Name("width"), IntegerLiteral())
    print(f"Inner: {type_of(expr, scopes).value}")
    scopes.pop()
    try:
        print(f"Outer: {type_of(expr, scopes).value}")
    except ExpectedIntegerError as exc:
        print(f"Outer: Err(ExpectedInteger({exc.found.value}))")


class ScopeTypesTests(unittest.TestCase):
    def test_resolves_shadowing_and_preserves_first_declaration(self) -> None:
        outer: dict[str, Type] = {}
        declare(outer, "width", Type.BOOLEAN)
        with self.assertRaises(DuplicateError):
            declare(outer, "width", Type.INTEGER)
        expr = Add(Name("width"), IntegerLiteral())
        with self.assertRaises(ExpectedIntegerError) as ctx:
            type_of(expr, [dict(outer)])
        self.assertEqual(ctx.exception.found, Type.BOOLEAN)
        self.assertEqual(type_of(expr, [outer, {"width": Type.INTEGER}]), Type.INTEGER)
        with self.assertRaises(UnknownError):
            type_of(Name("absent"), [])


if __name__ == "__main__":
    main()
