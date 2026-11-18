# Local value numbering

Reuse a previously computed value within one basic block. See the [Rust source](main.rs) and [Python source](main.py).

```sh
cargo run --example value_numbering
cargo test --example value_numbering
python3 examples/value_numbering/main.py
python3 -m unittest examples/value_numbering/main.py
```

## What the example teaches

### Why we need this and when it is sound

Compilers frequently lower source expressions into sequences of small three-address instructions inside a **basic block**—a straight-line sequence of instructions entered only at the top and exited only at the bottom. Along the way, programmers and earlier lowering passes (such as address arithmetic or macro expansion) often repeat the same pure calculation under different temporary names, e.g. `t2 = a + b`, `t3 = b + a`, and then `t4 = t3 - a` followed by `t6 = t2 - a`. Syntactically `t3 - a` and `t2 - a` look distinct, so simple textual matching misses the redundancy.

**Local value numbering (LVN)** assigns every distinct runtime value a canonical integer **value number** (its first computing temporary) and indexes each expression by `(operator, left_value_number, right_value_number)`.

For the transformation to be **sound** (preserve the exact behavior of the original block for every possible input), three conditions must hold:

1. **Purity / no side effects:** Reusing a previous result is valid only if the operation has no observable side effects (like trapping or writing memory) and reads no state modified in between.
2. **Value-number identity, not variable-name identity:** Two instructions compute the same value only when their operands hold the same *value numbers*. (In general code with mutable variables, reassigning a variable must give it a fresh value number; here each instruction writes a fresh, single-assignment temporary.)
3. **Algebraic canonicalization matches machine semantics:** Swapping operands is sound only for operations that are genuinely commutative under the target arithmetic. Here, two's-complement wrapping `Add` is commutative even at integer overflow (`i64::MAX + 1 == 1 + i64::MAX`), whereas `Sub` is *not* commutative and must preserve operand order.

### Give each value a representative

`number` maintains two tables as it walks the basic block once from top to bottom:

- `values` maps each input or temporary index to its **canonical representative** (the earliest index guaranteed to hold that exact value). Inputs `0..inputs` initially represent themselves.
- `known` maps a normalized expression key `(Op, left_rep, right_rep)` to the destination temporary that first computed it.

When `number` sees `Compute(op, left, right)`, it looks up `values[left]` and `values[right]`, sorts them if `op == Add`, and checks `known`. If the key is already present, the instruction is rewritten to `Copy(previous)` and the new temporary records `previous` as its representative; otherwise it emits `Compute` using the canonical operand representatives and claims the new temporary index as the key's representative. Notice how normalizing operands *before* emitting `Compute` lets downstream instructions (`3 - 0` becoming `2 - 0`) match later uses (`2 - 0`)!

### Trace the numbered temporaries

Inputs occupy indices `0` and `1` (shown below with sample inputs `[9, 4]`). Each instruction appends one temporary starting at index `2`:

| Dest | Original `CODE` | Lookup operands (`values[...]`) | Normalized key | `known` hit? | Optimized instruction | `values[Dest]` | Runtime value (`[9, 4]`) |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `2` | `0 + 1` | `(0, 1)` | `(Add, 0, 1)` | Miss (insert `2`) | `Compute(Add, 0, 1)` | `2` | `13` |
| `3` | `1 + 0` | `(1, 0)` → sorted `(0, 1)` | `(Add, 0, 1)` | Hit (`2`) | `Copy(2)` | `2` | `13` |
| `4` | `3 - 0` | `(values[3]=2, 0)` | `(Sub, 2, 0)` | Miss (insert `4`) | `Compute(Sub, 2, 0)` | `4` | `4` |
| `5` | `0 - 2` | `(0, values[2]=2)` | `(Sub, 0, 2)` | Miss (insert `5`) | `Compute(Sub, 0, 2)` | `5` | `-4` |
| `6` | `2 - 0` | `(values[2]=2, 0)` | `(Sub, 2, 0)` | Hit (`4`) | `Copy(4)` | `4` | `4` |

The interpreter evaluates both the original and optimized sequences on `[9, 4]`, `[4, -9]`, and the wrapping overflow boundary `[i64::MAX, 1]` and checks that every single temporary holds the same value.

### How to read the code

| Component | Rust ([`main.rs`](main.rs)) | Python ([`main.py`](main.py)) | Purpose |
| --- | --- | --- | --- |
| Operations & IR | `Op`, `Expr` (`Compute`, `Copy`) | `Op`, `Compute`, `Copy`, `Expr` | Straight-line three-address instructions with implicit destination indices |
| Value-numbering pass | `number(inputs, code)` | `number(inputs, code)` | Single forward scan maintaining `values` (rep table) and `known` (expression hash map) |
| Reference interpreter | `execute(inputs, code)` | `execute(inputs, code)` + `wrap_i64` | Executes both versions with 64-bit wrapping arithmetic to verify semantic equivalence |
| Sample & test | `CODE`, `main`, test fn | `CODE`, `main`, `ValueNumberingTests` | Runs the 5-instruction demo and checks structural rewrites and overflow equivalence |

## What to look for

The second addition becomes a copy. Two subtractions with reversed operands stay distinct. Tests compare execution before and after rewriting, including wrapping arithmetic.

## Check your understanding

- Add an expression `5 + 4` and then `4 + 5` to `CODE`. Predict the optimized form of the second expression.
- Why would using the variable's spelling alone as a representative be unsafe in a language that lets the same variable be assigned more than once?

## Edition mapping and scope

| 1977 Green | 1986 Red | 2006 Purple |
| --- | --- | --- |
| 12 | 9–10 | 8–9 |

Numbers refer to chapters. Related readings and appendices are marked where applicable. See the [edition guide](../../README.md#edition-guide) for the bibliographic references and chapter coverage.

Temporary destinations are implicit and assigned once. The example assumes straight-line, side-effect-free code.
