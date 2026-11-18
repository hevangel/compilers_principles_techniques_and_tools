# Integrated `.l` + `.y` Front-End Pipeline Constructing an AST (Ch. 3–5)

Connect a compiled `.l` lexical specification directly to a compiled `.y` LALR(1) parser whose semantic actions build a span-annotated AST (`AstProgram`), then check semantics and lower to executed three-address code (TAC). See the [Rust source](main.rs) and [Python source](main.py).

```sh
cargo run --example lex_yacc_ast
cargo test --example lex_yacc_ast
python3 examples/lex_yacc_ast/main.py
python3 -m unittest examples/lex_yacc_ast/main.py
```

## What the example teaches

### Why we need this / What problem this solves

The standalone [`yacc_generator`](../yacc_generator/README.md) compiles a `.y` spec that reduces directly to evaluated integers, hiding the boundary between parser generation and later compiler stages. Real Lex + Yacc toolchains never execute the target program inside grammar reductions—instead, the `.l` scanner feeds `(TokenKind, lexeme, Span)` semantic values (`yylval`) into the `.y` LALR(1) actions, which assemble a typed **Abstract Syntax Tree (AST)** for downstream semantic analysis, optimization, and IR code generation.

### Pipeline boundary contract

```text
  .l spec ──▶ compile_lex_spec ──▶ lex_source ──▶ Vec<Token { kind, lexeme, Span }>
                                                        │
  .y spec ──▶ compile_yacc_spec ─▶ parse_to_ast ◀───────┘  (YyVal / yylval + Span::merge)
                                        │
                                        ▼
                                   AstProgram ──▶ check_semantics ──▶ lower_to_tac ──▶ eval_tac
```

1. **`.l` Longest Match + Rule Priority:** `letter` matches `[a-zA-Z_][a-zA-Z0-9_]*` (length 6) instead of `let` (length 3), while `let` matches `"let"` (earliest rule at equal length 3).
2. **`.y` LALR(1) + Precedence & Actions:** Compiles `%left`, `%right`, `%prec UMINUS`, and AST-constructor semantic actions (`LetStmt($2, $4)`, `Bin($1, $2, $3)`, `Neg($2)`), merging child spans upward.
3. **End-to-End Diagnostic Spans:** An undeclared variable in Chapter 5 semantic analysis highlights the exact source span captured back in `.l` lexical scanning.

### How to read the code

| Component | Rust (`main.rs`) | Python (`main.py`) | Role |
| --- | --- | --- | --- |
| `.l` compiler & scanner | `compile_lex_spec`, `lex_source` | `compile_lex_spec`, `lex_source` | Parses `.l` rules and produces tokens with `Span`s |
| `.y` LALR(1) & `yylval` | `enum YyVal`, `compile_yacc_spec`, `eval_yacc_action` | `compile_yacc_spec`, `_eval_yacc_action` | Resolves LR(1)→LALR(1) conflicts and builds `AstProgram` |
| Downstream stages | `check_semantics`, `lower_to_tac`, `eval_tac` | `check_semantics`, `lower_to_tac`, `eval_tac` | Verifies declared names & zero-div, lowers AST to TAC, runs VM |

## What to look for

The sample lowers `"let price = 6; let total = (price + 2) * 3 - -1; emit total;"` cleanly to `[25]` and renders a pointed caret error at `line 2, col 10` for `ghost` on `"let x = 5;\nemit x + ghost;\n"`.

## Check your understanding

- Why does storing `Span` inside `Token` and merging spans inside `eval_yacc_action` eliminate the need for semantic analysis to keep the original token array?
- How would adding a new AST node (such as `If` or `While`) change the `.l` spec, `.y` spec, and `YyVal` without touching the LALR(1) table builder?

## Edition mapping and scope

| 1977 Green | 1986 Red | 2006 Purple |
| --- | --- | --- |
| 3, 5–7 | 3–5 | 3–5 |

Numbers refer to chapters. Connects [Lex-Style Scanner Generator](../lex_generator/README.md), [Yacc-Style Parser Generator](../yacc_generator/README.md), and [AST Construction](../ast_construction/README.md).
