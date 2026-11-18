# Tomita GLR Parsing, Graph-Structured Stacks, and Parse Forests *(Modern Extension)*

Parse ambiguous LR tables with unresolved shift/reduce conflicts by forking and merging parser heads on a Graph-Structured Stack (GSS) and packing all parse trees into a Shared Packed Parse Forest (SPPF). See the [Rust source](main.rs) and [Python source](main.py).

> **Edition note:** Tomita's GLR algorithm (1985; used in Bison `%glr-parser` and Tree-sitter) is a modern extension beyond classical Dragon Book deterministic LR parsing.

```sh
cargo run --example glr_parsing
cargo test --example glr_parsing
python3 examples/glr_parsing/main.py
python3 -m unittest examples/glr_parsing/main.py
```

## What the example teaches

### Why we need this / What problem this solves

Classical SLR/LALR/LR(1) parsers abort table generation if an ambiguous grammar—or a tricky language like C++ templates vs. `<` or natural-language PP attachment—has shift/reduce or reduce/reduce conflicts. Naive backtracking over LR copies entire stacks exponentially.

**GLR (Generalized LR)** solves ambiguity in polynomial/near-linear time on practical grammars by keeping **all** conflicting actions in `ACTION[state, sym]` and synchronizing parallel LR heads breadth-first per token:

1. **Graph-Structured Stack (GSS):** When a state has both `Shift` and `Reduce`, the active head forks; whenever two parallel heads reach the same LR state at the same token offset, they **merge** into one GSS node with multiple parent links, sharing downstream parsing work.
2. **Shared Packed Parse Forest (SPPF):** Deduplicates sub-derivations by `(symbol, start_pos, end_pos)`. When two derivations produce the same nonterminal span `[l..r]`, both child lists live as `packs` under one shared SPPF node rather than exponentially duplicating trees.
3. **Full Ambiguity Enumeration:** On `a + b * c`, GLR recovers both `((a + b) * c)` and `(a + (b * c))`; on `a + b + c + d + e`, it compacts all `C_4 = 14` Catalan parse trees into fewer than 40 SPPF nodes.

### How to read the code

| Component | Rust (`main.rs`) | Python (`main.py`) | Role |
| --- | --- | --- | --- |
| Conflict-preserving table | `struct GlrTable`, `build_ambiguous_glr_table` | `class GlrTable` | Retains multi-action `Vec<Action>` cells instead of aborting on S/R conflicts |
| GSS + SPPF driver | `glr_parse`, `struct GssNode`, `struct SpfNode` | `glr_parse`, `GssNode`, `SpfNode` | Breadth-first reductions + shifts with stack merging and packed forest sharing |
| Tree unpacker | `enumerate_sppf_exprs` | `enumerate_sppf_exprs` | Expands all derivations captured in the SPPF deterministically |

## What to look for

Running the example prints all `2` interpretations of `id + id * id` and all `5` Catalan `C_3` trees of `id + id + id + id`, alongside exact GSS merge and SPPF node counts.

## Check your understanding

- Why does `glr_parse` finish **all** pending reductions at position `pos` before shifting the next token to `pos + 1`?
- How does keying SPPF nodes by `(symbol, start, end)` avoid exponential blowup on `a + b + c + d + e`?

## Edition mapping and scope

| 1977 Green | 1986 Red | 2006 Purple |
| --- | --- | --- |
| — *(modern extension; cf. Ch. 5–6)* | — *(modern extension; cf. §4.7)* | — *(modern extension; cf. §4.7–4.8)* |
