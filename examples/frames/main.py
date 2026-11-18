"""Model activation records and follow static links to nonlocal data."""

from dataclasses import dataclass
import unittest


@dataclass(frozen=True)
class Frame:
    """Activation record (stack frame) for a single procedure invocation."""

    # Local variable storage for this activation (simplified to a single integer slot).
    local: int
    # Dynamic (control) link: stack index of the caller frame to return to.
    caller: int | None
    # Static (access) link: stack index of the lexically enclosing procedure's active frame.
    static_parent: int | None


def read_nonlocal(stack: list[Frame], frame: int, lexical_hops: int) -> int | None:
    """Follow `lexical_hops` static links outward from `frame` and read that scope's local."""
    current: int | None = frame
    # Follow static_parent (NOT caller) because visibility depends on source nesting, not call order.
    while lexical_hops > 0:
        if current is None or not (0 <= current < len(stack)):
            return None
        current = stack[current].static_parent
        lexical_hops -= 1
    # Zero hops left: verify we landed on a valid frame and return its local slot.
    if current is None or not (0 <= current < len(stack)):
        return None
    return stack[current].local


def sample() -> list[Frame]:
    """Build a stack snapshot where dynamic call order (0 -> 1 -> 2) differs from lexical nesting."""
    return [
        # Frame 0: `outer` at top level.
        Frame(local=21, caller=None, static_parent=None),
        # Frame 1: `helper` called by `outer` (0) and declared inside `outer` (0).
        Frame(local=90, caller=0, static_parent=0),
        # Frame 2: `inner` called by `helper` (1), yet declared inside `outer` (0).
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
