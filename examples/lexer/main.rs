//! Scan a source string into tokens with borrowed lexemes and byte offsets.

#[derive(Debug, PartialEq, Eq)]
enum Kind {
    Let,
    Name,
    Integer,
    Equal,
    EqualEqual,
    Semicolon,
}

#[derive(Debug, PartialEq, Eq)]
// The kind serves the parser. The lexeme and offset preserve source information.
struct Token<'a> {
    kind: Kind,
    lexeme: &'a str,
    start: usize,
}

fn scan(source: &str) -> Result<Vec<Token<'_>>, usize> {
    let bytes = source.as_bytes();
    let mut tokens = Vec::new();
    let mut end = 0;
    // Each token starts at the current cursor and consumes the range start..end.
    while end < bytes.len() {
        let start = end;
        end += 1;
        let kind = match bytes[start] {
            b if b.is_ascii_whitespace() => continue,
            b if b.is_ascii_alphabetic() || b == b'_' => {
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
                while bytes.get(end).is_some_and(u8::is_ascii_digit) {
                    end += 1;
                }
                Kind::Integer
            }
            // Prefer the longer operator when the next byte completes it.
            b'=' if bytes.get(end) == Some(&b'=') => {
                end += 1;
                Kind::EqualEqual
            }
            b'=' => Kind::Equal,
            b';' => Kind::Semicolon,
            _ => return Err(start),
        };
        // Borrow the matched slice. The scanner consumes ASCII bytes before slicing.
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
