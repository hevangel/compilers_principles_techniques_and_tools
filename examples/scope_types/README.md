# Scoped names and type checks

Resolve a shadowed name and use its type to check arithmetic. See the [Rust source](main.rs).

```sh
cargo run --example scope_types
cargo test --example scope_types
```

## What the example teaches

`declare` rejects duplicates within one scope. `type_of` searches scopes from innermost to outermost and requires both operands of addition to have integer type.

A **symbol table** associates names with information about their declarations. Here each entry records a type. With **lexical scope**, the binding that applies to a use is determined by the nesting of the source program. An inner declaration can **shadow** an outer declaration with the same name. Two declarations in the same scope are a duplicate in this language.

### Separate declaration from use

1. Each scope is a `BTreeMap`, and a slice of scopes is ordered outermost to innermost. `declare` inserts only into the current scope. An occupied entry produces `Duplicate` and preserves the original binding.
2. To check a name, `type_of` searches the maps in reverse order and stops at the first match. If no scope contains it, the error is `Unknown`.
3. To check an addition, `type_of` recursively checks the two operand expressions. Each must have type `Integer`. An integer literal always has that type, so the example does not need to store its value.

### Trace one expression in two scopes

The outer scope declares `width: Boolean`. The inner scope declares `width: Integer`. When both maps are present, the expression formed by adding an integer literal to `width` has type `Integer`. After the inner scope is popped, the same syntax resolves to the Boolean binding and fails the addition rule.

This illustrates the job of **semantic analysis**. A parser can recognize a syntactically valid addition even if a later type check rejects its operands.

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
