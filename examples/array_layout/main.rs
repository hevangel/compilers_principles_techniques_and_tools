//! Lower a checked row-major array index to a byte offset.

/// Converts a 2D `(row, col)` index into a byte offset from the start of a
/// `rows × cols` row-major array whose elements are each `width` bytes wide.
/// Returns `None` on out-of-bounds coordinates, zero-sized elements, or integer overflow.
fn row_major(row: usize, col: usize, rows: usize, cols: usize, width: usize) -> Option<usize> {
    // Check each dimension separately before flattening: `a[0, cols]` has the same
    // linear index as `a[1, 0]`, so flattening first would mask column overflow.
    if row >= rows || col >= cols || width == 0 {
        return None;
    }
    // Row-major layout packs each row contiguously: `row * cols` skips whole rows
    // (row stride in elements), `+ col` steps across the row, and `* width` yields bytes.
    // Checked arithmetic prevents wraparound from silently producing a small valid-looking offset.
    row.checked_mul(cols)?.checked_add(col)?.checked_mul(width)
}

fn main() {
    // Keep base-address addition separate from the reusable offset calculation:
    // the compiler computes `(row * cols + col) * width` relative to an abstract base `0`.
    let base = 1000;
    let offset = row_major(2, 1, 4, 5, 8).unwrap();
    println!(
        "Element [2, 1]: byte offset {offset}, address {}",
        base + offset
    );
}

#[test]
fn applies_row_stride_and_checks_bounds_and_overflow() {
    assert_eq!(row_major(2, 1, 4, 5, 8), Some(88));
    assert_eq!(row_major(0, 0, 4, 5, 8), Some(0));
    assert_eq!(row_major(0, 5, 4, 5, 8), None);
    assert_eq!(row_major(1, 0, 2, usize::MAX, 8), None);
}
