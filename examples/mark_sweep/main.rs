//! Trace a simulated heap from its roots, then reclaim unreachable slots.

struct Object {
    // A reference is a heap-slot index in this model.
    references: Vec<usize>,
}

fn collect(heap: &mut [Option<Object>], roots: &[usize]) -> Vec<usize> {
    let mut marked = vec![false; heap.len()];
    // Start traversal from roots, such as references held in active stack frames.
    let mut pending = roots.to_vec();
    while let Some(index) = pending.pop() {
        let Some(Some(object)) = heap.get(index) else {
            continue;
        };
        // Expand each reachable object once, even when references form a cycle.
        if !marked[index] {
            marked[index] = true;
            pending.extend(&object.references);
        }
    }
    // Sweep the heap in slot order, clearing every unmarked occupied slot.
    let mut reclaimed = Vec::new();
    for (index, (slot, live)) in heap.iter_mut().zip(marked).enumerate() {
        if !live && slot.take().is_some() {
            reclaimed.push(index);
        }
    }
    reclaimed
}

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
