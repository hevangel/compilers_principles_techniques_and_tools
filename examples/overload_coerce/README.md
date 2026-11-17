# Type Coercions and Two-Pass Overload Resolution

Widen primitive types along an acyclic hierarchy and resolve overloaded functions using bottom-up candidate synthesis and top-down return-type narrowing. See the [Rust source](main.rs) and [Python source](main.py).

```sh
cargo run --example overload_coerce
cargo test --example overload_coerce
python3 examples/overload_coerce/main.py
python3 -m unittest examples/overload_coerce/main.py
```

## What the example teaches

Languages with implicit widening coercions (`Bool -> Char -> Int -> Float -> Double`) and overloaded operators or functions cannot always determine a unique signature in a single bottom-up pass when two signatures share parameter types but differ in return type. A second top-down pass uses the parent's expected type and minimum coercion cost to pick a unique overload or report ambiguity.

### Two-pass resolution

1. `widen` finds the least common supertype along `Bool -> Char -> Int -> Float -> Double` and emits explicit conversion ops (`B2C`, `C2I`, `I2F`, `F2D`).
2. `synthesize_candidates` walks bottom-up, collecting all feasible overload signatures and their accumulated coercion costs.
3. `resolve_overload` narrows the candidate set against the parent's expected return type and picks the unique minimum-cost candidate, failing with `Ambiguous` if ties remain or `NoViableCandidate` if none fit.

### Disambiguating `f(Int)` (`Int -> Int` vs. `Int -> Double`)

| Context | Selected overload | Coercion cost |
| --- | --- | --- |
| Expected `Int` | `f(Int) -> Int` | `0` |
| Expected `Double` | `f(Int) -> Double` | `0` (vs. `2` for `Int -> Double`) |
| No expected type | Ambiguous | Tie (`0 == 0`) |

## What to look for

The worked trace demonstrates `widen(Int, Float)`, context-driven disambiguation of `f(Int)`, minimum-cost widening selection on `g(Char)` (`Char -> Float` costs 2 steps vs. 3 for `Double`), and nested propagation in `sink_double(f(Int))`. Tests also verify rejection of narrowing conversions (`Double` to `Int`).

## Check your understanding

- Why does `f(Int)` succeed inside `sink_double(f(Int))` even though `f(Int)` is ambiguous in isolation?
- What happens if two overloads require the exact same total number of coercion steps for a given argument?

## Edition mapping and scope

| 1977 Green | 1986 Red | 2006 Purple |
| --- | --- | --- |
| — | 6 | 6 |

Numbers refer to chapters. Related readings and appendices are marked where applicable. See the [edition guide](../../README.md#edition-guide) for the bibliographic references and chapter coverage.

Overload resolution uses a linear widening chain and single-expression call trees rather than arbitrary graph-shaped coercion lattices.
