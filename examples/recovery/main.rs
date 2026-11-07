//! Recover from a malformed statement by resuming after a semicolon.

#[derive(Debug, PartialEq, Eq)]
struct Diagnostic {
    offset: usize,
    message: &'static str,
}

fn recover(source: &str) -> (Vec<u32>, Vec<Diagnostic>) {
    let mut values = Vec::new();
    let mut diagnostics = Vec::new();
    let mut offset = 0;
    // Keep the delimiter so an incomplete final statement can be detected.
    for chunk in source.split_inclusive(';') {
        let statement = chunk.trim_end_matches(';');
        // Measure whitespace in bytes because diagnostics index the original source.
        let leading_space = statement.len() - statement.trim_start().len();
        let statement = statement.trim();
        if statement.is_empty() {
            offset += chunk.len();
            continue;
        }
        // Check the complete chunk before keeping its value or recording one diagnostic.
        let message = match (
            chunk.ends_with(';'),
            statement.split_whitespace().collect::<Vec<_>>().as_slice(),
        ) {
            (false, _) => Some("expected a semicolon"),
            (true, ["emit", digits]) if digits.bytes().all(|byte| byte.is_ascii_digit()) => {
                match digits.parse() {
                    Ok(number) => {
                        values.push(number);
                        None
                    }
                    Err(_) => Some("number out of range"),
                }
            }
            _ => Some("expected 'emit' and an unsigned integer"),
        };
        // Discard this chunk on error. The next iteration starts at a synchronization point.
        if let Some(message) = message {
            diagnostics.push(Diagnostic {
                offset: offset + leading_space,
                message,
            });
        }
        offset += chunk.len();
    }
    (values, diagnostics)
}

fn main() {
    let (values, diagnostics) = recover("emit x; emit 7; bad; emit 19;");
    println!("Recovered values: {values:?}");
    println!("Diagnostics: {diagnostics:?}");
}

#[test]
fn resumes_after_semicolons_and_checks_unterminated_tail() {
    let (values, diagnostics) = recover("emit x; emit 7; é; emit 19");
    assert_eq!(values, vec![7]);
    assert_eq!(
        diagnostics
            .iter()
            .map(|diagnostic| diagnostic.offset)
            .collect::<Vec<_>>(),
        [0, 16, 20]
    );
    assert_eq!(diagnostics[2].message, "expected a semicolon");
}
