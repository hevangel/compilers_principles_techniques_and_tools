"""Evaluate inherited origins and synthesized widths on a small tree."""

from dataclasses import dataclass
import unittest


# Syntax tree nodes for 1D layout: a leaf Strip has a fixed width, while a
# Join places its right subtree immediately after its left subtree.
@dataclass(frozen=True)
class Strip:
    width: int


@dataclass(frozen=True)
class Join:
    left: "Shape"
    right: "Shape"


Shape = Strip | Join


# Synthesized attribute bundle returned bottom-up from each subtree.
@dataclass(frozen=True)
class Attributes:
    width: int
    placements: list[tuple[int, int]]


def place(shape: Shape, origin: int) -> Attributes:
    """Evaluate an L-attributed layout tree in a single left-to-right DFS pass.

    `origin` is an inherited attribute supplied by the parent or left sibling.
    """
    if isinstance(shape, Strip):
        return Attributes(width=shape.width, placements=[(origin, shape.width)])
    # Left child inherits our origin; its synthesized width determines the right origin.
    left = place(shape.left, origin)
    right = place(shape.right, origin + left.width)
    # Synthesize combined width and ordered placements back to the parent.
    return Attributes(
        width=left.width + right.width,
        placements=[*left.placements, *right.placements],
    )


def sample() -> Shape:
    """Return Join(Join(Strip(2), Strip(5)), Strip(4)) for the worked trace."""
    return Join(Join(Strip(2), Strip(5)), Strip(4))


def main() -> None:
    print(place(sample(), 10))


class AttributesTests(unittest.TestCase):
    def test_inherited_origin_uses_left_siblings_synthesized_width(self) -> None:
        self.assertEqual(
            place(sample(), 10),
            Attributes(width=11, placements=[(10, 2), (12, 5), (17, 4)]),
        )


if __name__ == "__main__":
    main()
