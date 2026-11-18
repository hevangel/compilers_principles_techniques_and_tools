//! Cheney breadth-first semispace copying GC and 3-pass sliding Mark-Compact GC.

use std::collections::BTreeMap;

/// Heap object storing outgoing references plus GC metadata (`marked`, `forwarded`).
#[derive(Clone, Debug, PartialEq, Eq)]
struct HeapObj {
    tag: String,
    /// Slot indices pointing to child objects (`None` = null field).
    fields: Vec<Option<usize>>,
    /// Reachability flag used by Mark-Compact Pass 1.
    marked: bool,
    /// Forwarding pointer: new slot index in `to_space` (Cheney) or compacted prefix (Mark-Compact).
    forwarded: Option<usize>,
}

/// Two equal-sized semispaces: mutator allocates in `from_space`; GC evacuates live data to `to_space`.
#[derive(Clone, Debug)]
struct SemispaceHeap {
    from_space: Vec<Option<HeapObj>>,
    to_space: Vec<Option<HeapObj>>,
}

impl SemispaceHeap {
    fn new(capacity: usize) -> Self {
        Self {
            from_space: vec![None; capacity],
            to_space: vec![None; capacity],
        }
    }

    fn alloc_in_from(&mut self, tag: &str, fields: Vec<Option<usize>>) -> usize {
        for (i, slot) in self.from_space.iter_mut().enumerate() {
            if slot.is_none() {
                *slot = Some(HeapObj {
                    tag: tag.to_string(),
                    fields,
                    marked: false,
                    forwarded: None,
                });
                return i;
            }
        }
        panic!("from_space exhausted");
    }

    /// Cheney breadth-first copying using `scan` and `free` cursors over `to_space`.
    /// Invariant throughout the loop:
    /// - `0..scan`: copied objects whose child pointers already point into `to_space`.
    /// - `scan..free`: copied objects whose child pointers still point into `from_space` (the BFS queue!).
    /// - `free..`: empty space ready for bump allocation.
    fn cheney_collect(&mut self, roots: &mut [usize]) -> (usize, usize) {
        self.to_space.fill(None);
        let mut free = 0usize;
        let mut scan = 0usize;
        let mut copy_count = 0usize;

        // Evacuate `old_idx` from `from_space` to `to_space` if not yet forwarded.
        let copy_ref = |old_idx: usize,
                        from_space: &mut [Option<HeapObj>],
                        to_space: &mut [Option<HeapObj>],
                        free: &mut usize,
                        copy_count: &mut usize|
         -> usize {
            let slot = from_space[old_idx].as_mut().expect("dangling heap ptr");
            // If already evacuated (shared DAG node or back-edge in a cycle), reuse its forwarding address.
            if let Some(fwd) = slot.forwarded {
                return fwd;
            }
            let new_idx = *free;
            *free += 1;
            *copy_count += 1;
            // Record forwarding address in `from_space` *before* scanning children so cycles terminate.
            slot.forwarded = Some(new_idx);
            let mut cloned = slot.clone();
            cloned.forwarded = None;
            cloned.marked = false;
            to_space[new_idx] = Some(cloned);
            new_idx
        };

        // Step 1: evacuate all directly rooted objects and rewrite the root pointers.
        for r in roots.iter_mut() {
            *r = copy_ref(
                *r,
                &mut self.from_space,
                &mut self.to_space,
                &mut free,
                &mut copy_count,
            );
        }

        // Step 2: advance `scan` across `to_space` until it catches `free`, evacuating each child.
        while scan < free {
            let num_fields = self.to_space[scan].as_ref().unwrap().fields.len();
            for fi in 0..num_fields {
                if let Some(old_ptr) = self.to_space[scan].as_ref().unwrap().fields[fi] {
                    let new_ptr = copy_ref(
                        old_ptr,
                        &mut self.from_space,
                        &mut self.to_space,
                        &mut free,
                        &mut copy_count,
                    );
                    self.to_space[scan].as_mut().unwrap().fields[fi] = Some(new_ptr);
                }
            }
            scan += 1;
        }

        // Step 3: flip semispaces so the newly compacted `to_space` becomes the active `from_space`.
        std::mem::swap(&mut self.from_space, &mut self.to_space);
        self.to_space.fill(None);
        (free, copy_count)
    }
}

/// Single-space heap compacted in place via 3-pass sliding Mark-Compact collection.
#[derive(Clone, Debug)]
struct SlidingHeap {
    slots: Vec<Option<HeapObj>>,
}

impl SlidingHeap {
    /// 3-pass sliding Mark-Compact collector:
    /// 1. Mark reachable objects from roots.
    /// 2. Compute dense contiguous addresses in scan order (`forwarded`), preserving allocation order.
    /// 3. Rewrite all pointers (roots + fields) using `forwarded`, then slide objects into place.
    fn mark_compact(&mut self, roots: &mut [usize]) -> usize {
        for obj in self.slots.iter_mut().flatten() {
            obj.marked = false;
            obj.forwarded = None;
        }

        // Pass 1: Mark reachable subgraph from roots using an explicit stack.
        let mut stack: Vec<usize> = roots.to_vec();
        while let Some(idx) = stack.pop() {
            if let Some(obj) = self.slots[idx].as_mut() {
                if !obj.marked {
                    obj.marked = true;
                    for child in obj.fields.iter().flatten() {
                        stack.push(*child);
                    }
                }
            }
        }

        // Pass 2: Assign each marked object its final compacted slot index without moving anything yet.
        let mut free = 0usize;
        for slot in &mut self.slots {
            if let Some(obj) = slot.as_mut() {
                if obj.marked {
                    obj.forwarded = Some(free);
                    free += 1;
                }
            }
        }

        // Pass 3: Update all pointers *before* overwriting old slots, then slide live objects down.
        let new_addr = |idx: usize, slots: &[Option<HeapObj>]| -> usize {
            slots[idx]
                .as_ref()
                .and_then(|o| o.forwarded)
                .expect("unmarked live ref")
        };

        for r in roots.iter_mut() {
            *r = new_addr(*r, &self.slots);
        }

        // Build relocated copies with updated field pointers while old `forwarded` metadata is still intact.
        let mut updates: Vec<(usize, usize, HeapObj)> = Vec::new();
        for (old_idx, slot) in self.slots.iter().enumerate() {
            if let Some(obj) = slot {
                if let Some(dst) = obj.forwarded {
                    let mut updated = obj.clone();
                    updated.marked = false;
                    updated.forwarded = None;
                    updated.fields = obj
                        .fields
                        .iter()
                        .map(|opt| opt.map(|p| new_addr(p, &self.slots)))
                        .collect();
                    updates.push((old_idx, dst, updated));
                }
            }
        }

        // Slide live objects into contiguous slots `0..free`, dropping all unmarked garbage.
        self.slots.fill(None);
        for (_, dst, obj) in updates {
            self.slots[dst] = Some(obj);
        }
        free
    }
}

/// Canonicalize reachable graph (tags + deterministic BFS structure) to verify graph isomorphism
/// regardless of which physical slot indices the collector chose.
fn canonical_graph(
    slots: &[Option<HeapObj>],
    roots: &[usize],
) -> Vec<(String, Vec<Option<usize>>)> {
    let mut visited: BTreeMap<usize, usize> = BTreeMap::new();
    let mut queue: Vec<usize> = Vec::new();
    for &r in roots {
        if !visited.contains_key(&r) {
            let id = visited.len();
            visited.insert(r, id);
            queue.push(r);
        }
    }
    let mut head = 0usize;
    let mut result = Vec::new();
    while head < queue.len() {
        let cur = queue[head];
        head += 1;
        let obj = slots[cur].as_ref().unwrap();
        let mut children = Vec::with_capacity(obj.fields.len());
        for f in &obj.fields {
            if let Some(c) = f {
                let next_id = if let Some(&id) = visited.get(c) {
                    id
                } else {
                    let id = visited.len();
                    visited.insert(*c, id);
                    queue.push(*c);
                    id
                };
                children.push(Some(next_id));
            } else {
                children.push(None);
            }
        }
        result.push((obj.tag.clone(), children));
    }
    result
}

fn build_diamond_and_cycle_sample() -> (SemispaceHeap, SlidingHeap, Vec<usize>) {
    let mut semi = SemispaceHeap::new(10);
    // Slot 0: dead; 1: dead; 2: shared leaf; 3,4: diamond parents; 5<->6: live cycle; 7<->8: dead cycle.
    semi.alloc_in_from("dead_a", vec![None]);
    semi.alloc_in_from("dead_b", vec![None]);
    let shared = semi.alloc_in_from("shared", vec![None]);
    let d_left = semi.alloc_in_from("d_left", vec![Some(shared)]);
    let d_right = semi.alloc_in_from("d_right", vec![Some(shared)]);
    let root_diamond = semi.alloc_in_from("diamond", vec![Some(d_left), Some(d_right)]);
    let cyc_a = semi.alloc_in_from("cyc_a", vec![None]);
    let cyc_b = semi.alloc_in_from("cyc_b", vec![Some(cyc_a)]);
    semi.from_space[cyc_a].as_mut().unwrap().fields[0] = Some(cyc_b);
    let dead_x = semi.alloc_in_from("dead_x", vec![None]);
    let dead_y = semi.alloc_in_from("dead_y", vec![Some(dead_x)]);
    semi.from_space[dead_x].as_mut().unwrap().fields[0] = Some(dead_y);

    let sliding = SlidingHeap {
        slots: semi.from_space.clone(),
    };
    (semi, sliding, vec![root_diamond, cyc_a])
}

fn main() {
    let (mut semi, mut sliding, mut roots_semi) = build_diamond_and_cycle_sample();
    let mut roots_slide = roots_semi.clone();
    let before = canonical_graph(&semi.from_space, &roots_semi);

    let (live_semi, copies) = semi.cheney_collect(&mut roots_semi);
    let after_semi = canonical_graph(&semi.from_space, &roots_semi);
    let live_slide = sliding.mark_compact(&mut roots_slide);
    let after_slide = canonical_graph(&sliding.slots, &roots_slide);

    assert_eq!(before, after_semi);
    assert_eq!(before, after_slide);
    println!("Cheney live={live_semi}, objects copied once={copies}, roots={roots_semi:?}");
    println!("Mark-Compact live={live_slide}, roots={roots_slide:?}");
}

#[test]
fn cheney_copies_shared_diamond_once_and_handles_cycles() {
    let (mut semi, _, mut roots) = build_diamond_and_cycle_sample();
    let before = canonical_graph(&semi.from_space, &roots);
    let (live, copies) = semi.cheney_collect(&mut roots);
    assert_eq!(live, 6);
    assert_eq!(copies, 6); // shared leaf copied strictly once
    let after = canonical_graph(&semi.from_space, &roots);
    assert_eq!(before, after);
    // Live objects are packed contiguously at slots 0..6.
    assert!(semi.from_space[..live].iter().all(|s| s.is_some()));
    assert!(semi.from_space[live..].iter().all(|s| s.is_none()));
    // Shared leaf pointer from both diamond branches points to the exact same slot.
    let d = semi.from_space[roots[0]].as_ref().unwrap();
    let left_child = semi.from_space[d.fields[0].unwrap()]
        .as_ref()
        .unwrap()
        .fields[0];
    let right_child = semi.from_space[d.fields[1].unwrap()]
        .as_ref()
        .unwrap()
        .fields[0];
    assert_eq!(left_child, right_child);
}

#[test]
fn mark_compact_slides_live_objects_preserving_graph_isomorphism() {
    let (_, mut sliding, mut roots) = build_diamond_and_cycle_sample();
    let before = canonical_graph(&sliding.slots, &roots);
    let live = sliding.mark_compact(&mut roots);
    assert_eq!(live, 6);
    assert_eq!(before, canonical_graph(&sliding.slots, &roots));
    assert!(sliding.slots[..live].iter().all(|s| s.is_some()));
    assert!(sliding.slots[live..].iter().all(|s| s.is_none()));
}
