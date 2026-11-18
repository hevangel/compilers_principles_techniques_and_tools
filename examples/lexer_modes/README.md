# Lexer Start Conditions and Context-Sensitive Modes (Ch. 3 Extension)

Tokenize nested block comments, escape-decoded strings with nested `${expr}` interpolation, and `#directive` lines using exclusive start conditions (`%x`) and an explicit mode stack. See the [Rust source](main.rs) and [Python source](main.py).

```sh
cargo run --example lexer_modes
cargo test --example lexer_modes
python3 examples/lexer_modes/main.py
python3 -m unittest examples/lexer_modes/main.py
```

## What the example teaches

### Why we need this / What problem this solves

A single flat DFA cannot cleanly track nested constructs (`/* outer /* inner */ */`) or context switches where identical characters mean completely different things (`let` is a keyword in code, plain characters inside `"let"`, and an argument inside `#pragma let`). Worse, modern languages embed expressions *inside* string literals (`"a=${b + "c=${d}"}"`), interleaving string scanning with full expression scanning.

**Start conditions** (`%x` exclusive modes in Flex/Lex) plus a **mode stack** solve this by letting the active lexer mode choose which rule table runs next:

| Mode | Trigger in | Rule behavior | Exit trigger |
| --- | --- | --- | --- |
| `INITIAL` | Default / `${` inside strings | Scans keywords, identifiers, ints, operators; counts `{ ... }` inside interpolations | `"` → `%x STRING`, `/*` → `%x COMMENT`, `#` → `%x DIRECTIVE`, matching `}` → `pop_mode` |
| `%x COMMENT` | `/*` | Ignores comment bytes; increments `comment_depth` on `/*`, decrements on `*/` | `comment_depth == 0` → `pop_mode` |
| `%x STRING` | `"` | Decodes escapes (`\n`, `\"`, `\$`), emits `STR_TEXT` chunks | `${` → pushes `INITIAL`, closing `"` → `pop_mode` |
| `%x DIRECTIVE` | `#` | Splits preprocessor command and args without polluting normal tokens | `\n` or EOF → `pop_mode` |

### How to read the code

| Component | Rust (`main.rs`) | Python (`main.py`) | Role |
| --- | --- | --- | --- |
| Mode state & stack | `enum Mode`, `struct ModalLexer` | `class ModalLexer` | Tracks `mode_stack`, `interp_brace_depth`, and `comment_depth` |
| Stack operations | `push_mode`, `pop_mode` | `push_mode`, `pop_mode` | Records every contextual switch for inspection and debugging |
| Per-mode scanners | `step_initial`, `step_comment`, `step_string`, `step_directive` | `_step_initial`, `_step_comment`, `_step_string`, `_step_directive` | Isolates each context's lexical rules |

## What to look for

The sample run mixes `#define LIMIT 10`, nested `/* outer /* nested comment */ still ignored */`, and a doubly-nested interpolated string `"sum=${a + "inner_${k}"}"`, printing every push/pop transition and cleanly recovering the full nested token stream.

## Check your understanding

- Why does a stack (`push_mode`/`pop_mode`) succeed on `"a=${b + "c=${d}"}"` where a plain `BEGIN(mode)` register fails?
- Why must `INITIAL` also track brace depth (`{` vs `}`) when scanning inside `${ ... }`?

## Edition mapping and scope

| 1977 Green | 1986 Red | 2006 Purple |
| --- | --- | --- |
| 3 *(related)* | 3 (§3.5 extension) | 3 (§3.5 extension) |

Numbers refer to chapters. Companion to [Lex-Style Scanner Generator](../lex_generator/README.md).
