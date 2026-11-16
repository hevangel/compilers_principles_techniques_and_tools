# Dragon Book Edition Coverage Review & Agent Implementation Guide

This review audits [the Dragon Book in Rust companion](../README.md) against the local Dragon Book textbook transcriptions in [`learning/textbooks/DragonBook`](../../textbooks/DragonBook) and converts every identified coverage gap and metadata mismatch into self-contained, agent-executable implementation specifications.

---

## 1. Executive Summary

- **Current baseline:** [Cargo.toml](../Cargo.toml) (`dragon-book-rust`, Rust edition `2021`, `rust-version = "1.85"`, zero external crates) auto-discovers **21 standalone examples** under `examples/<name>/main.rs`, all paired with `examples/<name>/README.md`, inline `#[test]` cases, [README.md](../README.md), and [index.html](../index.html):
  - **Start here (1):** `tiny_compiler`
  - **Scanning and parsing (5):** `lexer`, `subset_dfa`, `ll1`, `slr`, `recovery`
  - **Translation and semantic analysis (5):** `attributes`, `scope_types`, `unification`, `array_layout`, `backpatch`
  - **Runtime and code generation (3):** `frames`, `mark_sweep`, `register_codegen`
  - **Optimization and program analysis (7):** `value_numbering`, `liveness`, `reaching_defs`, `natural_loops`, `list_schedule`, `loop_locality`, `points_to`
- **Core finding:** The 21 existing demos provide clean, working introductions along each compiler phase, while leaving **28 distinct algorithms and formalisms (`DB-01` through `DB-28`)** unimplemented across scanning/parsing, semantic translation, SSA/IR, runtime GC/calling sequences, target code generation, scalar/loop optimization, parallelism/locality, and interprocedural/Datalog/BDD analysis. The single highest-value missing intermediate representation is **Static Single-Assignment (SSA) form** (`2006 §6.2.4`).
- **Two verified metadata and source discrepancies:**
  1. **`examples/unification` omits its 2006 Second Edition mapping:** [`examples/unification/README.md`](../examples/unification/README.md#L37-L39), [`README.md`](../README.md#L55), and [`index.html`](../index.html#L156) list `—` under `2006 Purple`, even though Chapter 6 of the 2006 edition explicitly covers type inference and unification in `§6.5.4`–`§6.5.5` ([`Compilers_2nd_Ed_2006.md`](../../textbooks/DragonBook/Compilers_2nd_Ed_2006.md#L10707-L10886)), as does Chapter 6 of the 2014 New International Edition ([`Compilers_New_Intl_Ed_2014.md`](../../textbooks/DragonBook/Compilers_New_Intl_Ed_2014.md#L31253-L31622)).
  2. **`Compilers_New_Intl_Ed_2014.md` intentionally omits Chapter 12 while keeping stale cross-references:** The Table of Contents ([`L83-L144`](../../textbooks/DragonBook/Compilers_New_Intl_Ed_2014.md#L83-L144)) jumps directly from Chapter 11 (book p. 769) to Appendix A (book p. 903, [`L70181`](../../textbooks/DragonBook/Compilers_New_Intl_Ed_2014.md#L70181)), while two sentences inside Chapter 9 still refer to `Section 12.4` ([`L56169`](../../textbooks/DragonBook/Compilers_New_Intl_Ed_2014.md#L56169)) and `Chapter 12` ([`L56181`](../../textbooks/DragonBook/Compilers_New_Intl_Ed_2014.md#L56181)). Treat 2014 as matching 2006 Chapters 1–11 + Appendices A–B (with Chapter 12 dropped by the publisher).

---

## 2. Agent Implementation Protocol & Repository Contract

An AI agent adding examples or fixing metadata in `learning/compilers_principles_techniques_and_tools` must follow these observed repository contracts:

### 2.1 Zero-Dependency Crate & Discovery Rules
- Keep [`Cargo.toml`](../Cargo.toml) dependency-free (`std` only; no `[dependencies]`, `[dev-dependencies]`, or `[[example]]` blocks). Cargo auto-discovers every `examples/<snake_case_name>/main.rs`.
- Use only Rust standard library types supported cleanly on MSRV `1.85` (`2021` edition).
- **Determinism:** Prefer `std::collections::{BTreeMap, BTreeSet}` over `HashMap`/`HashSet` so iteration order, state numbering, constraints, and printed tables never vary across runs.

### 2.2 Per-Example File Anatomy
Every work item `DB-XX` creates two files under `examples/<name>/`:

1. **`examples/<name>/main.rs`** (follow [`examples/subset_dfa/main.rs`](../examples/subset_dfa/main.rs) and [`examples/unification/main.rs`](../examples/unification/main.rs)):
   - Start on line 1 with a single-sentence `//!` module doc comment summarizing the technique.
   - Define cohesive domain structs/enums with clear, self-contained pure functions (no global mutable state, no filesystem/network I/O).
   - Provide `fn main()` printing a small, readable worked trace on realistic sample input.
   - Include inline `#[test]` functions (or `#[cfg(test)] mod tests`) asserting happy paths, edge/boundary cases, and invalid-input/error diagnostics.
   - Develop original code, types, sample grammars/CFGs, and explanations; **never copy textbook prose, figures, code listings, or exercise solutions**.

2. **`examples/<name>/README.md`** (follow [`examples/subset_dfa/README.md`](../examples/subset_dfa/README.md) and [`examples/points_to/README.md`](../examples/points_to/README.md)):
   - `# <Title>` followed by a one-sentence lead and `See the [Rust source](main.rs).`
   - A `sh` block with `cargo run --example <name>` and `cargo test --example <name>`.
   - `## What the example teaches` (mechanism overview, `###` step-by-step subsections, small ASCII/Markdown trace table).
   - `## What to look for` (summary of printed `main()` output and tested invariants).
   - `## Check your understanding` (2 original self-test questions; do not reproduce textbook exercises).
   - `## Edition mapping and scope` containing the three-column Markdown table (`| 1977 Green | 1986 Red | 2006 Purple |`), the standard reference to `[edition guide](../../README.md#edition-guide)`, and a closing scope sentence stating exact simplifications/exclusions.

### 2.3 Manual Documentation & HTML Sync Checklist
Unlike sister repositories with JSON generators, **this repository maintains [`README.md`](../README.md) and [`index.html`](../index.html) directly**. Whenever adding an example `examples/<name>/`:

1. Update the total count in [`README.md#L3`](../README.md#L3) (`21 original Rust demonstrations...`) and [`index.html#L38`](../index.html#L38) (`21 original programs...`).
2. Add a table row `| [<Title>](examples/<name>/README.md) | <1977> | <1986> | <2006> |` under the matching section heading in [`README.md`](../README.md#L27-L81).
3. Update [`README.md#coverage-gaps`](../README.md#L83-L91) if adding the example closes a gap currently listed there.
4. Add a matching `<article>` card inside the corresponding `<section>` (`#start`, `#front-end`, `#translation`, `#runtime`, `#optimization`) of [`index.html`](../index.html#L51-L293) matching this exact markup structure:
   ```html
        <li><article>
          <h3><a href="examples/<name>/README.md"><Title></a></h3>
          <p><One-sentence card summary.></p>
          <dl aria-label="Related chapters by edition">
            <dt>1977 Green</dt><dd><1977></dd>
            <dt>1986 Red</dt><dd><1986></dd>
            <dt>2006 Purple</dt><dd><2006></dd>
          </dl>
          <p><a href="examples/<name>/main.rs" aria-label="Rust source for <Title>">Rust source</a></p>
        </article></li>
   ```

### 2.4 Required Verification Commands
Run all three repository CI gates (documented in [`README.md#L96-L99`](../README.md#L96-L99)) plus the per-example run check from `learning/compilers_principles_techniques_and_tools`:

```sh
cargo run --example <name>
cargo test --example <name>
cargo fmt --all -- --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --all-targets --locked
```

---

## 3. Local Textbook Markdown Corpus Audit & Exact Line-Number Index

Three Dragon Book transcriptions exist locally under `learning/textbooks/DragonBook` (cataloged in [`learning/textbooks/README.md`](../../textbooks/README.md)). Use **2006 Purple** (`Compilers_2nd_Ed_2006.md`) as the primary structural lookup because it has clean Markdown `##` / `###` / `####` headings and zero OCR corruption on algorithms.

| Volume | Local File | Lines | Structure & Quality Notes |
| --- | --- | --- | --- |
| **1977 Green** (*Principles of Compiler Design*) | *None locally* | — | No local Markdown file in `textbooks/DragonBook/`. Existing repo mappings to 1977 chapters are tracked in `README.md` / `index.html`. |
| **1986 Red** (1st Edition) | [`Compilers_1st_Ed_1986.md`](../../textbooks/DragonBook/Compilers_1st_Ed_1986.md) | 20,265 | OCR from DjVu; `<!-- page N -->` markers (PDF page ≈ book page + 14); frequent spacing/symbol OCR noise; **includes 1986-unique Ch 11, Ch 12, and Pascal App A**. |
| **2006 Purple** (2nd Edition) | [`Compilers_2nd_Ed_2006.md`](../../textbooks/DragonBook/Compilers_2nd_Ed_2006.md) | 26,410 | Clean `##` chapter and `###`/`####` section headings; full Ch 1–12 and Appendices A–B. |
| **2014 New Intl Ed** | [`Compilers_New_Intl_Ed_2014.md`](../../textbooks/DragonBook/Compilers_New_Intl_Ed_2014.md) | 79,372 | Clean `##` chapters matching 2006 Ch 1–11 and Appendices A–B; **omits Chapter 12**; occasional OCR typos (e.g., `Unication` at `L31409`). |

### Exact Chapter & Appendix Line Boundaries

| Section | `Compilers_1st_Ed_1986.md` | `Compilers_2nd_Ed_2006.md` | `Compilers_New_Intl_Ed_2014.md` |
| --- | --- | --- | --- |
| Front Matter / TOC | `L1–L552` (TOC `L95–L660`) | `L1–L258` (TOC `L139–L258`) | `L1–L144` (TOC `L83–L144`) |
| Chapter 1 | `L553–L1082` (`p. 1`) | `L259–L1172` | `L145–L2898` |
| Chapter 2 | `L1083–L2444` (`p. 25`) | `L1173–L3026` | `L2899–L8880` |
| Chapter 3 | `L2445–L3806` (`p. 83`) | `L3027–L5244` | `L8881–L15212` |
| Chapter 4 | `L3807–L6666` (`p. 159`) | `L5245–L8332` | `L15213–L24368` |
| Chapter 5 | `L6667–L8056` (`p. 279`) | `L8333–L9728` | `L24369–L28256` |
| Chapter 6 | `L8057–L9242` (`p. 343`, Type Checking) | `L9729–L11688` (IR + Type Checking) | `L28257–L33966` (IR + Type Checking) |
| Chapter 7 | `L9243–L10830` (`p. 389`) | `L11689–L13602` | `L33967–L39384` |
| Chapter 8 | `L10831–L12114` (`p. 463`, IR) | `L13603–L15596` (CodeGen) | `L39385–L45294` (CodeGen) |
| Chapter 9 | `L12115–L13976` (`p. 513`, CodeGen) | `L15597–L18652` (Opt & Dataflow) | `L45295–L55736` (Opt & Dataflow) |
| Chapter 10 | `L13977–L16506` (`p. 585`, Code Opt) | `L18653–L20118` (ILP) | `L55737–L60056` (ILP) |
| Chapter 11 | `L16507–L16714` (`p. 723`, Want to Write?) | `L20119–L23470` (Parallelism & Locality) | `L60057–L70180` (Parallelism & Locality) |
| Chapter 12 | `L16715–L16930` (`p. 733`, Case Studies) | `L23471–L24972` (Interprocedural Analysis) | *Omitted by publisher* (`L56169, L56181` stale refs) |
| Appendix A | `L16931–L17030` (`p. 745`, Pascal Project) | `L24973–L25560` (Complete Front End) | `L70181–L74508` (Complete Front End) |
| Appendix B | *None* (Bibliography `L17031`, Index `L17253`) | `L25561–L25638` (Linearly Indep. Solns) | `L74509–L74812` (Linearly Indep. Solns) |

---

## 4. Edition-by-Edition Chapter & Appendix Crosswalk

Note how topics reorganized between **1986 Red** and **2006 Purple** (and why 2006 Type Checking moved into Chapter 6, pushing Intermediate Code + Type Checking into one chapter, CodeGen from Ch 9 → Ch 8, and Optimization from Ch 10 → Ch 9):

| Topic Domain | 1977 Green *(per README)* | 1986 Red (`1st Ed`) | 2006 Purple (`2nd Ed`) | 2014 New Intl | Existing Rust Demos | Priority Gaps (`DB-XX`) |
| --- | --- | --- | --- | --- | --- | --- |
| Compiler architecture & tiny front end | Ch 1–2 | Ch 1–2 (`§2.1–2.9`) | Ch 1–2 (`§1.1–1.6`, `§2.1–2.8`) | Ch 1–2 | `tiny_compiler`, `lexer` | Covered conceptually; App A capstone |
| Lexical analysis, NFA/DFA, Lex, minimization | Ch 3 | Ch 3 (`§3.1–3.9`) | Ch 3 (`§3.1–3.9`) | Ch 3 | `lexer`, `subset_dfa` | `DB-01`, `DB-02` |
| CFGs, LL, LR/SLR, LALR, Yacc, error recovery | Ch 4–6, 11 | Ch 4 (`§4.1–4.9`, incl. `§4.6` op-prec) | Ch 4 (`§4.1–4.9`, no op-prec) | Ch 4 | `ll1`, `slr`, `recovery` | `DB-03`, `DB-04`, `DB-05` |
| Syntax-directed definitions & translation schemes | Ch 7 | Ch 5 (`§5.1–5.10`) | Ch 5 (`§5.1–5.5`) | Ch 5 | `attributes` | `DB-06`, `DB-07` |
| IR forms, DAGs, **SSA**, arrays, control/backpatching | Ch 2, 7–8 | Ch 8 (`§8.1–8.7`) | Ch 6 (`§6.1–6.4`, `§6.6–6.9`) | Ch 6 | `array_layout`, `backpatch` | **`DB-08` (SSA)**, `DB-11`, `DB-12` |
| Scope, types, overloading, coercions, **unification** | Ch 2, 9, 11 | Ch 6 (`§6.1–6.7`) | Ch 6 (`§6.3.2`, `§6.5.1–6.5.5`) | Ch 6 | `scope_types`, `unification` *(fix `2006` map)* | `DB-09`, `DB-10` |
| Activation records, static links, display, heap, **GC** | Ch 2, 10 | Ch 7 (`§7.1–7.9`) | Ch 7 (`§7.1–7.8`) | Ch 7 | `frames`, `mark_sweep` | `DB-13`, `DB-14`, `DB-15` |
| Instruction selection, peephole, regalloc, DP tiling | Ch 15 | Ch 9 (`§9.1–9.12`) | Ch 8 (`§8.1–8.11`) | Ch 8 | `register_codegen`, `value_numbering` | `DB-16`, `DB-17`, `DB-18` |
| Dataflow frameworks, CSE, const-prop, PRE, loops, regions | Ch 12–14 | Ch 10 (`§10.1–10.13`) | Ch 9 (`§9.1–9.8`) | Ch 9 | `value_numbering`, `liveness`, `reaching_defs`, `natural_loops` | `DB-19`, `DB-20`, `DB-21`, `DB-22`, `DB-23` |
| Instruction-level parallelism (list & global sched, pipelining) | — | *None* | Ch 10 (`§10.1–10.5`) | Ch 10 | `list_schedule` | `DB-24` |
| Affine loops, dependence tests, locality, partitioning | — | *None* | Ch 11 (`§11.1–11.11`) | Ch 11 | `loop_locality` | `DB-25`, `DB-26` |
| Interprocedural points-to, **$k$-CFA**, Datalog, BDDs | Ch 14 *(rel.)* | Ch 10 *(rel.)* | Ch 12 (`§12.1–12.7`) | *Omitted* | `points_to` | `DB-27`, `DB-28` |
| Pragmatic compiler engineering & historical compilers | App A *(part)* | Ch 11–12 | *Dropped* | *Dropped* | — | Out of algorithmic scope (narrative/history) |
| Front-end project language | App B *(per README)* | App A (Pascal subset) | App A (Front-end AST/IR) | App A | `tiny_compiler` *(thematic)* | Keep small end-to-end scope |
| Linear-independence / column-echelon algorithm | — | *None* | App B | App B | — | Folded into `DB-26` (or standalone) |

---

## 5. Detailed Section-by-Section Gap Analysis

Legend: **Covered** (dedicated working Rust example), **Partial** (introductory subset shown; major algorithmic variant missing), **Missing** (no dedicated demo), **Narrative** (prose/historical survey without standalone algorithm).

### Chapters 1–2: Introduction & One-Pass Syntax-Directed Translator (`2006 L259–L3026`, `1986 L553–L2444`)
- **Covered / Narrative:** Compiler phases (`§1.1–1.2`), predictive parsing & simple translator (`§2.4–2.5`), hand lexer (`§2.6`), chained symbol table (`§2.7`), and stack bytecode (`§2.8`) are represented conceptually by `tiny_compiler`, `lexer`, and `scope_types`.

### Chapter 3: Lexical Analysis (`2006 L3027–L5244`, `1986 L2445–L3806`)
- `§3.1–3.4` Token attributes, longest match, transition diagrams (`L3029–L3892`): **Covered** (`lexer`). Wait-free sentinel double buffering (`§3.2`, `L3207–L3264`) is **Missing** (low priority; `tiny`/`&str` slices suffice in Rust).
- `§3.5, §3.8` Lex specification & scanner-generator architecture (`L3893–L4108`, `L4635–L4776`): **Missing** (multi-pattern priority & trailing lookahead operator `/`).
- `§3.6–3.7` Thompson NFA construction from regex & Subset Construction (`L4109–L4634`): **Partial** (`subset_dfa` takes pre-built NFA edges; regex parser + Thompson construction `§3.7.4` [`L4447–L4562`] are **Missing** → **`DB-01`**).
- `§3.9` Direct regex-to-DFA (`nullable`/`firstpos`/`lastpos`/`followpos`, `L4777–L4950`) & Hopcroft DFA state minimization + state-table compression (`L4951–L5126`): **Missing** → **`DB-01`**, **`DB-02`**.

### Chapter 4: Syntax Analysis (`2006 L5245–L8332`, `1986 L3807–L6666`)
- `§4.1–4.2, §4.4.1–4.4.4` CFGs, `FIRST`/`FOLLOW`, LL(1) table construction (`L5263–L5772`, `L6037–L6250`): **Covered** (`ll1`).
- `§4.1.4, §4.4.5, §4.8.3` Error recovery (`L5343–L5396`, `L6251–L6402`, `L7719–L7846`): **Partial** (`recovery` shows panic-mode semicolon sync; phrase-level / LL synchronizing set computation from `FOLLOW` is not generalized).
- `§4.3` Grammar rewriting — ambiguity elimination, immediate/indirect left-recursion elimination (`§4.3.3 L5851–L5902`), left factoring (`§4.3.4 L5903–L5932`): **Missing** → **`DB-03`**.
- `§4.5–4.6` Shift-reduce mechanics, LR(0) items, SLR(1) table (`L6403–L7120`): **Covered** (`slr`).
- `1986 §4.6` Operator-precedence parsing (`1986 L460–L470`, dropped in 2006) & `2006 §4.8.1–4.8.2` Ambiguous grammar precedence/associativity + dangling-else resolution (`L7629–L7718`): **Missing** → **`DB-05`**.
- `§4.7` Canonical LR(1) (`§4.7.1–4.7.3 L7155–L7330`), LALR(1) state merging & efficient lookahead propagation (`§4.7.4–4.7.5 L7331–L7542`), table compaction (`§4.7.6 L7543–L7602`): **Missing** → **`DB-04`**.
- `§4.9` Yacc parser-generator mechanics (`L7847–L8164`): **Missing** (precedence-driven LALR conflict resolution covered by `DB-04` + `DB-05`).

### Chapter 5: Syntax-Directed Translation (`2006 L8333–L9728`, `1986 L6667–L8056`)
- `§5.1, §5.4–5.5` Inherited & synthesized attributes, S-/L-attributed classification (`L8351–L8500`, `L8899–L9652`): **Partial** (`attributes` evaluates fixed inherited origin + synthesized width on a tiny tree).
- `§5.2` Attribute dependency graphs, topological evaluation ordering, cycle detection (`L8501–L8722`): **Missing** → **`DB-06`**.
- `§5.3` AST vs DAG construction (`§5.3.1–5.3.2 L8723–L8898`): **Missing** → **`DB-07`**.
- `§5.4.4–5.5.4` Left-recursion removal inside SDTs & bottom-up marker insertion for L-attributed SDDs (`L9009–L9652`): **Partial** (can be shown inside `DB-03` / `DB-06`).

### Chapter 6: Intermediate-Code Generation & Type Checking (`2006 L9729–L11688`, `1986 Ch 6 L8057–L9242 & Ch 8 L10831–L12114`)
- `§6.1` Value-number method for expression DAGs (`§6.1.2 L9835–L9886`): **Partial** (`value_numbering` works on flat instructions inside one basic block; tree/DAG hash-consing & cycle/copy rules belong in **`DB-07`**).
- `§6.2.1–6.2.3` Three-address code, quadruples, triples, indirect triples (`L9905–L10038`): **Missing** → **`DB-12`**.
- **`§6.2.4` Static Single-Assignment (SSA) Form (`L10039–L10064`)**: **Missing — highest-priority IR gap** → **`DB-08`** ($\phi$-functions via dominance frontiers & stack renaming).
- `§6.3.1–6.3.6` Types, structural vs name equivalence (`§6.3.2 L10129–L10160`), local storage offsets & nested record/class field layouts (`§6.3.4–6.3.6 L10163–L10316`): **Partial** (`scope_types` only models flat scalar `Int`/`Bool`; structural type equivalence & record/union offsets are **Missing** → **`DB-09`**, **`DB-12`**).
- `§6.4` Expressions & row-major array addressing (`L10353–L10576`): **Covered** (`array_layout`, though dope vectors/incremental subexpression codegen are light).
- `§6.5.2–6.5.3` Type coercions / widening (`L10609–L10676`) and overloaded operator/function resolution (`L10677–L10706`): **Missing** → **`DB-10`**.
- `§6.5.4–6.5.5` Polymorphic type inference (`Algorithm 6.16 L10707–L10812`) & unification (`L10813–L10886`): **Partial** (`unification` implements first-order term unification + occurs check, misses `let`-generalization/fresh instantiation and its `2006 Purple` credit is currently `—`!) → **`DB-09`** + **Section 7 fix**.
- `§6.6–6.7.3` Short-circuit boolean translation & forward backpatching (`L10911–L11396`): **Covered** (`backpatch`).
- `§6.7.4, §6.8` `break`/`continue`/`goto` backpatching & `switch` lowering (jump table vs decision tree vs sequential tests, `L11397–L11562`): **Missing** → **`DB-11`**.
- `§6.9` Procedure call/return IR (`L11563–L11606`): **Partial** (modeled in frame layout rather than explicit IR emission).

### Chapter 7: Run-Time Environments (`2006 L11689–L13602`, `1986 L9243–L10830`)
- `§7.1–7.3.7` Stack allocation, activation trees/records, caller/callee calling sequences, nesting depth & static access links (`L11729–L12250`): **Partial** (`frames` traces static access links; variable-length arrays on stack `§7.2.4` and **`§7.3.8` Lexical Display** `L12251–L12300` are **Missing** → **`DB-13`**).
- `§7.4` Heap free-list management, coalescing, first/best-fit fragmentation (`L12301–L12562`): **Missing** (can optionally complement `DB-14` or standalone).
- `§7.5.3` Reference counting (`L12653–L12724`) & **`§7.6.1–7.6.3` Mark-and-sweep** (`L12727–L12882`): **Partial** (`mark_sweep` implements basic stop-the-world trace & sweep; reference counting cycle failure is **Missing** → **`DB-15`**).
- `§7.6.4–7.6.5` **Mark-Compact & Cheney Copying Collectors** (`L12883–L13024`): **Missing** → **`DB-14`**.
- `§7.7–7.8` Incremental tri-color marking, write barriers, generational collection, Train algorithm, conservative/weak refs (`L13069–L13496`): **Missing** → **`DB-15`**.

### Chapter 8: Code Generation (`2006 L13603–L15596`, `1986 Ch 9 L12115–L13976`)
- `§8.1–8.4` Target machine cost model, basic blocks, next-use backward scan (`§8.4.2 L14157–L14186`): **Partial** (liveness exists separately in Ch 9; integrated basic-block next-use + codegen is **Missing** → **`DB-16`**).
- `§8.5` Basic-block DAG optimization — CSE, DCE, algebraic identities, array/pointer aliases, reassembly (`L14301–L14536`): **Partial** (`value_numbering` covers pure local CSE; algebraic/alias barriers belong in **`DB-07`**).
- **`§8.6` Register & Address Descriptors + `getReg` Code Generator (`L14537–L14744`)**: **Missing** → **`DB-16`**.
- `§8.7` Peephole Optimization (redundant load/store, unreachable jumps, jump-to-jump, strength reduction, machine idioms, `L14745–L14864`): **Missing** → **`DB-18`**.
- **`§8.8` Global Register Allocation (`§8.8.2–8.8.4 L14865–L14978`, usage counts & graph coloring)**: **Missing** → **`DB-17`**.
- `§8.9, §8.11` Instruction selection by bottom-up tree rewriting (`L14979–L15188`) & Aho-Johnson dynamic programming (`L15363–L15492`): **Missing** → **`DB-18`**.
- `§8.10` Ershov numbers & optimal tree codegen (`L15189–L15362`): **Partial** (`register_codegen` computes Ershov register needs; finite-register spilling `§8.10.3 L15283–L15362` is not demonstrated).

### Chapter 9: Machine-Independent Optimizations (`2006 L15597–L18652`, `1986 Ch 10 L13977–L16506`)
- `§9.1.4–9.1.8` Global CSE, copy propagation, DCE, LICM, induction-variable strength reduction (`L15685–L15874`): **Missing** → **`DB-19`**, **`DB-22`**.
- `§9.2.4–9.2.5` Reaching definitions & live variables (`L15963–L16182`): **Covered** (`reaching_defs`, `liveness`).
- `§9.2.6` Available expressions (`L16183–L16236`): **Missing** → **`DB-19`**.
- `§9.3–9.4` Meet-semilattices, MOP vs MFP, monotone frameworks & **Constant Propagation** (`L16399–L17000`, proving nondistributivity!): **Missing** → **`DB-20`**.
- `§9.5` **Partial Redundancy Elimination (PRE) via Lazy Code Motion** (`L17001–L17440`, 4 bidirectional/unidirectional passes): **Missing** → **`DB-21`**.
- `§9.6` Dominators, DFS spanning tree, reducibility, natural loops (`L17441–L17864`): **Covered** (`natural_loops` demonstrates dominators & back-edge loops; postorder/depth metric is omitted).
- `§9.7` **Region-Based Data-Flow Analysis** (`L17865–L18216`): **Missing** → **`DB-23`**.
- `§9.8` Symbolic analysis & affine value ranges (`L18217–L18502`): **Missing** (lower priority; complements `DB-23` / `DB-25`).
- `1986 §10.12–10.13` Type estimation & symbolic debugging of optimized code (`1986 L567–L572`): **Missing / Narrative**.

### Chapter 10: Instruction-Level Parallelism (`2006 L18653–L20118`, not in 1986)
- `§10.1–10.3` Resource/latency constraints, dependence DAG, list scheduling (`L18677–L19114`): **Covered** (`list_schedule` implements basic-block fixed-latency list scheduling with cycle detection).
- `§10.4` Global code scheduling — upward/downward motion across branches, compensation code, trace/superblock scheduling (`L19115–L19400`): **Missing** (defer unless expanding ILP track beyond `DB-24`).
- `§10.5` **Software pipelining (acyclic/cyclic modulo scheduling + modular variable expansion)** (`L19401–L20036`): **Missing** → **`DB-24`**.

### Chapter 11: Optimizing for Parallelism and Locality (`2006 L20119–L23470`, not in 1986)
- `§11.1–11.2, §11.10` Matrix multiply locality intuition & loop interchange (`L20153–L20494`, `L22959–L23234`): **Partial** (`loop_locality` compares hard-coded `i,j,k` vs `i,k,j` without legality proof).
- `§11.3, §11.5–11.6` Convex polytope iteration spaces, Fourier-Motzkin elimination (`L20495–L20824`), reuse vectors (`L20907–L21158`), **GCD test & array data-dependence analysis** (`L21159–L21522`): **Missing** → **`DB-25`**.
- `§11.7–11.9, §11.10–11.11` Affine partitioning, synchronization-free parallelism (`L21523–L22160`), Farkas wavefront pipelining (`L22363–L22888`), loop tiling/skewing/contraction: **Missing** → **`DB-26`**.

### Chapter 12: Interprocedural Analysis (`2006 L23471–L24972`, omitted in 2014)
- `§12.1–12.2, §12.4–12.5` Flow-insensitive / context-insensitive Andersen-style inclusion points-to & call graphs (`L23489–L23862`, `L24181–L24488`): **Partial** (`points_to` solves shared-node inclusion constraints; Datalog encoding & dynamic virtual call-graph discovery are absent).
- **`§12.3` Datalog bottom-up semi-naive fixed-point evaluator (`L23863–L24180`)**: **Missing** → **`DB-28`**.
- **`§12.6` Context-sensitive $k$-CFA / call-string pointer & taint analysis (`L24489–L24632`)**: **Missing** → **`DB-27`**.
- **`§12.7` Binary Decision Diagrams (ROBDD) relational engine (`L24633–L24826`)**: **Missing** → **`DB-28`**.

### 1986 Chapters 11–12 & Appendices A–B
- `1986 Ch 11–12` Want to Write a Compiler? / A Look at Some Compilers (`1986 L16507–L16930`): **Narrative** (methodology & historical surveys of EQN, Pascal, C, Fortran H, BLISS/11, Modula-2).
- `1986 App A` (`L16931–L17030`, Pascal subset) & `2006 App A` (`L24973–L25560`, complete Java front-end AST/IR): **Thematically referenced** by `tiny_compiler`; neither full source language is required verbatim.
- `2006 App B` Finding Linearly Independent Solutions (`L25561–L25638`): **Missing** → paired into **`DB-26`** (exact rational column-echelon reduction supporting affine dependence/partitioning).

---

## 6. Concrete Work-Item Backlog & Module Specifications

Implement work items independently in priority order (`P1` → `P2` → `P3`). Every card specifies the target path, exact textbook anchors, required Rust types/functions, and minimum unit-test invariants.

---

### Track A: Scanning, Parsing & Syntax-Directed Translation

#### `DB-01` [P1] Regex-to-NFA (Thompson) & Direct Regex-to-DFA
- **Target:** `examples/regex_automata/main.rs` + `README.md`
- **Edition Mapping:** `1977 Green: 3 | 1986 Red: 3 | 2006 Purple: 3` (`§3.7.4 L4447–L4562`, `§3.9.1–3.9.5 L4777–L4950`)
- **Algorithm & Rust Design:**
  - Parse a small ASCII regex syntax (literals, concatenation, `|`, `*`, `+`, `?`, parentheses) into an explicit `enum RegexNode { Empty, Literal(u8, usize), Cat(..), Alt(..), Star(..), EndMarker(usize) }`.
  - Provide two compilers from the same parsed AST:
    1. `to_thompson_nfa(&RegexNode) -> Nfa` producing $\varepsilon$-transitions and literal transitions.
    2. `compile_direct_dfa(&RegexNode) -> Dfa` computing `nullable`, `firstpos`, `lastpos`, and `followpos` (`BTreeMap<usize, BTreeSet<usize>>`) via postorder AST traversal with appended `EndMarker`, then building deterministic state rows directly from `followpos`.
  - Keep `Dfa` matching compatible in style with `examples/subset_dfa/main.rs`.
- **Verification Invariants (`main` & `#[test]`):**
  - Assert that Thompson NFA simulation and direct DFA accept the exact same strings on `(a|b)*abb`, `a(b|c)*`, optional/empty matches, and reject malformed regexes cleanly with `Result::Err`.

#### `DB-02` [P1] DFA State Minimization (Hopcroft Partition Refinement)
- **Target:** `examples/dfa_minimize/main.rs` + `README.md`
- **Edition Mapping:** `1977 Green: 3 | 1986 Red: 3 | 2006 Purple: 3` (`§3.9.6–3.9.7 L4951–L5074`)
- **Algorithm & Rust Design:**
  - `struct Dfa { start: usize, alphabet: Vec<u8>, accepting: BTreeSet<usize>, trans: Vec<Vec<usize>> }`
  - `fn prune_unreachable(dfa: &Dfa) -> Dfa`: BFS from `start`.
  - `fn minimize(dfa: &Dfa) -> (Dfa, Vec<BTreeSet<usize>>)`: initial partition $\Pi_0 = \{F, S \setminus F\}$ (dropping empty blocks), iteratively split each group $G \in \Pi$ whenever two states $s, t \in G$ map to different partition groups under some symbol $a$, using deterministic `BTreeSet` block IDs until $\Pi_{k+1} = \Pi_k$.
- **Verification Invariants (`main` & `#[test]`):**
  - Reduce an intentionally inflated unminimized DFA (such as the 5-to-4-state `(a|b)*abb` table or redundant equivalent branches) to its minimal quotient state count.
  - Assert `original.accepts(w) == minimized.accepts(w)` across all strings of length $\le 6$, plus edge cases (single-state all-accept/all-reject DFAs, unreachable sink states).

#### `DB-03` [P2] Grammar Rewriting: Left Recursion Elimination & Left Factoring
- **Target:** `examples/grammar_rewrite/main.rs` + `README.md`
- **Edition Mapping:** `1977 Green: 4–5 | 1986 Red: 4 | 2006 Purple: 4` (`§4.3.3–4.3.4 L5851–L5932`)
- **Algorithm & Rust Design:**
  - `struct Cfg { nonterminals: Vec<String>, start: String, prods: BTreeMap<String, Vec<Vec<Symbol>>> }`
  - `fn eliminate_left_recursion(cfg: &Cfg) -> Cfg`: ordered $A_1 \dots A_n$ substitution (`Algorithm 4.19`) replacing indirect $A_i \to A_j \gamma$ ($j < i$), then replacing direct left recursion $A \to A\alpha \mid \beta$ with fresh `A'` productions $A \to \beta A'$ and $A' \to \alpha A' \mid \varepsilon$.
  - `fn left_factor(cfg: &Cfg) -> Cfg`: iteratively extract longest common non-empty prefixes among alternatives of each nonterminal into fresh tail nonterminals.
- **Verification Invariants (`main` & `#[test]`):**
  - Transform standard indirect left-recursive grammars (e.g. $S \to A a \mid b$, $A \to S d \mid \varepsilon$) and dangling-else prefix grammars so zero direct/indirect left recursion remains and common prefixes are factored cleanly; reject malformed empty/epsilon-cycle inputs safely or verify bounded language preservation up to length $k$.

#### `DB-04` [P1] Canonical LR(1) & LALR(1) Parser Generator + Table Compaction
- **Target:** `examples/lalr/main.rs` + `README.md`
- **Edition Mapping:** `1977 Green: 5–6 | 1986 Red: 4 | 2006 Purple: 4` (`§4.7.1–4.7.6 L7155–L7602`)
- **Algorithm & Rust Design:**
  - Extend the pattern of `examples/slr/main.rs` with LR(1) items: `struct Lr1Item { prod: usize, dot: usize, lookahead: Term }`.
  - Implement `closure_lr1` (using precomputed `FIRST` over $\beta a$), `goto_lr1`, canonical LR(1) collection, and LALR(1) merging (`merge_cores`: union lookaheads across states sharing identical LR(0) cores `(prod, dot)`).
  - Detect exact shift/reduce and reduce/reduce conflicts separately for SLR(1), LALR(1), and LR(1).
  - Include compact row-displacement or default-action reduction (`§4.7.6`) reporting compacted vs dense table sizes.
- **Verification Invariants (`main` & `#[test]`):**
  - Use the textbook LR(1)/LALR(1) vs SLR(1) discriminating lvalue/pointer grammar ($S \to L = R \mid R$, $L \to *R \mid \mathbf{id}$, $R \to L$): assert SLR reports a conflict while LALR(1) and LR(1) parse cleanly.
  - Also test a grammar where LALR(1) merges two valid LR(1) states into a reduce/reduce conflict to demonstrate the LR(1) > LALR(1) strict hierarchy!

#### `DB-05` [P3] Ambiguous Precedence/Associativity Conflict Resolution
- **Target:** `examples/precedence_parse/main.rs` + `README.md`
- **Edition Mapping:** `1977 Green: 5–6 | 1986 Red: 4 (§4.6, §4.8) | 2006 Purple: 4` (`§4.8.1–4.8.2 L7629–L7718`)
- **Algorithm & Rust Design:**
  - Build a shift-reduce driver for the inherently ambiguous grammar $E \to E + E \mid E * E \mid -E \mid (E) \mid \mathbf{id}$ parameterized by a precedence/associativity table `BTreeMap<Op, (u8, Assoc)>` (`Assoc::{Left, Right, NonAssoc}`) plus `%prec` unary precedence override and explicit "shift beats reduce" rule for dangling-else.
- **Verification Invariants (`main` & `#[test]`):**
  - Parse `id + id * id`, `id ^ id ^ id` (right-assoc), `id - id - id` (left-assoc), and nested `if c1 then if c2 then s1 else s2` confirming exact AST parenthesization and non-associative error rejection (`a < b < c`).

#### `DB-06` [P2] Attribute Dependency Graph & Topological Evaluator
- **Target:** `examples/sdd_eval/main.rs` + `README.md`
- **Edition Mapping:** `1977 Green: 7 | 1986 Red: 5 | 2006 Purple: 5` (`§5.2.1–5.2.4 L8503–L8616`)
- **Algorithm & Rust Design:**
  - Model parse-tree nodes, attribute instances `(NodeId, AttrName)`, and semantic rules `Rule { target, deps, compute }`.
  - Construct the explicit attribute dependency DAG, verify whether all edges follow S-attributed (bottom-up only) or L-attributed (parent/left-sibling) constraints, run Kahn's topological sort (`BTreeMap`/`BTreeSet`), and evaluate attributes in valid topological order or return `Err(Cycle(Vec<(NodeId, String)>))`.
- **Verification Invariants (`main` & `#[test]`):**
  - Evaluate an L-attributed type/offset or expression syntax tree matching a non-trivial cross-sibling dependency order; verify cycle detection rejects circular SDD specifications cleanly.

#### `DB-07` [P2] Expression DAG Construction & Value-Number Hashing
- **Target:** `examples/expr_dag/main.rs` + `README.md`
- **Edition Mapping:** `1977 Green: 7, 12 | 1986 Red: 5, 9 | 2006 Purple: 5–6, 8` (`§5.3.1 L8725–L8836`, `§6.1.1–6.1.2 L9785–L9886`, `§8.5 L14301–L14536`)
- **Algorithm & Rust Design:**
  - Hash-cons (`BTreeMap<DagKey, usize>`) expressions (`Leaf(String)`, `Const(i64)`, `Node(Op, usize, usize)`) with commutativity normalization ($+$ / $*$) and algebraic identity folding ($x + 0 \to x$, $x * 1 \to x$, $x - x \to 0$) into minimal DAG node indices (`value_number`).
  - Include array-store / pointer-kill epoch invalidation (`§8.5.5–8.5.6`) when building a basic-block DAG before reassembling minimal three-address code.
- **Verification Invariants (`main` & `#[test]`):**
  - Verify `(a + b) + (a + b) * (b + a)` shares a single `a + b` DAG node, verify commutativity collapse, and verify an intervening write/kill prevents unsafe reuse of a stale value number.

---

### Track B: Intermediate Representation, Types & Semantics

#### `DB-08` [P1] Static Single-Assignment (SSA) Construction & $\phi$-Placement
- **Target:** `examples/ssa/main.rs` + `README.md`
- **Edition Mapping:** `1977 Green: — | 1986 Red: — | 2006 Purple: 6, 9` (`§6.2.4 L10039–L10064`, `§9.6.1 L17443–L17520`)
- **Algorithm & Rust Design:**
  - **Input:** Small CFG of basic blocks (`entry = 0`, branches, loop header/back-edge) containing assignments `dst = rhs` (`Const`, `Var`, `BinOp`).
  - **Step 1 (Dominance):** Compute `dom`, immediate dominator tree (`idom`), and dominator-tree children (`dom_children`), then compute **Dominance Frontiers** $DF(n)$ for every block $n$.
  - **Step 2 (Minimal $\phi$-Insertion):** For each variable $v$ assigned in blocks $\text{Defs}(v)$, iterate worklist over $DF$ to insert `Phi { var: v, preds: ... }` at join nodes (avoiding duplicate $\phi$s).
  - **Step 3 (Variable Renaming):** Walk dominator tree depth-first with per-variable version stacks `BTreeMap<Var, Vec<usize>>` and counters, rewriting all uses/defs to `(var, version)` pairs and populating $\phi$ incoming `(pred_block, version)` arguments on CFG outgoing edges.
  - **Step 4 (Verification & Lowering):** Include an SSA invariant checker (every use dominated by its single definition, including $\phi$ edge semantics) plus sparse constant folding on SSA values or $\phi$-copy destruction (`de_ssa`).
- **Verification Invariants (`main` & `#[test]`):**
  - Test diamond `if/else` merge ($\phi$ placed only at join block), loop header with preheader + latch ($\phi$ placed at header with back-edge version distinct from preheader version), and unreachable/dead variable pruning; verify executed program output before and after SSA conversion matches identically on an interpreter!

#### `DB-09` [P2] Hindley-Milner Let-Polymorphic Type Inference & Type Equivalence
- **Target:** `examples/poly_infer/main.rs` + `README.md`
- **Edition Mapping:** `1977 Green: — | 1986 Red: 6 | 2006 Purple: 6` (`§6.3.2 L10129–L10160`, `§6.5.4–6.5.5 L10707–L10886`)
- **Algorithm & Rust Design:**
  - Extend `examples/unification/main.rs` from a pre-supplied equation list into full **Algorithm W / constraint-based HM inference** over a minimal functional AST: `Var`, `IntLit`, `BoolLit`, `Pair`, `Lam(param, body)`, `App(fun, arg)`, `Let(name, rhs, body)`, plus nominal `Record(Tag, fields)` vs structural equivalence comparison (`§6.3.2`).
  - Represent `TypeScheme { quantified: BTreeSet<u32>, ty: Type }` so `Let` bindings generalize type variables not free in `TypeEnv`, and every `Var` reference instantiates quantified variables with fresh TVar IDs.
- **Verification Invariants (`main` & `#[test]`):**
  - `let id = \x. x in (id 1, id true)` succeeds (`(Int, Bool)`), whereas `(\id. (id 1, id true)) (\x. x)` fails unification (`Int` vs `Bool` clash), and `\x. x x` triggers an occurs-check `InfiniteType` error!

#### `DB-10` [P2] Type Coercions (Widening) & Two-Pass Overload Resolution
- **Target:** `examples/overload_coerce/main.rs` + `README.md`
- **Edition Mapping:** `1977 Green: — | 1986 Red: 6 | 2006 Purple: 6` (`§6.5.2–6.5.3 L10609–L10706`)
- **Algorithm & Rust Design:**
  - Define an acyclic widening hierarchy (`Bool -> Char -> Int -> Float -> Double` / user table) and `widen(t1, t2) -> Option<Type>` returning least common supertype + emitted explicit conversion ops (`i2f`, etc.).
  - Implement bottom-up + top-down (two-pass) Ada/general **overload resolution** (`possible_types` set synthesis bottom-up, narrowed top-down by expected parent context) detecting unique solution vs ambiguous (`>= 2`) vs type mismatch (`0`).
- **Verification Invariants (`main` & `#[test]`):**
  - Disambiguate overloaded `f(x)` calls using return-type context when two argument signatures overlap; test widening cost selection and ambiguity error diagnostics.

#### `DB-11` [P2] Switch Lowering & Structured Control-Flow Target Patching
- **Target:** `examples/switch_lower/main.rs` + `README.md`
- **Edition Mapping:** `1977 Green: 7 | 1986 Red: 8 | 2006 Purple: 6` (`§6.7.4 L11397–L11484`, `§6.8 L11485–L11562`)
- **Algorithm & Rust Design:**
  - Given sorted `(case_val, label)` arms + `default_label` and density threshold (e.g. `<= 4` arms → linear `if/else`, dense range $\text{count}/\text{span} \ge 0.5$ → bounds-checked `JumpTable`, sparse large set → balanced binary-search tree), emit the optimal control-flow strategy.
  - Extend with nested loop/switch `break` and `continue` patch stacks (`§6.7.4`) so `break` inside a nested `switch` jumps out of the switch while `continue` reaches the enclosing loop header!
- **Verification Invariants (`main` & `#[test]`):**
  - Assert strategy selection across dense 10-case, sparse 10-case (`0, 100, 10000...`), and tiny 2-case switches; interpret emitted jumps on in-range, gap, and out-of-range keys; verify nested `loop { switch { break; continue; } }`.

#### `DB-12` [P3] Quadruples/Triples IR & Record/Class Field Offset Layout
- **Target:** `examples/record_ir/main.rs` + `README.md`
- **Edition Mapping:** `1977 Green: 2, 8 | 1986 Red: 8 | 2006 Purple: 6` (`§6.2.2–6.2.3 L9959–L10038`, `§6.3.5–6.3.6 L10253–L10316`)
- **Algorithm & Rust Design:**
  - Compute aligned byte offsets, padding bytes, and total `sizeof` for nested `Struct` and `Union` definitions, then compare identical AST lowering into `Quadruple` (explicit named `result` temps) vs `Triple` (positional `(index)` pointers) vs `IndirectTriple` reordering under code motion.
- **Verification Invariants (`main` & `#[test]`):**
  - Verify alignment padding differences (`{u8, u32, u8}` = 12 bytes vs reordered `{u32, u8, u8}` = 8 bytes); verify swapping two instructions preserves `IndirectTriple` references cleanly without rewriting operand indices.

---

### Track C: Runtime Environments, GC & Code Generation

#### `DB-13` [P2] Lexical Display vs Static Links & Stack VLA Layout
- **Target:** `examples/display_vla/main.rs` + `README.md`
- **Edition Mapping:** `1977 Green: 2, 10 | 1986 Red: 7 | 2006 Purple: 7` (`§7.2.4 L11945–L12046`, `§7.3.8 L12251–L12300`)
- **Algorithm & Rust Design:**
  - Contrast `examples/frames/main.rs` (walking $k$ static-link hops at runtime) with an explicit `Display` register array `d[depth]` saving/restoring at procedure entry/exit so any nonlocal read at nesting depth $d$ executes in $O(1)$ hops (`d[target_depth] + offset`).
  - Include stack-frame **variable-length array (VLA)** pointers (`§7.2.4`) stored at fixed frame offsets pointing past fixed locals to dynamic runtime stack regions, restored on return.
- **Verification Invariants (`main` & `#[test]`):**
  - Simulate mutually recursive nested procedures accessing outer scopes at depth 1 and 2; assert `Display` save/restore produces identical values to `static_link` traversal while counting hop depth = 1 per lookup.

#### `DB-14` [P1] Cheney Stop-and-Copy & Mark-Compact Garbage Collectors
- **Target:** `examples/copying_gc/main.rs` + `README.md`
- **Edition Mapping:** `1977 Green: — | 1986 Red: 7 (reclamation) | 2006 Purple: 7` (`§7.6.4–7.6.5 L12883–L13024`)
- **Algorithm & Rust Design:**
  - Model two explicit collector strategies over a flat word/object heap (`Vec<Slot>` with forwarding pointers / `free`/`scan` cursors):
    1. **Cheney Semispace Copying (`§7.6.5`):** Breadth-first copy from `from_space` to contiguous `to_space` using `scan` and `free` pointers with forwarding headers (no explicit recursion stack needed!).
    2. **Mark-Compact (`§7.6.4`):** 3-pass sliding compaction: Mark reachable objects → Compute contiguous new addresses → Update pointers & slide objects in place.
- **Verification Invariants (`main` & `#[test]`):**
  - Handle cyclic object graphs (`A <-> B`), shared diamond pointers (copied **once** via forwarding pointer, all references updated!), and dense live compaction verifying live reachability graph isomorphism after collection.

#### `DB-15` [P2] Generational & Incremental Tri-Color GC (+ Ref-Count Cycles)
- **Target:** `examples/generational_gc/main.rs` + `README.md`
- **Edition Mapping:** `1977 Green: — | 1986 Red: 7 | 2006 Purple: 7` (`§7.5.3 L12653–L12724`, `§7.7.1–7.7.4 L13101–L13244`)
- **Algorithm & Rust Design:**
  - Model young nursery (`Gen0`) + tenured space (`Gen1`) with a write barrier maintaining a `remembered_set` whenever a `Gen1` object stores a reference to a `Gen0` object.
  - Perform minor GC scanning only `roots ∪ remembered_set`, promoting surviving objects after `age_threshold` collections; also demonstrate an explicit tri-color incremental collector invariant (`Black` cannot point directly to `White` without barrier re-graying) and a short comparison showing why naive reference counting (`§7.5.3`) leaks a symmetric cycle.
- **Verification Invariants (`main` & `#[test]`):**
  - Verify an old→young pointer missed without the write barrier is preserved once the barrier logs `remembered_set`; assert `Gen1` objects are untouched during minor collection; assert tri-color invariant holds after mutator store steps.

#### `DB-16` [P1] Register/Address Descriptors, Next-Use & `getReg` Code Generator
- **Target:** `examples/reg_desc_codegen/main.rs` + `README.md`
- **Edition Mapping:** `1977 Green: 15 | 1986 Red: 9 | 2006 Purple: 8` (`§8.4.2 L14157–L14186`, `§8.6 L14537–L14744`)
- **Algorithm & Rust Design:**
  - **Step 1 (`§8.4.2`):** Backward pass over a basic block of 3-address quads computing per-instruction `next_use` & `liveness` (`live` at block exit for non-temps, dead for dead temps).
  - **Step 2 (`§8.6`):** Maintain `reg_desc: Vec<BTreeSet<Var>>` (what each register holds) and `addr_desc: BTreeMap<Var, BTreeSet<Location>>` (which registers + memory home hold each var's latest value).
  - Implement `get_reg(&op, &mut state)` selecting free registers, dead-after-use operand reuse, or minimal-spill victim (evicting variables already stored in memory or farthest next-use), emit abstract RISC `LD`/`ST`/`ADD`/`SUB` instructions, and flush live variables to memory at block exit!
- **Verification Invariants (`main` & `#[test]`):**
  - Run an interpreter over both the input 3-address block and the generated target machine instructions (with 2 or 3 registers forcing a spill & reload!) and assert final memory values match identically for every live variable.

#### `DB-17` [P1] Global Register Allocation by Graph Coloring
- **Target:** `examples/graph_color_regalloc/main.rs` + `README.md`
- **Edition Mapping:** `1977 Green: 15 | 1986 Red: 9 | 2006 Purple: 8` (`§8.8 L14865–L14978`)
- **Algorithm & Rust Design:**
  - Build a register **interference graph** (`BTreeMap<Var, BTreeSet<Var>>`) from per-instruction `live_out` sets across a multi-block CFG, weighted by estimated loop-nesting execution frequency (`10^depth` heuristic, `§8.8.2–8.8.3`).
  - Implement Kempe's `simplify` (`degree < K`), potential-spill selection minimizing $\text{cost}(v)/\text{deg}(v)$, and `select` color-popping (`0..K`). When actual spill occurs, mark variable for stack-slot rewrite and report spill cost.
- **Verification Invariants (`main` & `#[test]`):**
  - Test a $K=3$ colorable planar graph vs $K_4$ diamond forcing 1 spill (verifying the loop-outer/low-cost node is spilled rather than inner-loop hot variable!); assert adjacent interfering nodes never share a register color.

#### `DB-18` [P2] Dynamic-Programming Tree Tiling & Peephole Optimization
- **Target:** `examples/dp_tiling/main.rs` + `README.md`
- **Edition Mapping:** `1977 Green: 15 | 1986 Red: 9 | 2006 Purple: 8` (`§8.7 L14745–L14864`, `§8.9 L14979–L15188`, `§8.11 L15363–L15492`)
- **Algorithm & Rust Design:**
  - Define an instruction tile catalog (e.g., `LD r, Mem(base+off)`, `ADD r, r, imm`, `LEA r, r + scale*r` / complex indexed load) with pattern shapes + cycle costs.
  - Implement bottom-up **DP cost vectors** (`§8.11`, cost `C[node][regs_available]`) choosing contiguous/non-contiguous evaluation order and minimum-cost tile cover, followed by a sliding-window **peephole optimizer** (`§8.7`: redundant `ST r, x; LD r, x`, `ADD r, 0`, `MUL r, 2 -> SHL`, unreachable jump removal).
- **Verification Invariants (`main` & `#[test]`):**
  - Verify DP selects fused indexed addressing or `MEM + MEM` tiles strictly cheaper than naive postorder AST expansion; verify peephole pass preserves semantics while shrinking instruction count.

---

### Track D: Optimization, ILP, Loop Parallelism & Interprocedural Analysis

#### `DB-19` [P1] Available Expressions, Global CSE & Copy Propagation
- **Target:** `examples/available_exprs/main.rs` + `README.md`
- **Edition Mapping:** `1977 Green: 12, 14 | 1986 Red: 10 | 2006 Purple: 9` (`§9.1.4–9.1.5 L15685–L15746`, `§9.2.6 L16183–L16236`)
- **Algorithm & Rust Design:**
  - Forward **must-analysis** (`IN[B] = ⋂ PRED OUT[P]`, initialized to $\mathcal{U}$ Universal set except `OUT[ENTRY] = ∅` — crucial contrast with `reaching_defs`'s union init!).
  - Compute `e_gen[B]` and `e_kill[B]` over normalized canonical binary expressions; use fixed-point `OUT[B]` to replace globally available re-computations with a shared temporary (`CSE`) and follow with **Copy Propagation** (`§9.1.5`) when `x = y` reaches a use without intervening writes to `x` or `y`.
- **Verification Invariants (`main` & `#[test]`):**
  - Verify an expression computed on *only one* branch of an `if/else` is **not** available after the merge, whereas one computed on *both* branches **is** available; check loop-header convergence starting from $\mathcal{U}$.

#### `DB-20` [P1] Semilattice Constant Propagation (and Nondistributivity)
- **Target:** `examples/const_prop/main.rs` + `README.md`
- **Edition Mapping:** `1977 Green: — | 1986 Red: — | 2006 Purple: 9` (`§9.3–9.4 L16399–L17000`)
- **Algorithm & Rust Design:**
  - Model the 3-level flat semilattice per variable: `enum ConstVal { Top /* UNDEF */, Const(i64), Bottom /* NAC */ }` with `meet`: `Top ∧ x = x`, `Const(c) ∧ Const(c) = Const(c)`, distinct constants → `Bottom`.
  - Implement forward transfer functions (`Const(a) + Const(b) = Const(a+b)`, `Top + Const(0) -> Top` vs `NAC * 0 -> Const(0)` or conservative `NAC`), iterate to MFP, and fold/simplify instructions cleanly.
- **Verification Invariants (`main` & `#[test]`):**
  - Implement the classic **nondistributivity witness** (`§9.4.5`): `if c { x = 2; y = 3 } else { x = 3; y = 2 }; z = x + y` — assert MFP yields `z = NAC` while exact path-by-path MOP yields `z = 5`!

#### `DB-21` [P2] Lazy Code Motion (Partial Redundancy Elimination — PRE)
- **Target:** `examples/lazy_code_motion/main.rs` + `README.md`
- **Edition Mapping:** `1977 Green: — | 1986 Red: — | 2006 Purple: 9` (`§9.5 L17001–L17440`)
- **Algorithm & Rust Design:**
  - Split critical edges (`pred` has $>1$ succ AND `succ` has $>1$ pred) so hoisted computations never introduce new paths.
  - Solve the 4 data-flow passes of Knoop-Rüthing-Steffen **Lazy Code Motion** (`§9.5.5`):
    1. **Anticipated** (backward must)
    2. **Available** (forward must, seeded by Anticipated)
    3. **Postponable** (forward must, delaying placement to minimize live range)
    4. **Used** (backward may)
  - Compute `earliest`, `latest`, hoist sites (`latest[B] ∩ used_out[B]`), and redundant replacements (`(e_use[B] ∪ ...)`).
- **Verification Invariants (`main` & `#[test]`):**
  - Demonstrate (a) partial redundancy across one diamond branch hoisted safely into the cold branch without executing along untouched paths, and (b) a loop-invariant expression automatically hoisted into the loop preheader without lengthening register lifetime unnecessarily!

#### `DB-22` [P1] Loop-Invariant Code Motion (LICM) & Induction-Variable Strength Reduction
- **Target:** `examples/licm_iv/main.rs` + `README.md`
- **Edition Mapping:** `1977 Green: 12–13 | 1986 Red: 10 | 2006 Purple: 9` (`§9.1.7–9.1.8 L15775–L15874`, `§9.6 L17441–L17864`)
- **Algorithm & Rust Design:**
  - Using natural loops + reaching definitions (`examples/natural_loops` + `examples/reaching_defs` concepts), mark instructions as **loop-invariant** iteratively (all operands either constant, external to loop, or defined by a single in-loop invariant def).
  - Check **code-motion safety conditions** (`dominates all loop exits` or `dead after loop` + unique definition in loop) before hoisting to a synthesized preheader.
  - Detect basic (`i = i + c`) and derived (`t = a * i + b`) **induction variables**, apply strength reduction (`t' = t' + a * c`), and rewrite exit tests `i < n` → `t' < a * n + b` (`dead-IV cleanup`).
- **Verification Invariants (`main` & `#[test]`):**
  - Verify an invariant inside an `if`-guarded block that does not dominate a loop exit is *not* speculatively hoisted if it could trap or overwrite live exit state; verify loop execution trace produces identical output with fewer multiplications.

#### `DB-23` [P2] Region-Based Data-Flow Analysis (T1/T2 Reducible Regions)
- **Target:** `examples/region_dataflow/main.rs` + `README.md`
- **Edition Mapping:** `1977 Green: 13–14 | 1986 Red: 10 | 2006 Purple: 9` (`§9.7 L17865–L18216`)
- **Algorithm & Rust Design:**
  - Construct the region hierarchy via T1 (self-edge absorption `LoopRegion`) and T2 (single-predecessor folding `BodyRegion`) for reducible CFGs (`§9.7.2`).
  - Represent transfer functions explicitly as bitvector affine composition $(GEN_R, KILL_R)$ + closure operator $(GEN_R^*, KILL_R)$ over loop regions (`§9.7.4–9.7.5`), computing bottom-up summary functions and top-down concrete `IN[B]` facts without iterating bitvectors globally.
- **Verification Invariants (`main` & `#[test]`):**
  - Cross-check computed region-based reaching-definitions / live-variable sets against iterative MFP on a multi-loop reducible CFG and reject or report irreducible $T_1/T_2$ stymie (`while`-into-middle double-entry loop).

#### `DB-24` [P2] Cyclic Modulo Scheduling (Software Pipelining) & Modular Variable Expansion
- **Target:** `examples/software_pipeline/main.rs` + `README.md`
- **Edition Mapping:** `1977 Green: — | 1986 Red: — | 2006 Purple: 10` (`§10.5 L19401–L20036`)
- **Algorithm & Rust Design:**
  - Given an inner loop body with cross-iteration dependence edges `(from, to, latency, iter_dist)` and functional-unit capacities, compute lower bounds: Resource $\text{MII}_{\text{res}}$ and Recurrence $\text{MII}_{\text{rec}} = \max_C \lceil \sum L / \sum D \rceil$.
  - Search increasing initiation intervals $T \ge \max(\text{MII}_{\text{res}}, \text{MII}_{\text{rec}})$ scheduling each operation at cycle $S(op) \bmod T$ respecting modulo resource reservation table + cross-iteration constraint $S(u) + L - D \cdot T \le S(v)$.
  - Emit prologue, steady-state kernel (length $T$), and epilogue, plus **unroll-by-$\lceil \text{lifetime}/T \rceil$ modular variable expansion** (`§10.5.10`) so overlapping iterations do not clobber scalar registers!
- **Verification Invariants (`main` & `#[test]`):**
  - Verify resource-bound loop ($T=2$), recurrence-bound loop ($D=1, L=3 \implies T=3$), unrolled variable renaming, and exact equivalence against sequential execution of $N$ loop iterations.

#### `DB-25` [P2] Array Dependence Testing (GCD, Banerjee & Fourier-Motzkin)
- **Target:** `examples/dependence_test/main.rs` + `README.md`
- **Edition Mapping:** `1977 Green: — | 1986 Red: — | 2006 Purple: 11` (`§11.3 L20495–L20824`, `§11.6 L21159–L21522`)
- **Algorithm & Rust Design:**
  - Given bounded loop nests $L_k \le i_k \le U_k$ and two affine subscript accesses $A[a_0 + \sum a_k i_k]$ and $A[b_0 + \sum b_k i'_k]$:
    1. **GCD test (`§11.6`):** Check whether $\gcd(a_1, \dots, b_1, \dots) \mid (b_0 - a_0)$ (sound independence proof if false!).
    2. **Fourier-Motzkin elimination (`§11.3`):** Project linear inequality system $A x \le c$ variable by variable (combining positive/negative upper and lower bounds); detect infeasible $L > U$ (`0 <= negative`) to prove independence across coupled triangular loop bounds!
- **Verification Invariants (`main` & `#[test]`):**
  - `A[2*i]` vs `A[2*i + 1]` → proved independent by GCD ($\gcd(2,2)=2 \nmid 1$); `A[i]` vs `A[i + 5]` for $0 \le i < 4$ → GCD passes ($\gcd=1$) but Fourier-Motzkin proves empty intersection! Verify valid dependence vectors `(d_1, ..., d_m)` when solutions exist.

#### `DB-26` [P2] Unimodular Loop Transforms, Affine Partitioning & Appendix B Linear Solver
- **Target:** `examples/affine_partition/main.rs` + `README.md`
- **Edition Mapping:** `1977 Green: — | 1986 Red: — | 2006 Purple: 11, app. B` (`§11.7 L21523–L22160`, `§11.10 L22959–L23234`, `App B L25561–L25638`)
- **Algorithm & Rust Design:**
  - Implement **Appendix B** (`Algorithm B.1` / exact rational `Rational(i64, i64)` column-echelon reduction and nullspace basis extraction for homogeneous integer matrices $A x = 0$).
  - Use nullspace vectors to find **synchronization-free affine space partitions** (`§11.7`: $c \cdot (i_1 - i_2) = 0$) and apply legal **unimodular matrices** (`det = ±1`, e.g., **skewing** $S = \begin{pmatrix} 1 & 0 \\ 1 & 1 \end{pmatrix}$ to turn negative dependence $(1, -1)$ into $(1, 0)$, enabling **interchange** or **2D loop tiling** `§11.10`).
- **Verification Invariants (`main` & `#[test]`):**
  - Verify Appendix B nullspace vectors satisfy $A v = 0$ and are linearly independent; verify loop skewing makes `A[i, j] = f(A[i-1, j+1])` legal for outer-parallel or tiled execution while matching sequential output element-for-element!

#### `DB-27` [P1] Context-Sensitive ($k$-CFA / Call-String) Points-To Analysis
- **Target:** `examples/context_points_to/main.rs` + `README.md`
- **Edition Mapping:** `1977 Green: 14 *(rel.)* | 1986 Red: 10 *(rel.)* | 2006 Purple: 12` (`§12.1–12.2 L23489–L23862`, `§12.5–12.6 L24387–L24632`)
- **Algorithm & Rust Design:**
  - Revisit the exact dual-call `relay` problem from `examples/points_to/main.rs`, parameterizing call sensitivity `k`:
    - `k = 0`: recovers context-insensitive merged points-to sets (`x` and `y` polluted by both `alloc_A` and `alloc_B`).
    - `k >= 1`: qualifies abstract variables/heap locations with bounded call-string prefix/suffix (`Vec<CallSiteId>` capped at length `k`), propagating through `Call` push and `Return` pop, plus virtual dispatch target discovery (`on-the-fly call graph §12.5.2`) when receiver points-to sets grow!
- **Verification Invariants (`main` & `#[test]`):**
  - Assert `k = 0` merges `{alloc_A, alloc_B}` into both `x` and `y`, while `k = 1` keeps `pts(x) = {alloc_A}` and `pts(y) = {alloc_B}` disjoint; verify bounded termination under recursion (`k = 2` cyclic calls saturate finite call strings).

#### `DB-28` [P2] Semi-Naive Datalog Engine & ROBDD Relational Solver
- **Target:** `examples/datalog_bdd/main.rs` + `README.md`
- **Edition Mapping:** `1977 Green: — | 1986 Red: — | 2006 Purple: 12` (`§12.3–12.4 L23863–L24386`, `§12.7 L24633–L24826`)
- **Algorithm & Rust Design:**
  - **Part 1 — Semi-Naive Datalog (`§12.3.4–12.3.5`):** Store EDB/IDB relations as sorted `BTreeSet<Vec<u32>>`; evaluate Horn-clause rules using $\Delta$-relations (`new_tuples = join(..., delta_R, ...) \ R`) until all $\Delta$ sets become empty. Encode flow-insensitive points-to (`Pts(v, h) :- Alloc(v, h).`, `Pts(d, h) :- Move(d, s), Pts(s, h).`, `Load`/`Store` field rules `§12.4.4`).
  - **Part 2 — Compact ROBDD Engine (`§12.7`):** Arena-backed Reduced Ordered Binary Decision Diagram (`Zero=0`, `One=1`, `Node(var, low, high)`) with canonical unique-table deduplication (`low == high => return low`) and memoized `apply(op, u, v)` + `exists(vars, u)` relational composition!
- **Verification Invariants (`main` & `#[test]`):**
  - Verify naive vs semi-naive Datalog produce identical fixed points (while counting fewer redundant derivations in semi-naive); verify ROBDD canonical uniqueness ($a \land b \equiv b \land a$ shares identical node ID) and solve a small transitive-closure / points-to relation with BDD `exists`+`and`+variable rename matching the tuple-set solver bit-for-bit!

---

## 7. Immediate Metadata & Documentation Fixes Checklist

Even before adding any `DB-XX` Rust examples, an agent can resolve all existing documentation discrepancies with the following surgical edits:

1. **Fix `unification` missing 2006 Purple credit (3 files):**
   - `2006 Purple` Chapter 6 explicitly teaches Hindley-Milner polymorphic type inference (`§6.5.4`, [`Compilers_2nd_Ed_2006.md#L10707`](../../textbooks/DragonBook/Compilers_2nd_Ed_2006.md#L10707)) and the unification algorithm (`§6.5.5`, [`Compilers_2nd_Ed_2006.md#L10813`](../../textbooks/DragonBook/Compilers_2nd_Ed_2006.md#L10813)). Change `—` to `6` in all three places:
     - [`examples/unification/README.md#L37-L39`](../examples/unification/README.md#L37-L39): `| — | 6 | — |` → `| — | 6 | 6 |`
     - [`README.md#L55`](../README.md#L55): `| ... | — | 6 | — |` → `| ... | — | 6 | 6 |`
     - [`index.html#L156`](../index.html#L156): `<dt>2006 Purple</dt><dd>—</dd>` → `<dt>2006 Purple</dt><dd>6</dd>`

2. **Optional clarification note on `Compilers_New_Intl_Ed_2014.md` in [`README.md#edition-guide`](../README.md#L17-L25):**
   - Add one brief bullet noting that the **2014 Pearson New International Edition** aligns chapter-for-chapter with `2006 Purple` Chapters 1–11 and Appendices A–B, while omitting Chapter 12 (`Interprocedural Analysis`) as a separate chapter (though two stray references to `Section 12.4` and `Chapter 12` remain in Chapter 9). Because it introduces no chapter renumbering for 1–11 or A–B, keep the existing three-edition tables (`1977 Green`, `1986 Red`, `2006 Purple`) rather than adding a redundant 4th column.

3. **Keep `tiny_compiler` Appendix A distinction intact (no fix needed, verified accurate):**
   - [`examples/tiny_compiler/README.md#L50`](../examples/tiny_compiler/README.md#L50) properly distinguishes `1986 Red` (`app. A (project theme)`, Pascal subset [`L16931–L17030`](../../textbooks/DragonBook/Compilers_1st_Ed_1986.md#L16931-L17030)) from `2006 Purple` (`app. A (front-end theme)`, complete AST/TAC front end [`L24973–L25560`](../../textbooks/DragonBook/Compilers_2nd_Ed_2006.md#L24973-L25560)).

---

## 8. Review Caveats

- **1977 Green coverage (`Principles of Compiler Design`):** No local Markdown copy exists under `learning/textbooks/DragonBook`; 1977 chapter references in Sections 4 and 6 follow existing mappings in `README.md`, `index.html`, and per-example `README.md` files rather than direct text inspection.
- **OCR quality differences:** `Compilers_1st_Ed_1986.md` contains dense spacing and mathematical OCR artifacts from the DjVu conversion, and `Compilers_New_Intl_Ed_2014.md` contains minor heading OCR errors (e.g., `Unication` in `§6.5.5`); verify pseudocode and equations against `Compilers_2nd_Ed_2006.md` first when implementing `DB-01`–`DB-28`.
- **Execution status:** No `cargo fmt`, `cargo clippy`, or `cargo test` commands were run during this documentation audit; every Rust-example verification claim above is based on file inspection and `README.md` contracts rather than live execution in this session.
