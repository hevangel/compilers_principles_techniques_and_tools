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
struct Token<'a> {
    kind: Kind,
    lexeme: &'a str,
    start: usize,
}

fn scan(source: &str) -> Result<Vec<Token<'_>>, usize> {
    let bytes = source.as_bytes();
    let mut tokens = Vec::new();
    let mut end = 0;
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
            b'=' if bytes.get(end) == Some(&b'=') => {
                end += 1;
                Kind::EqualEqual
            }
            b'=' => Kind::Equal,
            b';' => Kind::Semicolon,
            _ => return Err(start),
        };
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
