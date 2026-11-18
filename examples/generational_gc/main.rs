//! Generational GC write barriers, incremental tri-color invariants, and reference-counting cycles.

use std::collections::{BTreeMap, BTreeSet};

/// Generation tag separating the frequently collected nursery (`Young`) from tenured storage (`Old`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Gen {
    Young,
    Old,
}

/// Stable object handle used across collections in this simulated heap.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct ObjId(usize);

/// Heap object with its current generation, survival counter (`age`), and pointer slots.
#[derive(Clone, Debug)]
struct GenObj {
    tag: String,
    gen: Gen,
    /// Number of minor collections survived so far; promoted to `Old` at `age_threshold`.
    age: usize,
    fields: Vec<Option<ObjId>>,
}

/// Two-generation heap tracking `Old -> Young` cross-generational pointers in `remembered_set`.
struct GenerationalHeap {
    objects: BTreeMap<ObjId, GenObj>,
    /// Set of `Old` object IDs that may point to `Young` objects (populated by the write barrier).
    remembered_set: BTreeSet<ObjId>,
    next_id: usize,
    age_threshold: usize,
    /// Toggle to demonstrate what breaks if `write_field` omits the remembered-set barrier.
    use_write_barrier: bool,
}

impl GenerationalHeap {
    fn new(age_threshold: usize, use_write_barrier: bool) -> Self {
        Self {
            objects: BTreeMap::new(),
            remembered_set: BTreeSet::new(),
            next_id: 1,
            age_threshold,
            use_write_barrier,
        }
    }

    fn alloc(&mut self, tag: &str, gen: Gen, arity: usize) -> ObjId {
        let id = ObjId(self.next_id);
        self.next_id += 1;
        self.objects.insert(
            id,
            GenObj {
                tag: tag.to_string(),
                gen,
                age: 0,
                fields: vec![None; arity],
            },
        );
        id
    }

    /// Mutator pointer store `owner.fields[slot] = target`, guarded by a generational write barrier.
    /// Invariant: every `Old -> Young` pointer must have `owner` recorded in `remembered_set`
    /// so minor GC can treat those `Young` targets as roots without scanning all of `Gen1`.
    fn write_field(&mut self, owner: ObjId, slot: usize, target: Option<ObjId>) {
        self.objects.get_mut(&owner).unwrap().fields[slot] = target;
        if self.use_write_barrier && self.objects[&owner].gen == Gen::Old {
            if let Some(t) = target {
                if self.objects[&t].gen == Gen::Young {
                    self.remembered_set.insert(owner);
                }
            }
        }
    }

    /// Minor GC collects ONLY `Young` objects; `Old` objects are never reclaimed or scanned
    /// unless listed in `remembered_set`. Returns `(swept_young, promoted_to_old)`.
    fn minor_collect(&mut self, roots: &[ObjId]) -> (usize, usize) {
        // Seed the worklist from (1) external `Young` roots and (2) `Young` children of `remembered_set`.
        let mut worklist: Vec<ObjId> = Vec::new();
        for &r in roots {
            if self.objects.get(&r).is_some_and(|o| o.gen == Gen::Young) {
                worklist.push(r);
            }
        }
        for &rem in &self.remembered_set {
            if let Some(old_obj) = self.objects.get(&rem) {
                for child in old_obj.fields.iter().flatten() {
                    if self.objects.get(child).is_some_and(|o| o.gen == Gen::Young) {
                        worklist.push(*child);
                    }
                }
            }
        }

        // Trace strictly within the `Young` generation.
        let mut live_young = BTreeSet::new();
        while let Some(id) = worklist.pop() {
            if !live_young.insert(id) {
                continue;
            }
            if let Some(obj) = self.objects.get(&id) {
                for child in obj.fields.iter().flatten() {
                    if self.objects.get(child).is_some_and(|o| o.gen == Gen::Young) {
                        worklist.push(*child);
                    }
                }
            }
        }

        // Sweep: keep all `Old` objects (even unreferenced ones) plus reachable `Young` objects.
        let young_before = self
            .objects
            .values()
            .filter(|o| o.gen == Gen::Young)
            .count();
        self.objects
            .retain(|id, o| o.gen == Gen::Old || live_young.contains(id));

        // Aging & tenuring: promote survivors whose age reaches `age_threshold`.
        let mut promoted = 0usize;
        for id in live_young {
            if let Some(obj) = self.objects.get_mut(&id) {
                obj.age += 1;
                if obj.age >= self.age_threshold {
                    obj.gen = Gen::Old;
                    promoted += 1;
                }
            }
        }

        // Refresh `remembered_set` so newly promoted objects with `Young` children stay tracked,
        // and `Old` objects whose `Young` children were promoted/overwritten drop out.
        let mut new_rem = BTreeSet::new();
        for (&id, obj) in &self.objects {
            if obj.gen == Gen::Old
                && obj
                    .fields
                    .iter()
                    .flatten()
                    .any(|c| self.objects.get(c).is_some_and(|co| co.gen == Gen::Young))
            {
                new_rem.insert(id);
            }
        }
        if self.use_write_barrier {
            self.remembered_set = new_rem;
        } else {
            self.remembered_set.clear();
        }
        let swept = young_before
            - self
                .objects
                .values()
                .filter(|o| o.gen == Gen::Young)
                .count()
            - promoted;
        (swept, promoted)
    }
}

/// Tri-color abstraction for incremental/concurrent marking:
/// - `White`: unvisited (candidate garbage at end of marking)
/// - `Gray`: discovered, outgoing edges not yet scanned
/// - `Black`: fully scanned; must never point directly to a `White` node (strong invariant).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Color {
    White,
    Gray,
    Black,
}

#[derive(Clone, Debug)]
struct TriColorHeap {
    colors: BTreeMap<usize, Color>,
    edges: BTreeMap<usize, Vec<usize>>,
}

impl TriColorHeap {
    fn new(nodes: &[usize], edges: &[(usize, usize)]) -> Self {
        let mut colors = BTreeMap::new();
        let mut adj: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
        for &n in nodes {
            colors.insert(n, Color::White);
            adj.insert(n, Vec::new());
        }
        for &(u, v) in edges {
            adj.entry(u).or_default().push(v);
        }
        Self { colors, edges: adj }
    }

    /// Perform one incremental marking step: pop a `Gray` node, gray its `White` children, turn it `Black`.
    fn step_mark(&mut self) -> bool {
        let Some(&gray_node) = self
            .colors
            .iter()
            .find_map(|(k, &c)| (c == Color::Gray).then_some(k))
        else {
            return false;
        };
        let neighbors = self.edges.get(&gray_node).cloned().unwrap_or_default();
        for n in neighbors {
            if self.colors[&n] == Color::White {
                self.colors.insert(n, Color::Gray);
            }
        }
        self.colors.insert(gray_node, Color::Black);
        true
    }

    /// Dijkstra-style incremental insertion write barrier:
    /// mutating a `Black` node to point to a `White` node re-grays `to` so marking never misses it.
    fn mutator_add_edge(&mut self, from: usize, to: usize) {
        self.edges.entry(from).or_default().push(to);
        if self.colors[&from] == Color::Black && self.colors[&to] == Color::White {
            self.colors.insert(to, Color::Gray);
        }
    }

    /// Verify the strong tri-color invariant: no `Black` object points directly to a `White` object.
    fn has_black_to_white_violation(&self) -> bool {
        for (&u, &cu) in &self.colors {
            if cu == Color::Black {
                for &v in &self.edges[&u] {
                    if self.colors[&v] == Color::White {
                        return true;
                    }
                }
            }
        }
        false
    }
}

/// Minimal naive reference-counting simulator demonstrating leaked cyclic islands:
/// dropping external roots leaves internal cycle counts at 1 forever.
fn naive_refcount_cycle_leak() -> usize {
    let mut rc = [1usize, 1usize]; // external root holds node 0, external root holds node 1
                                   // Connect cycle 0 <-> 1
    rc[1] += 1; // 0 -> 1
    rc[0] += 1; // 1 -> 0
                // Drop external root to node 0
    rc[0] -= 1;
    // Drop external root to node 1
    rc[1] -= 1;
    // Both retain rc == 1 and are never reclaimed without cycle detection or tracing GC!
    rc.iter().filter(|&&c| c > 0).count()
}

fn main() {
    let mut heap = GenerationalHeap::new(2, true);
    let old_holder = heap.alloc("old_holder", Gen::Old, 1);
    let young_kept = heap.alloc("young_kept", Gen::Young, 0);
    let _young_dead = heap.alloc("young_dead", Gen::Young, 0);
    heap.write_field(old_holder, 0, Some(young_kept));

    let (swept1, prom1) = heap.minor_collect(&[old_holder]);
    println!(
        "Minor GC #1: swept={swept1}, promoted={prom1}, kept_tag={}, rem={:?}",
        heap.objects[&young_kept].tag, heap.remembered_set
    );
    let (swept2, prom2) = heap.minor_collect(&[old_holder]);
    println!(
        "Minor GC #2 (promotes young_kept): swept={swept2}, promoted={prom2}, gen={:?}",
        heap.objects[&young_kept].gen
    );

    let mut tri = TriColorHeap::new(&[0, 1, 2], &[(0, 1)]);
    tri.colors.insert(0, Color::Gray);
    tri.step_mark(); // 0 becomes Black, 1 becomes Gray
    tri.mutator_add_edge(0, 2); // Black -> White triggers write barrier to re-gray 2
    assert!(!tri.has_black_to_white_violation());
    while tri.step_mark() {}
    println!("Tri-color final colors: {:?}", tri.colors);
    println!(
        "Naive RC leaked cycle nodes: {}",
        naive_refcount_cycle_leak()
    );
}

#[test]
fn write_barrier_protects_old_to_young_pointers_while_skipping_gen1() {
    // Without write barrier: old -> young pointer is missed by minor GC!
    let mut unbarriered = GenerationalHeap::new(2, false);
    let old_u = unbarriered.alloc("old", Gen::Old, 1);
    let young_u = unbarriered.alloc("young_lost", Gen::Young, 0);
    unbarriered.write_field(old_u, 0, Some(young_u));
    unbarriered.minor_collect(&[old_u]);
    assert!(!unbarriered.objects.contains_key(&young_u));

    // With write barrier: remembered_set preserves young_k and promotes it after age_threshold.
    let mut heap = GenerationalHeap::new(2, true);
    let old_root = heap.alloc("old_root", Gen::Old, 1);
    let old_unreferenced = heap.alloc("old_unreferenced", Gen::Old, 0);
    let young_k = heap.alloc("young_kept", Gen::Young, 0);
    let young_garbage = heap.alloc("young_garbage", Gen::Young, 0);
    heap.write_field(old_root, 0, Some(young_k));

    let (swept1, prom1) = heap.minor_collect(&[old_root]);
    assert_eq!((swept1, prom1), (1, 0));
    assert!(!heap.objects.contains_key(&young_garbage));
    assert!(heap.objects.contains_key(&young_k));
    assert!(heap.objects.contains_key(&old_unreferenced)); // Gen1 untouched by minor GC

    let (swept2, prom2) = heap.minor_collect(&[old_root]);
    assert_eq!((swept2, prom2), (0, 1));
    assert_eq!(heap.objects[&young_k].gen, Gen::Old);
    assert!(heap.remembered_set.is_empty());
}

#[test]
fn tricolor_invariant_and_refcount_cycle_leak_hold() {
    let mut tri = TriColorHeap::new(&[0, 1, 2], &[(0, 1)]);
    tri.colors.insert(0, Color::Gray);
    tri.step_mark();
    assert_eq!(tri.colors[&0], Color::Black);
    assert_eq!(tri.colors[&1], Color::Gray);
    assert_eq!(tri.colors[&2], Color::White);

    tri.mutator_add_edge(0, 2);
    assert!(!tri.has_black_to_white_violation());
    assert_eq!(tri.colors[&2], Color::Gray);
    while tri.step_mark() {}
    assert!(tri.colors.values().all(|&c| c == Color::Black));
    assert_eq!(naive_refcount_cycle_leak(), 2);
}
