# Context-insensitive points-to analysis

Propagate possible allocation sites through assignments and calls. See the [Rust source](main.rs) and [Python source](main.py).

```sh
cargo run --example points_to
cargo test --example points_to
python3 examples/points_to/main.py
python3 -m unittest examples/points_to/main.py
```

## What the example teaches

Each inclusion edge adds the source’s possible targets to the destination’s set. Both calls to `relay` share one formal parameter and one return node, so the solver merges their facts as it iterates to a fixed point.

A **points-to set** summarizes the objects a reference could refer to at runtime. Here objects are identified by **allocation sites**, meaning source locations that allocate them. One site can stand for many runtime objects if the allocation executes repeatedly. An analysis is **interprocedural** when it follows information through procedure calls and returns.

### Why we need points-to analysis

Optimizing compilers routinely ask: *can writing through `*p` change the value read through `*q`?* Without pointer analysis, a compiler must conservatively assume every pointer might alias every other pointer, which blocks dead-store elimination, register promotion, devirtualization, and safe parallelization.

This demo models **Andersen-style inclusion constraints**: an assignment `dst = src` creates a directed subset constraint `pts(src) ⊆ pts(dst)` rather than merging `pts(src) == pts(dst)` symmetrically. Even so, keeping only one summary node per callee (`relay.p` and `relay.return`) introduces **context-insensitive** imprecision across multiple call sites—a concrete trade-off explored further in [context_points_to](../context_points_to/README.md).

### Translate the code into constraints

An assignment `destination = source` adds the constraint `points_to(source) ⊆ points_to(destination)`. Passing an argument adds an edge from the actual argument to the formal parameter. Returning adds an edge from the return expression to the procedure's return node and from there to each result variable.

```text
a {alloc_A} ─┐
             ├→ relay.p → relay.return ─┬→ x
b {alloc_B} ─┘                         └→ y
```

`solve` repeatedly copies source facts along edges with set union. It begins with the two allocation facts (`a -> {alloc_A}`, `b -> {alloc_B}`). The order of edges in the demo deliberately puts return edges before incoming argument edges (`(RETURN, Y), (RETURN, X), (PARAMETER, RETURN), (B, PARAMETER), (A, PARAMETER)`):

1. **Pass 1**: `(RETURN, Y)` and `(RETURN, X)` copy `{}` because `relay.return` is empty; `(PARAMETER, RETURN)` copies `{}`; then `(B, PARAMETER)` and `(A, PARAMETER)` seed `relay.p = {alloc_A, alloc_B}`.
2. **Pass 2**: `relay.p` reaches `relay.return`, so `relay.return = {alloc_A, alloc_B}`.
3. **Pass 3**: `relay.return` flows to `x` and `y`.
4. **Pass 4**: No set grows (`changed == false`), so the solver reaches its least fixed point and terminates.

### Understand the precision trade-off

At runtime `x` gets the reference passed in as `a` and `y` gets the one passed in as `b`. The analysis uses the same formal-parameter and return nodes for both calls, a choice called **context insensitivity**. Once both sites enter the shared formal set, both result variables receive both sites. The extra possibilities are conservative. For example, a client of this analysis cannot prove that `x` and `y` refer to different allocation sites.

### How to read the code

| Symbol | Role in [Rust](main.rs) / [Python](main.py) |
| --- | --- |
| `Sites` / `set[str]` | Abstract points-to set holding allocation-site labels for one variable |
| `solve` | Fixed-point inclusion solver: repeatedly propagates `pts[source] ⊆ pts[destination]` until a full pass adds no new sites |
| `example` | Encodes `a = new A; b = new B; x = relay(a); y = relay(b)` into 6 nodes and 5 reversed inclusion edges |
| `main` / tests | Prints the converged points-to sets and verifies both call-site merging and termination on cyclic constraints |

## What to look for

Both callers’ result variables are conservatively associated with both allocation sites. The test also propagates an allocation-site fact around a cycle.

## Check your understanding

- Draw a version of the graph with separate parameter and return nodes for each call site. Which sets would then reach `x` and `y`?
- Why does the cycle in the test still terminate? Consider the finite set of sites and what union can change on each pass.

## Edition mapping and scope

| 1977 Green | 1986 Red | 2006 Purple |
| --- | --- | --- |
| 14 (related interprocedural flow) | 10 (related alias analysis) | 12 |

Numbers refer to chapters. Related readings and appendices are marked where applicable. See the [edition guide](../../README.md#edition-guide) for the bibliographic references and chapter coverage.

The example models allocation, copy, and statically known calls with shared callee nodes. Fields, indirect loads and stores, and context-sensitive analysis are outside this model.
