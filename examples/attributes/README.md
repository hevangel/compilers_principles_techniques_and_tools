# Inherited and synthesized attributes

Pass an origin down a tree and synthesize widths on the way back up. See the [Rust source](main.rs).

```sh
cargo run --example attributes
cargo test --example attributes
```

## What the example teaches

A strip synthesizes its width. A join passes its inherited origin to the left child, then adds the left child’s width to obtain the right child’s inherited origin. The left-to-right evaluation order satisfies both dependencies.

An **attribute** is a piece of information attached to a syntax-tree node. A **synthesized attribute** is computed at the node and returned to its parent. An **inherited attribute** is supplied by the node's surroundings, such as a parent or a sibling. Compilers use these two directions of information flow for tasks like passing scope information down and computing types or code fragments on the way back up.

### Follow the information through the tree

This example lays out strips side by side. `Strip(width)` is a leaf, and `Join(left, right)` places its right subtree immediately after its left subtree. Each call to `place(shape, origin)` inherits the coordinate where that subtree should begin. It synthesizes a total width and a list of `(origin, width)` placements.

1. A strip returns its width and one placement at the inherited origin.
2. A join first places the left subtree at its own origin. The left result supplies the width needed to compute the right origin.
3. After placing the right subtree, the join adds the widths and concatenates the placements in left-to-right order.

### Work the example from left to right

The tree is `Join(Join(Strip(2), Strip(5)), Strip(4))` and the root starts at 10. The first strip occupies `[10, 12)`, so the second starts at 12 and ends at 17. Their parent returns width 7. The outer join uses that width to start the last strip at 17 and returns total width 11.

The order matters because the right child's inherited attribute depends on the left child's synthesized result. This is an example of an **L-attributed** dependency, which can be evaluated in a left-to-right traversal.

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

The tree and its evaluation order are written directly in Rust.
