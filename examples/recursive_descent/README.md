# Recursive-Descent Parsing (§4.4.1)

Translate a rewritten, left-recursion-free EBNF grammar directly into mutually recursive Rust and Python functions—using `while` loops for left-associative repetition, right recursion for right-associative `^`, `LL(2)` lookahead (`peek(1)`) to distinguish assignments from calls, dangling-else nesting, statement recovery, and recursion-depth bounds. See the [Rust source](main.rs) and [Python source](main.py).

```sh
cargo run --example recursive_descent
cargo test --example recursive_descent
python3 examples/recursive_descent/main.py
python3 -m unittest examples/recursive_descent/main.py
```

## What the example teaches

### Why we need this / What problem this solves

After [`grammar_rewrite`](../grammar_rewrite/README.md) eliminates left recursion and factors shared prefixes, how do you actually turn those cleaned-up rules into working code? You have two choices in top-down parsing:

1. Build a `FIRST`/`FOLLOW` table and run a generic stack interpreter ([`ll1`](../ll1/README.md)), **or**
2. Write **one function per nonterminal** that reads tokens directly from the stream—**handwritten recursive-descent parsing (§4.4.1)**.

Most production compilers (GCC's C/C++ front ends, Clang, rustc, V8, and Python's parser generators) choose recursive descent because control flow mirrors the grammar directly, attaching AST construction or bytecode emission takes normal local variables, and checking two tokens ahead (`LL(2)`) takes a single `peek(1)` call instead of inflating an explicit parsing table.

### How BNF/EBNF rules map to functions

Every nonterminal becomes a method on `Parser`, and every grammar combinator maps mechanically to control flow:

| EBNF Construct | Code Pattern in `parse_*` | Purpose in Example |
| --- | --- | --- |
| `Stmt -> 'let' ... \| 'if' ...` | `match peek(0)` | `LL(1)` branch selection by leading token |
| `Ident ':=' Expr` vs `Ident '(' Args ')'` | `peek(0)` is `Ident`, `peek(1)` is `':='` or `'('` | `LL(2)` 2-token lookahead avoids backtracking |
| `Expr -> Term { ('+' \| '-') Term }` | `while peek(0) in {'+', '-'}` | Eliminated left recursion turned into loop for **left-associativity** (`(10 - 3) - 2`) |
| `Power -> Atom [ '^' Unary ]` | Conditional recursive call `parse_unary()` | Right recursion yields **right-associativity** (`2 ^ (3 ^ 2) = 512`) |
| `if Expr then Stmt [else Stmt]` | `parse_stmt()` inside `if`/`then`/`else` + `if peek(0) == 'else'` | Recursive nesting + eager dangling-else resolution |
| `{ Stmt* }` / program errors | `synchronize_to_stmt()` skipping to `;` or `}` | Panic-mode recovery resumes after bad statements |
| `((((...1...))))` | `enter()` / `leave()` depth counter (`max_depth`) | Prevents host call-stack overflow on pathological nesting |

### Handwritten Recursive Descent vs. Table-Driven `LL(1)`

Compare this lesson with [`ll1`](../ll1/README.md), [`tiny_compiler`](../tiny_compiler/README.md), and [`pascal_frontend`](../pascal_frontend/README.md):

| Dimension | Handwritten Recursive Descent (`recursive_descent`) | Table-Driven Predictive (`ll1`) |
| --- | --- | --- |
| Dispatch mechanism | Native host call stack (`parse_stmt` -> `parse_expr` -> `parse_atom`) | Explicit parse stack + `(nonterminal, token)` lookup table |
| Left recursion | Rewritten into EBNF `{ ... }` loops (`while` folds left-associatively) | Requires pure non-left-recursive CFG rules (`A -> beta A'`, `A' -> alpha A' \| eps`) |
| Multi-token lookahead | Trivial `peek(1)` (`LL(2)`) for `x := ...` vs `log(...)` | Strictly 1 token unless table size grows exponentially |
| Errors & recovery | Domain-specific messages (`expected ':=' or '(' after identifier 'x'`) + `;` skip | Generic table-miss diagnostics + `FOLLOW`-set synchronization |
| Full-compiler usage | Powers [`tiny_compiler`](../tiny_compiler/README.md) (stack bytecode) and [`pascal_frontend`](../pascal_frontend/README.md) (scoped TAC) | Often generated automatically from declarative grammar specs |

### How to read the code

| Component | Rust (`main.rs`) | Python (`main.py`) | Role |
| --- | --- | --- | --- |
| Lexer | `fn lex`, `TokKind` | `def lex`, `Token` | Tokenizes keywords, identifiers, integers, `:=`, `^`, and byte offsets |
| Lookahead & guards | `Parser::peek`, `enter`, `leave` | `Parser.peek`, `enter`, `leave` | Supports `LL(1)`/`LL(2)` peeking and enforces `max_depth` |
| Grammar rules | `parse_stmt`, `parse_expr`, `parse_term`, `parse_unary`, `parse_power`, `parse_atom` | Matching `Parser.parse_*` methods | Direct 1-to-1 encoding of the left-recursion-free EBNF rules |
| Error recovery | `synchronize_to_stmt` | `synchronize_to_stmt` | Records `Diagnostic` with byte offset and resumes at `;` / `}` |
| Evaluator | `eval_expr`, `exec_stmts` | `eval_expr`, `exec_stmts` | Verifies precedence (`1 + 2 * 3 ^ 2 - 4 / 2 = 17`), associativity (`5`, `512`), and branches |

## What to look for

Running the demo prints the parsed AST statements, the exact `LL(1)` vs `LL(2)` lookahead trace (`LL(2) peek(0)=ident 'a', peek(1)=':='` vs `LL(2) peek(0)=ident 'log', peek(1)='('`), and evaluates `env={'a': 9, 'p': 512}`, `calls=[('log', [0, 512])]`. Tests also check dangling-else binding on `if 0 then if 1 then emit(1); else emit(2);`, error synchronization across broken statements, and clean failure when parentheses exceed `max_depth`.

## Check your understanding

- Why does `while peek(0) in {'+', '-'}` produce left-associative `(10 - 3) - 2 = 5`, whereas calling `parse_expr()` recursively on the right like `parse_power()` would wrongly compute `10 - (3 - 2) = 9`?
- Look at `peek(1)` in `parse_stmt`: why does an `LL(1)` parser choke when a language has both `x := 1;` and `f(1);` starting with `Ident`, and how would [`grammar_rewrite`](../grammar_rewrite/README.md) rewrite the rule if you insisted on `LL(1)`?

## Larger examples in this repository

Once you see the pattern here, follow the exact same recursive-descent structure inside two larger compilers:

- [`tiny_compiler`](../tiny_compiler/README.md) — Uses `statement`, `expression`, `term`, and `factor` to emit stack-machine bytecode in one pass.
- [`pascal_frontend`](../pascal_frontend/README.md) — Scales recursive descent (`parse_block`, `parse_stmt`, `parse_expr`) up to nested scopes, `array[low..high]` bounds, type checking, and Three-Address Code.

## Edition mapping and scope

| 1977 Green | 1986 Red | 2006 Purple |
| --- | --- | --- |
| 4–5 | 4 (§4.4) | 4 (§4.4.1) |

Numbers refer to chapters. Related readings and appendices are marked where applicable. See the [edition guide](../../README.md#edition-guide) for the bibliographic references and chapter coverage.
