# Context-Sensitive (k-CFA / Call-String) Points-To Analysis

Distinguish call sites by parameterizing points-to variables with bounded `k`-length call-string contexts and discovering virtual dispatch targets on the fly. See the [Rust source](main.rs) and [Python source](main.py).

```sh
cargo run --example context_points_to
cargo test --example context_points_to
python3 examples/context_points_to/main.py
python3 -m unittest examples/context_points_to/main.py
```

## What the example teaches

A context-insensitive solver merges all callers of a shared helper like `relay`, polluting both callers' return variables. Qualifiying variables and abstract contexts with the last `k` call sites separates distinct call paths while keeping recursion finite.

### Parameterized call-string precision (`k`-CFA)

| Setting | Call-string state | Behavior on `relay` and recursion |
| --- | --- | --- |
| `k = 0` | Empty context `[]` | Merges `{alloc_A, alloc_B}` into both `x` and `y` |
| `k = 1` | Most recent site `[c1]` or `[c2]` | Separates `x -> {alloc_A}` and `y -> {alloc_B}` cleanly |
| `k = 2` | Last 2 sites `[r0, r_self]`, `[r_self, r_self]` | Terminates at finite saturation even under direct self-recursion |
| On-the-fly dispatch | Resolve `(class_type, method)` as receiver pts grow | Discovers `SubA::apply` without pulling in unreachable `SubB::apply` |

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
