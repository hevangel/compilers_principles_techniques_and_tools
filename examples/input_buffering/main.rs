//! Chapter 3 (§3.2): Double-Buffer Input Scanning with EOF Sentinels.
//!
//! Demonstrates:
//! 1. Buffer pair of size `2 * half_size + 2` with an out-of-band `EOF_SENTINEL` at the end
//!    of each half so normal character advance performs a single test (`c == EOF_SENTINEL`),
//! 2. `lexeme_begin` and `forward` pointers spanning half-buffer boundaries,
//! 3. Safe multi-step `retract()` across boundaries (e.g. `1234567..4` failing float lookahead
//!    and retracting both `.` bytes back across the half seam) using `half_reloaded_for_cycle`
//!    so advancing past the boundary again never overwrites unread data,
//! 4. Overflow detection when a single lexeme exceeds `half_size` before overwriting `lexeme_begin`,
//! 5. Exact equivalence between buffered scanning (`N = 8`) and direct slice scanning.

use std::fmt;

const EOF_SENTINEL: i16 = -1;

#[derive(Clone, Debug, PartialEq, Eq)]
struct Token {
    kind: String,
    lexeme: String,
    start: usize,
    end: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum BufferEvent {
    LoadHalf { half: usize, bytes_read: usize },
    HitSentinel { at_slot: usize },
    RetractAcrossBoundary { from_slot: usize, to_slot: usize },
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct BufferError {
    offset: usize,
    message: String,
}

impl fmt::Display for BufferError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "lexical buffer error at {}: {}",
            self.offset, self.message
        )
    }
}

struct DoubleBuffer {
    source: Vec<u8>,
    src_pos: usize,
    half_size: usize,
    buf: Vec<i16>,
    lexeme_begin: usize,
    forward: usize,
    lexeme_global_start: usize,
    /// Tracks whether the next half already holds fresh bytes ahead of `forward`, preventing a
    /// double-reload when `retract()` steps back across the seam.
    half_reloaded_for_cycle: [bool; 2],
    events: Vec<BufferEvent>,
}

impl DoubleBuffer {
    fn new(source: &[u8], half_size: usize) -> Self {
        assert!(half_size >= 2, "half_size must be at least 2");
        let total = 2 * half_size + 2;
        let mut db = Self {
            source: source.to_vec(),
            src_pos: 0,
            half_size,
            buf: vec![EOF_SENTINEL; total],
            lexeme_begin: 0,
            forward: 0,
            lexeme_global_start: 0,
            half_reloaded_for_cycle: [false, false],
            events: Vec::new(),
        };
        db.load_half(0);
        db
    }

    fn sentinel_slot(&self, half: usize) -> usize {
        if half == 0 {
            self.half_size
        } else {
            2 * self.half_size + 1
        }
    }

    fn half_start(&self, half: usize) -> usize {
        if half == 0 {
            0
        } else {
            self.half_size + 1
        }
    }

    fn load_half(&mut self, half: usize) {
        let start = self.half_start(half);
        let remaining = self.source.len().saturating_sub(self.src_pos);
        let to_copy = remaining.min(self.half_size);
        for i in 0..to_copy {
            self.buf[start + i] = i16::from(self.source[self.src_pos + i]);
        }
        self.buf[start + to_copy] = EOF_SENTINEL;
        let sentinel = self.sentinel_slot(half);
        self.buf[sentinel] = EOF_SENTINEL;
        self.src_pos += to_copy;
        self.half_reloaded_for_cycle[half] = true;
        self.events.push(BufferEvent::LoadHalf {
            half,
            bytes_read: to_copy,
        });
    }

    fn lexeme_begin_half(&self) -> usize {
        if self.lexeme_begin <= self.sentinel_slot(0) {
            0
        } else {
            1
        }
    }

    /// Reads the next character via `forward`, transparently crossing sentinel boundaries.
    fn next_char(&mut self) -> Result<Option<u8>, BufferError> {
        loop {
            let ch = self.buf[self.forward];
            if ch != EOF_SENTINEL {
                self.forward += 1;
                return Ok(Some(ch as u8));
            }
            if self.forward == self.sentinel_slot(0) {
                self.events.push(BufferEvent::HitSentinel {
                    at_slot: self.forward,
                });
                if !self.half_reloaded_for_cycle[1] {
                    if self.lexeme_begin_half() == 1 {
                        return Err(BufferError {
                            offset: self.lexeme_global_start,
                            message: format!(
                                "lexeme length exceeds half-buffer size N={}",
                                self.half_size
                            ),
                        });
                    }
                    self.load_half(1);
                }
                self.half_reloaded_for_cycle[0] = false;
                self.forward = self.half_start(1);
            } else if self.forward == self.sentinel_slot(1) {
                self.events.push(BufferEvent::HitSentinel {
                    at_slot: self.forward,
                });
                if !self.half_reloaded_for_cycle[0] {
                    if self.lexeme_begin_half() == 0 {
                        return Err(BufferError {
                            offset: self.lexeme_global_start,
                            message: format!(
                                "lexeme length exceeds half-buffer size N={}",
                                self.half_size
                            ),
                        });
                    }
                    self.load_half(0);
                }
                self.half_reloaded_for_cycle[1] = false;
                self.forward = self.half_start(0);
            } else {
                // Real end-of-file sentinel inside a partially filled half.
                return Ok(None);
            }
        }
    }

    /// Retracts `forward` by one logical character, stepping back across half boundaries if needed.
    fn retract(&mut self) {
        if self.forward == self.half_start(1) {
            let from = self.forward;
            // Moving back into half 0: half 1 already holds fresh data ahead of us.
            self.half_reloaded_for_cycle[1] = true;
            self.forward = self.sentinel_slot(0) - 1;
            self.events.push(BufferEvent::RetractAcrossBoundary {
                from_slot: from,
                to_slot: self.forward,
            });
        } else if self.forward == self.half_start(0) {
            let from = self.forward;
            self.half_reloaded_for_cycle[0] = true;
            self.forward = self.sentinel_slot(1) - 1;
            self.events.push(BufferEvent::RetractAcrossBoundary {
                from_slot: from,
                to_slot: self.forward,
            });
        } else {
            self.forward -= 1;
        }
    }

    fn commit_lexeme(&mut self) -> Result<(String, usize, usize), BufferError> {
        let mut bytes = Vec::new();
        let mut idx = self.lexeme_begin;
        while idx != self.forward {
            if idx == self.sentinel_slot(0) {
                idx = self.half_start(1);
                continue;
            }
            if idx == self.sentinel_slot(1) {
                idx = self.half_start(0);
                continue;
            }
            let ch = self.buf[idx];
            if ch == EOF_SENTINEL {
                break;
            }
            bytes.push(ch as u8);
            idx += 1;
        }
        let start = self.lexeme_global_start;
        let end = start + bytes.len();
        let text = String::from_utf8_lossy(&bytes).into_owned();
        self.lexeme_begin = self.forward;
        self.lexeme_global_start = end;
        Ok((text, start, end))
    }

    fn discard_whitespace(&mut self) -> Result<(), BufferError> {
        while let Some(b) = self.next_char()? {
            if !b.is_ascii_whitespace() {
                self.retract();
                break;
            }
            self.lexeme_begin = self.forward;
            self.lexeme_global_start += 1;
        }
        Ok(())
    }
}

fn scan_buffered(
    src: &str,
    half_size: usize,
) -> Result<(Vec<Token>, Vec<BufferEvent>), BufferError> {
    let mut db = DoubleBuffer::new(src.as_bytes(), half_size);
    let mut tokens = Vec::new();

    loop {
        db.discard_whitespace()?;
        let Some(first) = db.next_char()? else {
            break;
        };

        let kind = if first.is_ascii_alphabetic() || first == b'_' {
            while let Some(c) = db.next_char()? {
                if !(c.is_ascii_alphanumeric() || c == b'_') {
                    db.retract();
                    break;
                }
            }
            "IDENT"
        } else if first.is_ascii_digit() {
            while let Some(c) = db.next_char()? {
                if !c.is_ascii_digit() {
                    db.retract();
                    break;
                }
            }
            match db.next_char()? {
                Some(b'.') => match db.next_char()? {
                    Some(d) if d.is_ascii_digit() => {
                        while let Some(d2) = db.next_char()? {
                            if !d2.is_ascii_digit() {
                                db.retract();
                                break;
                            }
                        }
                        "FLOAT"
                    }
                    Some(_) => {
                        // Lookahead failed on second char and '.': retract twice (crosses seam!).
                        db.retract();
                        db.retract();
                        "INT"
                    }
                    None => {
                        db.retract();
                        "INT"
                    }
                },
                Some(_) => {
                    db.retract();
                    "INT"
                }
                None => "INT",
            }
        } else if first == b'.' {
            match db.next_char()? {
                Some(b'.') => "RANGE",
                Some(_) => {
                    db.retract();
                    return Err(BufferError {
                        offset: db.lexeme_global_start,
                        message: "unexpected '.'".to_string(),
                    });
                }
                None => {
                    return Err(BufferError {
                        offset: db.lexeme_global_start,
                        message: "unexpected '.'".to_string(),
                    });
                }
            }
        } else if first == b'<' {
            match db.next_char()? {
                Some(b'=') => "LE",
                Some(_) => {
                    db.retract();
                    "LT"
                }
                None => "LT",
            }
        } else if first == b'=' {
            match db.next_char()? {
                Some(b'=') => "EQEQ",
                Some(_) => {
                    db.retract();
                    "ASSIGN"
                }
                None => "ASSIGN",
            }
        } else if first == b'+' {
            "PLUS"
        } else if first == b';' {
            "SEMI"
        } else {
            return Err(BufferError {
                offset: db.lexeme_global_start,
                message: format!("unexpected byte '{}'", first as char),
            });
        };

        // Wait: in the `else if first.is_ascii_digit()` `else` branch above, `db.next_char()` returned
        // `Some(non_dot)` or `None` — let's make sure `Some(non_dot)` retracts!
        let _ = kind;
        let (lexeme, start, end) = db.commit_lexeme()?;
        tokens.push(Token {
            kind: kind.to_string(),
            lexeme,
            start,
            end,
        });
    }

    Ok((tokens, db.events))
}

fn scan_direct(src: &str) -> Result<Vec<Token>, BufferError> {
    let bytes = src.as_bytes();
    let mut i = 0;
    let mut tokens = Vec::new();
    while i < bytes.len() {
        if bytes[i].is_ascii_whitespace() {
            i += 1;
            continue;
        }
        let start = i;
        let kind = if bytes[i].is_ascii_alphabetic() || bytes[i] == b'_' {
            i += 1;
            while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                i += 1;
            }
            "IDENT"
        } else if bytes[i].is_ascii_digit() {
            i += 1;
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                i += 1;
            }
            if bytes.get(i) == Some(&b'.') && bytes.get(i + 1).is_some_and(|b| b.is_ascii_digit()) {
                i += 1;
                while i < bytes.len() && bytes[i].is_ascii_digit() {
                    i += 1;
                }
                "FLOAT"
            } else {
                "INT"
            }
        } else if bytes[i] == b'.' && bytes.get(i + 1) == Some(&b'.') {
            i += 2;
            "RANGE"
        } else if bytes[i] == b'<' {
            if bytes.get(i + 1) == Some(&b'=') {
                i += 2;
                "LE"
            } else {
                i += 1;
                "LT"
            }
        } else if bytes[i] == b'=' {
            if bytes.get(i + 1) == Some(&b'=') {
                i += 2;
                "EQEQ"
            } else {
                i += 1;
                "ASSIGN"
            }
        } else if bytes[i] == b'+' {
            i += 1;
            "PLUS"
        } else if bytes[i] == b';' {
            i += 1;
            "SEMI"
        } else {
            return Err(BufferError {
                offset: start,
                message: format!("unexpected byte '{}'", bytes[i] as char),
            });
        };
        tokens.push(Token {
            kind: kind.to_string(),
            lexeme: src[start..i].to_string(),
            start,
            end: i,
        });
    }
    Ok(tokens)
}

fn main() {
    let src = "alpha = beta1 <= 2.5 + 1..4 == 42;";
    let (tokens, events) = scan_buffered(src, 8).expect("buffered scan should succeed");
    let direct = scan_direct(src).expect("direct scan should succeed");
    assert_eq!(tokens, direct);

    println!("=== Double-Buffer Input Scanning with Sentinels (§3.2) ===");
    println!("Source ({} bytes), half-buffer size N = 8:", src.len());
    println!("  {src}\n");
    println!(
        "Tokens matched ({}, identical to direct slice scan):",
        tokens.len()
    );
    for t in &tokens {
        println!(
            "  [{:02}..{:02}] {:<6} {:?}",
            t.start, t.end, t.kind, t.lexeme
        );
    }
    println!(
        "\nBuffer reloads, sentinel hits & boundary retractions ({}):",
        events.len()
    );
    for ev in &events {
        match ev {
            BufferEvent::LoadHalf { half, bytes_read } => {
                println!("  load_half({half}) -> copied {bytes_read} byte(s) + EOF sentinel");
            }
            BufferEvent::HitSentinel { at_slot } => {
                println!("  hit_sentinel(slot={at_slot}) -> switch buffer half");
            }
            BufferEvent::RetractAcrossBoundary { from_slot, to_slot } => {
                println!(
                    "  retract({from_slot} -> {to_slot}) across boundary (fence prevents double-reload)"
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn buffered_scan_matches_direct_across_varied_half_sizes() {
        let src = "x1 <= y2 + 12.34 + 1..9; a==b; c<d; total = 99;";
        let direct = scan_direct(src).expect("direct scan works");
        for half_size in [8usize, 9, 10, 11, 16, 64] {
            let (buffered, events) =
                scan_buffered(src, half_size).expect("buffered scan should match direct");
            assert_eq!(buffered, direct, "mismatch at half_size={half_size}");
            assert!(
                events
                    .iter()
                    .any(|e| matches!(e, BufferEvent::LoadHalf { .. })),
                "expected buffer loads"
            );
        }
    }

    #[test]
    fn retract_across_boundary_does_not_reload_same_half_twice() {
        // Indices 0..7 are "1234567.", slot 8 is sentinel, index 9+ is ".4"
        // Trying FLOAT fails at second '.', forcing 2 retractions across seam 9 -> 7!
        let src = "1234567..4";
        let (tokens, events) = scan_buffered(src, 8).expect("scan succeeds");
        assert_eq!(
            tokens,
            vec![
                Token {
                    kind: "INT".into(),
                    lexeme: "1234567".into(),
                    start: 0,
                    end: 7,
                },
                Token {
                    kind: "RANGE".into(),
                    lexeme: "..".into(),
                    start: 7,
                    end: 9,
                },
                Token {
                    kind: "INT".into(),
                    lexeme: "4".into(),
                    start: 9,
                    end: 10,
                },
            ]
        );
        let half1_loads = events
            .iter()
            .filter(|e| matches!(e, BufferEvent::LoadHalf { half: 1, .. }))
            .count();
        assert_eq!(half1_loads, 1);
        assert!(events
            .iter()
            .any(|e| matches!(e, BufferEvent::RetractAcrossBoundary { .. })));
    }

    #[test]
    fn rejects_lexeme_longer_than_half_buffer() {
        let src = "very_long_identifier";
        let err = scan_buffered(src, 8).expect_err("identifier exceeds N=8");
        assert!(err.message.contains("exceeds half-buffer"));
    }
}
