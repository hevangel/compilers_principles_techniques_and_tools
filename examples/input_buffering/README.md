# Double-Buffer Input Scanning and EOF Sentinels (§3.2)

Stream source text through a `2 * N` buffer pair with sentinel markers, two scanning pointers (`lexeme_begin`, `forward`), safe cross-boundary `retract()`, and lexeme-length overflow checks. See the [Rust source](main.rs) and [Python source](main.py).

```sh
cargo run --example input_buffering
cargo test --example input_buffering
python3 examples/input_buffering/main.py
python3 -m unittest examples/input_buffering/main.py
```

## What the example teaches

### Why we need this / What problem this solves

A naive scanner either loads an entire multi-megabyte file into one contiguous string or runs *two* checks per character (`if forward == end_of_half` and `if end_of_input`) inside its hottest inner loop. Worse, reading one chunk at a time fails whenever a token like `alpha`, `12.5`, or `<=` straddles a disk-read boundary—or when maximal munch advances `forward` across the boundary, sees `a` after `<`, and must **retract** `forward` back into the previous chunk.

Dragon Book **§3.2** solves both with a **two-half buffer pair** (`N` bytes per half) terminated by an explicit **`EOF` sentinel**:

```text
[ half 0: N bytes ][ EOF ] [ half 1: N bytes ][ EOF ]
 ^                     ^   ^                     ^
 0                     N  N+1                  2N+1
```

### How sentinels, two pointers, and retract work

1. **One test per character:** `next_char()` checks only `buf[forward] == EOF_SENTINEL`. Only at index `N`, `2N+1`, or genuine end-of-input does execution enter the slow path.
2. **Boundary refill + fence (`half_reloaded_for_cycle`):** If a multi-character operator (`<` followed by `a` across the half-buffer seam) hits the sentinel, loads half 1, and then calls `retract()`, `forward` moves backward from slot `N+1` back to `N-1`. Setting the fence flag ensures advancing past `N` a second time reuses half 1 instead of reloading and silently skipping `N` bytes!
3. **Lexeme overflow limit:** Because `lexeme_begin` and `forward` must coexist inside the two halves, any single lexeme longer than `N` triggers a clear `BufferError` (why `N` in real scanners defaults to OS page sizes like 4096 bytes).

### How to read the code

| Component | Rust (`main.rs`) | Python (`main.py`) | Role |
| --- | --- | --- | --- |
| Buffer pair | `struct DoubleBuffer` | `class DoubleBuffer` | Maintains `2 * N + 2` slots, sentinel indices, and reload fence flags |
| Cursor advance | `DoubleBuffer::next_char` | `DoubleBuffer.next_char` | Single sentinel test in fast path; transparent half refill at `N` / `2N+1` |
| Safe lookahead undo | `DoubleBuffer::retract` | `DoubleBuffer.retract` | Steps `forward` backward across seams without double-reloading the next half |
| Scanner parity | `scan_buffered` vs `scan_direct` | `scan_buffered` vs `scan_direct` | Asserts bit-for-bit token equality against direct scanning across `N` values |

## What to look for

Running the example scans `"alpha = beta1 <= 20 + gamma == 42;"` with tiny `N = 8` so reloads, sentinel switches, and boundary retractions fire multiple times while producing the exact same 11 tokens as `scan_direct`. Unit tests verify cross-boundary `<` followed by `abcd` reloads half 1 exactly once.

## Check your understanding

- Why does a classic scanner use **two** `N`-byte halves instead of **one** `2N`-byte buffer?
- What bug happens if `retract()` steps back from half 1 into half 0 without updating the reload fence before `forward` advances into half 1 again?

## Edition mapping and scope

| 1977 Green | 1986 Red | 2006 Purple |
| --- | --- | --- |
| 3 (input buffering) | 3 (§3.2) | 3 (§3.2) |

Numbers refer to chapters. See [Tokens and longest match](../lexer/README.md) for the single-buffer scanner foundation and [DFA Table Compression](../dfa_optimization/README.md) for inner-loop table speedup.
