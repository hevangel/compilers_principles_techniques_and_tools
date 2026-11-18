//! Trace a simulated heap from its roots, then reclaim unreachable slots.

/// Simulated heap object whose pointer fields are slot indices into the heap vector.
struct Object {
    references: Vec<usize>,
}

/// Perform one stop-the-world mark-sweep collection starting from external `roots`.
/// Returns the sorted list of heap slot indices reclaimed during the sweep phase.
fn collect(heap: &mut [Option<Object>], roots: &[usize]) -> Vec<usize> {
    // Fresh mark bit per heap slot; false means "assumed unreachable until proven live".
    let mut marked = vec![false; heap.len()];
    // Worklist seeded with roots (e.g., pointers in registers or active stack frames).
    let mut pending = roots.to_vec();

    // Mark phase: DFS over the object graph starting from roots.
    while let Some(index) = pending.pop() {
        let Some(Some(object)) = heap.get(index) else {
            continue;
        };
        // Invariant: set `marked[index] = true` *before* pushing neighbors so cycles
        // (such as 0 -> 1 -> 0) are traversed once and never cause infinite loops.
        if !marked[index] {
            marked[index] = true;
            pending.extend(&object.references);
        }
    }

    // Sweep phase: linearly scan all heap slots and free any occupied slot left unmarked.
    // Unreachable cycles (like 2 <-> 3) have `marked[i] == false` and are reclaimed here.
    let mut reclaimed = Vec::new();
    for (index, (slot, live)) in heap.iter_mut().zip(marked).enumerate() {
        if !live && slot.take().is_some() {
            reclaimed.push(index);
        }
    }
    reclaimed
}

/// Build a 5-slot heap with two 2-cycles (`0<->1` and `2<->3`) and one isolated node (`4`).
fn sample() -> Vec<Option<Object>> {
    [vec![1], vec![0], vec![3], vec![2], vec![]]
        .into_iter()
        .map(|references| Some(Object { references }))
        .collect()
}

fn main() {
    let mut heap = sample();
    println!("Reclaimed slots: {:?}", collect(&mut heap, &[0, 4]));
}

#[test]
fn preserves_rooted_cycle_and_reclaims_unreachable_cycle() {
    let mut heap = sample();
    assert_eq!(collect(&mut heap, &[0, 4]), [2, 3]);
    assert!(collect(&mut heap, &[0, 4]).is_empty());
    assert_eq!(collect(&mut heap, &[]), [0, 1, 4]);
}
