# Panic-mode recovery

Report a malformed statement and resume at the next semicolon. See the [Rust source](main.rs).

```sh
cargo run --example recovery
cargo test --example recovery
```

## What the example teaches

`recover` treats the statement delimiter as a synchronization point. After a malformed statement, it discards the rest of that statement and continues scanning the following one. Diagnostics carry starting byte offsets.

## What to look for

The sample recovers values `[7, 19]` from a mix of valid and invalid statements. The test also checks byte offsets after a multibyte character and a missing final semicolon.

## Edition mapping and scope

| 1977 Green | 1986 Red | 2006 Purple |
| --- | --- | --- |
| 11 | 4 | 4 |

Numbers refer to chapters. Related readings and appendices are marked where applicable. See the [edition guide](../../README.md#edition-guide) for the bibliographic references and chapter coverage.

The language is a sequence of flat `emit N;` statements. Semicolons are always safe synchronization points in this grammar.
