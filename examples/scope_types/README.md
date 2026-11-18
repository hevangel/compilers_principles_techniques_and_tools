# Scoped names and type checks

Resolve a shadowed name and use its type to check arithmetic. See the [Rust source](main.rs) and [Python source](main.py).

```sh
cargo run --example scope_types
cargo test --example scope_types
python3 examples/scope_types/main.py
python3 -m unittest examples/scope_types/main.py
```

## What the example teaches

### Why we need this

A lexer and parser only see tokens and tree structure. To the parser, `width + 1` looks completely valid whether `width` was declared as an `Integer`, declared as a `Boolean`, or never declared at all. Even after adding a lookup table, a single flat dictionary cannot model block-structured languages:

- **Shadowing across scopes**: an inner block declaring `width: Integer` must temporarily hide an outer `width: Boolean`, and popping the inner block must restore the outer `Boolean` binding.
- **Duplicate detection within a scope**: declaring `width` twice in the *same* block is an error, and the compiler must preserve the original entry so downstream checks don't cascade with misleading errors.

A **stack of scope maps** searched from innermost to outermost solves both problems cleanly.

`declare` rejects duplicates within one scope. `type_of` searches scopes from innermost to outermost and requires both operands of addition to have integer type.

A **symbol table** associates names with information about their declarations. Here each entry records a type. With **lexical scope**, the binding that applies to a use is determined by the nesting of the source program. An inner declaration can **shadow** an outer declaration with the same name. Two declarations in the same scope are a duplicate in this language.

### Separate declaration from use

1. Each scope is a map (`BTreeMap` in Rust, `dict` in Python), and the scope stack is ordered outermost to innermost. `declare` inserts only into the current scope. An occupied entry produces `Duplicate` and preserves the original binding.
2. To check a name, `type_of` searches the maps in reverse order and stops at the first match. If no scope contains it, the error is `Unknown`.
3. To check an addition, `type_of` recursively checks the two operand expressions. Each must have type `Integer`. An integer literal always has that type, so the example does not need to store its value.

### Trace one expression in two scopes

For `expr = Add(Name("width"), IntegerLiteral)`:

| Active scope stack (outer -> inner) | Lookup `"width"` | Left operand type | Right operand type | `type_of(expr)` result |
| --- | --- | --- | --- | --- |
| `[{width: Boolean}, {width: Integer}]` | Matches inner (`Integer`) | `Integer` | `Integer` | `Ok(Integer)` |
| `[{width: Boolean}]` (after `pop()`) | Matches outer (`Boolean`) | `Boolean` | `Integer` | `Err(ExpectedInteger(Boolean))` |
| `[]` (`Name("absent")`) | Not found | — | — | `Err(Unknown("absent"))` |

This illustrates the job of **semantic analysis**. A parser can recognize a syntactically valid addition even if a later type check rejects its operands.

## How to read the code

| Concept | Rust (`main.rs`) | Python (`main.py`) | Role |
| --- | --- | --- | --- |
| Types & AST | `enum Type`, `enum Expr` | `class Type`, `Name`/`IntegerLiteral`/`Add` | Language types and expression tree |
| Semantic errors | `enum Error` | `DuplicateError`, `UnknownError`, `ExpectedIntegerError` | Distinguishes redeclaration, missing name, and type mismatch |
| Symbol table scope | `type Scope = BTreeMap<&str, Type>` | `dict[str, Type]` | Single lexical block's bindings |
| Declaration check | `fn declare(scope, name, ty)` | `def declare(scope, name, ty)` | Inserts into current scope only if vacant |
| Type checker | `fn type_of(expr, scopes)` | `def type_of(expr, scopes)` | Reverse-searches scope stack and checks `Add` operands |

## What to look for

The same addition succeeds in the inner scope and reports a Boolean operand in the outer scope. The test also checks unknown names and confirms that a rejected declaration leaves the original entry in place.

## Check your understanding

- Predict the difference between declaring `width` a second time in the outer map and declaring it for the first time in the inner map.
- Replace the inner declaration with a differently named variable. Which binding does the original expression use now?

## Edition mapping and scope

| 1977 Green | 1986 Red | 2006 Purple |
| --- | --- | --- |
| 2, 9, 11 | 6–7 | 2, 6 |

Numbers refer to chapters. Related readings and appendices are marked where applicable. See the [edition guide](../../README.md#edition-guide) for the bibliographic references and chapter coverage.

The example checks a small, manually constructed expression tree with two types and exact type matching.
