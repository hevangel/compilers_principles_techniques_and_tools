# Pratt Parsing and Precedence Climbing *(Modern Extension)*

Parse prefix, infix, postfix, right-associative, and mixfix (`? :`) expressions concisely with `(lbp, rbp)` binding powers (`nud` / `led`), alongside a precedence-climbing implementation producing the identical AST. See the [Rust source](main.rs) and [Python source](main.py).

> **Edition note:** Pratt (1973) / Precedence Climbing (1990s) are modern practical parsing techniques placed here under **Advanced parsing techniques** rather than classical Dragon Book chapters.

```sh
cargo run --example pratt_parsing
cargo test --example pratt_parsing
python3 examples/pratt_parsing/main.py
python3 -m unittest examples/pratt_parsing/main.py
```

## What the example teaches

### Why we need this / What problem this solves

Classical recursive descent forces one separate function per precedence tier (`parse_assign`, `parse_ternary`, `parse_add`, `parse_mul`, `parse_pow`, `parse_unary`, `parse_postfix`). Adding a new operator tier means editing multiple call chains, whereas table-driven LR requires generator tooling.

**Pratt parsing** (Top-Down Operator Precedence) collapses all precedence tiers into **one loop** parameterized by integer binding powers:

```text
  left = nud(tok)                      // prefix / atom
  while min_bp < lbp(peek()) {         // compare caller min_bp vs next operator's lbp
      op = advance()
      left = led(left, op)             // infix / postfix / ternary consumes rhs with rbp
  }
```

### Binding powers used

| Construct | Kind | `(lbp, rbp)` | Effect |
| --- | --- | --- | --- |
| `=` | Right-assoc infix | `(2, 1)` | `a = b = c` parses as `(a = (b = c))` |
| `? :` | Right-assoc mixfix | `(4, 3)` (then-branch `0`) | Nested ternary selects right-to-left cleanly |
| `+`, `-` | Left-assoc infix | `(10, 11)` | `1 - 2 - 3` parses as `((1 - 2) - 3)` |
| `*`, `/` | Left-assoc infix | `(20, 21)` | Higher than `+`/`-` |
| `-` (unary) | Prefix (`nud`) | `(_, 25)` | Tighter than `*`, looser than `^`: `-2 ^ 2 == -(2 ^ 2)` |
| `^` | Right-assoc infix | `(30, 29)` | `2 ^ 3 ^ 2 == 2 ^ (3 ^ 2)` |
| `!` (factorial) | Postfix (`led`) | `(40, _)` | Tighter than prefix `-` and `^` |

### How to read the code

| Component | Rust (`main.rs`) | Python (`main.py`) | Role |
| --- | --- | --- | --- |
| Pratt parser | `struct PrattParser`, `nud`, `led`, `parse_bp` | `class PrattParser` | Binding-power-driven top-down parser |
| Precedence climbing | `struct ClimbParser`, `parse_climb` | `class ClimbParser` | Equivalent loop-based precedence climbing for direct AST comparison |
| Evaluator | `fn eval_expr` | `def eval_expr` | Evaluates concrete values to prove associativity & precedence |

## What to look for

Both `parse_pratt` and `parse_climb` produce identical parenthesizations and values (`-2 ^ 2 + 3! * 4 -> ((-(2 ^ 2)) + ((3!) * 4)) == 20`, `2 ^ 3 ^ 2 == 512`).

## Check your understanding

- Why does a left-associative operator with `lbp = 10` pass `rbp = 11` to `parse_bp`, while right-associative `^` with `lbp = 30` passes `rbp = 29`?
- How would changing unary `-`'s `rbp` from `25` to `35` change the parse tree of `-2 ^ 2`?

## Edition mapping and scope

| 1977 Green | 1986 Red | 2006 Purple |
| --- | --- | --- |
| — *(modern extension; cf. §5.3)* | — *(modern extension; cf. §4.6)* | — *(modern extension; cf. §4.8)* |
