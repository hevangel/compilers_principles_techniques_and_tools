"""Model activation records and follow static links to nonlocal data."""

from dataclasses import dataclass
import unittest


@dataclass(frozen=True)
class Frame:
    local: int
    caller: int | None
    static_parent: int | None


def read_nonlocal(stack: list[Frame], frame: int, lexical_hops: int) -> int | None:
    current: int | None = frame
    while lexical_hops > 0:
        if current is None or not (0 <= current < len(stack)):
            return None
        current = stack[current].static_parent
        lexical_hops -= 1
    if current is None or not (0 <= current < len(stack)):
        return None
    return stack[current].local


def sample() -> list[Frame]:
    return [
        Frame(local=21, caller=None, static_parent=None),
        Frame(local=90, caller=0, static_parent=0),
        Frame(local=7, caller=1, static_parent=0),
    ]


def main() -> None:
    stack = sample()
    print(f"Return to frame {stack[2].caller}")
    print(f"Read outer local {read_nonlocal(stack, 2, 1)}")


class FramesTests(unittest.TestCase):
    def test_uses_lexical_parent_instead_of_immediate_caller(self) -> None:
        stack = sample()
        self.assertEqual(stack[2].caller, 1)
        self.assertEqual(read_nonlocal(stack, 2, 0), 7)
        self.assertEqual(read_nonlocal(stack, 2, 1), 21)
        self.assertIsNone(read_nonlocal(stack, 2, 2))


if __name__ == "__main__":
    main()
