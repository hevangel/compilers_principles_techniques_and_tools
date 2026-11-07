# The Dragon Book in Rust

21 original Rust demonstrations of compiler concepts from the three books commonly known as the Dragon Book. Start with a small compiler and stack machine, or run an individual example to see one technique in isolation. Each example has a guide beside its source.

## Run the examples

Use Rust 1.85 or later with Cargo. The examples use only the Rust standard library. Run these commands from the repository root.

```sh
cargo run --example tiny_compiler
cargo run --example tiny_compiler -- "let n = 6; emit n + 2 * 3; emit (n + 2) * 3;"
cargo test --all-targets --locked
```

The first command prints bytecode and `Output: [27]`. The second prints `Output: [12, 24]`. Use `cargo test --example ll1` to run the tests for a single example. Open [index.html](index.html) for the standalone browser index.

## Edition guide

The 1977 Green book is the predecessor to the two numbered editions of *Compilers: Principles, Techniques, and Tools*.

- **1977 Green.** *Principles of Compiler Design* by Alfred V. Aho and Jeffrey D. Ullman. [Open Library contents](https://openlibrary.org/works/OL9922572W/Principles_of_compiler_design?edition=principlesofcomp0000ahoa_x2t2).
- **1986 Red.** *Compilers: Principles, Techniques, and Tools*, first edition, by Alfred V. Aho, Ravi Sethi, and Jeffrey D. Ullman. [1986 contents](https://elmhurst.ecampus.com/compilers-principles-techniques-tools-aho/bk/9780201100884).
- **2006 Purple.** *Compilers: Principles, Techniques, and Tools*, second edition, by Alfred V. Aho, Monica S. Lam, Ravi Sethi, and Jeffrey D. Ullman. [Pearson contents](https://www.pearson.com/en-ca/subject-catalog/p/compilers-principles-techniques-and-tools/P200000003472/9780321486813).

The numbers below refer to chapters unless marked as an appendix. A reference points to relevant reading within a chapter. An em dash indicates that this guide assigns no direct reading in that volume.

### Start here

A compiler translates a program from one language into another. Follow one tiny program from source text through tokens, grammatical structure, name resolution, and bytecode. Then run the bytecode on a stack machine and track what each instruction does.

| Example | 1977 Green | 1986 Red | 2006 Purple |
| --- | --- | --- | --- |
| [A tiny compiler and stack machine](examples/tiny_compiler/README.md) | 1, 2, 5, 7 | 1–2; app. A (project theme) | 1–2; app. A (front-end theme) |

### Scanning and parsing

Scanning groups characters into tokens, such as names and operators. Parsing checks how those tokens fit together according to a grammar. This section starts with token boundaries and finite automata, compares top-down and bottom-up parsing, and shows how to resume after a syntax error.

| Example | 1977 Green | 1986 Red | 2006 Purple |
| --- | --- | --- | --- |
| [Tokens and longest match](examples/lexer/README.md) | 3 | 2–3 | 2–3 |
| [Epsilon-NFA to DFA](examples/subset_dfa/README.md) | 3 | 3 | 3 |
| [FIRST, FOLLOW, and LL(1)](examples/ll1/README.md) | 4–5 | 4 | 4 |
| [LR(0) items and SLR parsing](examples/slr/README.md) | 5–6 | 4 | 4 |
| [Panic-mode recovery](examples/recovery/README.md) | 11 | 4 | 4 |

### Translation and semantic analysis

A valid grammatical structure is the starting point for checking what a program means. Semantic analysis resolves names and checks types. Translation uses that information to produce a lower-level representation, for example by turning array indices into byte offsets and Boolean expressions into jumps.

| Example | 1977 Green | 1986 Red | 2006 Purple |
| --- | --- | --- | --- |
| [Inherited and synthesized attributes](examples/attributes/README.md) | 7 (translation schemes) | 5 | 5 |
| [Scoped names and type checks](examples/scope_types/README.md) | 2, 9, 11 | 6–7 | 2, 6 |
| [Type unification and the occurs check](examples/unification/README.md) | — | 6 | — |
| [Row-major array addresses](examples/array_layout/README.md) | 2, 8 | 8–9 | 6, 8 |
| [Backpatching and short-circuit jumps](examples/backpatch/README.md) | 7 (Boolean translation) | 8 | 6 |

### Runtime and code generation

A compiled program still needs ways to store locals, call procedures, manage heap objects, and evaluate expressions on its target machine. These examples connect the runtime representation of data with the instructions a compiler emits to use it.

| Example | 1977 Green | 1986 Red | 2006 Purple |
| --- | --- | --- | --- |
| [Activation records and static links](examples/frames/README.md) | 2, 10 | 7 | 7 |
| [Tracing and mark-sweep collection](examples/mark_sweep/README.md) | — | 7 (storage reclamation) | 7 |
| [Register needs for expression trees](examples/register_codegen/README.md) | 15 | 9 | 8 |

### Optimization and program analysis

An optimization changes how a program runs while preserving its observable behavior. Analyses collect the facts needed to justify those changes, such as which values might be read or which assignments can reach a block. Start inside a basic block, then follow information through branches, loops, and procedure calls.

| Example | 1977 Green | 1986 Red | 2006 Purple |
| --- | --- | --- | --- |
| [Local value numbering](examples/value_numbering/README.md) | 12 | 9–10 | 8–9 |
| [Backward liveness analysis](examples/liveness/README.md) | 12, 14 | 10 | 9 |
| [Forward reaching definitions](examples/reaching_defs/README.md) | 12, 14 | 10 | 9 |
| [Dominators and natural loops](examples/natural_loops/README.md) | 13 | 10 | 9 |
| [Basic-block list scheduling](examples/list_schedule/README.md) | — | — | 10 |
| [Loop interchange and spatial locality](examples/loop_locality/README.md) | — | — | 11 |
| [Context-insensitive points-to analysis](examples/points_to/README.md) | 14 (related interprocedural flow) | 10 (related alias analysis) | 12 |

## Coverage gaps

This is a concept companion with independently written programs, inputs, and explanations. Coverage is selective at the chapter level and excludes textbook listings, figures, and exercise solutions.

- **Across the books.** Scanner generators, DFA minimization, canonical LR(1), LALR, parser generators, general type conversions and overload resolution, calling conventions, and native code generation have no dedicated demos. The optimization examples cover selected analyses and local transformations. They leave out constant propagation, partial redundancy elimination, and loop-invariant code motion.
- **1977 Green.** The language survey in chapter 2 is represented by a small language, scoped names, and array layout. The loop example finds natural loops. It leaves out induction-variable elimination. The historical compiler survey and Pascal project in appendices A and B are outside the collection.
- **1986 Red.** Chapter 6 has a type checker and a unification demo. Full polymorphic inference is outside the collection. Chapters 11 and 12 on compiler development and historical implementations have no dedicated demos. The tiny compiler is a companion to the project theme in appendix A.
- **2006 Purple.** The runtime demo covers basic tracing and sweep. Generational, incremental, and short-pause collectors are outside its scope. Chapters 10–12 have introductory examples. Global scheduling, software pipelining, general affine dependence analysis, context-sensitive pointer analysis, and Datalog/BDD implementations have no dedicated demos. The tiny compiler is a companion to the front-end theme in appendix A. Appendix B’s linear algebra has no dedicated demo.

## Checks

Every example has a deterministic sample in `main` and focused unit tests beside the implementation. Tests check accepting and rejecting parser inputs, short-circuit traces, analysis fixed points, and preservation of results across code transformations.

```sh
cargo fmt --all -- --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --all-targets --locked
```
