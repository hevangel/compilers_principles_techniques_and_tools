# Ambiguous Precedence and Associativity Resolution

Resolve shift/reduce ambiguities in an expression grammar and the dangling-else construct using precedence tables and shift-beats-reduce rules. See the [Rust source](main.rs) and [Python source](main.py).

```sh
cargo run --example precedence_parse
cargo test --example precedence_parse
python3 examples/precedence_parse/main.py
python3 -m unittest examples/precedence_parse/main.py
```

## What the example teaches

### Why we need this / What problem this solves

A naive unambiguous expression grammar forces you to stratify every precedence level into its own nonterminal (`Expr -> Expr + Term`, `Term -> Term * Factor`, `Factor -> ...`). Adding 10+ operator tiers (like C's precedence table) produces a bloated grammar and long chains of unit reductions. Worse, some ambiguities—like the **dangling-else** (`if c1 then if c2 then s1 else s2`)—are awkward to disambiguate purely in a grammar.

Writing the natural **ambiguous grammar** `E -> E + E | E * E | -E | id` keeps the grammar tiny, but an ordinary LR generator flags **shift/reduce conflicts**: after reading `id + id`, seeing `*` leaves the parser unsure whether to *reduce* `id + id` first or *shift* `*`. **Operator-precedence / disambiguation rules** solve this cleanly by comparing the top operator on the stack against the lookahead operator via a small precedence/associativity table (plus `%prec` for unary `-`, and "shift beats reduce" for `else`).

### Resolution rules

1. Higher precedence on the stack reduces before shifting a lower-precedence lookahead (`id * id + id` becomes `(id * id) + id`).
2. Equal precedence uses associativity: `Left` reduces (`id - id - id` becomes `(id - id) - id`), `Right` shifts (`id ^ id ^ id` becomes `id ^ (id ^ id)`), and `NonAssoc` (`Neither`) rejects chained comparisons (`a < b < c`).
3. A `%prec` override lets unary `-` (precedence `4`) bind tighter than `*` (`3`) while remaining looser than `^` (`5`).
4. For `if c1 then if c2 then s1 else s2`, shifting `else` when `IfHeader` sits below `Reduced(s1)` binds `else` to the innermost open `if`.

### Precedence table and stack trace

| Operator | Precedence | Associativity |
| --- | --- | --- |
| `<` | `1` | `NonAssoc` |
| `+`, `-` (binary) | `2` | `Left` |
| `*` | `3` | `Left` |
| `-` (unary `%prec`) | `4` | `Right` |
| `^` | `5` | `Right` |

Trace for `id + id * id`:

| Remaining tokens | Value stack | Op stack | Decision |
| --- | --- | --- | --- |
| `id + id * id` | `[id]` | `[]` | Shift `id` (`expect_operand = false`) |
| `+ id * id` | `[id]` | `[+]` | Op stack empty -> push `+` |
| `id * id` | `[id, id]` | `[+]` | Shift `id` |
| `* id` | `[id, id]` | `[+, *]` | `prec(+) = 2 < prec(*) = 3` -> **shift** `*` |
| `id` | `[id, id, id]` | `[+, *]` | Shift `id` |
| *(EOF)* | `[(id + (id * id))]` | `[]` | Reduce `*`, then reduce `+` |

### How to read the code

| Component | Rust (`main.rs`) | Python (`main.py`) | Role |
| --- | --- | --- | --- |
| Operator & AST types | `Op`, `Assoc`, `Token`, `Expr`, `Stmt` | `Op`, `Assoc`, `Expr` classes, `Stmt` classes | Precedence keys, associativity modes, and expression/if-statement ASTs |
| Precedence table | `default_prec_table`, `should_reduce` | `default_prec_table`, `should_reduce` | Compares `(top_prec, top_assoc)` vs. `lookahead_prec` |
| Expression parser | `tokenize_expr`, `reduce_top`, `parse_expr` | `tokenize_expr`, `parse_expr` | Shunting-yard / operator-precedence parser with `expect_operand` unary detection |
| Dangling-else parser | `fn parse_if_stmts` | `def parse_if_stmts` | Explicit shift-reduce stack resolving `else` via shift-beats-reduce |

## What to look for

The worked trace prints explicit parenthesizations for binary precedence, right-associative exponentiation, left-associative subtraction, unary negation precedence, and dangling-else attachment. Tests verify all precedence rules and confirm that chained `<` is rejected as a non-associative error.

## Check your understanding

- Why does `-id ^ id` group as `-(id ^ id)` while `-id * id` groups as `(-id) * id`?
- How would changing `shift beats reduce` to `reduce beats shift` on `else` alter `if c1 then if c2 then s1 else s2`?

## Edition mapping and scope

| 1977 Green | 1986 Red | 2006 Purple |
| --- | --- | --- |
| 5–6 | 4 (§4.6, §4.8) | 4 |

Numbers refer to chapters. Related readings and appendices are marked where applicable. See the [edition guide](../../README.md#edition-guide) for the bibliographic references and chapter coverage.

The driver resolves shift/reduce conflicts directly on operator/statement stacks via `%left`/`%right`/`%nonassoc`/`%prec`-style declarations. For Floyd's classical operator-precedence relation matrix (`<·`, `=·`, `·>`) and precedence functions (`1977` §5.3 / `1986` §4.6), see [Classical Operator-Precedence Parsing](../operator_precedence_parser/README.md); for modern `nud`/`led` binding powers, see [Pratt Parsing and Precedence Climbing](../pratt_parsing/README.md).
