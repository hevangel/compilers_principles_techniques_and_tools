# The Dragon Book in Rust and Python

57 original Rust and Python demonstrations of compiler concepts from the books commonly known as the Dragon Book. Start with a small compiler and stack machine, or run an individual example to see one technique in isolation. Each example has a guide beside both its Rust and Python sources, and you can browse the full catalog on [GitHub Pages](https://hevangel.github.io/compilers_principles_techniques_and_tools/).

## Run the examples

Use Rust 1.85 or later with Cargo, or Python 3.11 or later. Every example uses only the Rust standard library and the Python standard library. Run these commands from the repository root.

```sh
cargo run --example tiny_compiler
cargo run --example tiny_compiler -- "let n = 6; emit n + 2 * 3; emit (n + 2) * 3;"
cargo test --all-targets --locked

python3 examples/tiny_compiler/main.py
python3 examples/tiny_compiler/main.py "let n = 6; emit n + 2 * 3; emit (n + 2) * 3;"
python3 -m unittest examples/*/main.py
```

The first command prints bytecode and `Output: [27]`. The second prints `Output: [12, 24]`. Use `cargo test --example ll1` or `python3 -m unittest examples/ll1/main.py` to run the tests for a single example. Browse the [GitHub Pages index](https://hevangel.github.io/compilers_principles_techniques_and_tools/) or open [index.html](index.html) for the standalone browser index.

## Edition guide

The 1977 Green book is the predecessor to the two numbered editions of *Compilers: Principles, Techniques, and Tools*.

- **1977 Green.** *Principles of Compiler Design* by Alfred V. Aho and Jeffrey D. Ullman. [Open Library contents](https://openlibrary.org/works/OL9922572W/Principles_of_compiler_design?edition=principlesofcomp0000ahoa_x2t2).
- **1986 Red.** *Compilers: Principles, Techniques, and Tools*, first edition, by Alfred V. Aho, Ravi Sethi, and Jeffrey D. Ullman. [1986 contents](https://elmhurst.ecampus.com/compilers-principles-techniques-tools-aho/bk/9780201100884).
- **2006 Purple.** *Compilers: Principles, Techniques, and Tools*, second edition, by Alfred V. Aho, Monica S. Lam, Ravi Sethi, and Jeffrey D. Ullman. [Pearson contents](https://www.pearson.com/en-ca/subject-catalog/p/compilers-principles-techniques-and-tools/P200000003472/9780321486813).
- **2014 New International Edition.** Aligns chapter-for-chapter with 2006 Purple Chapters 1–11 and Appendices A–B, while omitting Chapter 12 (*Interprocedural Analysis*) as a separate chapter.

The numbers below refer to chapters unless marked as an appendix. A reference points to relevant reading within a chapter. An em dash indicates that this guide assigns no direct reading in that volume.

### Start here

A compiler translates a program from one language into another. Follow one tiny program from source text through tokens, grammatical structure, name resolution, and bytecode. Then run the bytecode on a stack machine and track what each instruction does.

| Example | 1977 Green | 1986 Red | 2006 Purple |
| --- | --- | --- | --- |
| [A tiny compiler and stack machine](examples/tiny_compiler/README.md) | 1, 2, 5, 7 | 1–2; app. A (project theme) | 1–2; app. A (front-end theme) |

### Scanning and parsing

Scanning groups characters into tokens, such as names and operators. Parsing checks how those tokens fit together according to a grammar. This section starts with regular expressions and finite automata, covers grammar transformations and top-down/bottom-up parsing, resolves ambiguities, and shows how to resume after a syntax error.

| Example | 1977 Green | 1986 Red | 2006 Purple |
| --- | --- | --- | --- |
| [Tokens and longest match](examples/lexer/README.md) | 3 | 2–3 | 2–3 |
| [Regex to Thompson NFA and Direct DFA](examples/regex_automata/README.md) | 3 | 3 | 3 |
| [Epsilon-NFA to DFA](examples/subset_dfa/README.md) | 3 | 3 | 3 |
| [DFA State Minimization](examples/dfa_minimize/README.md) | 3 | 3 | 3 |
| [Lex-Style Scanner Generator (`.l` DSL Compiler)](examples/lex_generator/README.md) | 3 | 3 (§3.5, §3.8) | 3 (§3.5, §3.8) |
| [Grammar Rewriting: Left Recursion Elimination and Left Factoring](examples/grammar_rewrite/README.md) | 4–5 | 4 | 4 |
| [FIRST, FOLLOW, and LL(1)](examples/ll1/README.md) | 4–5 | 4 | 4 |
| [LR(0) items and SLR parsing](examples/slr/README.md) | 5–6 | 4 | 4 |
| [Canonical LR(1), LALR(1), and Table Compaction](examples/lalr/README.md) | 5–6 | 4 | 4 |
| [Ambiguous Precedence and Associativity Resolution](examples/precedence_parse/README.md) | 5–6 | 4 (§4.6, §4.8) | 4 |
| [Yacc-Style Parser Generator (`.y` DSL Compiler)](examples/yacc_generator/README.md) | 5–6, 8 | 4 (§4.8–4.9) | 4 (§4.9) |
| [Panic-mode recovery](examples/recovery/README.md) | 11 | 4 | 4 |

### Translation and semantic analysis

A valid grammatical structure is the starting point for checking what a program means. Semantic analysis resolves names, infers polymorphic types, checks coercions and overloadings, and lays out records and arrays. Translation uses that information to build expression DAGs, SSA graphs, three-address IR, switch dispatch, and short-circuit jumps.

| Example | 1977 Green | 1986 Red | 2006 Purple |
| --- | --- | --- | --- |
| [Inherited and synthesized attributes](examples/attributes/README.md) | 7 (translation schemes) | 5 | 5 |
| [Attribute Dependency Graph and Topological Evaluator](examples/sdd_eval/README.md) | 7 | 5 | 5 |
| [Expression DAGs and Value-Number Hashing](examples/expr_dag/README.md) | 7, 12 | 5, 9 | 5–6, 8 |
| [Static Single-Assignment (SSA) Construction and Phi Placement](examples/ssa/README.md) | — | — | 6, 9 |
| [Scoped names and type checks](examples/scope_types/README.md) | 2, 9, 11 | 6–7 | 2, 6 |
| [Type unification and the occurs check](examples/unification/README.md) | — | 6 | 6 |
| [Hindley-Milner Let-Polymorphic Type Inference and Type Equivalence](examples/poly_infer/README.md) | — | 6 | 6 |
| [Type Coercions and Two-Pass Overload Resolution](examples/overload_coerce/README.md) | — | 6 | 6 |
| [Row-major array addresses](examples/array_layout/README.md) | 2, 8 | 8–9 | 6, 8 |
| [Backpatching and short-circuit jumps](examples/backpatch/README.md) | 7 (Boolean translation) | 8 | 6 |
| [Switch Lowering and Break/Continue Backpatching](examples/switch_lower/README.md) | 7 | 8 | 6 |
| [Record Field Layout and Quadruple / Triple / Indirect-Triple IR](examples/record_ir/README.md) | 2, 8 | 8 | 6 |
| [Pascal-Subset Compiler Front End (1986 Appendix A)](examples/pascal_frontend/README.md) | — | app. A (Pascal project) | 2, 6 |
| [Complete Structured Front End: Lexer, Scoped AST, and TAC (2006 Appendix A)](examples/complete_frontend/README.md) | — | 2, 6, 8 | app. A (complete front end) |
| [Rust-Subset Compiler Front End (Appendix A)](examples/rust_frontend/README.md) | — | 2, 6, 8 | app. A (Rust front end) |
| [Python-Subset Compiler Front End (Appendix A)](examples/python_frontend/README.md) | — | 2, 3, 6 | app. A (Python front end) |

### Runtime and code generation

A compiled program still needs ways to manage lexical nesting, heap garbage collection, register allocation, instruction selection, and peephole cleanup on its target machine. These examples connect the runtime representation of data with the machine instructions emitted to use it.

| Example | 1977 Green | 1986 Red | 2006 Purple |
| --- | --- | --- | --- |
| [Activation records and static links](examples/frames/README.md) | 2, 10 | 7 | 7 |
| [Lexical Display vs Static Links and Stack VLAs](examples/display_vla/README.md) | 2, 10 | 7 | 7 |
| [Tracing and mark-sweep collection](examples/mark_sweep/README.md) | — | 7 (storage reclamation) | 7 |
| [Cheney Semispace Copying and Sliding Mark-Compact GC](examples/copying_gc/README.md) | — | 7 (reclamation) | 7 |
| [Generational, Tri-Color Incremental, and Reference-Counting GC](examples/generational_gc/README.md) | — | 7 | 7 |
| [Register needs for expression trees](examples/register_codegen/README.md) | 15 | 9 | 8 |
| [Register/Address Descriptors, Next-Use, and `getReg` Code Generator](examples/reg_desc_codegen/README.md) | 15 | 9 | 8 |
| [Global Register Allocation by Graph Coloring](examples/graph_color_regalloc/README.md) | 15 | 9 | 8 |
| [Dynamic-Programming Tree Tiling and Peephole Optimization](examples/dp_tiling/README.md) | 15 | 9 | 8 |

### Optimization and program analysis

An optimization changes how a program runs while preserving its observable behavior. Analyses collect the facts needed to justify those changes, from basic-block value numbering and global bitvector/lattice data-flow frameworks to software pipelining, polyhedral loop transformations, and Datalog/ROBDD interprocedural pointer analysis.

| Example | 1977 Green | 1986 Red | 2006 Purple |
| --- | --- | --- | --- |
| [Local value numbering](examples/value_numbering/README.md) | 12 | 9–10 | 8–9 |
| [Backward liveness analysis](examples/liveness/README.md) | 12, 14 | 10 | 9 |
| [Forward reaching definitions](examples/reaching_defs/README.md) | 12, 14 | 10 | 9 |
| [Available Expressions, Global CSE, and Copy Propagation](examples/available_exprs/README.md) | 12, 14 | 10 | 9 |
| [Semilattice Constant Propagation (MFP vs MOP Nondistributivity)](examples/const_prop/README.md) | — | — | 9 |
| [Lazy Code Motion (Partial Redundancy Elimination)](examples/lazy_code_motion/README.md) | — | — | 9 |
| [Dominators and natural loops](examples/natural_loops/README.md) | 13 | 10 | 9 |
| [Loop-Invariant Code Motion and Induction-Variable Strength Reduction](examples/licm_iv/README.md) | 12–13 | 10 | 9 |
| [Region-Based Data-Flow Analysis (T1/T2 Reducible Regions)](examples/region_dataflow/README.md) | 13–14 | 10 | 9 |
| [Basic-block list scheduling](examples/list_schedule/README.md) | — | — | 10 |
| [Software Pipelining and Modular Variable Expansion](examples/software_pipeline/README.md) | — | — | 10 |
| [Global Trace Scheduling and Speculative Code Motion (§10.4)](examples/trace_schedule/README.md) | — | — | 10 (§10.4) |
| [Hardware-Predicated Execution and If-Converted Hyperblocks (§10.4)](examples/hyperblock_predication/README.md) | — | — | 10 (§10.4) |
| [Loop interchange and spatial locality](examples/loop_locality/README.md) | — | — | 11 |
| [Array Dependence Testing (GCD, Banerjee, and Fourier-Motzkin)](examples/dependence_test/README.md) | — | — | 11 |
| [Affine Partitioning, Unimodular Transforms, and Appendix B Linear Algebra](examples/affine_partition/README.md) | — | — | 11, app. B |
| [Context-insensitive points-to analysis](examples/points_to/README.md) | 14 (related interprocedural flow) | 10 (related alias analysis) | 12 |
| [Context-Sensitive (k-CFA / Call-String) Points-To Analysis](examples/context_points_to/README.md) | 14 *(related)* | 10 *(related)* | 12 |
| [Semi-Naive Datalog Engine and ROBDD Relational Solver](examples/datalog_bdd/README.md) | — | — | 12 |

## Coverage gaps

This is a concept companion with independently written programs, inputs, and explanations. Core algorithms across all editions' compiler pipeline chapters (Chapters 3–12 and Appendices A–B) are implemented in both Rust and Python, excluding textbook listings, figures, and exercise solutions. The `.l`/`.y` DSL generators, `Appendix A` front-end architectures, and `§10.4–10.5` instruction-level scheduling techniques are covered in self-contained form by [`lex_generator`](examples/lex_generator/README.md), [`yacc_generator`](examples/yacc_generator/README.md), [`pascal_frontend`](examples/pascal_frontend/README.md), [`complete_frontend`](examples/complete_frontend/README.md), [`rust_frontend`](examples/rust_frontend/README.md), [`python_frontend`](examples/python_frontend/README.md), [`tiny_compiler`](examples/tiny_compiler/README.md), [`trace_schedule`](examples/trace_schedule/README.md), [`hyperblock_predication`](examples/hyperblock_predication/README.md), and [`software_pipeline`](examples/software_pipeline/README.md).

- **Historical survey chapters.** Compiler history and architectural surveys (`1977 Green` Ch. 1–2 language survey and Appendix B; `1986 Red` Ch. 11–12 historical compiler notes) have no standalone demos.
- **External C-source emission.** Each DSL generator and front end compiles specs directly into in-memory DFA/LALR(1) tables or three-address code interpreters rather than emitting standalone `.c`/`.h` toolchain files or full POSIX `yacc`/`flex` start-condition ecosystems.

## Checks

Every example provides matching deterministic Rust and Python demonstrations plus unit tests checking accepting and rejecting parser inputs, short-circuit traces, analysis fixed points, and semantic preservation across code transformations.

```sh
cargo fmt --all -- --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --all-targets --locked
python3 -m unittest examples/*/main.py
```
