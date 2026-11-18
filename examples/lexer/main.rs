//! Scan a source string into tokens with borrowed lexemes and byte offsets.

/// Token category consumed by a parser to make grammatical decisions.
#[derive(Debug, PartialEq, Eq)]
enum Kind {
    Let,
    Name,
    Integer,
    Equal,
    EqualEqual,
    Semicolon,
}

/// A scanned token carrying its category, borrowed source text, and starting byte offset.
#[derive(Debug, PartialEq, Eq)]
// The kind serves the parser. The lexeme and offset preserve source information.
struct Token<'a> {
    kind: Kind,
    lexeme: &'a str,
    start: usize,
}

/// Convert `source` into tokens using longest-match (maximal munch) rules.
/// Returns `Err(byte_offset)` on the first unsupported byte.
fn scan(source: &str) -> Result<Vec<Token<'_>>, usize> {
    let bytes = source.as_bytes();
    let mut tokens = Vec::new();
    let mut end = 0;
    // Invariant: `end` is always at a UTF-8 character boundary (every accepted token
    // consumes pure ASCII bytes, and non-ASCII bytes immediately return Err(start)).
    while end < bytes.len() {
        let start = end;
        end += 1;
        let kind = match bytes[start] {
            // Skip ASCII whitespace without emitting a token, while still advancing `end`
            // so subsequent tokens retain accurate byte offsets.
            b if b.is_ascii_whitespace() => continue,
            b if b.is_ascii_alphabetic() || b == b'_' => {
                // Longest match: consume all contiguous identifier bytes before keyword lookup
                // so words like `letter` are not split into `let` + `ter`.
                while bytes
                    .get(end)
                    .is_some_and(|b| b.is_ascii_alphanumeric() || *b == b'_')
                {
                    end += 1;
                }
                // Classify the whole identifier before checking for reserved words.
                if &source[start..end] == "let" {
                    Kind::Let
                } else {
                    Kind::Name
                }
            }
            b if b.is_ascii_digit() => {
                // Note: leading zeros (e.g. `007`) stay intact in `lexeme`.
                while bytes.get(end).is_some_and(u8::is_ascii_digit) {
                    end += 1;
                }
                Kind::Integer
            }
            // One-byte lookahead: prefer the longer operator `==` over `=` when available.
            b'=' if bytes.get(end) == Some(&b'=') => {
                end += 1;
                Kind::EqualEqual
            }
            b'=' => Kind::Equal,
            b';' => Kind::Semicolon,
            // Non-ASCII bytes (e.g. `λ`) or unknown punctuation fail with their start byte index.
            _ => return Err(start),
        };
        // Borrow the matched slice. The scanner consumes ASCII bytes before slicing,
        // guaranteeing `&source[start..end]` lies on valid UTF-8 boundaries.
        tokens.push(Token {
            kind,
            lexeme: &source[start..end],
            start,
        });
    }
    Ok(tokens)
}

fn main() {
    for token in scan("let letter = 12; letter == 12;").unwrap() {
        println!("{} {:?} {:?}", token.start, token.kind, token.lexeme);
    }
}

#[test]
fn recognizes_longest_token_and_reports_byte_offsets() {
    let tokens = scan("let letter==12;").unwrap();
    assert_eq!(tokens[0].kind, Kind::Let);
    assert_eq!(tokens[1].kind, Kind::Name);
    assert_eq!(
        tokens[2],
        Token {
            kind: Kind::EqualEqual,
            lexeme: "==",
            start: 10
        }
    );
    assert_eq!(scan("a = λ"), Err(4));
}
