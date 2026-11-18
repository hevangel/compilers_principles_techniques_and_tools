"""Contrast O(1) lexical display lookups against static-link hops and manage stack VLAs."""

from dataclasses import dataclass
import unittest


@dataclass
class Frame:
    """Activation record storing static-link, display save/restore, and VLA layout metadata."""

    proc_name: str
    # Lexical nesting depth (0 = outermost); selects which slot in `display` this frame occupies.
    depth: int
    # Enclosing lexical frame (depth - 1) used by static-link traversal.
    static_link: int | None
    # Previous `display[depth]` saved on entry so returning restores the caller/sibling frame.
    saved_display: int | None
    # Base offset in `RuntimeMachine.stack` where fixed-size locals start.
    locals_start: int
    # Base offset of the dynamic VLA placed *after* fixed locals (or None if vla_len == 0).
    vla_ptr: int | None
    vla_len: int


class RuntimeMachine:
    """Simulated call stack comparing O(1) display lookups with O(depth) static-link walks."""

    def __init__(self, max_depth: int) -> None:
        self.stack: list[int] = []
        self.frames: list[Frame] = []
        # Lexical display: `display[d]` caches the currently visible frame ID at depth `d`.
        self.display: list[int | None] = [None] * (max_depth + 1)
        self.static_hops = 0
        self.display_hops = 0

    def push_frame(
        self, proc_name: str, depth: int, fixed_locals: list[int], vla_len: int
    ) -> int:
        # Static parent is the active frame one lexical level outward.
        static_link = None if depth == 0 else self.display[depth - 1]
        # Snapshot old `display[depth]` so recursive or same-depth sibling calls don't lose it.
        saved_display = self.display[depth]
        # Fixed locals come first so their offsets from `locals_start` stay constant at compile time.
        locals_start = len(self.stack)
        self.stack.extend(fixed_locals)
        # Dynamic VLA sits after fixed locals and is accessed indirectly via `vla_ptr`.
        vla_ptr = None
        if vla_len > 0:
            vla_ptr = len(self.stack)
            self.stack.extend([0] * vla_len)
        frame_id = len(self.frames)
        self.frames.append(
            Frame(
                proc_name,
                depth,
                static_link,
                saved_display,
                locals_start,
                vla_ptr,
                vla_len,
            )
        )
        # Invariant: `display[depth]` always points to the innermost active frame at `depth`.
        self.display[depth] = frame_id
        return frame_id

    def pop_frame(self) -> None:
        """Restore `display[depth]` and reclaim both fixed locals and VLA space in one step."""
        frame = self.frames.pop()
        self.display[frame.depth] = frame.saved_display
        del self.stack[frame.locals_start :]

    def read_via_display(self, target_depth: int, local_offset: int) -> int:
        """O(1) nonlocal access: index `display[target_depth]` directly (always 1 hop)."""
        self.display_hops += 1
        frame_id = self.display[target_depth]
        assert frame_id is not None
        return self.stack[self.frames[frame_id].locals_start + local_offset]

    def read_via_static_link(self, target_depth: int, local_offset: int) -> int:
        """O(depth) nonlocal access: walk `static_link` pointers up to `target_depth`."""
        cur = len(self.frames) - 1
        while self.frames[cur].depth > target_depth:
            self.static_hops += 1
            nxt = self.frames[cur].static_link
            assert nxt is not None
            cur = nxt
        # Count the final dereference into the target frame's local slot.
        self.static_hops += 1
        return self.stack[self.frames[cur].locals_start + local_offset]

    def write_vla(self, frame_id: int, index: int, value: int) -> None:
        ptr = self.frames[frame_id].vla_ptr
        assert ptr is not None and 0 <= index < self.frames[frame_id].vla_len
        self.stack[ptr + index] = value

    def read_vla(self, frame_id: int, index: int) -> int:
        ptr = self.frames[frame_id].vla_ptr
        assert ptr is not None and 0 <= index < self.frames[frame_id].vla_len
        return self.stack[ptr + index]


def simulate_mutual_recursion(base_val: int, scale: int, n: int) -> tuple[int, int, int, int]:
    rt = RuntimeMachine(2)
    rt.push_frame("outer", 0, [base_val], 0)
    mid_frame = rt.push_frame("mid", 1, [scale], max(n, 1))
    for i in range(max(n, 1)):
        rt.write_vla(mid_frame, i, i + 1)

    def ping(k: int) -> int:
        f = rt.push_frame("ping", 2, [k], max(k, 1))
        rt.write_vla(f, 0, 10)
        from_disp = rt.read_via_display(0, 0) + rt.read_via_display(1, 0)
        from_sl = rt.read_via_static_link(0, 0) + rt.read_via_static_link(1, 0)
        assert from_disp == from_sl
        local_bonus = rt.read_vla(f, 0)
        mid_slot = rt.read_vla(mid_frame, k % rt.frames[mid_frame].vla_len)
        res = (
            from_disp + local_bonus + mid_slot
            if k == 0
            else from_disp + mid_slot + pong(k - 1)
        )
        rt.pop_frame()
        return res

    def pong(k: int) -> int:
        f = rt.push_frame("pong", 2, [k], 1)
        rt.write_vla(f, 0, 5)
        from_disp = rt.read_via_display(0, 0)
        from_sl = rt.read_via_static_link(0, 0)
        assert from_disp == from_sl
        res = (
            from_disp + rt.read_vla(f, 0)
            if k == 0
            else from_disp + rt.read_vla(f, 0) + ping(k - 1)
        )
        rt.pop_frame()
        return res

    total = ping(n)
    rt.pop_frame()
    rt.pop_frame()
    return total, len(rt.stack), rt.display_hops, rt.static_hops


def main() -> None:
    total, stack_left, disp_hops, sl_hops = simulate_mutual_recursion(10, 3, 4)
    print(f"Mutual recursion result = {total}, stack after return = {stack_left}")
    print(f"Lookup hops: display = {disp_hops} (1/lookup), static-link = {sl_hops}")


class DisplayVlaTests(unittest.TestCase):
    def test_display_matches_static_links_and_restores_vlas(self) -> None:
        total, stack_left, disp_hops, sl_hops = simulate_mutual_recursion(7, 2, 5)
        self.assertEqual(stack_left, 0)
        self.assertGreater(sl_hops, disp_hops)
        self.assertGreater(total, 0)

    def test_display_restores_previous_depth_frame_after_same_depth_call(self) -> None:
        rt = RuntimeMachine(2)
        rt.push_frame("outer", 0, [42], 0)
        mid_a = rt.push_frame("mid_a", 1, [100], 2)
        rt.write_vla(mid_a, 1, 999)
        rt.push_frame("mid_b", 1, [200], 3)
        self.assertEqual(rt.read_via_display(1, 0), 200)
        self.assertEqual(rt.read_via_static_link(0, 0), 42)
        rt.pop_frame()
        self.assertEqual(rt.read_via_display(1, 0), 100)
        self.assertEqual(rt.read_vla(mid_a, 1), 999)
        rt.pop_frame()
        rt.pop_frame()
        self.assertEqual(rt.stack, [])
        self.assertEqual(rt.display, [None, None, None])


if __name__ == "__main__":
    main()
