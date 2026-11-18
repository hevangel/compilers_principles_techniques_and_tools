//! Chapter 3 (§3.1.4, §3.4): Lexical Error Handling and Panic-Mode Recovery.
//!
//! Demonstrates:
//! 1. Invalid stray characters (`@`, `?`) recovered via single-character panic deletion,
//! 2. Unterminated string literals recovered at unescaped newlines so subsequent lines stay valid,
//! 3. Unterminated block comments (`/* ...`) diagnosed at their opening `/*` span to EOF,
//! 4. Malformed numeric literals (`0x`, `12e+`, `99bad`) consumed intact as recovered tokens,
//! 5. Clear separation between lexical errors and downstream parser syntax errors (`let = 5;`).

use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Span {
    start: usize,
    end: usize,
    line: usize,
    col: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum TokKind {
    Let,
    Ident,
    Int,
    Float,
    Str,
    Eq,
    Plus,
    Semi,
    LexError,
}

impl fmt::Display for TokKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            TokKind::Let => "LET",
            TokKind::Ident => "IDENT",
            TokKind::Int => "INT",
            TokKind::Float => "FLOAT",
            TokKind::Str => "STR",
            TokKind::Eq => "EQ",
            TokKind::Plus => "PLUS",
            TokKind::Semi => "SEMI",
            TokKind::LexError => "LEX_ERR",
        };
        f.write_str(s)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Token {
    kind: TokKind,
    lexeme: String,
    span: Span,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum RecoveryStrategy {
    DeleteChar,
    TerminateStringAtNewline,
    TerminateCommentAtEof,
    ConsumeMalformedNumber,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct LexDiagnostic {
    span: Span,
    strategy: RecoveryStrategy,
    message: String,
}

fn format_diagnostic(src: &str, diag: &LexDiagnostic) -> String {
    let lines: Vec<&str> = src.lines().collect();
    let line_str = lines
        .get(diag.span.line.saturating_sub(1))
        .copied()
        .unwrap_or("");
    let pad = " ".repeat(diag.span.col.saturating_sub(1));
    let avail = line_str
        .len()
        .saturating_sub(diag.span.col.saturating_sub(1))
        .max(1);
    let width = diag
        .span
        .end
        .saturating_sub(diag.span.start)
        .max(1)
        .min(avail);
    let carets = "^".repeat(width);
    format!(
        "lex error at line {}, col {} ({:?}): {}\n  | {}\n  | {}{}",
        diag.span.line, diag.span.col, diag.strategy, diag.message, line_str, pad, carets
    )
}

fn scan_with_recovery(src: &str) -> (Vec<Token>, Vec<LexDiagnostic>) {
    let bytes = src.as_bytes();
    let n = bytes.len();
    let mut i = 0;
    let mut line = 1usize;
    let mut line_start = 0usize;
    let mut tokens = Vec::new();
    let mut diags = Vec::new();

    while i < n {
        let b = bytes[i];
        if b == b'\n' {
            i += 1;
            line += 1;
            line_start = i;
            continue;
        }
        if b.is_ascii_whitespace() {
            i += 1;
            continue;
        }

        // Line comment: // ...
        if b == b'/' && bytes.get(i + 1) == Some(&b'/') {
            i += 2;
            while i < n && bytes[i] != b'\n' {
                i += 1;
            }
            continue;
        }

        // Block comment: /* ... */ with EOF recovery pinned to opening `/*`
        if b == b'/' && bytes.get(i + 1) == Some(&b'*') {
            let start = i;
            let open_line = line;
            let open_col = start - line_start + 1;
            i += 2;
            let mut closed = false;
            while i < n {
                if bytes[i] == b'\n' {
                    i += 1;
                    line += 1;
                    line_start = i;
                } else if bytes[i] == b'*' && bytes.get(i + 1) == Some(&b'/') {
                    i += 2;
                    closed = true;
                    break;
                } else {
                    i += 1;
                }
            }
            if !closed {
                diags.push(LexDiagnostic {
                    span: Span {
                        start,
                        end: start + 2,
                        line: open_line,
                        col: open_col,
                    },
                    strategy: RecoveryStrategy::TerminateCommentAtEof,
                    message: "unterminated block comment; reached end of file".to_string(),
                });
            }
            continue;
        }

        let start = i;
        let col = start - line_start + 1;

        // String literal with escape handling + newline synchronization
        if b == b'"' {
            i += 1;
            let mut closed = false;
            while i < n {
                if bytes[i] == b'\n' {
                    // Synchronize before the newline so the next line parses normally.
                    break;
                }
                if bytes[i] == b'\\' && i + 1 < n && bytes[i + 1] != b'\n' {
                    i += 2;
                    continue;
                }
                if bytes[i] == b'"' {
                    i += 1;
                    closed = true;
                    break;
                }
                i += 1;
            }
            let span = Span {
                start,
                end: i,
                line,
                col,
            };
            if closed {
                tokens.push(Token {
                    kind: TokKind::Str,
                    lexeme: src[start..i].to_string(),
                    span,
                });
            } else {
                diags.push(LexDiagnostic {
                    span,
                    strategy: RecoveryStrategy::TerminateStringAtNewline,
                    message: "unterminated string literal; synchronized at end of line".to_string(),
                });
                tokens.push(Token {
                    kind: TokKind::Str,
                    lexeme: src[start..i].to_string(),
                    span,
                });
            }
            continue;
        }

        // Identifiers / keywords
        if b.is_ascii_alphabetic() || b == b'_' {
            i += 1;
            while i < n && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                i += 1;
            }
            let lexeme = src[start..i].to_string();
            let kind = if lexeme == "let" {
                TokKind::Let
            } else {
                TokKind::Ident
            };
            tokens.push(Token {
                kind,
                lexeme,
                span: Span {
                    start,
                    end: i,
                    line,
                    col,
                },
            });
            continue;
        }

        // Numbers: decimal, float (`12.34e+5`), hex (`0x1f`) with malformed recovery
        if b.is_ascii_digit() {
            let mut kind = TokKind::Int;
            let mut err_msg: Option<String> = None;

            if b == b'0' && matches!(bytes.get(i + 1), Some(b'x' | b'X')) {
                i += 2;
                let hex_start = i;
                while i < n && bytes[i].is_ascii_hexdigit() {
                    i += 1;
                }
                if i == hex_start {
                    err_msg = Some("hex literal '0x' requires at least one hex digit".to_string());
                }
                // Consume trailing non-hex identifier chars (e.g. `0x1g`) to avoid cascading `g` token errors.
                if i < n && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                    while i < n && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                        i += 1;
                    }
                    err_msg = Some(format!("malformed hex literal '{}'", &src[start..i]));
                }
            } else {
                while i < n && bytes[i].is_ascii_digit() {
                    i += 1;
                }
                if bytes.get(i) == Some(&b'.')
                    && bytes.get(i + 1).is_some_and(|c| c.is_ascii_digit())
                {
                    kind = TokKind::Float;
                    i += 1;
                    while i < n && bytes[i].is_ascii_digit() {
                        i += 1;
                    }
                }
                if matches!(bytes.get(i), Some(b'e' | b'E')) {
                    kind = TokKind::Float;
                    i += 1;
                    if matches!(bytes.get(i), Some(b'+' | b'-')) {
                        i += 1;
                    }
                    let exp_start = i;
                    while i < n && bytes[i].is_ascii_digit() {
                        i += 1;
                    }
                    if i == exp_start {
                        err_msg = Some(format!(
                            "malformed exponent in numeric literal '{}'",
                            &src[start..i]
                        ));
                    }
                }
                if i < n && (bytes[i].is_ascii_alphabetic() || bytes[i] == b'_') {
                    while i < n && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                        i += 1;
                    }
                    err_msg = Some(format!(
                        "invalid suffix in numeric literal '{}'",
                        &src[start..i]
                    ));
                }
            }

            let span = Span {
                start,
                end: i,
                line,
                col,
            };
            if let Some(msg) = err_msg {
                diags.push(LexDiagnostic {
                    span,
                    strategy: RecoveryStrategy::ConsumeMalformedNumber,
                    message: msg,
                });
                tokens.push(Token {
                    kind: TokKind::LexError,
                    lexeme: src[start..i].to_string(),
                    span,
                });
            } else {
                tokens.push(Token {
                    kind,
                    lexeme: src[start..i].to_string(),
                    span,
                });
            }
            continue;
        }

        let single_kind = match b {
            b'=' => Some(TokKind::Eq),
            b'+' => Some(TokKind::Plus),
            b';' => Some(TokKind::Semi),
            _ => None,
        };
        if let Some(kind) = single_kind {
            i += 1;
            tokens.push(Token {
                kind,
                lexeme: src[start..i].to_string(),
                span: Span {
                    start,
                    end: i,
                    line,
                    col,
                },
            });
            continue;
        }

        // Panic-mode single-character deletion on stray character
        i += 1;
        diags.push(LexDiagnostic {
            span: Span {
                start,
                end: i,
                line,
                col,
            },
            strategy: RecoveryStrategy::DeleteChar,
            message: format!("unexpected character '{}'; skipped", b as char),
        });
    }

    (tokens, diags)
}

/// Checks simple `let ident = expr;` syntax so we can prove valid tokens with syntax errors
/// produce ZERO lexical diagnostics (`let = 5;`).
fn has_valid_let_syntax(tokens: &[Token]) -> Result<(), String> {
    if tokens.len() < 5 {
        return Err("incomplete statement".to_string());
    }
    if tokens[0].kind != TokKind::Let {
        return Err("expected 'let'".to_string());
    }
    if tokens[1].kind != TokKind::Ident {
        return Err(format!(
            "syntax error at L{}:C{}: expected identifier after 'let', found {}",
            tokens[1].span.line, tokens[1].span.col, tokens[1].kind
        ));
    }
    Ok(())
}

fn main() {
    let src = "\
let x = @ 12e+ + 0x;
let msg = \"unterminated string
let ok = 42 + 0x1a;
/* unclosed block comment";

    let (tokens, diags) = scan_with_recovery(src);
    println!("=== Lexical Error Handling and Panic-Mode Recovery (Ch. 3) ===");
    println!(
        "Recovered {} tokens across 4 lines despite {} lexical errors:\n",
        tokens.len(),
        diags.len()
    );
    for tok in &tokens {
        println!(
            "  L{}:C{:<2} {:<7} {:?}",
            tok.span.line, tok.span.col, tok.kind, tok.lexeme
        );
    }
    println!("\nDiagnostics:");
    for d in &diags {
        println!("{}", format_diagnostic(src, d));
    }

    let syntax_only = "let = 5;";
    let (syn_tokens, syn_lex_diags) = scan_with_recovery(syntax_only);
    println!("\nLexing vs. Parsing boundary check on {:?}:", syntax_only);
    println!("  Lexical diagnostics: {}", syn_lex_diags.len());
    println!(
        "  Parser check: {}",
        has_valid_let_syntax(&syn_tokens).err().unwrap_or_default()
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recovers_multiple_lexical_errors_without_halting() {
        let src = "let a = @ 0x;\nlet s = \"abc\nlet b = 99xyz + 7;\n/* open";
        let (tokens, diags) = scan_with_recovery(src);
        assert_eq!(diags.len(), 5);
        assert_eq!(diags[0].strategy, RecoveryStrategy::DeleteChar);
        assert_eq!(diags[1].strategy, RecoveryStrategy::ConsumeMalformedNumber);
        assert_eq!(
            diags[2].strategy,
            RecoveryStrategy::TerminateStringAtNewline
        );
        assert_eq!(diags[3].strategy, RecoveryStrategy::ConsumeMalformedNumber);
        assert_eq!(diags[4].strategy, RecoveryStrategy::TerminateCommentAtEof);
        // Recovery preserves downstream tokens on subsequent lines (`let`, `b`, `=`, ..., `+`, `7`, `;`).
        assert!(tokens
            .iter()
            .any(|t| t.kind == TokKind::Ident && t.lexeme == "b"));
        assert!(tokens
            .iter()
            .any(|t| t.kind == TokKind::Int && t.lexeme == "7"));
    }

    #[test]
    fn unterminated_string_does_not_swallow_following_line() {
        let src = "let a = \"oops\nlet b = 10;";
        let (tokens, diags) = scan_with_recovery(src);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].span.line, 1);
        let line2: Vec<&str> = tokens
            .iter()
            .filter(|t| t.span.line == 2)
            .map(|t| t.lexeme.as_str())
            .collect();
        assert_eq!(line2, vec!["let", "b", "=", "10", ";"]);
    }

    #[test]
    fn syntax_error_produces_zero_lexical_diagnostics() {
        let (tokens, diags) = scan_with_recovery("let = 5;");
        assert!(diags.is_empty());
        assert_eq!(tokens.len(), 4);
        assert!(has_valid_let_syntax(&tokens).is_err());
    }
}
