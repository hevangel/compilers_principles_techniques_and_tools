# Lossless Immutable Syntax Trees *(Modern Extension)*

Build a **lossless immutable syntax tree** (Rowan green/red architecture) that preserves every byte of whitespace, newlines, line/block comments, exact literal spelling (`0x2A` vs `42`), and error tokens so any source file—valid or malformed—reconstructs byte-for-byte while exposing a typed AST view and format-preserving functional edits. See the [Rust source](main.rs) and [Python source](main.py).

> **Edition note:** Lossless syntax trees are a **modern extension** rather than a dedicated Dragon Book technique. Contrast directly with [CST vs. AST (§2.2.3, §2.5.1, §2.6.1)](../cst_vs_ast/README.md) and [Incremental Parsing CSTs](../incremental_cst/README.md).

```sh
cargo run --example lossless_syntax_tree
cargo test --example lossless_syntax_tree
python3 examples/lossless_syntax_tree/main.py
python3 -m unittest examples/lossless_syntax_tree/main.py
```

## What the textbook covers vs. modern lossless trees

| Representation | Covered? | Where in the 2006 Purple Dragon Book | Repo lesson |
| --- | --- | --- | --- |
| **Parse tree / CST** | Yes | **§2.2.3** — Parse Trees | [`cst_vs_ast`](../cst_vs_ast/README.md) |
| **CST versus AST** | Yes | **§2.5.1** — Abstract and Concrete Syntax | [`cst_vs_ast`](../cst_vs_ast/README.md) |
| **AST construction** | Yes | **§2.8.2** and **§5.3.1** | [`ast_construction`](../ast_construction/README.md), [`cst_vs_ast`](../cst_vs_ast/README.md) |
| **Syntax-tree variants** | Yes | **§6.1** (Trees vs. DAGs) | [`cst_vs_ast`](../cst_vs_ast/README.md), [`expr_dag`](../expr_dag/README.md) |
| **Lossless syntax tree** | Not as a dedicated technique | **Modern extension** *(Rowan / Tree-sitter)* | [`lossless_syntax_tree`](README.md) (this lesson) |

## What the example teaches

### AST vs. Lossless Syntax Tree on `let x  =  42;  // important comment`

In Dragon Book **§2.6.1**, the lexer strips whitespace and comments before parsing, which is right for traditional batch compilation—but means neither the classical CST nor the AST can reconstruct formatting:

```text
Traditional AST:                     Lossless Syntax Tree:
Let                                  LetDeclaration
├── Name("x")                        ├── Keyword("let")
└── Number(42)                       ├── Whitespace(" ")
                                     ├── Identifier("x")
                                     ├── Whitespace("  ")
                                     ├── Equals("=")
                                     ├── Whitespace("  ")
                                     ├── Number("42")
                                     ├── Semicolon(";")
                                     ├── Whitespace("  ")
                                     └── Comment("// important comment")
```

### Key mechanisms demonstrated here

1. **Full trivia & error preservation (`lex_lossless` / `LosslessParser`):** Preserves `Whitespace`, `Newline` (`\n` / `\r\n`), `Comment` (`// ...`), `BlockComment` (`/* ... */`), exact number lexemes (`0x2A`), and `ErrorToken`/`ErrorNode` wrappers—so `tree.text() == original_src` holds byte-for-byte even with syntax errors.
2. **Structural interning (`NodeCache`):** Deduplicates repeated `GreenToken`s (like `" "`) and immutable `GreenNode` subtrees just like Rowan's `NodeCache`.
3. **Parent-linked `SyntaxNode` (Red) cursors:** Computes absolute byte `TextSpan`s (`start..end`), `(line, col)` locations, and `ancestors()` lazily on top of relative-width `GreenNode`s.
4. **Typed semantic views (`view_let_declaration`):** Gives semantic passes a clean AST-like summary (`name`, `name_span`, `eval_value`) by skipping trivia while staying pinned to the lossless tree.
5. **Format-preserving functional edits (`rewrite_tokens`):** Renaming `x` → `answer` and changing `42` → `0x2A` produces `"let answer  =  0x2A;  // important comment"`—keeping the double spaces and comment intact and sharing unchanged downstream `GreenNode`s by pointer (`Rc::ptr_eq` / `is`).

### Modern reference architectures: Rowan vs. Tree-sitter

- **Rowan** (used by `rust-analyzer`): Focuses specifically on **lossless, immutable green/red syntax trees** (`GreenNode` + `SyntaxNode`), structural interning (`NodeCache`), typed `ast::*` wrappers, and functional tree replacement for refactorings and formatters.
- **Tree-sitter**: Focuses on **incremental GLR parsing** (byte-range re-lexing and subtree reuse across live editor keystrokes) over concrete syntax trees with language-agnostic query grammars (see [`incremental_cst`](../incremental_cst/README.md) and [`glr_parsing`](../glr_parsing/README.md)).

### How to read the code

| Component | Rust (`main.rs`) | Python (`main.py`) | Role |
| --- | --- | --- | --- |
| Lossless tokens & trivia | `enum SyntaxKind`, `lex_lossless` | `lex_lossless`, `TRIVIA_KINDS` | Emits every byte including whitespace, newlines, comments, errors |
| Immutable Green cache | `struct NodeCache`, `GreenNode` | `class NodeCache`, `GreenNode` | Deduplicates & builds relative-width lossless trees |
| Parent-aware Red cursor | `struct SyntaxNode`, `line_col` | `class SyntaxNode`, `line_col` | Computes spans, line/col, parent chains |
| AST facade & functional edit | `view_let_declaration`, `rewrite_tokens` | `view_let_declaration`, `rewrite_tokens` | Extracts semantics ignoring trivia; rewrites without losing formatting |

## What to look for

Running the example prints the side-by-side traditional AST and lossless `LetDeclaration` tree for `let x  =  42;  // important comment`, confirms `tree.text() == sample` byte-for-byte, resolves `name_span = 4..5` via the Red cursor, and rewrites the binding into `"let answer  =  0x2A;  // important comment"` while preserving all double spaces and comments verbatim.

## Check your understanding

- Why is a classical Dragon Book parse tree (§2.2.3) still insufficient for a formatter or IDE refactoring tool?
- Why must a Rowan-style `GreenNode` omit absolute byte offsets and parent pointers if unchanged statements are to be pointer-shared across edits?

## Edition mapping and scope

| 1977 Green | 1986 Red | 2006 Purple |
| --- | --- | --- |
| — *(modern extension; cf. Ch. 5)* | — *(modern extension; cf. Ch. 4–5)* | — *(modern extension; cf. §2.2.3, §2.5.1, §2.6.1)* |

Numbers refer to chapters. Related readings and appendices are marked where applicable. See the [edition guide](../../README.md#edition-guide) for the bibliographic references and chapter coverage.
