//! Modern Extension: Unicode-Aware Source Lexing, Display Columns & Security Diagnostics.
//!
//! Explicitly labeled as a modern compiler front-end extension complementing
//! Dragon Book Ch. 3:
//! 1. Dual coordinate tracking: UTF-8 byte offsets (`byte_start..byte_end`), Unicode
//!    scalar indices (`scalar_start..scalar_end`), and visual `line:col` coordinates
//!    with Unicode whitespace handling (e.g., U+00A0, U+3000),
//! 2. Deterministic UAX #31 style `XID_Start` / `XID_Continue` identifier scanning
//!    for ASCII, accented Latin (`café`), Greek (`αβγ`), and CJK (`変数`) identifiers,
//! 3. Security diagnostics detecting bidirectional overrides (Trojan Source,
//!    U+202A..U+202E / U+2066..U+2069), invisible zero-width characters (U+200B),
//!    and mixed-script homoglyph/confusable identifiers (Latin + Cyrillic `а`).

use std::collections::BTreeSet;
use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Script {
    AsciiLatin,
    LatinExt,
    Greek,
    Cyrillic,
    Han,
    HiraganaKatakana,
}

impl fmt::Display for Script {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            Script::AsciiLatin => "Latin",
            Script::LatinExt => "Latin",
            Script::Greek => "Greek",
            Script::Cyrillic => "Cyrillic",
            Script::Han => "Han",
            Script::HiraganaKatakana => "Kana",
        };
        f.write_str(name)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TokKind {
    KwLet,
    Ident,
    Int,
    Str,
    Eq,
    Plus,
    Semi,
    Eof,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct UnicodeSpan {
    byte_start: usize,
    byte_end: usize,
    scalar_start: usize,
    scalar_end: usize,
    line: usize,
    col: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Token {
    kind: TokKind,
    lexeme: String,
    span: UnicodeSpan,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DiagKind {
    BidiControl,
    ZeroWidthChar,
    MixedScriptConfusable,
    InvalidChar,
    UnterminatedString,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct LexDiagnostic {
    kind: DiagKind,
    byte_offset: usize,
    scalar_offset: usize,
    line: usize,
    col: usize,
    message: String,
}

fn is_bidi_control(c: char) -> bool {
    matches!(
        c as u32,
        0x200E | 0x200F | 0x202A..=0x202E | 0x2066..=0x2069
    )
}

fn is_zero_width(c: char) -> bool {
    matches!(c as u32, 0x200B | 0x200C | 0x200D | 0xFEFF)
}

fn is_unicode_ws(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\r' | '\n' | '\u{00A0}' | '\u{3000}')
}

fn letter_script(c: char) -> Option<Script> {
    let cp = c as u32;
    match cp {
        0x0041..=0x005A | 0x0061..=0x007A => Some(Script::AsciiLatin),
        0x00C0..=0x00D6 | 0x00D8..=0x00F6 | 0x00F8..=0x024F => Some(Script::LatinExt),
        0x0370..=0x03FF => {
            if c.is_alphabetic() {
                Some(Script::Greek)
            } else {
                None
            }
        }
        0x0400..=0x04FF => {
            if c.is_alphabetic() {
                Some(Script::Cyrillic)
            } else {
                None
            }
        }
        0x3041..=0x3096 | 0x30A1..=0x30FA => Some(Script::HiraganaKatakana),
        0x4E00..=0x9FFF => Some(Script::Han),
        _ => None,
    }
}

fn is_xid_start(c: char) -> bool {
    c == '_' || letter_script(c).is_some()
}

fn is_combining_mark(c: char) -> bool {
    matches!(c as u32, 0x0300..=0x036F)
}

fn is_xid_continue(c: char) -> bool {
    is_xid_start(c) || c.is_ascii_digit() || is_combining_mark(c)
}

fn canonical_script_name(s: Script) -> &'static str {
    match s {
        Script::AsciiLatin | Script::LatinExt => "Latin",
        Script::Greek => "Greek",
        Script::Cyrillic => "Cyrillic",
        Script::Han => "Han",
        Script::HiraganaKatakana => "Kana",
    }
}

/// Counts visible column cells (CJK wide characters = 2 columns; combining marks = 0; ASCII = 1).
fn display_width(c: char) -> usize {
    if is_combining_mark(c) || is_zero_width(c) || is_bidi_control(c) {
        0
    } else if matches!(c as u32, 0x3000..=0x30FF | 0x4E00..=0x9FFF) {
        2
    } else {
        1
    }
}

struct Scanner<'a> {
    src: &'a str,
    byte_pos: usize,
    scalar_pos: usize,
    line: usize,
    col: usize,
}

impl<'a> Scanner<'a> {
    fn new(src: &'a str) -> Self {
        Self {
            src,
            byte_pos: 0,
            scalar_pos: 0,
            line: 1,
            col: 1,
        }
    }

    fn peek(&self) -> Option<char> {
        self.src[self.byte_pos..].chars().next()
    }

    fn advance(&mut self) -> Option<char> {
        let c = self.peek()?;
        self.byte_pos += c.len_utf8();
        self.scalar_pos += 1;
        if c == '\n' {
            self.line += 1;
            self.col = 1;
        } else if c == '\t' {
            let tab_stop = 4;
            self.col += tab_stop - ((self.col - 1) % tab_stop);
        } else {
            self.col += display_width(c);
        }
        Some(c)
    }

    fn lex(mut self) -> (Vec<Token>, Vec<LexDiagnostic>) {
        let mut tokens = Vec::new();
        let mut diags = Vec::new();

        while let Some(c) = self.peek() {
            if is_unicode_ws(c) {
                self.advance();
                continue;
            }

            let byte_start = self.byte_pos;
            let scalar_start = self.scalar_pos;
            let line = self.line;
            let col = self.col;

            if is_bidi_control(c) {
                self.advance();
                diags.push(LexDiagnostic {
                    kind: DiagKind::BidiControl,
                    byte_offset: byte_start,
                    scalar_offset: scalar_start,
                    line,
                    col,
                    message: format!(
                        "Trojan Source bidi control U+{:04X} rejected outside literals",
                        c as u32
                    ),
                });
                continue;
            }

            if is_zero_width(c) {
                self.advance();
                diags.push(LexDiagnostic {
                    kind: DiagKind::ZeroWidthChar,
                    byte_offset: byte_start,
                    scalar_offset: scalar_start,
                    line,
                    col,
                    message: format!("invisible zero-width code point U+{:04X}", c as u32),
                });
                continue;
            }

            if c == '/' && self.src[self.byte_pos..].starts_with("//") {
                while let Some(ch) = self.peek() {
                    if ch == '\n' {
                        break;
                    }
                    if is_bidi_control(ch) {
                        diags.push(LexDiagnostic {
                            kind: DiagKind::BidiControl,
                            byte_offset: self.byte_pos,
                            scalar_offset: self.scalar_pos,
                            line: self.line,
                            col: self.col,
                            message: format!(
                                "Trojan Source bidi control U+{:04X} inside comment",
                                ch as u32
                            ),
                        });
                    }
                    self.advance();
                }
                continue;
            }

            if is_xid_start(c) {
                let mut scripts: BTreeSet<&'static str> = BTreeSet::new();
                while let Some(ch) = self.peek() {
                    if is_zero_width(ch) {
                        diags.push(LexDiagnostic {
                            kind: DiagKind::ZeroWidthChar,
                            byte_offset: self.byte_pos,
                            scalar_offset: self.scalar_pos,
                            line: self.line,
                            col: self.col,
                            message: format!(
                                "invisible zero-width character U+{:04X} inside identifier",
                                ch as u32
                            ),
                        });
                        self.advance();
                        break;
                    }
                    if !is_xid_continue(ch) {
                        break;
                    }
                    if let Some(sc) = letter_script(ch) {
                        scripts.insert(canonical_script_name(sc));
                    }
                    self.advance();
                }
                let lexeme = self.src[byte_start..self.byte_pos].to_string();
                if scripts.len() > 1 {
                    let list: Vec<&str> = scripts.into_iter().collect();
                    diags.push(LexDiagnostic {
                        kind: DiagKind::MixedScriptConfusable,
                        byte_offset: byte_start,
                        scalar_offset: scalar_start,
                        line,
                        col,
                        message: format!(
                            "mixed-script identifier '{}' mixes [{}] (possible homoglyph)",
                            lexeme,
                            list.join(", ")
                        ),
                    });
                }
                let kind = if lexeme == "let" {
                    TokKind::KwLet
                } else {
                    TokKind::Ident
                };
                tokens.push(Token {
                    kind,
                    lexeme,
                    span: UnicodeSpan {
                        byte_start,
                        byte_end: self.byte_pos,
                        scalar_start,
                        scalar_end: self.scalar_pos,
                        line,
                        col,
                    },
                });
                continue;
            }

            if c.is_ascii_digit() {
                while let Some(ch) = self.peek() {
                    if ch.is_ascii_digit() {
                        self.advance();
                    } else {
                        break;
                    }
                }
                tokens.push(Token {
                    kind: TokKind::Int,
                    lexeme: self.src[byte_start..self.byte_pos].to_string(),
                    span: UnicodeSpan {
                        byte_start,
                        byte_end: self.byte_pos,
                        scalar_start,
                        scalar_end: self.scalar_pos,
                        line,
                        col,
                    },
                });
                continue;
            }

            if c == '"' {
                self.advance();
                let mut closed = false;
                while let Some(ch) = self.peek() {
                    if is_bidi_control(ch) {
                        diags.push(LexDiagnostic {
                            kind: DiagKind::BidiControl,
                            byte_offset: self.byte_pos,
                            scalar_offset: self.scalar_pos,
                            line: self.line,
                            col: self.col,
                            message: format!(
                                "Trojan Source bidi control U+{:04X} inside string literal",
                                ch as u32
                            ),
                        });
                    }
                    if ch == '\n' {
                        break;
                    }
                    self.advance();
                    if ch == '"' {
                        closed = true;
                        break;
                    }
                }
                if !closed {
                    diags.push(LexDiagnostic {
                        kind: DiagKind::UnterminatedString,
                        byte_offset: byte_start,
                        scalar_offset: scalar_start,
                        line,
                        col,
                        message: "unterminated UTF-8 string literal".into(),
                    });
                }
                tokens.push(Token {
                    kind: TokKind::Str,
                    lexeme: self.src[byte_start..self.byte_pos].to_string(),
                    span: UnicodeSpan {
                        byte_start,
                        byte_end: self.byte_pos,
                        scalar_start,
                        scalar_end: self.scalar_pos,
                        line,
                        col,
                    },
                });
                continue;
            }

            let simple = match c {
                '=' => Some(TokKind::Eq),
                '+' => Some(TokKind::Plus),
                ';' => Some(TokKind::Semi),
                _ => None,
            };
            self.advance();
            if let Some(kind) = simple {
                tokens.push(Token {
                    kind,
                    lexeme: c.to_string(),
                    span: UnicodeSpan {
                        byte_start,
                        byte_end: self.byte_pos,
                        scalar_start,
                        scalar_end: self.scalar_pos,
                        line,
                        col,
                    },
                });
            } else {
                diags.push(LexDiagnostic {
                    kind: DiagKind::InvalidChar,
                    byte_offset: byte_start,
                    scalar_offset: scalar_start,
                    line,
                    col,
                    message: format!("unexpected Unicode character '{}' (U+{:04X})", c, c as u32),
                });
            }
        }

        tokens.push(Token {
            kind: TokKind::Eof,
            lexeme: "$".into(),
            span: UnicodeSpan {
                byte_start: self.byte_pos,
                byte_end: self.byte_pos,
                scalar_start: self.scalar_pos,
                scalar_end: self.scalar_pos,
                line: self.line,
                col: self.col,
            },
        });
        (tokens, diags)
    }
}

fn lex_unicode(src: &str) -> (Vec<Token>, Vec<LexDiagnostic>) {
    Scanner::new(src).lex()
}

fn main() {
    println!("=== Advanced Parsing (Modern): Unicode-Aware Lexing & Security Diagnostics ===");
    let clean = "let café = 10;\nlet αβγ\u{3000}= café + 変数;";
    let (tokens, diags) = lex_unicode(clean);
    assert!(diags.is_empty());
    println!(
        "Clean multilingual program ({} tokens, 0 warnings):",
        tokens.len()
    );
    for t in &tokens {
        println!(
            "  {:<6?} {:<6} bytes={}..{} scalars={}..{} @{}:{}",
            t.kind,
            t.lexeme,
            t.span.byte_start,
            t.span.byte_end,
            t.span.scalar_start,
            t.span.scalar_end,
            t.span.line,
            t.span.col
        );
    }

    let suspect = "let v\u{0430}r = 1; // \u{202E}locked";
    let (_, sec_diags) = lex_unicode(suspect);
    println!(
        "Security diagnostics on spoofed source ({}):",
        sec_diags.len()
    );
    for d in &sec_diags {
        println!(
            "  {:?} at {}:{} (byte {}, scalar {}): {}",
            d.kind, d.line, d.col, d.byte_offset, d.scalar_offset, d.message
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tracks_utf8_byte_offsets_scalar_indices_and_display_columns_independently() {
        let src = "let café = 変数 + αβγ;";
        let (tokens, diags) = lex_unicode(src);
        assert!(diags.is_empty());
        // `café` is 4 scalars, 5 UTF-8 bytes (`é` is 2 bytes).
        assert_eq!(tokens[1].lexeme, "café");
        assert_eq!(tokens[1].span.byte_start, 4);
        assert_eq!(tokens[1].span.byte_end, 9);
        assert_eq!(tokens[1].span.scalar_start, 4);
        assert_eq!(tokens[1].span.scalar_end, 8);
        assert_eq!(tokens[1].span.col, 5);
        // `変数` is 2 wide CJK scalars (6 bytes), occupying display columns 12..16.
        let cjk = &tokens[3];
        assert_eq!(cjk.lexeme, "変数");
        assert_eq!(cjk.span.byte_end - cjk.span.byte_start, 6);
        assert_eq!(cjk.span.scalar_end - cjk.span.scalar_start, 2);
        assert_eq!(tokens[4].lexeme, "+");
        assert_eq!(tokens[4].span.col, cjk.span.col + 4 + 1);
    }

    #[test]
    fn flags_mixed_script_homoglyph_trojan_bidi_and_zero_width_separately() {
        // Cyrillic small 'а' (U+0430) inside Latin identifier + Trojan Source RLO U+202E + ZWSP U+200B.
        let src = "let p\u{0430}ypal = 1; // \u{202E}admin\nlet x\u{200B}y = 2;";
        let (_, diags) = lex_unicode(src);
        let kinds: Vec<DiagKind> = diags.iter().map(|d| d.kind).collect();
        assert_eq!(
            kinds,
            vec![
                DiagKind::MixedScriptConfusable,
                DiagKind::BidiControl,
                DiagKind::ZeroWidthChar,
            ]
        );
    }

    #[test]
    fn rejects_non_identifier_emoji_without_panicking_on_multibyte_boundaries() {
        let src = "let x = 🚀;";
        let (tokens, diags) = lex_unicode(src);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].kind, DiagKind::InvalidChar);
        assert_eq!(tokens.iter().filter(|t| t.kind == TokKind::Semi).count(), 1);
    }
}
