#[derive(Debug)]
struct Task {
    name: &'static str,
    latency: u32,
    depends_on: &'static [usize],
}

fn schedule(tasks: &[Task], width: usize) -> Option<Vec<u32>> {
    if width == 0
        || tasks
            .iter()
            .any(|task| task.latency == 0 || task.depends_on.iter().any(|&id| id >= tasks.len()))
    {
        return None;
    }
    let mut issued = vec![None::<u32>; tasks.len()];
    let mut pending = tasks.len();
    let mut cycle = 0_u32;
    while pending > 0 {
        let mut slots = width;
        let mut next_ready = None;
        for (index, task) in tasks.iter().enumerate() {
            if issued[index].is_some() {
                continue;
            }
            let mut ready_at = Some(0);
            for &dep in task.depends_on {
                let Some(start) = issued[dep] else {
                    ready_at = None;
                    break;
                };
                ready_at = ready_at.max(Some(start.checked_add(tasks[dep].latency)?));
            }
            if let Some(ready_at) = ready_at {
                if ready_at <= cycle && slots > 0 {
                    issued[index] = Some(cycle);
                    slots -= 1;
                    pending -= 1;
                } else {
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
            // Tasks encountered before a producer may now have all predecessors issued.
            if slots < width {
                cycle = cycle.checked_add(1)?;
            } else {
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
