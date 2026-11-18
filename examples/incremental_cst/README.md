# Concrete Syntax Trees and Incremental Parsing *(Modern Extension)*

Build a lossless immutable **Green Tree** (preserving every byte of whitespace and comments as trivia), wrap it in on-demand **Red Tree** cursors for shifted absolute byte offsets, and incrementally reparse edited buffers while reusing unaffected statement subtrees. See the [Rust source](main.rs) and [Python source](main.py).

> **Edition note:** Concrete syntax trees (Green/Red trees) and incremental IDE reparsing (e.g., Roslyn, `rowan`/rust-analyzer, Tree-sitter) are modern interactive-tooling techniques placed under **Advanced parsing techniques** rather than classical batch Dragon Book chapters.

```sh
cargo run --example incremental_cst
cargo test --example incremental_cst
python3 examples/incremental_cst/main.py
python3 -m unittest examples/incremental_cst/main.py
```

## What the example teaches

### Why batch ASTs fail for formatters and IDEs

A traditional Dragon Book AST ([AST Construction](../ast_construction/README.md)) discards whitespace and comments and stores absolute source spans on every node:

1. Formatters and refactoring tools cannot emit preserved comments if trivia was dropped by the lexer.
2. Editing one character at the top of a file invalidates every downstream absolute byte offset, forcing batch parsers to rebuild the entire file on every keystroke.

### Green Tree vs. Red Tree architecture

| Layer | Mutability & sharing | Stored coordinates | Key property |
| --- | --- | --- | --- |
| **Green Tree** (`GreenNode` / `GreenToken`) | Immutable, reference-shared (`Rc` in Rust) | **Relative byte `width` only**; zero parent pointers | Exact `text()` lossless round-trip; unaffected subtrees survive edits unchanged |
| **Red Tree** (`RedNode`) | Ephemeral cursor over a `GreenNode` | **Absolute `offset .. offset + width`** computed lazily downward | Exposes shifted `TextRange`s for downstream statements without mutating Green nodes |

### How incremental reparse works

Given an edit replacing `old_src[edit_start .. edit_start + del_len]` with `replacement`:

1. Prefix statements strictly before `edit_start` are copied by shared Green reference (`reused_stmts += 1`).
2. Only statements overlapping the edited window are re-lexed and reparsed (`reparsed_stmts += 1`).
3. Downstream statements whose shifted start matches `new_pos` and whose slice still matches cleanly are linked directly back into the new `Program` Green node (`Rc::ptr_eq` in Rust, `is` in Python) without re-allocating or re-parsing.

### How to read the code

| Component | Rust (`main.rs`) | Python (`main.py`) | Role |
| --- | --- | --- | --- |
| Lossless lexer | `fn tokenize_lossless` | `def tokenize_lossless` | Emits every byte including `Whitespace` and `Comment` trivia |
| Green CST | `struct GreenNode`, `GreenNode::text` | `class GreenNode` | Relative-width lossless syntax tree |
| Red cursor | `struct RedNode`, `RedNode::range` | `class RedNode` | Computes absolute `[start, end)` spans on demand |
| Incremental driver | `fn incremental_reparse` | `def incremental_reparse` | Reuses prefix/suffix Green subtrees and matches full reparse identically |

## What to look for

When editing the middle `let y = (4 + 5);` statement to `let y = 100 + 200;`, `stmt[0]` (`LetStmt`) and `stmt[2]` (`ReturnStmt`) report `ptr_reused=true` even though `stmt[2]`'s Red Tree range shifts forward by 4 bytes, and the resulting tree equals a fresh `parse_cst_full` tree node-for-node.

## Check your understanding

- Why must `GreenNode` store **width** instead of `(start, end)` byte offsets if subtrees are to be reused after an earlier insertion?
- Why does keeping 1-byte margin at `oe < edit_start` prevent erroneously reusing a statement when a user appends characters immediately after its terminating semicolon?

## Edition mapping and scope

| 1977 Green | 1986 Red | 2006 Purple |
| --- | --- | --- |
| — *(modern extension; cf. Ch. 5)* | — *(modern extension; cf. Ch. 4–5)* | — *(modern extension; cf. Ch. 2, 4–5)* |
