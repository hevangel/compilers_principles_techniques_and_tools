//! Schedule instructions subject to dependencies, latency, and issue width.

/// Represents one instruction node in a basic-block data-dependence DAG.
/// - `latency`: cycles from issue until the instruction's result can be read by consumers.
/// - `depends_on`: indices of producer instructions whose results must be ready first.
#[derive(Debug)]
struct Task {
    name: &'static str,
    latency: u32,
    depends_on: &'static [usize],
}

/// Greedy basic-block list scheduler.
/// Returns `Some(issue_cycles)` matching `tasks` order, or `None` on invalid input or cycles.
/// Invariant: for every dependence `dep -> index`, `issued[index] >= issued[dep] + tasks[dep].latency`,
/// and at most `width` instructions share the same issue cycle.
fn schedule(tasks: &[Task], width: usize) -> Option<Vec<u32>> {
    // Reject zero issue width, zero-cycle latencies (which would break strict progress),
    // or out-of-bounds producer references up front.
    if width == 0
        || tasks
            .iter()
            .any(|task| task.latency == 0 || task.depends_on.iter().any(|&id| id >= tasks.len()))
    {
        return None;
    }
    // `issued[i]` stores the assigned start cycle once task `i` issues; `None` means unscheduled.
    let mut issued = vec![None::<u32>; tasks.len()];
    let mut pending = tasks.len();
    let mut cycle = 0_u32;
    while pending > 0 {
        let mut slots = width;
        // Tracks the earliest cycle > `cycle` at which at least one currently-blocked
        // (but predecessor-complete) task could issue, letting us fast-forward over stalls.
        let mut next_ready = None;
        for (index, task) in tasks.iter().enumerate() {
            if issued[index].is_some() {
                continue;
            }
            // Data-dependence constraint: a task is data-ready only after *all* producers have
            // issued, at `ready_at = max_{dep} (issued[dep] + latency[dep])`.
            let mut ready_at = Some(0);
            for &dep in task.depends_on {
                let Some(start) = issued[dep] else {
                    ready_at = None;
                    break;
                };
                ready_at = ready_at.max(Some(start.checked_add(tasks[dep].latency)?));
            }
            if let Some(ready_at) = ready_at {
                // Both data-readiness (`ready_at <= cycle`) and hardware capacity (`slots > 0`)
                // must hold to issue in the current cycle. (Input order breaks ties among ready tasks;
                // a priority list scheduler would sort ready tasks by critical-path height first.)
                if ready_at <= cycle && slots > 0 {
                    issued[index] = Some(cycle);
                    slots -= 1;
                    pending -= 1;
                } else {
                    // If already data-ready but out of slots, it can try again at `cycle + 1`;
                    // otherwise it cannot start before `ready_at`.
                    let candidate = if ready_at <= cycle {
                        cycle.checked_add(1)?
                    } else {
                        ready_at
                    };
                    next_ready =
                        Some(next_ready.map_or(candidate, |time: u32| time.min(candidate)));
                }
            }
        }
        if pending > 0 {
            // If anything issued this cycle (`slots < width`), advance by 1 cycle because
            // tasks earlier in input order might now see newly-issued producers.
            if slots < width {
                cycle = cycle.checked_add(1)?;
            } else {
                // Nothing could issue in `cycle`: jump straight to `next_ready` to skip idle cycles.
                // If `next_ready` is `None`, every remaining task has an unissued predecessor,
                // which implies a cycle in the dependence graph.
                cycle = next_ready?;
            }
        }
    }
    issued.into_iter().collect()
}

const TASKS: &[Task] = &[
    Task {
        name: "load a",
        latency: 3,
        depends_on: &[],
    },
    Task {
        name: "load b",
        latency: 2,
        depends_on: &[],
    },
    Task {
        name: "add a, b",
        latency: 1,
        depends_on: &[0, 1],
    },
    Task {
        name: "independent multiply",
        latency: 2,
        depends_on: &[],
    },
    Task {
        name: "consume sum and product",
        latency: 1,
        depends_on: &[2, 3],
    },
];

fn main() {
    for (task, cycle) in TASKS.iter().zip(schedule(TASKS, 2).unwrap()) {
        println!("Cycle {cycle}: {}", task.name);
    }
}

#[test]
fn respects_width_and_latencies() {
    assert_eq!(schedule(TASKS, 2), Some(vec![0, 0, 3, 1, 4]));
    assert_eq!(schedule(TASKS, 1), Some(vec![0, 1, 3, 2, 4]));
    let reversed = [
        Task {
            name: "consumer",
            latency: 1,
            depends_on: &[1],
        },
        Task {
            name: "producer",
            latency: 4,
            depends_on: &[],
        },
    ];
    assert_eq!(schedule(&reversed, 2), Some(vec![4, 0]));
}

#[test]
fn detects_a_cycle() {
    let tasks = [
        Task {
            name: "a",
            latency: 1,
            depends_on: &[1],
        },
        Task {
            name: "b",
            latency: 1,
            depends_on: &[0],
        },
    ];
    assert_eq!(schedule(&tasks, 2), None);
}
