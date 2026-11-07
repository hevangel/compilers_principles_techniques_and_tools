fn row_major(row: usize, col: usize, rows: usize, cols: usize, width: usize) -> Option<usize> {
    if row >= rows || col >= cols || width == 0 {
        return None;
    }
    row.checked_mul(cols)?.checked_add(col)?.checked_mul(width)
}

fn main() {
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
