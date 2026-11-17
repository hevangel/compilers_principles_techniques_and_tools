# Regex to Thompson NFA and Direct DFA

Parse regular expressions into an AST and lower them both to a Thompson NFA and directly to a DFA using `followpos`. See the [Rust source](main.rs) and [Python source](main.py).

```sh
cargo run --example regex_automata
cargo test --example regex_automata
python3 examples/regex_automata/main.py
python3 -m unittest examples/regex_automata/main.py
```

## What the example teaches

A regular expression can be converted into a deterministic recognizer in two ways: either construct a Thompson NFA with epsilon edges and determinize it, or compute `nullable`, `firstpos`, `lastpos`, and `followpos` directly over the augmented syntax tree `r#` to build DFA states in one pass.

### Two paths from one AST

1. `parse_regex` parses literals, concatenation, alternation (`|`), Kleene star (`*`), `+`, `?`, and parentheses into an AST, assigning a distinct integer position to each literal leaf.
2. `to_thompson_nfa` builds an epsilon-NFA compositionally with one start and one accept state per subexpression, then simulates epsilon-closures on input bytes.
3. `compile_direct_dfa` appends an `EndMarker` position, computes `nullable`, `firstpos`, `lastpos`, and `followpos` in a postorder walk, and discovers reachable DFA state subsets directly from `followpos`.

### Direct DFA positions for `(a|b)*abb#`

| Position | Symbol | `followpos` |
| --- | --- | --- |
| 1 | `a` | `{1, 2, 3}` |
| 2 | `b` | `{1, 2, 3}` |
| 3 | `a` | `{4}` |
| 4 | `b` | `{5}` |
| 5 | `b` | `{6}` |
| 6 | `#` | `{}` |

## What to look for

Both the Thompson NFA simulator and the direct followpos DFA agree on `(a|b)*abb` as well as optional/plus forms (`a+b?`) and empty expressions. Unit tests compare both automata exhaustively over short strings and check error diagnostics on malformed patterns such as unclosed parentheses and misplaced operators.

## Check your understanding

- Why must `a+` (desugared as `a a*`) receive distinct position numbers for its two `a` occurrences in `followpos`?
- Which `followpos` entries would change if `(a|b)*abb` became `(a|b)*a?bb`?

## Edition mapping and scope

| 1977 Green | 1986 Red | 2006 Purple |
| --- | --- | --- |
| 3 | 3 | 3 |

Numbers refer to chapters. Related readings and appendices are marked where applicable. See the [edition guide](../../README.md#edition-guide) for the bibliographic references and chapter coverage.

The parser handles ASCII literals, escaped metacharacters, concatenation, alternation, `*`, `+`, `?`, and grouping; character classes and anchors are omitted to keep the construction self-contained.
