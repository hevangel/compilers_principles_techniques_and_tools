//! Contrast O(1) lexical display lookups against static-link hops and manage stack VLAs.

#[derive(Clone, Debug, PartialEq, Eq)]
struct Frame {
    proc_name: String,
    depth: usize,
    static_link: Option<usize>,
    saved_display: Option<usize>,
    locals_start: usize,
    vla_ptr: Option<usize>,
    vla_len: usize,
}

#[derive(Clone, Debug)]
struct RuntimeMachine {
    stack: Vec<i64>,
    frames: Vec<Frame>,
    display: Vec<Option<usize>>,
    static_hops: usize,
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

    fn push_frame(
        &mut self,
        proc_name: &str,
        depth: usize,
        fixed_locals: &[i64],
        vla_len: usize,
    ) -> usize {
        let static_link = if depth == 0 {
            None
        } else {
            self.display[depth - 1]
        };
        let saved_display = self.display[depth];
        let locals_start = self.stack.len();
        self.stack.extend_from_slice(fixed_locals);
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
        self.display[depth] = Some(frame_id);
        frame_id
    }

    fn pop_frame(&mut self) {
        let frame = self.frames.pop().expect("empty call stack");
        self.display[frame.depth] = frame.saved_display;
        self.stack.truncate(frame.locals_start);
    }

    fn read_via_display(&mut self, target_depth: usize, local_offset: usize) -> i64 {
        self.display_hops += 1;
        let frame_id = self.display[target_depth].expect("display entry missing");
        let addr = self.frames[frame_id].locals_start + local_offset;
        self.stack[addr]
    }

    fn read_via_static_link(&mut self, target_depth: usize, local_offset: usize) -> i64 {
        let mut cur = self.frames.len() - 1;
        while self.frames[cur].depth > target_depth {
            self.static_hops += 1;
            cur = self.frames[cur]
                .static_link
                .expect("broken static link chain");
        }
        // Final frame dereference.
        self.static_hops += 1;
        let addr = self.frames[cur].locals_start + local_offset;
        self.stack[addr]
    }

    fn write_vla(&mut self, frame_id: usize, index: usize, value: i64) {
        let ptr = self.frames[frame_id].vla_ptr.expect("frame has no VLA");
        assert!(index < self.frames[frame_id].vla_len, "VLA out of bounds");
        self.stack[ptr + index] = value;
    }

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
