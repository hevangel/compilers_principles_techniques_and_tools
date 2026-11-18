# Available Expressions, Global CSE, and Copy Propagation

Compute available expressions via forward must-analysis initialized to the universal set, eliminate global common subexpressions, and propagate copies. See the [Rust source](main.rs) and [Python source](main.py).

```sh
cargo run --example available_exprs
cargo test --example available_exprs
python3 examples/available_exprs/main.py
python3 -m unittest examples/available_exprs/main.py
```

## What the example teaches

### Why we need this and when it is sound

Local value numbering eliminates duplicate expressions inside a single basic block, but real programs routinely repeat expressions across branches and loops—for example, computing `x + y` in both arms of an `if`/`else` and then computing `x + y` again after the join, or evaluating a loop-invariant expression on every iteration.

An expression `x op y` is **available** at a program point `p` if **every** control-flow path from the function entry to `p` evaluates `x op y`, and neither `x` nor `y` has been reassigned since the last such evaluation.

Reusing a previously computed value across blocks is **sound** only under strict **forward must-analysis** rules:

1. **Must (intersection) confluence:** `IN[B]` is the intersection (`∩`) of `OUT[P]` across all predecessors `P` of `B`. If *even one* incoming path skipped computing `x + y` or overwrote `x` or `y`, the old temporary does not hold the right value on that path and cannot be reused.
2. **Universal-set initialization (`U`) for non-entry blocks:** Unlike may-analyses that start at `∅`, a must-analysis starts `OUT[B]` (`B != entry`) at the universal set `U` of all expressions so back edges do not immediately wipe out expressions computed before a loop; meanwhile `IN[entry]` stays `∅` so nothing is assumed available before execution starts.
3. **Operand canonicalization & kill order:** Commutative `+` and `*` sort their operands alphabetically so `x + y` and `y + x` match. Self-redefining statements like `x = x + y` kill `x + y` after evaluation because `x` itself changes.

### Trace the diamond and paired passes

In `sample_diamond()` (`B0 -> {B1, B2} -> B3`), notice how `x + y` survives the merge at `B3` while `a * b` does not, and how CSE + copy propagation clean up `B3`:

| Block | Original statements | `in_avail` | `out_avail` | After `global_cse` + `copy_propagation` |
| --- | --- | --- | --- | --- |
| `B0` | `(branch)` | `{}` | `{}` | `(branch)` |
| `B1` | `s1 = x + y`; `m1 = a * b` | `{}` | `{x+y, a*b}` | `s1 = x + y`; `_cse_0 = s1`; `m1 = a * b` |
| `B2` | `s2 = y + x` | `{}` | `{x+y}` | `s2 = y + x`; `_cse_0 = s2` |
| `B3` | `s3 = x + y`; `m2 = a * b`; `res = s3 - m2`; `c = 1`; `out = res + c`; `return out` | `{x+y}` | `{x+y, a*b, res+c, s3-m2}` | `s3 = _cse_0`; `m2 = a * b`; `res = _cse_0 - m2`; `c = 1`; `out = res + c`; `return out` |

### Two paired global passes

1. **Global CSE (`global_cse`):** Any expression in `in_avail[B]` (and not locally killed before its use) reuses a single `_cse_k` temporary materialized right after earlier non-redundant computations of that expression.
2. **Copy Propagation (`copy_propagation`):** Because CSE replaces `s3 = x + y` with a copy `s3 = _cse_0`, a second forward must-analysis tracks `(dst, src)` copies valid along all paths (killed whenever either `dst` or `src` is overwritten) and substitutes `_cse_0` directly into `res = _cse_0 - m2`, leaving `s3` dead for later dead-code elimination.

### How to read the code

| Component | Rust ([`main.rs`](main.rs)) | Python ([`main.py`](main.py)) | Purpose |
| --- | --- | --- | --- |
| Expressions & IR | `Op`, `ExprKey::canonical`, `Stmt`, `BasicBlock` | `Op`, `canonical_expr`, `*Stmt`, `BasicBlock` | Canonicalizes commutative `+`/`*` keys and models CFG statements |
| Available expressions | `available_expressions(blocks)` | `available_expressions(blocks)` | Builds `universe`, `e_gen`/`e_kill`, and solves forward `∩` from `U` |
| Global CSE | `global_cse(blocks)` | `global_cse(blocks)` | Inserts `_cse_k` materializations and replaces available computations with copies |
| Copy propagation | `copy_propagation(blocks)` | `copy_propagation(blocks)` | Forward `∩` pass over `(dst, src)` copies that rewrites downstream operand uses |
| Verification | `eval_cfg`, `sample_diamond`, tests | `eval_cfg`, `sample_diamond`, `AvailableExprsTests` | Evaluates both paths of the diamond and checks loop convergence from `U` |

## What to look for

In the diamond CFG, `x + y` (canonicalized from `y + x` on the right branch) is available at join block `B3` because *both* branches compute it, whereas `a * b` is computed only in `B1` and therefore omitted from `in_avail[3]`. In the loop test, initializing `out[B]` to `U` correctly preserves `b + c` around the back-edge while killing `a + b` when `a` is reassigned in `B2`.

## Check your understanding

- Why would initializing `out[B]` to the empty set break loop-invariant available expressions at a loop header?
- Why does copy propagation follow CSE instead of running only before it?

## Edition mapping and scope

| 1977 Green | 1986 Red | 2006 Purple |
| --- | --- | --- |
| 12, 14 | 10 | 9 |

Numbers refer to chapters. Related readings and appendices are marked where applicable. See the [edition guide](../../README.md#edition-guide) for the bibliographic references and chapter coverage.

Expressions canonicalize commutative `+` and `*` operands alphabetically over scalar variables without alias-aware memory loads.
