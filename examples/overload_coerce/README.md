# Type Coercions and Two-Pass Overload Resolution

Widen primitive types along an acyclic hierarchy and resolve overloaded functions using bottom-up candidate synthesis and top-down return-type narrowing. See the [Rust source](main.rs) and [Python source](main.py).

```sh
cargo run --example overload_coerce
cargo test --example overload_coerce
python3 examples/overload_coerce/main.py
python3 -m unittest examples/overload_coerce/main.py
```

## What the example teaches

### Why we need this

In a language without overloading or implicit conversions, a single bottom-up AST walk computes one unique type per subexpression. With **implicit widening coercions** (`Bool -> Char -> Int -> Float -> Double`) and **function overloading**, a naive single-pass bottom-up checker breaks down in two ways:

1. **Return-type ambiguity at inner calls**: `f` has both `f(Int) -> Int` and `f(Int) -> Double`. Looking purely bottom-up at `f(1)`, both signatures take `Int` with zero coercions (`cost = 0`), so a single pass cannot decide which one to pick. Only the **enclosing parent context**—such as `expected_ret = Double` or `sink_double(f(1))`—disambiguates it!
2. **Multiple widened matches**: `g` has `g(Float) -> Float` and `g(Double) -> Double`. Given a `Char` argument, *both* overloads are feasible via widening (`Char -> Int -> Float` vs. `Char -> Int -> Float -> Double`), so the compiler must track coercion steps and pick the unique minimum-cost conversion (while rejecting narrowing conversions like `Double -> Int`).

A **two-pass algorithm**—bottom-up candidate synthesis (`synthesize_candidates`) followed by top-down context filtering and minimum-cost selection (`resolve_overload`)—solves both cleanly.

Languages with implicit widening coercions (`Bool -> Char -> Int -> Float -> Double`) and overloaded operators or functions cannot always determine a unique signature in a single bottom-up pass when two signatures share parameter types but differ in return type. A second top-down pass uses the parent's expected type and minimum coercion cost to pick a unique overload or report ambiguity.

### Two-pass resolution

1. `widen` finds the least common supertype along `Bool -> Char -> Int -> Float -> Double` and emits explicit conversion ops (`B2C`, `C2I`, `I2F`, `F2D`).
2. `synthesize_candidates` walks bottom-up, collecting all feasible overload signatures and their accumulated coercion costs.
3. `resolve_overload` narrows the candidate set against the parent's expected return type and picks the unique minimum-cost candidate, failing with `Ambiguous` if ties remain or `NoViableCandidate` if none fit.

### Disambiguating `f(Int)` and `sink_double(f(Int))`

For `f(Int)` (`Int -> Int` vs. `Int -> Double`), `synthesize_candidates` returns both candidates at `cost = 0`. Notice how `sink_double(Double) -> Double` resolves the nested call:

| Expression & Context | Candidate(s) considered | Coercions inserted | Total cost | Outcome |
| --- | --- | --- | --- | --- |
| `f(Int)`, expected `None` | `f(Int)->Int`, `f(Int)->Double` | `[]`, `[]` | `0`, `0` | `Ambiguous` (tie!) |
| `f(Int)`, expected `Int` | `f(Int)->Int` (`Double` cannot narrow to `Int`) | `[]` | `0` | Selects `f(Int) -> Int` |
| `f(Int)`, expected `Double` | `f(Int)->Int` (+ `I2F, F2D`), `f(Int)->Double` | `[I2F, F2D]`, `[]` | `2`, `0` | Selects `f(Int) -> Double` |
| `sink_double(f(Int))` | Arg `f->Int` (+ `I2F, F2D`) vs. `f->Double` (+ `[]`) | `[]` on `f->Double` | `2` vs. `0` | Selects `sink_double(f(Int)->Double)` |
| `g(Char)`, expected `None` | `g(Float)` (`C2I, I2F`) vs. `g(Double)` (`C2I, I2F, F2D`) | `[C2I, I2F]` vs. `[C2I, I2F, F2D]` | `2` vs. `3` | Selects `g(Float)` |

## How to read the code

| Concept | Rust (`main.rs`) | Python (`main.py`) | Role |
| --- | --- | --- | --- |
| Types & coercions | `enum Type`, `enum CoercionOp` | `class Type`, `class CoercionOp` | Linear widening hierarchy and explicit conversion ops |
| Widening path | `next_wider`, `coercion_chain`, `widen` | `NEXT_WIDER`, `coercion_chain`, `widen` | Computes one-way widening chain (rejects narrowing) |
| AST & resolved IR | `Expr`, `ResolvedExpr`, `Signature` | `LitExpr`/`CallExpr`, `Resolved*`, `Signature` | Unannotated calls vs. calls annotated with `CoercionOp`s |
| Pass 1 (bottom-up) | `synthesize_candidates` -> `Vec<Candidate>` | `synthesize_candidates` -> `list[Candidate]` | Collects all viable overloads and their arg coercion costs |
| Pass 2 (top-down) | `resolve_overload` | `resolve_overload` | Filters by `expected_ret` and picks unique min-cost winner |

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
