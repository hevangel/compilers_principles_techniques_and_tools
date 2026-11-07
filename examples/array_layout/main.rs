//! Lower a checked row-major array index to a byte offset.

fn row_major(row: usize, col: usize, rows: usize, cols: usize, width: usize) -> Option<usize> {
    // Validate each coordinate before flattening it into a single index.
    if row >= rows || col >= cols || width == 0 {
        return None;
    }
    // Skip complete rows, add the column, then convert elements to bytes.
    row.checked_mul(cols)?.checked_add(col)?.checked_mul(width)
}

fn main() {
    // Keep base-address arithmetic separate from the reusable offset calculation.
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
