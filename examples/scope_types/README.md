# Scoped names and type checks

Resolve a shadowed name and use its type to check arithmetic. See the [Rust source](main.rs).

```sh
cargo run --example scope_types
cargo test --example scope_types
```

## What the example teaches

`declare` rejects duplicates within one scope. `type_of` searches scopes from innermost to outermost and requires both operands of addition to have integer type.

## What to look for

The same addition succeeds in the inner scope and reports a Boolean operand in the outer scope. The test also checks unknown names and confirms that a rejected declaration leaves the original entry in place.

## Edition mapping and scope

| 1977 Green | 1986 Red | 2006 Purple |
| --- | --- | --- |
| 2, 9, 11 | 6–7 | 2, 6 |

Numbers refer to chapters. Related readings and appendices are marked where applicable. See the [edition guide](../../README.md#edition-guide) for the bibliographic references and chapter coverage.

The example checks a small, manually constructed expression tree with two types and exact type matching.
