"""Evaluate inherited origins and synthesized widths on a small tree."""

from dataclasses import dataclass
import unittest


@dataclass(frozen=True)
class Strip:
    width: int


@dataclass(frozen=True)
class Join:
    left: "Shape"
    right: "Shape"


Shape = Strip | Join


@dataclass(frozen=True)
class Attributes:
    width: int
    placements: list[tuple[int, int]]


def place(shape: Shape, origin: int) -> Attributes:
    if isinstance(shape, Strip):
        return Attributes(width=shape.width, placements=[(origin, shape.width)])
    left = place(shape.left, origin)
    right = place(shape.right, origin + left.width)
    return Attributes(
        width=left.width + right.width,
        placements=[*left.placements, *right.placements],
    )


def sample() -> Shape:
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
