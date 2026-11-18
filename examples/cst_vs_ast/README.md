# Concrete Parse Trees (CST) vs. Abstract Syntax Trees (AST) (§2.2.3, §2.5.1, §2.6.1, §5.3.1, §6.1)

Trace concrete grammar derivations (`=>_lm`) into a full **Concrete Syntax Tree (CST)**, lower it into a compact **Abstract Syntax Tree (AST)** and a **§6.1 value-numbered DAG**, rebuild a canonical formatted CST back from the AST, and see directly why classical Dragon Book CSTs are **not lossless**. See the [Rust source](main.rs) and [Python source](main.py).

```sh
cargo run --example cst_vs_ast
cargo test --example cst_vs_ast
python3 examples/cst_vs_ast/main.py
python3 -m unittest examples/cst_vs_ast/main.py
```

## What the textbook covers

| Representation | Covered? | Where in the 2006 Purple Dragon Book | Repo lesson |
| --- | --- | --- | --- |
| **Parse tree / CST** | Yes | **§2.2.3** — Parse Trees | [`cst_vs_ast`](README.md) (this lesson) |
| **CST versus AST** | Yes | **§2.5.1** — Abstract and Concrete Syntax | [`cst_vs_ast`](README.md) (this lesson) |
| **AST construction** | Yes | **§2.8.2** and **§5.3.1** | [`cst_vs_ast`](README.md), [`ast_construction`](../ast_construction/README.md) |
| **Syntax-tree variants** | Yes | **§6.1** (Trees vs. DAGs) | [`cst_vs_ast`](README.md), [`expr_dag`](../expr_dag/README.md) |
| **Lossless syntax tree** | Not as a dedicated technique | **Modern extension** *(Rowan / Tree-sitter)* | [`lossless_syntax_tree`](../lossless_syntax_tree/README.md), [`incremental_cst`](../incremental_cst/README.md) |

The Dragon Book explicitly identifies a **parse tree** as a **concrete syntax tree (CST)**. However, a **classical CST is not necessarily lossless**: Dragon Book **§2.6.1** removes whitespace and comments during lexical analysis before the parser ever runs. That design is ideal for batch compilation, whereas formatters, linters, and refactoring tools need a [Lossless Syntax Tree](../lossless_syntax_tree/README.md) to preserve every byte.

## What the example teaches

### 1. Why classical CSTs lose formatting (`§2.6.1`)

Consider this source line:

```text
let x  =  42;  // important comment
```

1. **Classical lexer (`§2.6.1`):** Strips the 7 whitespace bytes (`" "` and `"  "`) and the comment (`"// important comment"`), handing the parser `[LET, IDENT("x"), EQ, INT("42"), SEMI]`.
2. **Classical CST (`§2.2.3`):** Preserves grammar rules (`Stmt -> 'let' Ident '=' Expr ';'`, `Expr -> Term -> Factor -> Int(42)`), punctuation (`=`, `;`), and the exact leftmost derivation (`=>_lm`).
3. **Canonical round-trip limitation:** Pretty-printing `ast_to_canonical_cst(ast)` emits `let x = 42;`—valid code with identical AST semantics, **not** identical bytes.

### 2. Transformations between CST, AST, and §6.1 DAG

| Transformation | Direction | What happens |
| --- | --- | --- |
| `CstParser` (`§2.2.3`) | Tokens → CST | Expands `Program -> StmtList -> Stmt -> Expr -> Term -> Factor` and matches terminals |
| `leftmost_derivation` (`§2.2.3`) | CST → `=>_lm` | Replays the pre-order expansion from `Program` to `let x = 42 ;` |
| `cst_to_ast` (`§2.5.1`, `§2.8.2`, `§5.3.1`) | CST → AST | Collapses unit chains (`Expr -> Term -> Factor`) and drops keywords, `=`, `;`, `(`, `)` |
| `ast_to_canonical_cst` | AST → CST | Inserts minimal parentheses (`(1 + 2) * 3`, `x - (4 - 1)`) and re-wraps grammar levels |
| `ExprDag::insert` (`§6.1`) | AST → DAG | Deduplicates repeated subtrees: `(a + b) * 4 + (a + b) * 4` shrinks from **11** AST nodes to **6** DAG nodes |

### How to read the code

| Component | Rust (`main.rs`) | Python (`main.py`) | Role |
| --- | --- | --- | --- |
| Classical §2.6.1 lexer | `fn lex_strip_trivia` | `def lex_strip_trivia` | Removes whitespace and `//` comments, counting discarded bytes |
| Parse tree & derivations | `enum CstNode`, `fn leftmost_derivation` | `CstRule`, `def leftmost_derivation` | Builds grammar parse trees and traces `=>_lm` |
| CST → AST | `fn cst_to_ast` | `def cst_to_ast` | Extracts clean `Let` / `BinOp` AST |
| AST → Canonical CST | `fn ast_to_canonical_cst`, `pretty_print_cst` | `def ast_to_canonical_cst`, `pretty_print_cst` | Rebuilds canonical CST & precedence-minimal source |
| §6.1 DAG variant | `struct ExprDag` | `class ExprDag` | Value-numbers repeated AST subexpressions into a shared DAG |

## What to look for

Running the example prints the stripped trivia counts for `let x  =  42;  // important comment`, the full 11-node classical CST and its `=>_lm` derivation steps, the 4-node compact AST (`Let`, `Name("x")`, `Number(42)`), the mismatch between canonical output (`"let x = 42;"`) and the original source, and the §6.1 tree-vs-DAG comparison.

## Check your understanding

- Why does a classical parse tree (CST) for `let x  =  42;  // important comment` still fail a byte-for-byte round-trip even though it retains `let`, `=`, and `;`?
- When converting an AST back to a canonical CST (`ast_to_canonical_cst`), why must `x - (4 - 1)` keep parentheses around `(4 - 1)` even though `-` and `-` share the same precedence?
- Continue directly to [Lossless Syntax Trees *(Modern Extension)*](../lossless_syntax_tree/README.md) and [AST Construction](../ast_construction/README.md).

## Edition mapping and scope

| 1977 Green | 1986 Red | 2006 Purple |
| --- | --- | --- |
| 4–5, 7 | 2, 4, 5, 8 | 2, 5–6 (§2.2.3, §2.5.1, §2.6.1, §2.8.2, §5.3.1, §6.1) |

Numbers refer to chapters. Related readings and appendices are marked where applicable. See the [edition guide](../../README.md#edition-guide) for the bibliographic references and chapter coverage.
