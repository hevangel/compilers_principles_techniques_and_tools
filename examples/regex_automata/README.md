# Regex to Thompson NFA and Direct DFA

Parse regular expressions into an AST and lower them both to a Thompson NFA and directly to a DFA using `followpos`. See the [Rust source](main.rs) and [Python source](main.py).

```sh
cargo run --example regex_automata
cargo test --example regex_automata
python3 examples/regex_automata/main.py
python3 -m unittest examples/regex_automata/main.py
```

## What the example teaches

### Why we need this / What problem this solves

Lexers match token rules written as regular expressions, and they must scan input in **linear time** ($O(n)$ in the input length) without backtracking. A naive backtracking regex matcher can take exponential time on nested repetitions or alternations, and even a hand-written ad-hoc state machine quickly gets error-prone for patterns like `(a|b)*abb` or `a+b?`.

Compiler generators solve this by compiling a regular expression AST into a finite automaton in one of two classic ways:

1. **Thompson NFA construction:** Build a small epsilon-NFA compositionally (one fragment per AST node) and simulate all reachable states via epsilon-closure at runtime. Construction is $O(|r|)$, though runtime simulation maintains a state set on every character.
2. **Direct `followpos` DFA construction (McNaughton–Yamada–Glushkov):** Augment the syntax tree with a unique end marker `#`, compute four local tree properties (`nullable`, `firstpos`, `lastpos`, `followpos`), and build a **deterministic** finite automaton directly—skipping the intermediate NFA while guaranteeing $O(1)$ work per input byte at runtime.

### Two paths from one AST

1. `parse_regex` parses literals, concatenation, alternation (`|`), Kleene star (`*`), `+`, `?`, and grouping `(...)` into an AST, desugaring `a+` into `a a*` and `a?` into `a | eps` while giving every literal leaf a **distinct integer position**.
2. `to_thompson_nfa` builds an epsilon-NFA compositionally with one start and one accept state per subexpression, then simulates epsilon-closures (`eps_closure`) on input bytes.
3. `compile_direct_dfa` appends an `EndMarker` (`#`) at position `6`, computes `nullable`, `firstpos`, `lastpos`, and `followpos` in one postorder traversal (`collect_positions`), and discovers reachable DFA state subsets directly from `followpos`. Any DFA state containing the end-marker position is accepting.

### Direct DFA positions and state trace for `(a|b)*abb#`

The syntax tree assigns positions `1..5` to the leaves of `(a|b)*abb` (`1:a, 2:b, 3:a, 4:b, 5:b`) and `6` to `#`:

| Position | Symbol | `followpos` | Why? |
| --- | --- | --- | --- |
| `1` | `a` | `{1, 2, 3}` | Inside `(a\|b)*`: can loop back to `{1, 2}` or exit to `3` (`firstpos(abb#)`) |
| `2` | `b` | `{1, 2, 3}` | Inside `(a\|b)*`: same `followpos` as position `1` |
| `3` | `a` | `{4}` | Must be followed by position `4` (`b`) |
| `4` | `b` | `{5}` | Must be followed by position `5` (`b`) |
| `5` | `b` | `{6}` | Must be followed by end marker `#` (`6`) |
| `6` | `#` | `{}` | End of pattern |

From `root.firstpos = {1, 2, 3}`, expanding `followpos` discovers 4 DFA states:

| DFA State | Position subset | On `a` | On `b` | Accepting? |
| --- | --- | --- | --- | --- |
| `S0` (start) | `{1, 2, 3}` | `S1` (`{1, 2, 3, 4}`) | `S0` (`{1, 2, 3}`) | No |
| `S1` | `{1, 2, 3, 4}` | `S1` (`{1, 2, 3, 4}`) | `S2` (`{1, 2, 3, 5}`) | No |
| `S2` | `{1, 2, 3, 5}` | `S1` (`{1, 2, 3, 4}`) | `S3` (`{1, 2, 3, 6}`) | No |
| `S3` | `{1, 2, 3, 6}` | `S1` (`{1, 2, 3, 4}`) | `S0` (`{1, 2, 3}`) | Yes (`6` in set) |

### How to read the code

| Component | Rust (`main.rs`) | Python (`main.py`) | Role |
| --- | --- | --- | --- |
| Regex AST | `enum RawAst`, `enum RegexNode` | `_Raw*`, `RegexNode` union | Raw AST (with `+`/`?`) and lowered AST with leaf position IDs |
| Parser & desugarer | `struct RegexParser`, `assign_positions`, `parse_regex` | `class _Parser`, `_assign_positions`, `parse_regex` | Recursive-descent parser (`alt > cat > postfix > atom`) and position numbering |
| Thompson NFA | `struct Nfa` (`build`, `eps_closure`, `accepts`), `to_thompson_nfa` | `class Nfa`, `to_thompson_nfa` | Compositional epsilon-NFA construction and on-the-fly subset simulation |
| Direct DFA | `struct DirectInfo`, `collect_positions`, `compile_direct_dfa`, `struct Dfa` | `_collect_positions`, `compile_direct_dfa`, `class Dfa` | Postorder `nullable`/`firstpos`/`lastpos`/`followpos` pass and deterministic table builder |

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
