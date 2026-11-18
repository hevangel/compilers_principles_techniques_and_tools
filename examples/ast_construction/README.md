# Abstract Syntax Tree (AST) Construction and Compiler Representations (§4.9 -> Ch. 5–6)

Contrast a verbose Concrete Parse Tree (CST) against a compact Abstract Syntax Tree (AST), construct the **exact same spanned AST** using both handwritten recursive descent and Yacc-style LALR(1) semantic actions (`$N`), walk it with the Visitor pattern, report source-pinned semantic errors, and lower the AST into Three-Address Code (TAC). See the [Rust source](main.rs) and [Python source](main.py).

```sh
cargo run --example ast_construction
cargo test --example ast_construction
python3 examples/ast_construction/main.py
python3 -m unittest examples/ast_construction/main.py
```

## What the example teaches

### Why we need ASTs in the compiler pipeline

```text
Lex (tokens) -> Yacc / RD (parse tree) -> AST -> Semantic Analysis -> IR (TAC) -> Codegen
```

Parsers check grammar structure using concrete rules (`Expr -> Term -> Unary -> Factor`, `Stmt -> 'let' Ident '=' Expr ';'`), producing a **Concrete Parse Tree (CST)** full of grammar scaffolding and punctuation tokens (`=`, `(`, `)`, `;`). Carrying that parse tree straight into semantic analysis, optimization, and code generation would force every pass to wade through redundant wrapper nodes and syntactic noise.

An **Abstract Syntax Tree (AST)** keeps only the meaningful operators, operands, statements, and source **`Span`** locations:

| Representation | Focus | What gets kept vs. dropped | Where taught in this repo |
| --- | --- | --- | --- |
| **Tokens** (`Token`) | Lexical stream | Keeps kinds, lexemes, byte/line spans; drops whitespace & comments | [`lexer`](../lexer/README.md), [`lex_generator`](../lex_generator/README.md) |
| **Parse Tree / CST** (`CstNode`) | Exact grammar derivation | Keeps every nonterminal chain (`Expr -> Term -> Unary -> Factor`) and `(`, `)`, `;` *(drops trivia per §2.6.1)* | [`cst_vs_ast`](../cst_vs_ast/README.md), [`ast_construction`](README.md), [`ll1`](../ll1/README.md) |
| **AST** (`AstExpr`, `AstStmt`) | Clean semantic structure | Drops punctuation & unit chains; encodes precedence/grouping in tree shape + `Span` | [`cst_vs_ast`](../cst_vs_ast/README.md), [`ast_construction`](README.md) (this lesson) |
| **Lossless Syntax Tree** | Modern IDE / formatter CST | Preserves whitespace, comments, exact token spelling & error spans byte-for-byte | [`lossless_syntax_tree`](../lossless_syntax_tree/README.md), [`incremental_cst`](../incremental_cst/README.md) |
| **Symbol Table & Attributes** | Names & types | Maps declared identifiers to scopes, byte widths, and types | [`scope_types`](../scope_types/README.md), [`attributes`](../attributes/README.md) |
| **TAC IR & SSA** (`TacInst`) | Flat virtual-register ops | Flattens nested AST trees into `t0 := 2 + 3` instructions and `phi` nodes | [`record_ir`](../record_ir/README.md), [`ssa`](../ssa/README.md) |
| **CFG** | Basic-block control flow | Groups IR into blocks and branch edges for data-flow & scheduling | [`natural_loops`](../natural_loops/README.md), [`trace_schedule`](../trace_schedule/README.md) |

### Seven core AST concepts demonstrated here

1. **Parse Tree (CST) vs. AST (`cst_node_count` vs. `AstStatsVisitor`):** For `let base = (2 + 3) * 4;\nemit base + -1;`, the CST takes **more than twice as many nodes** as the compact AST (`CstNode` chain wrappers + `;` + `(` + `)` vs. clean `AstStmt` / `AstExpr`).
2. **Designing AST nodes in Rust and Python:** Algebraic `enum AstExpr` with `Box<AstExpr>` (`Rust`) and frozen `@dataclass` unions (`Python`) guarantee every binary operator has two operands and every node carries its `Span`.
3. **Constructing an AST via Recursive Descent (`RdParser`):** `parse_expr`, `parse_term`, `parse_unary`, and `parse_atom` build both `CstNode` (for teaching) and `AstProgram` simultaneously.
4. **Constructing the exact same AST via Yacc semantic actions (`parse_yacc_ast`):** A self-contained LALR(1) table (`%left '+' '-'`, `%left '*' '/'`, `%prec UMINUS`) drives a semantic value stack (`vals`) running `{ AstExpr::Bin($1, $3) }` on every reduction—and `assert_eq!(rd_ast, yacc_ast)` proves both front ends yield bit-for-bit identical trees and spans!
5. **Preserving source locations (`Span::merge`):** Every internal node merges its children's `(start, end, line, col)` spans so semantic passes report `error at line 2, col 10: use of undeclared variable 'missing_var'` with `^^^` underlines long after the token list is discarded.
6. **Walking an AST with visitors (`AstVisitor`):** `PrettyVisitor`, `AstStatsVisitor`, `SemanticChecker`, and `TacLowerer` each traverse the AST independently without cluttering the AST node types.
7. **Passing the AST to Chapters 5–6 (`SemanticChecker` -> `TacLowerer` -> `eval_tac`):** Checks scope declarations and division-by-constant-zero, then lowers the AST into flat Three-Address Code (`t0 := 2 + 3`, `t2 := t0 * 4`, ...) and executes it (`[19]`).

### How to read the code

| Component | Rust (`main.rs`) | Python (`main.py`) | Role |
| --- | --- | --- | --- |
| Source locations | `struct Span`, `format_diagnostic` | `class Span`, `format_diagnostic` | Tracks `(start, end, line, col)`, merges spans, renders caret diagnostics |
| CST vs. AST types | `enum CstNode`, `enum AstExpr`, `enum AstStmt` | `CstRule`/`CstLeaf`, `IntExpr`/`BinExpr`/etc. | Concrete grammar tree vs. compact AST nodes |
| RD builder | `RdParser` | `RdParser` | Top-down recursive-descent builder emitting both CST and AST |
| Yacc LALR(1) builder | `build_yacc_ast_tables`, `parse_yacc_ast` | `build_yacc_ast_tables`, `parse_yacc_ast` | Bottom-up shift-reduce semantic actions producing identical `AstProgram` |
| AST Visitors | `trait AstVisitor` (`PrettyVisitor`, `SemanticChecker`) | `class AstVisitor` (`PrettyVisitor`, `SemanticChecker`) | Decoupled tree traversal for printing, node stats, and Chapter 5 checking |
| Chapter 6 IR lowering | `TacLowerer`, `eval_tac` | `TacLowerer`, `eval_tac` | Translates checked AST to flat Three-Address Code and executes it |

## What to look for

Running the demo prints the exact CST vs. AST node count comparison, the concrete parse tree hierarchy, the `PrettyVisitor` S-expression with `@L:C` spans proving `RD == Yacc`, and the lowered TAC instructions executing to `[19]`. Tests also verify undeclared-variable and constant-zero diagnostics pinned to exact line/column spans.

## Check your understanding

- Why does `(1 + 2)` disappear as a node type in the AST (`Bin(Mul, Bin(Add, 1, 2), 3)`), whereas in the CST `Factor(Paren)` must preserve `'('` and `')'`?
- Why does storing `Span` directly on every AST node avoid having to keep raw token vectors or re-lex the file during Chapter 5–6 semantic checks?

## Edition mapping and scope

| 1977 Green | 1986 Red | 2006 Purple |
| --- | --- | --- |
| 5–7 | 4–5 (§4.9, §5.2) | 4–6 (§4.9, §5.3, §6.2) |

Numbers refer to chapters. Related readings and appendices are marked where applicable. See the [edition guide](../../README.md#edition-guide) for the bibliographic references and chapter coverage.
