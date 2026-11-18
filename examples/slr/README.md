# LR(0) items and SLR parsing

Generate item sets and shift/reduce actions for a left-recursive list. See the [Rust source](main.rs) and [Python source](main.py).

```sh
cargo run --example slr
cargo test --example slr
python3 examples/slr/main.py
python3 -m unittest examples/slr/main.py
```

## What the example teaches

### Why we need this / What problem this solves

A top-down LL parser must commit to a production *before* reading its right-hand side, which forces grammars to eliminate left recursion and often twists natural left-associative rules like `List → List , Item | Item`. A **bottom-up (shift-reduce) parser** works in reverse: it reads tokens onto a stack (**shift**) until the top of the stack forms a complete right-hand side (a **handle**), and then collapses that handle to its left-hand nonterminal (**reduce**). Because the whole phrase is already on the stack before reducing, left-recursive rules work natively and express left-associativity directly.

The core challenge for a bottom-up parser is knowing *when* to shift versus *when* to reduce. **SLR(1)** builds an **LR(0) automaton** of parser states (sets of dotted productions) and resolves completed items (`A → α ·`) by reducing only when the next lookahead token is in `FOLLOW(A)`—preventing premature reductions that would strand the remaining input.

### From productions to a parser

1. An **LR(0) item** is a production with a dot marking progress (`(rule_index, dot_position)`). For example, `List → List · , Item` means a `List` has just been recognized and `,` is expected next.
2. **`closure`** expands any item whose dot immediately precedes a nonterminal `B` by adding `(prod_of_B, 0)` until the set stabilizes. For instance, `Start → · List` brings in both productions of `List`, and `List → · Item` brings in `Item → · name`.
3. **`automaton`** builds the DFA of item sets: for each symbol `X` appearing after a dot in state `s`, it advances those items' dots past `X` and takes `closure` to get the target state `goto[(s, X)]`.
4. **`actions`** turns terminal transitions into `Shift(target)` and completed items (`dot == rhs.len()`) into `Reduce(rule)` on `FOLLOW(LHS) = {Comma, End}` (or `Accept` on `End` for rule 0: `Start → List ·`). Any overlapping cell is flagged as a grammar conflict.
5. **`parse`** keeps automaton states on its stack. Shifting pushes the next state and advances input; reducing pops `rhs.len()` states and pushes `goto[(uncovered_state, lhs)]`.

### Trace the shift-reduce stack for `name , name`

Grammar rules: `(0) Start → List`, `(1) List → List , Item`, `(2) List → Item`, `(3) Item → name`.

| Stack (states) | Input remaining | Action | Phrase / explanation |
| --- | --- | --- | --- |
| `[0]` | `name , name $` | `Shift` | Push state after `name` |
| `[0, s_name]` | `, name $` | `Reduce(3)` | Pop 1 state (`name`), push `goto[0, Item]` (`Item → name`) |
| `[0, s_item]` | `, name $` | `Reduce(2)` | Pop 1 state (`Item`), push `goto[0, List]` (`List → Item`) |
| `[0, s_list]` | `, name $` | `Shift` | Comma matches `List → List · , Item` |
| `[0, s_list, s_comma]` | `name $` | `Shift` | Shift second `name` |
| `[0, s_list, s_comma, s_name]` | `$` | `Reduce(3)` | Pop 1 state (`name`), push `goto[s_comma, Item]` |
| `[0, s_list, s_comma, s_item]` | `$` | `Reduce(1)` | Pop 3 states (`List , Item`), push `goto[0, List]` |
| `[0, s_list]` | `$` | `Accept` | `Start → List ·` sees `$` -> reductions `[3, 2, 3, 1]` |

The leading `Start → List` rule is an **augmented start production** added solely to distinguish accepting the entire program from an ordinary reduction.

### How to read the code

| Component | Rust (`main.rs`) | Python (`main.py`) | Role |
| --- | --- | --- | --- |
| Grammar & symbols | `enum Symbol`, `const RULES` | `class Symbol(Enum)`, `RULES` | Augmented `Start -> List` grammar rules |
| LR(0) closure & DFA | `after_dot`, `closure`, `automaton` | `after_dot`, `closure`, `automaton` | Builds canonical LR(0) item sets and `goto` transitions |
| SLR table builder | `enum Action`, `fn actions` | `class Action`, `def build_actions` | Adds shifts, `FOLLOW`-gated reductions, and conflict checks |
| Driver | `fn parse` | `def parse` | Shift/reduce/goto loop returning the list of reduced rule IDs |

## What to look for

Two names separated by a comma produce reductions `[3, 2, 3, 1]` in the example’s rule numbering. The test rejects a trailing comma, an embedded end marker, and an empty list.

## Check your understanding

- Predict the reduction trace for a single `name`. Then change the sample token array and check your prediction.
- For `name ,`, identify which token the parser still needs after shifting the comma. Why can it not reduce the list at end of input?

## Edition mapping and scope

| 1977 Green | 1986 Red | 2006 Purple |
| --- | --- | --- |
| 5–6 | 4 | 4 |

Numbers refer to chapters. Related readings and appendices are marked where applicable. See the [edition guide](../../README.md#edition-guide) for the bibliographic references and chapter coverage.

The program computes item sets and actions for one grammar with hand-supplied FOLLOW sets. A conflicting action is reported as a table construction error.
