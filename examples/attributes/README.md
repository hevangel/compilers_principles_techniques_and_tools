# Inherited and synthesized attributes

Pass an origin down a tree and synthesize widths on the way back up. See the [Rust source](main.rs) and [Python source](main.py).

```sh
cargo run --example attributes
cargo test --example attributes
python3 examples/attributes/main.py
python3 -m unittest examples/attributes/main.py
```

## What the example teaches

### Why we need this

A parser checks whether tokens form a valid syntax tree—such as `Join(Join(Strip(2), Strip(5)), Strip(4))`—but syntax alone cannot answer layout, type, or address questions that depend on **context from surrounding nodes**. Suppose you want to place boxes side by side starting at x-coordinate `10`:

- A purely bottom-up pass (computing only synthesized attributes) can sum the widths (`2 + 5 + 4 = 11`), because a parent's width depends only on its children. However, it does not know the starting `origin` (`10`) passed in from outside.
- A purely top-down pass (passing only inherited attributes) knows the root's `origin` (`10`), so it can pass `10` to the left child—but it cannot tell the right child where to start until it knows how wide the left child turned out to be.

Combining **inherited** attributes (flowing down/left-to-right from context) and **synthesized** attributes (flowing back up from children) solves both problems in a single traversal.

An **attribute** is a piece of information attached to a syntax-tree node. A **synthesized attribute** is computed at the node and returned to its parent. An **inherited attribute** is supplied by the node's surroundings, such as a parent or a sibling. Compilers use these two directions of information flow for tasks like passing scope tables or target addresses down and returning types or generated code fragments up.

### Follow the information through the tree

This example lays out strips side by side. `Strip(width)` is a leaf, and `Join(left, right)` places its right subtree immediately after its left subtree. Each call to `place(shape, origin)` inherits the coordinate where that subtree should begin. It synthesizes a total width and a list of `(origin, width)` placements.

1. A strip returns its width and one placement at the inherited origin.
2. A join first places the left subtree at its own origin. The left result supplies the width needed to compute the right origin (`origin + left.width`).
3. After placing the right subtree, the join adds the widths and concatenates the placements in left-to-right order.

### Work the example from left to right

For `Join(Join(Strip(2), Strip(5)), Strip(4))` starting at `origin = 10`, the traversal visits nodes in depth-first left-to-right order:

| Step | Node visited | Inherited `origin` | Synthesized `width` | Synthesized `placements` |
| --- | --- | --- | --- | --- |
| 1 | Outer `Join` (enter) | `10` | — | — |
| 2 | Inner `Join` (enter) | `10` | — | — |
| 3 | `Strip(2)` | `10` | `2` | `[(10, 2)]` |
| 4 | `Strip(5)` | `10 + 2 = 12` | `5` | `[(12, 5)]` |
| 5 | Inner `Join` (exit) | `10` | `2 + 5 = 7` | `[(10, 2), (12, 5)]` |
| 6 | `Strip(4)` | `10 + 7 = 17` | `4` | `[(17, 4)]` |
| 7 | Outer `Join` (exit) | `10` | `7 + 4 = 11` | `[(10, 2), (12, 5), (17, 4)]` |

The order matters because the right child's inherited attribute depends on the left child's synthesized result. This is an **L-attributed** dependency ("L" for left-to-right): every inherited attribute depends only on the parent or siblings to its left, so a single depth-first left-to-right pass evaluates every attribute without a general dependency graph.

## How to read the code

| Concept | Rust (`main.rs`) | Python (`main.py`) | Role |
| --- | --- | --- | --- |
| Syntax tree nodes | `enum Shape` (`Strip`, `Join`) | `Strip`, `Join` dataclasses (`Shape`) | Recursive layout AST |
| Synthesized bundle | `struct Attributes` | `class Attributes` | Holds total `width` and `placements` |
| L-attributed evaluator | `fn place(shape, origin)` | `def place(shape, origin)` | Passes `origin` down; returns `Attributes` up |
| Worked input | `fn sample()` | `def sample()` | Builds `Join(Join(Strip(2), Strip(5)), Strip(4))` |

## What to look for

Three strips of widths 2, 5, and 4 starting at 10 have origins 10, 12, and 17. The test checks those placements and total width 11.

## Check your understanding

- Predict the placements after changing the starting origin to 0. Which synthesized quantities change?
- Swap the outer join's children and work out all three positions again before running the program.

## Edition mapping and scope

| 1977 Green | 1986 Red | 2006 Purple |
| --- | --- | --- |
| 7 (translation schemes) | 5 | 5 |

Numbers refer to chapters. Related readings and appendices are marked where applicable. See the [edition guide](../../README.md#edition-guide) for the bibliographic references and chapter coverage.

The tree and its evaluation order are written directly in the source.
