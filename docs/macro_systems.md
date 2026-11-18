# Macro Systems: Lexer, Token-Preprocessor, and Syntax-Aware Architectures

"Macros" appear at very different stages of compiler front ends. Treating all macros as regex substitutions or DFA transformations conflates mechanisms that operate on completely different data structures, scoping rules, and compiler phases.

---

## Three distinct macro architectures

| Category | Compiler phase & unit of transformation | Hygiene & scoping | Canonical examples | Repository coverage |
| --- | --- | --- | --- | --- |
| **1. Lexer-specification macros** | Pre-automata regex expansion inside a `.l` file | Text/regex fragment alias before NFA/DFA build | Flex `{digit}`, `{id}` definitions | Implemented in [`lex_generator`](../examples/lex_generator/README.md) & [`lex_yacc_ast`](../examples/lex_yacc_ast/README.md) |
| **2. Token-based preprocessing** | Separate pass operating on token/character streams before parsing | No syntactic scope or AST awareness; textual pasting (`##`) & `#ifdef` | C preprocessor (`cpp`), SystemVerilog `` `define `` / `` `ifdef `` | Planned future topic (`macro_systems` study note) |
| **3. Syntax-aware macros** | Parser / post-parse AST or token-tree rewriting during compilation | Structural token-tree or S-expression scopes; hygienic or explicitly unhygienic | Rust `macro_rules!` / proc macros, Common Lisp `defmacro`, Scheme `syntax-rules` | Planned future topic (`macro_systems` study note) |

---

## 1. Lexer-specification macros (pre-automata)

In Lex and Flex, definitions like `digit [0-9]` and `id [A-Za-z_][A-Za-z0-9_]*` are purely **specification-time abbreviations**. They are expanded (checking for cycles and parenthesizing sub-expressions to preserve operator precedence) *before* Thompson NFA and subset DFA construction runs:

```text
  .l definitions ({digit}) --expand--> pure regex ([0-9]) --NFA/DFA--> scanner tables
```

Once the DFA exists, `{digit}` is gone—it cannot inspect source tokens at runtime or expand conditionally on language declarations. See [`expand_macros` in `lex_generator`](../examples/lex_generator/main.rs).

---

## 2. Token-based preprocessing (C and SystemVerilog)

The C preprocessor (`cpp`) and SystemVerilog preprocessor (`` `define ``, `` `ifdef ``, `` `include ``) run **before the grammar parser**, transforming a flat stream of preprocessor tokens rather than a regular expression or a syntax tree:

- Macros can splice tokens across unbalanced delimiters (e.g., `#define BEGIN {` or macro-generated module ports), making error locations and incremental reparsing hard unless expansion provenance is tracked explicitly.
- Conditional compilation branches (`` #ifdef FOO ... #else ... #endif ``) prune source text dynamically, which real-world tooling (Clang, Verible, Xezim) must either evaluate under a configuration or track alongside token origins.
- Object-like and function-like macros are **unhygienic**: an expanded body can accidentally capture caller variables (`int tmp;` colliding inside `SWAP(a, b)`) or reorder operator precedence when arguments are unparenthesized (`SQUARE(1 + 2)` -> `1 + 2 * 1 + 2`).

---

## 3. Syntax-aware macros (Rust, Lisp, Scheme)

Modern compiled languages increasingly replace flat token preprocessors with **syntax-aware macros** embedded into or interleaved with parsing:

- **Rust `macro_rules!` and procedural macros:** Match and emit **structured token trees** (balanced `( ... )`, `[ ... ]`, `{ ... }`) rather than raw byte streams. Arguments bound to grammar fragments like `$e:expr` or `$t:ty` already carry their syntactic grouping—no `1 + 2 * 1 + 2` precedence surprises—and mixed-site hygiene prevents accidental local variable capture.
- **Lisp (`defmacro`):** Macros are ordinary functions evaluated at compile time that accept and return **ASTs represented as S-expressions**. Traditional `defmacro` requires manual `gensym` discipline to avoid variable capture.
- **Scheme (`syntax-rules`, `syntax-case`):** Introduced **automatic hygienic macro expansion**, preserving lexical scope across macro definitions and call sites via syntax marks / scope sets so generated bindings never silently shadow user bindings.

---

## Why all macros are not regex substitutions

1. **Regexes cannot express nested syntactic fragments.** Trying to match balanced parentheses, expressions, or nested statements with `.l` regex macros fails for the same reason regular languages cannot parse arbitrary context-free nesting ([`cfg_derivations`](../examples/cfg_derivations/README.md)).
2. **Preprocessors alter the parser's input; syntax macros cooperate with the parser.** Lexer macros build the lexer itself; preprocessors transform raw token streams before parsing; syntax macros transform parsed token trees or AST nodes while preserving grammar boundaries, spans, and hygiene.

---

## Future `macro_systems` curriculum plan and references

This repository currently demonstrates lexer-specification regex macros in [`lex_generator`](../examples/lex_generator/README.md) and [`lex_yacc_ast`](../examples/lex_yacc_ast/README.md). A future standalone `macro_systems` example can contrast:
1. Pre-NFA `.l` regex expansion vs. C/SystemVerilog token preprocessing (`#define` / `` `ifdef `` / capture bug), and
2. A minimal hygienic syntax-tree macro expander (`syntax-rules` / `macro_rules!` fragment matching with scope marks).

### Primary references

- **Rust macros:** [The Rust Reference — Macros By Example](https://doc.rust-lang.org/reference/macros-by-example.html) and [The Little Book of Rust Macros](https://veykril.github.io/tlborm/)
- **Hygienic & Scheme macros:** [R7RS-small Specification (`syntax-rules`, §4.3 / §5.4)](https://small.r7rs.org/) and Kohlbecker et al., *Hygienic Macro Expansion* (1986) / Bawden & Clinger *Macros That Work*
- **Token preprocessors:** [GCC C Preprocessor Manual](https://gcc.gnu.org/onlinedocs/cpp/) and [Verible SystemVerilog Parser & Preprocessor Notes](https://verible.readthedocs.io/en/latest/parser_design.html)

---

## Continue reading

- [Open-Source Lexer and Parser Tools & Real Compiler Architectures](open_source_lexer_parser_tools.md)
- [A Reference Guide to Programming-Language Design](language_design_guide.md)
- Back to [Repository README](../README.md#scanning-and-parsing)
