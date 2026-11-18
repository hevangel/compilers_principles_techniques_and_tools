# Lex-Style Scanner Generator (`.l` DSL Compiler)

Compile a three-section `.l` lexical specification (`definitions %% rules %% user code`)—with macro expansion, character classes, rule priority, and `/` trailing-context lookahead—into a single multi-pattern DFA scanner. See the [Rust source](main.rs) and [Python source](main.py).

```sh
cargo run --example lex_generator
cargo test --example lex_generator
python3 examples/lex_generator/main.py
python3 -m unittest examples/lex_generator/main.py
```

## What the example teaches

### Why we need this / What problem this solves

Earlier scanning demos compile a single regular expression to an automaton (`regex_automata`, `subset_dfa`) or hardcode a hand-written switch statement (`lexer`). A real **`.l` scanner generator** (Lex / Flex, `§3.5` and `§3.8`) solves three problems automatically from a declarative rule list:

1. **Combining all token rules into one linear-time DFA:** Building one NFA whose start state has epsilon edges to every rule lets subset construction scan all patterns simultaneously—matching in `O(1)` per input character without trying regexes one by one.
2. **Disambiguating overlaps (`longest match` then `rule order`):** On `"while"`, both `"while"` and `{id}` accept length 5, so Lex picks the **earliest-listed rule** (`WHILE`); on `"while1"`, `{id}` accepts length 6 vs. 5, so **longest match** picks `IDENT`.
3. **Resolving forward ambiguity with trailing context (`/`):** If `{digit}+"."{digit}*` matches `1.` as a floating literal, then Pascal-style ranges like `1..4` break into `1.` + `.4`. Writing `{digit}+/".."` lets the scanner verify `..` follows `1`, win the longest-match competition (`len 3` > `len 2`), and **retract** the input cursor back to the `/` split so `..` and `4` stay in the stream for the next tokens.

### Worked trace on `while while1 = 1..4 + 12.5 + 3.;`

| Input slice | Competing rules | Winning rule & reason | Emitted token |
| --- | --- | --- | --- |
| `while` | `#1 "while"` (len 5), `#2 {id}` (len 5) | Equal length -> earliest rule `#1` wins | `WHILE ("while")` |
| `while1` | `#1 "while"` (len 5), `#2 {id}` (len 6) | Longest match (`6 > 5`) -> `#2` wins | `IDENT ("while1")` |
| `1..4` | `#3 {digit}+/".."` (len 3), `#4 {digit}+"."{digit}*` (len 2) | Longest match `#3` wins, then `/` retracts cursor after `1` | `INT ("1")`, then `RANGE ("..")`, then `INT ("4")` |
| `12.5` | `#4 {digit}+"."{digit}*` (len 4), `#5 {digit}+` (len 2) | Longest match `#4` wins | `REAL ("12.5")` |

### How to read the code

| Component | Rust (`main.rs`) | Python (`main.py`) | Role |
| --- | --- | --- | --- |
| Macro & spec parser | `expand_macros`, `parse_lex_spec` | `expand_macros`, `parse_lex_spec` | Splits `%%` sections, expands `{name}` macros safely, and splits `head/tail` rules |
| Regex parser | `LexRegexParser`, `RegexAst` | `LexRegexParser`, `RegexAst` | Parses `[0-9]`, `[^...]`, `.`, `"..."`, escapes, `+`, `?`, `*`, `\|` into ASTs |
| Multi-rule NFA -> DFA | `Nfa`, `Dfa::from_nfa`, `compile_lex_spec` | `Nfa`, `Dfa.from_nfa`, `compile_lex_spec` | Combines all rules under one NFA start state and records lowest accepting rule ID per DFA state |
| Maximal-munch runtime | `scan_with_compiled` | `scan_with_compiled` | Runs longest match over the combined DFA and retracts `/` rules via `(head_dfa, tail_dfa)` |

## What to look for

Running the demo prints the compiled definition/rule summary, NFA-to-DFA state count, preserved user-code footer, and the 11 tokens emitted for `"while while1 = 1..4 + 12.5 + 3.;\n"`. Unit tests also verify negated string classes (`\"[^"\n]*\"`), cyclic macro cycle detection, and invalid trailing-context syntax.

## Check your understanding

- Why does `"while1"` emit `IDENT` instead of `WHILE` followed by `INT("1")` even though `"while"` appears before `{id}` in the `.l` rules?
- What happens at runtime when `head/tail` matches `[pos .. match_end]` if `head` itself has variable length (like `{digit}+` on `12..4`)?

## Edition mapping and scope

| 1977 Green | 1986 Red | 2006 Purple |
| --- | --- | --- |
| 3 | 3 (§3.5, §3.8) | 3 (§3.5, §3.8) |

Numbers refer to chapters. Related readings and appendices are marked where applicable. See the [edition guide](../../README.md#edition-guide) for the bibliographic references and chapter coverage.

This self-contained Lex compiler compiles and executes `.l` specifications directly on printable ASCII + whitespace (`\t`, `\n`) DFAs without emitting external C files. See [DFA Table Optimization](../dfa_optimization/README.md) for §3.9 character-class and `next/check` table compression, [Context-Sensitive Lexer Modes](../lexer_modes/README.md) for `%x` start conditions, and [Integrated Lex + Yacc AST Pipeline](../lex_yacc_ast/README.md) for wiring a `.l` scanner directly into a `.y` parser that constructs an AST.
