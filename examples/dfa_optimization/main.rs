//! Chapter 3 (§3.9): DFA Optimization — Equivalence Classes, Comb Compression & Direct Code.
//!
//! Demonstrates:
//! 1. Building a multi-token scanner DFA (keywords `let`/`while`, `IDENT`, `INT`, `FLOAT`,
//!    operators `<=`, `<`, `=`, `+`, `;`, `WS`) over ASCII bytes `0..128`,
//! 2. Partitioning the alphabet into minimal character equivalence classes (`char_class[128]`),
//! 3. Compressing the transition matrix with Dragon Book §3.9 `default / base / next / check`
//!    row-displacement (comb) encoding,
//! 4. Generating and running a direct-coded scanner (explicit `match` transitions without tables),
//! 5. Verifying identical longest-match token streams across all three DFA representations.

use std::collections::BTreeMap;
use std::fmt::Write as _;

const DEAD: usize = usize::MAX;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum TokKind {
    Let,
    While,
    Ident,
    Int,
    Float,
    Le,
    Lt,
    Eq,
    Plus,
    Semi,
    Ws,
}

impl TokKind {
    fn name(self) -> &'static str {
        match self {
            TokKind::Let => "LET",
            TokKind::While => "WHILE",
            TokKind::Ident => "IDENT",
            TokKind::Int => "INT",
            TokKind::Float => "FLOAT",
            TokKind::Le => "LE",
            TokKind::Lt => "LT",
            TokKind::Eq => "EQ",
            TokKind::Plus => "PLUS",
            TokKind::Semi => "SEMI",
            TokKind::Ws => "WS",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Token {
    kind: TokKind,
    lexeme: String,
    start: usize,
    end: usize,
}

/// Full uncompressed 2D DFA over ASCII bytes `0..128`.
struct DenseDfa {
    accept: Vec<Option<TokKind>>,
    trans: Vec<[usize; 128]>,
}

fn build_scanner_dfa() -> DenseDfa {
    // States:
    //  0: start
    //  1: WS
    //  2: IDENT generic
    //  3: 'l' (IDENT)
    //  4: 'le' (IDENT)
    //  5: 'let' (LET)
    //  6: 'w' (IDENT)
    //  7: 'wh' (IDENT)
    //  8: 'whi' (IDENT)
    //  9: 'whil' (IDENT)
    // 10: 'while' (WHILE)
    // 11: INT
    // 12: INT '.' (non-accepting)
    // 13: FLOAT
    // 14: '<' (LT)
    // 15: '<=' (LE)
    // 16: '=' (EQ)
    // 17: '+' (PLUS)
    // 18: ';' (SEMI)
    let accept = vec![
        None,                 // 0
        Some(TokKind::Ws),    // 1
        Some(TokKind::Ident), // 2
        Some(TokKind::Ident), // 3 'l'
        Some(TokKind::Ident), // 4 'le'
        Some(TokKind::Let),   // 5 'let'
        Some(TokKind::Ident), // 6 'w'
        Some(TokKind::Ident), // 7 'wh'
        Some(TokKind::Ident), // 8 'whi'
        Some(TokKind::Ident), // 9 'whil'
        Some(TokKind::While), // 10 'while'
        Some(TokKind::Int),   // 11
        None,                 // 12 '123.'
        Some(TokKind::Float), // 13
        Some(TokKind::Lt),    // 14
        Some(TokKind::Le),    // 15
        Some(TokKind::Eq),    // 16
        Some(TokKind::Plus),  // 17
        Some(TokKind::Semi),  // 18
    ];
    let num_states = accept.len();
    let mut trans = vec![[DEAD; 128]; num_states];

    let is_id_start = |b: u8| b.is_ascii_alphabetic() || b == b'_';
    let is_id_cont = |b: u8| b.is_ascii_alphanumeric() || b == b'_';

    for b in 0u8..128 {
        let c = b as usize;
        if b == b' ' || b == b'\t' || b == b'\n' || b == b'\r' {
            trans[0][c] = 1;
            trans[1][c] = 1;
        } else if is_id_start(b) {
            trans[0][c] = match b {
                b'l' => 3,
                b'w' => 6,
                _ => 2,
            };
        } else if b.is_ascii_digit() {
            trans[0][c] = 11;
            trans[11][c] = 11;
            trans[12][c] = 13;
            trans[13][c] = 13;
        } else if b == b'<' {
            trans[0][c] = 14;
        } else if b == b'=' {
            trans[0][c] = 16;
            trans[14][c] = 15;
        } else if b == b'+' {
            trans[0][c] = 17;
        } else if b == b';' {
            trans[0][c] = 18;
        } else if b == b'.' {
            trans[11][c] = 12;
        }

        if is_id_cont(b) {
            for row in &mut trans[2..=10] {
                row[c] = 2;
            }
        }
    }
    // Keyword refinement edges override generic IDENT state 2:
    trans[3][b'e' as usize] = 4;
    trans[4][b't' as usize] = 5;
    trans[6][b'h' as usize] = 7;
    trans[7][b'i' as usize] = 8;
    trans[8][b'l' as usize] = 9;
    trans[9][b'e' as usize] = 10;

    DenseDfa { accept, trans }
}

struct EquivClassDfa {
    char_class: [usize; 128],
    num_classes: usize,
    class_rep: Vec<u8>,
    accept: Vec<Option<TokKind>>,
    table: Vec<Vec<usize>>, // [state][class]
}

fn build_equiv_classes(dfa: &DenseDfa) -> EquivClassDfa {
    let mut sig_to_class: BTreeMap<Vec<usize>, usize> = BTreeMap::new();
    let mut char_class = [0usize; 128];
    let mut class_rep = Vec::new();

    for b in 0u8..128 {
        let sig: Vec<usize> = dfa.trans.iter().map(|row| row[b as usize]).collect();
        let cid = match sig_to_class.get(&sig) {
            Some(&id) => id,
            None => {
                let id = class_rep.len();
                sig_to_class.insert(sig, id);
                class_rep.push(b);
                id
            }
        };
        char_class[b as usize] = cid;
    }

    let num_classes = class_rep.len();
    let mut table = vec![vec![DEAD; num_classes]; dfa.accept.len()];
    for (row, src_row) in table.iter_mut().zip(&dfa.trans) {
        for (cid, &rep) in class_rep.iter().enumerate() {
            row[cid] = src_row[rep as usize];
        }
    }

    EquivClassDfa {
        char_class,
        num_classes,
        class_rep,
        accept: dfa.accept.clone(),
        table,
    }
}

/// Dragon Book §3.9 four-array row-displacement (comb) compression: `default`, `base`, `next`, `check`.
struct CompressedCombDfa {
    char_class: [usize; 128],
    accept: Vec<Option<TokKind>>,
    default: Vec<Option<usize>>,
    base: Vec<usize>,
    next: Vec<usize>,
    check: Vec<Option<usize>>,
}

impl CompressedCombDfa {
    fn from_equiv(eq: &EquivClassDfa) -> Self {
        let n = eq.accept.len();
        let c = eq.num_classes;
        let mut default = vec![None; n];
        let mut diffs: Vec<Vec<(usize, usize)>> = Vec::with_capacity(n);

        // Pick a strictly earlier state (so default chains are guaranteed acyclic!) with minimal diffs.
        for (s, def_slot) in default.iter_mut().enumerate() {
            let mut best_def: Option<usize> = None;
            let mut best_diff: Vec<(usize, usize)> = (0..c)
                .filter_map(|cls| {
                    let t = eq.table[s][cls];
                    (t != DEAD).then_some((cls, t))
                })
                .collect();

            for cand in 0..s {
                let cand_diff: Vec<(usize, usize)> = (0..c)
                    .filter_map(|cls| {
                        let t = eq.table[s][cls];
                        (t != eq.table[cand][cls]).then_some((cls, t))
                    })
                    .collect();
                if cand_diff.len() < best_diff.len() {
                    best_diff = cand_diff;
                    best_def = Some(cand);
                }
            }
            *def_slot = best_def;
            diffs.push(best_diff);
        }

        // First-fit row displacement into `next` and `check`.
        let mut base = vec![0usize; n];
        let mut next: Vec<usize> = Vec::new();
        let mut check: Vec<Option<usize>> = Vec::new();

        for (s, entries) in diffs.iter().enumerate() {
            if entries.is_empty() {
                base[s] = 0;
                continue;
            }
            let mut b = 0usize;
            loop {
                let needed = b + c;
                if next.len() < needed {
                    next.resize(needed, DEAD);
                    check.resize(needed, None);
                }
                if entries.iter().all(|&(cls, _)| check[b + cls].is_none()) {
                    base[s] = b;
                    for &(cls, target) in entries {
                        check[b + cls] = Some(s);
                        next[b + cls] = target;
                    }
                    break;
                }
                b += 1;
            }
        }

        // Trim unused trailing slots.
        while check.last() == Some(&None) {
            check.pop();
            next.pop();
        }

        Self {
            char_class: eq.char_class,
            accept: eq.accept.clone(),
            default,
            base,
            next,
            check,
        }
    }

    fn step(&self, state: usize, byte: u8) -> usize {
        if byte >= 128 {
            return DEAD;
        }
        let cls = self.char_class[byte as usize];
        let mut cur = state;
        loop {
            let idx = self.base[cur] + cls;
            if idx < self.check.len() && self.check[idx] == Some(cur) {
                return self.next[idx];
            }
            match self.default[cur] {
                Some(d) => cur = d,
                None => return DEAD,
            }
        }
    }

    fn storage_cells(&self) -> usize {
        // char_class[128] + accept[S] + default[S] + base[S] + next[K] + check[K]
        128 + 3 * self.accept.len() + 2 * self.next.len()
    }
}

fn scan_with_step<F>(
    src: &str,
    accept: &[Option<TokKind>],
    mut step: F,
) -> Result<Vec<Token>, String>
where
    F: FnMut(usize, u8) -> usize,
{
    let bytes = src.as_bytes();
    let mut pos = 0usize;
    let mut tokens = Vec::new();
    while pos < bytes.len() {
        let mut state = 0usize;
        let mut cur = pos;
        let mut last_accept: Option<(TokKind, usize)> = None;
        while cur < bytes.len() {
            let next_state = step(state, bytes[cur]);
            if next_state == DEAD {
                break;
            }
            state = next_state;
            cur += 1;
            if let Some(kind) = accept[state] {
                last_accept = Some((kind, cur));
            }
        }
        let Some((kind, end)) = last_accept else {
            return Err(format!(
                "unexpected byte {:?} at {}",
                bytes[pos] as char, pos
            ));
        };
        if kind != TokKind::Ws {
            tokens.push(Token {
                kind,
                lexeme: src[pos..end].to_string(),
                start: pos,
                end,
            });
        }
        pos = end;
    }
    Ok(tokens)
}

/// Generates readable direct-coded Rust source snippet from the DFA transitions.
fn emit_direct_coded_snippet(dfa: &DenseDfa, max_states: usize) -> String {
    let mut out =
        String::from("fn direct_step(state: usize, b: u8) -> usize {\n    match state {\n");
    for s in 0..max_states.min(dfa.accept.len()) {
        let mut by_target: BTreeMap<usize, Vec<u8>> = BTreeMap::new();
        for b in 0u8..128 {
            let t = dfa.trans[s][b as usize];
            if t != DEAD {
                by_target.entry(t).or_default().push(b);
            }
        }
        if by_target.is_empty() {
            continue;
        }
        let _ = writeln!(out, "        {s} => match b {{");
        for (target, bytes) in by_target {
            if bytes.len() <= 3 {
                let lits: Vec<String> = bytes
                    .iter()
                    .map(|&b| {
                        if b.is_ascii_graphic() {
                            format!("b'{}'", b as char)
                        } else {
                            format!("{b}")
                        }
                    })
                    .collect();
                let _ = writeln!(out, "            {} => {target},", lits.join(" | "));
            } else {
                let _ = writeln!(
                    out,
                    "            /* {} byte class */ _ => {target},",
                    bytes.len()
                );
            }
        }
        out.push_str("            _ => DEAD,\n        },\n");
    }
    out.push_str("        _ => DEAD,\n    }\n}");
    out
}

/// Direct-coded handwritten/generated DFA stepper without any transition array lookups.
fn direct_coded_step(state: usize, b: u8) -> usize {
    let is_id_cont = b.is_ascii_alphanumeric() || b == b'_';
    match state {
        0 => match b {
            b' ' | b'\t' | b'\n' | b'\r' => 1,
            b'l' => 3,
            b'w' => 6,
            b if b.is_ascii_alphabetic() || b == b'_' => 2,
            b'0'..=b'9' => 11,
            b'<' => 14,
            b'=' => 16,
            b'+' => 17,
            b';' => 18,
            _ => DEAD,
        },
        1 => match b {
            b' ' | b'\t' | b'\n' | b'\r' => 1,
            _ => DEAD,
        },
        3 => {
            if b == b'e' {
                4
            } else if is_id_cont {
                2
            } else {
                DEAD
            }
        }
        4 => {
            if b == b't' {
                5
            } else if is_id_cont {
                2
            } else {
                DEAD
            }
        }
        6 => {
            if b == b'h' {
                7
            } else if is_id_cont {
                2
            } else {
                DEAD
            }
        }
        7 => {
            if b == b'i' {
                8
            } else if is_id_cont {
                2
            } else {
                DEAD
            }
        }
        8 => {
            if b == b'l' {
                9
            } else if is_id_cont {
                2
            } else {
                DEAD
            }
        }
        9 => {
            if b == b'e' {
                10
            } else if is_id_cont {
                2
            } else {
                DEAD
            }
        }
        2 | 5 | 10 => {
            if is_id_cont {
                2
            } else {
                DEAD
            }
        }
        11 => match b {
            b'0'..=b'9' => 11,
            b'.' => 12,
            _ => DEAD,
        },
        12 | 13 => {
            if b.is_ascii_digit() {
                13
            } else {
                DEAD
            }
        }
        14 => {
            if b == b'=' {
                15
            } else {
                DEAD
            }
        }
        _ => DEAD,
    }
}

fn main() {
    let dense = build_scanner_dfa();
    let eq = build_equiv_classes(&dense);
    let comb = CompressedCombDfa::from_equiv(&eq);

    let dense_cells = dense.accept.len() * 128;
    let eq_cells = 128 + dense.accept.len() * eq.num_classes;
    let comb_cells = comb.storage_cells();

    println!(
        "=== DFA Optimization: Equivalence Classes, Comb Compression & Direct Code (§3.9) ==="
    );
    println!("DFA states: {}", dense.accept.len());
    println!(
        "Alphabet compression: 128 ASCII bytes -> {} equivalence classes",
        eq.num_classes
    );
    println!("Equivalence class representatives:");
    for (cid, &rep) in eq.class_rep.iter().enumerate() {
        let label = if rep.is_ascii_graphic() {
            format!("{:?}", rep as char)
        } else {
            format!("byte({rep})")
        };
        println!("  class {cid:>2}: rep {label}");
    }
    println!("\nTable footprint (integer/option cells):");
    println!("  1. Full 2D DFA (states * 128):            {dense_cells} cells");
    println!("  2. Equiv-classed 2D (128 + states * C):   {eq_cells} cells");
    println!(
        "  3. Comb default/base/next/check:          {comb_cells} cells (next.len = {})",
        comb.next.len()
    );

    let sample = "let while1 = 12 + 3.5; while let2 <= while1;";
    let t_dense = scan_with_step(sample, &dense.accept, |s, b| {
        if b < 128 {
            dense.trans[s][b as usize]
        } else {
            DEAD
        }
    })
    .expect("dense scan succeeds");
    let t_eq = scan_with_step(sample, &eq.accept, |s, b| {
        if b < 128 {
            eq.table[s][eq.char_class[b as usize]]
        } else {
            DEAD
        }
    })
    .expect("equiv scan succeeds");
    let t_comb =
        scan_with_step(sample, &comb.accept, |s, b| comb.step(s, b)).expect("comb scan succeeds");
    let t_direct =
        scan_with_step(sample, &dense.accept, direct_coded_step).expect("direct scan succeeds");

    assert_eq!(t_dense, t_eq);
    assert_eq!(t_dense, t_comb);
    assert_eq!(t_dense, t_direct);

    println!(
        "\nAll 4 DFA engines matched identical {} tokens on {:?}:",
        t_comb.len(),
        sample
    );
    for t in &t_comb {
        println!(
            "  [{:02}..{:02}] {:<5} {:?}",
            t.start,
            t.end,
            t.kind.name(),
            t.lexeme
        );
    }

    println!(
        "\nEmitted direct-coded preview (first 4 states):\n{}",
        emit_direct_coded_snippet(&dense, 4)
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn equivalence_classes_and_comb_compression_preserve_all_transitions() {
        let dense = build_scanner_dfa();
        let eq = build_equiv_classes(&dense);
        let comb = CompressedCombDfa::from_equiv(&eq);

        for s in 0..dense.accept.len() {
            for b in 0u8..128 {
                let d_next = dense.trans[s][b as usize];
                let e_next = eq.table[s][eq.char_class[b as usize]];
                let c_next = comb.step(s, b);
                let dir_next = direct_coded_step(s, b);
                assert_eq!(e_next, d_next, "equiv mismatch at s={s}, b={b}");
                assert_eq!(c_next, d_next, "comb mismatch at s={s}, b={b}");
                assert_eq!(dir_next, d_next, "direct mismatch at s={s}, b={b}");
            }
        }
    }

    #[test]
    fn compression_achieves_large_footprint_reduction() {
        let dense = build_scanner_dfa();
        let eq = build_equiv_classes(&dense);
        let comb = CompressedCombDfa::from_equiv(&eq);

        let dense_cells = dense.accept.len() * 128;
        let eq_cells = 128 + dense.accept.len() * eq.num_classes;
        assert!(eq.num_classes < 20);
        assert!(eq_cells < dense_cells / 4);
        assert!(comb.storage_cells() < eq_cells);
    }

    #[test]
    fn longest_match_retracts_from_incomplete_float_to_dot() {
        let dense = build_scanner_dfa();
        let comb = CompressedCombDfa::from_equiv(&build_equiv_classes(&dense));
        let err = scan_with_step("12.", &comb.accept, |s, b| comb.step(s, b))
            .expect_err("'.' after retracted INT has no valid transition");
        assert!(err.contains("at 2"), "got {err}");
    }

    #[test]
    fn keyword_prefixes_and_operators_match_identically() {
        let dense = build_scanner_dfa();
        let comb = CompressedCombDfa::from_equiv(&build_equiv_classes(&dense));
        let src = "let letter while while_x 12 12.5 <= < = +;";
        let tokens =
            scan_with_step(src, &comb.accept, |s, b| comb.step(s, b)).expect("scan succeeds");
        let pairs: Vec<(&str, &str)> = tokens
            .iter()
            .map(|t| (t.kind.name(), t.lexeme.as_str()))
            .collect();
        assert_eq!(
            pairs,
            vec![
                ("LET", "let"),
                ("IDENT", "letter"),
                ("WHILE", "while"),
                ("IDENT", "while_x"),
                ("INT", "12"),
                ("FLOAT", "12.5"),
                ("LE", "<="),
                ("LT", "<"),
                ("EQ", "="),
                ("PLUS", "+"),
                ("SEMI", ";"),
            ]
        );
    }
}
