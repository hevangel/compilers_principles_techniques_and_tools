# Basic-block list scheduling

Issue ready instructions subject to dependencies and per-cycle capacity. See the [Rust source](main.rs).

```sh
cargo run --example list_schedule
cargo test --example list_schedule
```

## What the example teaches

`schedule` scans pending instructions in input order. An instruction is ready after every predecessor has issued and its latency has elapsed. At most the configured number of instructions can issue per cycle.

## What to look for

Two loads can issue together. An independent multiply fills a waiting cycle, and the consumer waits for the sum and product. The tests check one- and two-wide issue, a forward reference, and a dependency cycle.

## Edition mapping and scope

| 1977 Green | 1986 Red | 2006 Purple |
| --- | --- | --- |
| — | — | 10 |

Numbers refer to chapters. Related readings and appendices are marked where applicable. See the [edition guide](../../README.md#edition-guide) for the bibliographic references and chapter coverage.

The dependency graph and latencies are supplied. The model uses interchangeable issue slots and fixed latencies.
