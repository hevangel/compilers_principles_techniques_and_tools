# Context-Sensitive (k-CFA / Call-String) Points-To Analysis

Distinguish call sites by parameterizing points-to variables with bounded `k`-length call-string contexts and discovering virtual dispatch targets on the fly. See the [Rust source](main.rs) and [Python source](main.py).

```sh
cargo run --example context_points_to
cargo test --example context_points_to
python3 examples/context_points_to/main.py
python3 -m unittest examples/context_points_to/main.py
```

## What the example teaches

A context-insensitive solver merges all callers of a shared helper like `relay`, polluting both callers' return variables. Qualifying variables and abstract contexts with the last `k` call sites separates distinct call paths while keeping recursion finite.

### Why we need context-sensitive k-CFA and on-the-fly dispatch

In [points_to](../points_to/README.md), `x = relay(a)` and `y = relay(b)` merge inside `relay`, forcing an optimizer to believe `x` and `y` might alias even though they never share an object at runtime. In object-oriented programs, that imprecision cascades into virtual calls: if a receiver variable picks up spurious class types from unrelated callers, static devirtualization thinks `obj.apply()` might call *every* subclass override, pulling dead methods into the call graph and creating even more spurious points-to edges.

**`k`-CFA (call-string sensitivity)** plus **on-the-fly call-graph discovery** solves both problems:

1. **Separate call paths**: Every variable `(func, ctx, var)` is tagged with a call string `ctx` of up to `k` most recent call-site labels.
2. **Bound recursion**: Truncating call strings to length `k` keeps the number of contexts finite (`<= |sites|^k` per function), guaranteeing termination even when a function calls itself recursively.
3. **Discover virtual targets jointly**: Instead of pre-building an overly conservative call graph, the solver resolves `vcall receiver.method(...)` using only the `HeapSite.class_type` values that have actually reached `(func, ctx, receiver)` so far.

### Walkthrough of the two demo programs

1. **`relay` (`k = 0` vs `k = 1`)**:
   - At `k = 0`, `push_call_string` always returns `[]`. Both `c1: relay(a)` and `c2: relay(b)` write into `("relay", [], "p")`, merging `{alloc_A, alloc_B}` into both `x` and `y`.
   - At `k = 1`, `c1` enters `("relay", ["c1"])` and `c2` enters `("relay", ["c2"])`. Their parameters, temporaries, and `$ret` slots stay separate, proving `x -> {alloc_A}` and `y -> {alloc_B}`.
2. **Virtual dispatch + self-recursion (`k = 2`)**:
   - `main` allocates `obj_a` (`SubA`) and `payload` (`Data`), then executes `v1: obj_a.apply(payload)`.
   - Only `SubA` reaches `obj_a`, so the vtable lookup `("SubA", "apply")` adds `("SubA::apply", ["v1"])` on the fly; `SubB::apply` is never discovered.
   - `r0: rec(payload)` enters `("rec", ["r0"])`, whose recursive call `r_self: rec(n)` creates `("rec", ["r0", "r_self"])`, and the next recursive step truncates to the last 2 sites `["r_self", "r_self"]`. At that point, pushing `r_self` again yields `["r_self", "r_self"]`, so the reachable contexts saturate cleanly at 3 entries.

### Parameterized call-string precision (`k`-CFA)

| Setting | Call-string state | Behavior on `relay` and recursion |
| --- | --- | --- |
| `k = 0` | Empty context `[]` | Merges `{alloc_A, alloc_B}` into both `x` and `y` |
| `k = 1` | Most recent site `[c1]` or `[c2]` | Separates `x -> {alloc_A}` and `y -> {alloc_B}` cleanly |
| `k = 2` | Last 2 sites `[r0, r_self]`, `[r_self, r_self]` | Terminates at finite saturation even under direct self-recursion |
| On-the-fly dispatch | Resolve `(class_type, method)` as receiver pts grow | Discovers `SubA::apply` without pulling in unreachable `SubB::apply` |

### How to read the code

| Symbol | Role in [Rust](main.rs) / [Python](main.py) |
| --- | --- |
| `HeapSite`, `CallString` | Allocation-site abstraction (`label` + runtime `class_type`) and bounded tuple/vec of recent call-site IDs |
| `Stmt`, `Function`, `Program` | Mini-IR with `Alloc`, `Assign`, `DirectCall`, `VirtualCall`, `Return`, plus a `(class_type, method) -> fn` vtable |
| `push_call_string` | Appends `site` and keeps only the most recent `k` call-site labels (empty when `k == 0`) |
| `solve_k_cfa` | Co-evolves `reachable` contexts, `call_edges`, and `pts[(func, ctx, var)]` to a joint least fixed point |
| `merged_var` | Projects `(func, *, var)` across all contexts to inspect what a variable may point to overall |
| `relay_program`, `virtual_dispatch_and_recursion_program` | Demo programs contrasting `k = 0` vs `k = 1` and showcasing on-the-fly vtable dispatch + `k = 2` recursion |

## What to look for

Compare `k = 0` and `k = 1` on `relay(a)` / `relay(b)`: `k = 1` creates two contexts for `relay` (`["c1"]` and `["c2"]`), keeping `x` and `y` disjoint. On the virtual + recursive example, `SubA::apply` enters the reachable call graph while `SubB::apply` never does, and `rec` saturates at three length-bounded contexts.

## Check your understanding

- Why does `k = 2` still guarantee termination on arbitrarily deep recursive call chains?
- Why must virtual call graph construction run jointly with points-to propagation instead of purely beforehand?

## Edition mapping and scope

| 1977 Green | 1986 Red | 2006 Purple |
| --- | --- | --- |
| 14 *(related)* | 10 *(related)* | 12 |

Numbers refer to chapters. Related readings and appendices are marked where applicable. See the [edition guide](../../README.md#edition-guide) for the bibliographic references and chapter coverage.

Call-string length `k` bounds contexts over allocation-site abstractions without heap cloning or flow sensitivity.
