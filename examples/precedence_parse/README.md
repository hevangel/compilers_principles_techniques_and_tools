# Ambiguous Precedence and Associativity Resolution

Resolve shift/reduce ambiguities in an expression grammar and the dangling-else construct using precedence tables and shift-beats-reduce rules. See the [Rust source](main.rs) and [Python source](main.py).

```sh
cargo run --example precedence_parse
cargo test --example precedence_parse
python3 examples/precedence_parse/main.py
python3 -m unittest examples/precedence_parse/main.py
```

## What the example teaches

An ambiguous grammar such as `E -> E + E | E * E | -E | id` admits multiple parse trees for `id + id * id`. Rather than encoding precedence tiers into separate nonterminals, a shift-reduce driver consults a precedence and associativity table to choose between shifting the lookahead operator and reducing the operator on top of the stack.

### Resolution rules

1. Higher precedence on the stack reduces before shifting a lower-precedence lookahead (`id * id + id` becomes `(id * id) + id`).
2. Equal precedence uses associativity: `Left` reduces (`id - id - id` becomes `(id - id) - id`), `Right` shifts (`id ^ id ^ id` becomes `id ^ (id ^ id)`), and `NonAssoc` rejects chained comparisons (`a < b < c`).
3. A `%prec` override lets unary `-` bind tighter than `*` while remaining looser than `^`.
4. For `if c1 then if c2 then s1 else s2`, shifting `else` binds it to the innermost open `if`.

### Precedence table

| Operator | Precedence | Associativity |
| --- | --- | --- |
| `<` | 1 | `NonAssoc` |
| `+`, `-` (binary) | 2 | `Left` |
| `*` | 3 | `Left` |
| `-` (unary `%prec`) | 4 | `Right` |
| `^` | 5 | `Right` |

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

The driver resolves conflicts directly on operator/statement stacks instead of generating full LALR(1) conflict states first.
