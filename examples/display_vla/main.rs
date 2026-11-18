//! Contrast O(1) lexical display lookups against static-link hops and manage stack VLAs.

/// Activation record combining static links, display save/restore state, and VLA metadata.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Frame {
    proc_name: String,
    /// Lexical nesting depth (0 = outermost). Indexes directly into `RuntimeMachine::display`.
    depth: usize,
    /// Enclosing lexical frame (depth - 1) for traditional pointer-chasing lookups.
    static_link: Option<usize>,
    /// Previous value of `display[depth]` before this call overwrote it; restored on return.
    saved_display: Option<usize>,
    /// Starting index in `RuntimeMachine::stack`; fixed-size locals occupy `locals_start..`.
    locals_start: usize,
    /// Starting index of this frame's variable-length array, placed *after* fixed locals.
    vla_ptr: Option<usize>,
    /// Runtime-determined length of the VLA (0 if none).
    vla_len: usize,
}

/// Abstract call-stack machine tracking both a lexical display and static-link hop counts.
#[derive(Clone, Debug)]
struct RuntimeMachine {
    /// Contiguous value stack holding fixed locals followed by VLA elements for each active frame.
    stack: Vec<i64>,
    /// Stack of active frame descriptors; frame IDs are indices into this vector.
    frames: Vec<Frame>,
    /// Lexical display: `display[d]` caches the active frame ID at lexical depth `d` for O(1) access.
    display: Vec<Option<usize>>,
    /// Cumulative dereferences performed by `read_via_static_link` (for comparison).
    static_hops: usize,
    /// Cumulative lookups performed by `read_via_display` (always 1 per read).
    display_hops: usize,
}

impl RuntimeMachine {
    fn new(max_depth: usize) -> Self {
        Self {
            stack: Vec::new(),
            frames: Vec::new(),
            display: vec![None; max_depth + 1],
            static_hops: 0,
            display_hops: 0,
        }
    }

    /// Push a new frame at lexical `depth`, allocate fixed locals followed by a VLA, and update display.
    fn push_frame(
        &mut self,
        proc_name: &str,
        depth: usize,
        fixed_locals: &[i64],
        vla_len: usize,
    ) -> usize {
        // The static parent is whichever frame is currently active one lexical level outward.
        let static_link = if depth == 0 {
            None
        } else {
            self.display[depth - 1]
        };
        // Save the prior frame at `depth` (e.g., an earlier recursive call or sibling) before clobbering.
        let saved_display = self.display[depth];

        // Allocate fixed-size locals first so their offsets from `locals_start` are compile-time constants.
        let locals_start = self.stack.len();
        self.stack.extend_from_slice(fixed_locals);

        // Allocate runtime-sized VLA *after* fixed locals so it never shifts fixed local offsets.
        let vla_ptr = if vla_len > 0 {
            let ptr = self.stack.len();
            self.stack.resize(ptr + vla_len, 0);
            Some(ptr)
        } else {
            None
        };

        let frame_id = self.frames.len();
        self.frames.push(Frame {
            proc_name: proc_name.to_string(),
            depth,
            static_link,
            saved_display,
            locals_start,
            vla_ptr,
            vla_len,
        });
        // Invariant: `display[depth]` always points to the innermost active frame at `depth`.
        self.display[depth] = Some(frame_id);
        frame_id
    }

    /// Pop the top frame, restoring `display[depth]` and reclaiming both fixed locals and VLA storage.
    fn pop_frame(&mut self) {
        let frame = self.frames.pop().expect("empty call stack");
        self.display[frame.depth] = frame.saved_display;
        self.stack.truncate(frame.locals_start);
    }

    /// O(1) nonlocal read: index `display[target_depth]` directly without walking static links.
    fn read_via_display(&mut self, target_depth: usize, local_offset: usize) -> i64 {
        self.display_hops += 1;
        let frame_id = self.display[target_depth].expect("display entry missing");
        let addr = self.frames[frame_id].locals_start + local_offset;
        self.stack[addr]
    }

    /// O(depth) nonlocal read: follow `static_link` pointers from the current frame up to `target_depth`.
    fn read_via_static_link(&mut self, target_depth: usize, local_offset: usize) -> i64 {
        let mut cur = self.frames.len() - 1;
        while self.frames[cur].depth > target_depth {
            self.static_hops += 1;
            cur = self.frames[cur]
                .static_link
                .expect("broken static link chain");
        }
        // Final frame dereference to read the slot once the target depth is reached.
        self.static_hops += 1;
        let addr = self.frames[cur].locals_start + local_offset;
        self.stack[addr]
    }

    /// Write an element in `frame_id`'s stack-allocated VLA via its fixed-offset pointer.
    fn write_vla(&mut self, frame_id: usize, index: usize, value: i64) {
        let ptr = self.frames[frame_id].vla_ptr.expect("frame has no VLA");
        assert!(index < self.frames[frame_id].vla_len, "VLA out of bounds");
        self.stack[ptr + index] = value;
    }

    /// Read an element from `frame_id`'s stack-allocated VLA via its fixed-offset pointer.
    fn read_vla(&self, frame_id: usize, index: usize) -> i64 {
        let ptr = self.frames[frame_id].vla_ptr.expect("frame has no VLA");
        assert!(index < self.frames[frame_id].vla_len, "VLA out of bounds");
        self.stack[ptr + index]
    }
}

// Nested recursive procedure simulation:
// depth 0: outer(base_val)
//   depth 1: mid(scale, vla[n])
//     depth 2: ping(n) <-> pong(n) (mutually recursive, reading outer[0] & mid[0], using local VLA)
fn simulate_mutual_recursion(base_val: i64, scale: i64, n: usize) -> (i64, usize, usize, usize) {
    let mut rt = RuntimeMachine::new(2);
    let _outer = rt.push_frame("outer", 0, &[base_val], 0);
    let mid_frame = rt.push_frame("mid", 1, &[scale], n.max(1));
    for i in 0..n.max(1) {
        rt.write_vla(mid_frame, i, (i as i64) + 1);
    }

    fn ping(rt: &mut RuntimeMachine, k: usize, mid_frame: usize) -> i64 {
        let f = rt.push_frame("ping", 2, &[k as i64], k.max(1));
        rt.write_vla(f, 0, 10);
        let from_disp = rt.read_via_display(0, 0) + rt.read_via_display(1, 0);
        let from_sl = rt.read_via_static_link(0, 0) + rt.read_via_static_link(1, 0);
        assert_eq!(from_disp, from_sl);
        let local_bonus = rt.read_vla(f, 0);
        let mid_slot = rt.read_vla(mid_frame, k % rt.frames[mid_frame].vla_len);
        let res = if k == 0 {
            from_disp + local_bonus + mid_slot
        } else {
            from_disp + mid_slot + pong(rt, k - 1, mid_frame)
        };
        rt.pop_frame();
        res
    }

    fn pong(rt: &mut RuntimeMachine, k: usize, mid_frame: usize) -> i64 {
        let f = rt.push_frame("pong", 2, &[k as i64], 1);
        rt.write_vla(f, 0, 5);
        let from_disp = rt.read_via_display(0, 0);
        let from_sl = rt.read_via_static_link(0, 0);
        assert_eq!(from_disp, from_sl);
        let res = if k == 0 {
            from_disp + rt.read_vla(f, 0)
        } else {
            from_disp + rt.read_vla(f, 0) + ping(rt, k - 1, mid_frame)
        };
        rt.pop_frame();
        res
    }

    let total = ping(&mut rt, n, mid_frame);
    let mid_name = rt.frames[mid_frame].proc_name.clone();
    let vla_sum: i64 = (0..rt.frames[mid_frame].vla_len)
        .map(|i| rt.read_vla(mid_frame, i))
        .sum();
    assert_eq!(mid_name, "mid");
    assert!(vla_sum > 0);
    rt.pop_frame();
    rt.pop_frame();
    (total, rt.stack.len(), rt.display_hops, rt.static_hops)
}

fn main() {
    let (total, stack_left, disp_hops, sl_hops) = simulate_mutual_recursion(10, 3, 4);
    println!("Mutual recursion result = {total}, stack after return = {stack_left}");
    println!("Lookup hops: display = {disp_hops} (1/lookup), static-link = {sl_hops}");
}

#[test]
fn display_matches_static_links_with_constant_hops_and_restores_vlas() {
    let (total, stack_left, disp_hops, sl_hops) = simulate_mutual_recursion(7, 2, 5);
    assert_eq!(stack_left, 0);
    // Display reads 2 lookups in each ping (depth 0 & 1) + 1 lookup in each pong (depth 0),
    // always taking exactly 1 hop per lookup, whereas static links walk 2-3 frames per lookup.
    assert!(sl_hops > disp_hops);
    assert!(total > 0);
}

#[test]
fn display_restores_previous_depth_frame_after_nested_call_at_same_depth() {
    let mut rt = RuntimeMachine::new(2);
    rt.push_frame("outer", 0, &[42], 0);
    let mid_a = rt.push_frame("mid_a", 1, &[100], 2);
    rt.write_vla(mid_a, 1, 999);
    let _mid_b = rt.push_frame("mid_b", 1, &[200], 3);
    assert_eq!(rt.read_via_display(1, 0), 200);
    assert_eq!(rt.read_via_static_link(0, 0), 42);
    rt.pop_frame(); // pops mid_b and restores display[1] -> mid_a
    assert_eq!(rt.read_via_display(1, 0), 100);
    assert_eq!(rt.read_vla(mid_a, 1), 999);
    rt.pop_frame();
    rt.pop_frame();
    assert!(rt.stack.is_empty());
    assert_eq!(rt.display, vec![None, None, None]);
}
