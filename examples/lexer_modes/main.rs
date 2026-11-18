//! Chapter 3 Extension: Lexer Start Conditions (%s / %x) and Contextual Mode Stack.
//!
//! Demonstrates:
//! 1. Multiple finite-automaton rule tables keyed by lexer start condition:
//!    `Initial`, `Comment` (%x exclusive, supporting nested `/* /* ... */ */`),
//!    `String` (%x exclusive, decoding escapes and `${...}` expression interpolation),
//!    and `Directive` (%x exclusive, tokenizing `#define NAME value` to end-of-line),
//! 2. Explicit mode stack (`push_mode` / `pop_mode`) + brace-depth tracking so expressions
//!    inside `"hi ${a + {1}}"` return cleanly back into `String` mode on the matching `}`,
//! 3. Mode-transition trace showing how contextual lexing keeps each rule DFA small and clean.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    Initial,
    Comment,
    StringLit,
    Directive,
}

impl Mode {
    fn label(self) -> &'static str {
        match self {
            Mode::Initial => "INITIAL",
            Mode::Comment => "%x COMMENT",
            Mode::StringLit => "%x STRING",
            Mode::Directive => "%x DIRECTIVE",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Token {
    kind: String,
    lexeme: String,
    mode: Mode,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ModeTransition {
    at: usize,
    from: Mode,
    to: Mode,
    reason: String,
}

struct ModalLexer<'a> {
    src: &'a str,
    bytes: &'a [u8],
    pos: usize,
    mode_stack: Vec<Mode>,
    interp_brace_depth: Vec<usize>,
    comment_depth: usize,
    tokens: Vec<Token>,
    transitions: Vec<ModeTransition>,
}

impl<'a> ModalLexer<'a> {
    fn new(src: &'a str) -> Self {
        Self {
            src,
            bytes: src.as_bytes(),
            pos: 0,
            mode_stack: vec![Mode::Initial],
            interp_brace_depth: Vec::new(),
            comment_depth: 0,
            tokens: Vec::new(),
            transitions: Vec::new(),
        }
    }

    fn mode(&self) -> Mode {
        *self.mode_stack.last().unwrap_or(&Mode::Initial)
    }

    fn push_mode(&mut self, next: Mode, reason: impl Into<String>) {
        let from = self.mode();
        self.mode_stack.push(next);
        self.transitions.push(ModeTransition {
            at: self.pos,
            from,
            to: next,
            reason: reason.into(),
        });
    }

    fn pop_mode(&mut self, reason: impl Into<String>) {
        if self.mode_stack.len() > 1 {
            let from = self.mode_stack.pop().unwrap_or(Mode::Initial);
            let to = self.mode();
            self.transitions.push(ModeTransition {
                at: self.pos,
                from,
                to,
                reason: reason.into(),
            });
        }
    }

    fn run(mut self) -> Result<(Vec<Token>, Vec<ModeTransition>), String> {
        while self.pos < self.bytes.len() {
            match self.mode() {
                Mode::Initial => self.step_initial()?,
                Mode::Comment => self.step_comment()?,
                Mode::StringLit => self.step_string()?,
                Mode::Directive => self.step_directive(),
            }
        }
        match self.mode() {
            Mode::Comment => Err("unterminated nested block comment at EOF".to_string()),
            Mode::StringLit => Err("unterminated interpolated string at EOF".to_string()),
            Mode::Directive => {
                self.pop_mode("EOF ends directive");
                Ok((self.tokens, self.transitions))
            }
            Mode::Initial => {
                if !self.interp_brace_depth.is_empty() {
                    Err("unclosed `${ ...` interpolation at EOF".to_string())
                } else {
                    Ok((self.tokens, self.transitions))
                }
            }
        }
    }

    fn step_initial(&mut self) -> Result<(), String> {
        let b = self.bytes[self.pos];
        if b.is_ascii_whitespace() {
            self.pos += 1;
            return Ok(());
        }
        if b == b'#' {
            self.pos += 1;
            self.push_mode(Mode::Directive, "saw '#' pragma/directive start");
            return Ok(());
        }
        if b == b'/' && self.bytes.get(self.pos + 1) == Some(&b'*') {
            self.comment_depth = 1;
            self.pos += 2;
            self.push_mode(Mode::Comment, "saw '/*' comment open (depth=1)");
            return Ok(());
        }
        if b == b'"' {
            self.pos += 1;
            self.tokens.push(Token {
                kind: "STR_OPEN".to_string(),
                lexeme: "\"".to_string(),
                mode: Mode::Initial,
            });
            self.push_mode(Mode::StringLit, "saw '\"' string open");
            return Ok(());
        }
        if b == b'}' && !self.interp_brace_depth.is_empty() {
            let depth = self.interp_brace_depth.last_mut().unwrap();
            if *depth == 0 {
                self.interp_brace_depth.pop();
                self.pos += 1;
                self.tokens.push(Token {
                    kind: "INTERP_END".to_string(),
                    lexeme: "}".to_string(),
                    mode: Mode::Initial,
                });
                self.pop_mode("matching '}' closes `${...}` back to STRING");
                return Ok(());
            }
            *depth -= 1;
            self.pos += 1;
            self.tokens.push(Token {
                kind: "RBRACE".to_string(),
                lexeme: "}".to_string(),
                mode: Mode::Initial,
            });
            return Ok(());
        }
        if b == b'{' {
            if let Some(d) = self.interp_brace_depth.last_mut() {
                *d += 1;
            }
            self.pos += 1;
            self.tokens.push(Token {
                kind: "LBRACE".to_string(),
                lexeme: "{".to_string(),
                mode: Mode::Initial,
            });
            return Ok(());
        }
        if b.is_ascii_alphabetic() || b == b'_' {
            let start = self.pos;
            self.pos += 1;
            while self.pos < self.bytes.len()
                && (self.bytes[self.pos].is_ascii_alphanumeric() || self.bytes[self.pos] == b'_')
            {
                self.pos += 1;
            }
            let lexeme = self.src[start..self.pos].to_string();
            let kind = if lexeme == "let" { "LET" } else { "IDENT" };
            self.tokens.push(Token {
                kind: kind.to_string(),
                lexeme,
                mode: Mode::Initial,
            });
            return Ok(());
        }
        if b.is_ascii_digit() {
            let start = self.pos;
            self.pos += 1;
            while self.pos < self.bytes.len() && self.bytes[self.pos].is_ascii_digit() {
                self.pos += 1;
            }
            self.tokens.push(Token {
                kind: "INT".to_string(),
                lexeme: self.src[start..self.pos].to_string(),
                mode: Mode::Initial,
            });
            return Ok(());
        }
        let kind = match b {
            b'=' => "EQ",
            b'+' => "PLUS",
            b';' => "SEMI",
            _ => return Err(format!("unexpected character '{}' in INITIAL", b as char)),
        };
        let start = self.pos;
        self.pos += 1;
        self.tokens.push(Token {
            kind: kind.to_string(),
            lexeme: self.src[start..self.pos].to_string(),
            mode: Mode::Initial,
        });
        Ok(())
    }

    fn step_comment(&mut self) -> Result<(), String> {
        while self.pos < self.bytes.len() {
            if self.bytes[self.pos] == b'/' && self.bytes.get(self.pos + 1) == Some(&b'*') {
                self.comment_depth += 1;
                self.pos += 2;
                continue;
            }
            if self.bytes[self.pos] == b'*' && self.bytes.get(self.pos + 1) == Some(&b'/') {
                self.comment_depth -= 1;
                self.pos += 2;
                if self.comment_depth == 0 {
                    self.pop_mode("closed outermost '*/'");
                    return Ok(());
                }
                continue;
            }
            self.pos += 1;
        }
        Ok(())
    }

    fn step_string(&mut self) -> Result<(), String> {
        let chunk_start = self.pos;
        let mut decoded = String::new();
        while self.pos < self.bytes.len() {
            let b = self.bytes[self.pos];
            if b == b'"' {
                if !decoded.is_empty() {
                    self.tokens.push(Token {
                        kind: "STR_TEXT".to_string(),
                        lexeme: decoded,
                        mode: Mode::StringLit,
                    });
                }
                self.pos += 1;
                self.tokens.push(Token {
                    kind: "STR_CLOSE".to_string(),
                    lexeme: "\"".to_string(),
                    mode: Mode::StringLit,
                });
                self.pop_mode("saw closing '\"'");
                return Ok(());
            }
            if b == b'$' && self.bytes.get(self.pos + 1) == Some(&b'{') {
                if !decoded.is_empty() {
                    self.tokens.push(Token {
                        kind: "STR_TEXT".to_string(),
                        lexeme: decoded,
                        mode: Mode::StringLit,
                    });
                }
                self.pos += 2;
                self.tokens.push(Token {
                    kind: "INTERP_START".to_string(),
                    lexeme: "${".to_string(),
                    mode: Mode::StringLit,
                });
                self.interp_brace_depth.push(0);
                self.push_mode(
                    Mode::Initial,
                    "saw '${' -> push INITIAL for interpolated expr",
                );
                return Ok(());
            }
            if b == b'\\' {
                self.pos += 1;
                if self.pos >= self.bytes.len() {
                    return Err("trailing backslash inside string".to_string());
                }
                let esc = match self.bytes[self.pos] {
                    b'n' => '\n',
                    b't' => '\t',
                    b'"' => '"',
                    b'\\' => '\\',
                    b'$' => '$',
                    other => other as char,
                };
                decoded.push(esc);
                self.pos += 1;
                continue;
            }
            // UTF-8 scalar consumption
            let ch = self.src[self.pos..].chars().next().unwrap();
            decoded.push(ch);
            self.pos += ch.len_utf8();
        }
        let _ = chunk_start;
        Ok(())
    }

    fn step_directive(&mut self) {
        let start = self.pos;
        while self.pos < self.bytes.len() && self.bytes[self.pos] != b'\n' {
            self.pos += 1;
        }
        let body = self.src[start..self.pos].trim().to_string();
        let mut parts = body.split_whitespace();
        if let Some(name) = parts.next() {
            self.tokens.push(Token {
                kind: "DIR_CMD".to_string(),
                lexeme: name.to_string(),
                mode: Mode::Directive,
            });
            for arg in parts {
                self.tokens.push(Token {
                    kind: "DIR_ARG".to_string(),
                    lexeme: arg.to_string(),
                    mode: Mode::Directive,
                });
            }
        }
        if self.pos < self.bytes.len() && self.bytes[self.pos] == b'\n' {
            self.pos += 1;
        }
        self.pop_mode("newline closes directive");
    }
}

fn lex_with_modes(src: &str) -> Result<(Vec<Token>, Vec<ModeTransition>), String> {
    ModalLexer::new(src).run()
}

fn main() {
    let src = "\
#define LIMIT 10
/* outer /* nested comment */ still ignored */
let msg = \"sum=${a + \"inner_${k}\"}\";";

    let (tokens, transitions) = lex_with_modes(src).expect("modal lexing should succeed");
    println!("=== Lexer Start Conditions (%x) and Contextual Mode Stack ===");
    println!("Source:\n{src}\n");
    println!("Mode transitions ({}):", transitions.len());
    for tr in &transitions {
        println!(
            "  @{:02} {:<12} -> {:<12} ({})",
            tr.at,
            tr.from.label(),
            tr.to.label(),
            tr.reason
        );
    }
    println!("\nEmitted tokens ({}):", tokens.len());
    for tok in &tokens {
        println!(
            "  [{:<12}] {:<12} {:?}",
            tok.mode.label(),
            tok.kind,
            tok.lexeme
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn supports_nested_block_comments_and_resumes_initial() {
        let src = "/* lvl1 /* lvl2 */ still comment */ let x = 1;";
        let (tokens, _) = lex_with_modes(src).expect("nested comment lexes cleanly");
        let summary: Vec<(&str, &str)> = tokens
            .iter()
            .map(|t| (t.kind.as_str(), t.lexeme.as_str()))
            .collect();
        assert_eq!(
            summary,
            vec![
                ("LET", "let"),
                ("IDENT", "x"),
                ("EQ", "="),
                ("INT", "1"),
                ("SEMI", ";")
            ]
        );
    }

    #[test]
    fn supports_nested_string_interpolation_and_directives() {
        let src = "#pragma fast\nlet s = \"a=${b + \"c=${d}\"}\";";
        let (tokens, _) = lex_with_modes(src).expect("interpolation lexes cleanly");
        let kinds: Vec<(&str, &str)> = tokens
            .iter()
            .map(|t| (t.kind.as_str(), t.lexeme.as_str()))
            .collect();
        assert_eq!(
            kinds,
            vec![
                ("DIR_CMD", "pragma"),
                ("DIR_ARG", "fast"),
                ("LET", "let"),
                ("IDENT", "s"),
                ("EQ", "="),
                ("STR_OPEN", "\""),
                ("STR_TEXT", "a="),
                ("INTERP_START", "${"),
                ("IDENT", "b"),
                ("PLUS", "+"),
                ("STR_OPEN", "\""),
                ("STR_TEXT", "c="),
                ("INTERP_START", "${"),
                ("IDENT", "d"),
                ("INTERP_END", "}"),
                ("STR_CLOSE", "\""),
                ("INTERP_END", "}"),
                ("STR_CLOSE", "\""),
                ("SEMI", ";"),
            ]
        );
    }

    #[test]
    fn rejects_unclosed_nested_comment() {
        let err = lex_with_modes("/* a /* b */").expect_err("must detect unclosed outer comment");
        assert!(err.contains("unterminated"));
    }
}
