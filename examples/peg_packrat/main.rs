//! Modern Extension: Parsing Expression Grammars (PEGs) & Packrat Memoization.
//!
//! Explicitly labeled as a modern parsing technique (Bryan Ford 2004; CPython's `pegen`
//! foundation) complementing Dragon Book Chapter 4:
//! 1. Ordered choice (`/`), greedy repetition (`*`/`+`/`?`), and zero-width syntactic
//!    predicates (`&e` positive lookahead, `!e` negative lookahead),
//! 2. Contrasting deterministic ordered choice `/` with CFG unordered `|` (why `"ab"` fails
//!    under `'a' / 'ab'` when left-prefix comes first),
//! 3. Proving exponential `O(2^d)` naive backtracking blowup vs. linear `O(n)` Packrat
//!    memoization `(rule, pos) -> MemoResult` on deeply nested expressions,
//! 4. Recognizing the non-context-free language `{ a^n b^n c^n | n >= 1 }` cleanly using
//!    zero-width lookahead predicates `&(A "c") "a"+ B !.`.

use std::collections::BTreeMap;

#[derive(Clone, Debug)]
enum Peg {
    Lit(&'static str),
    Rule(&'static str),
    Seq(Vec<Peg>),
    Choice(Vec<Peg>), // Ordered choice `/`
    Opt(Box<Peg>),
    Plus(Box<Peg>),
    And(Box<Peg>),
    Not(Box<Peg>),
    AnyChar,
}

#[derive(Clone, Debug)]
struct PegGrammar {
    start: &'static str,
    rules: BTreeMap<&'static str, Peg>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MemoState {
    Active,
    Done(Option<usize>),
}

struct PegRunner<'a> {
    grammar: &'a PegGrammar,
    src: &'a [u8],
    memoize: bool,
    memo: BTreeMap<(&'static str, usize), MemoState>,
    rule_calls: usize,
    cache_hits: usize,
}

impl<'a> PegRunner<'a> {
    fn new(grammar: &'a PegGrammar, src: &'a str, memoize: bool) -> Self {
        Self {
            grammar,
            src: src.as_bytes(),
            memoize,
            memo: BTreeMap::new(),
            rule_calls: 0,
            cache_hits: 0,
        }
    }

    fn parse(&mut self) -> Option<usize> {
        self.eval_rule(self.grammar.start, 0)
    }

    fn eval_rule(&mut self, name: &'static str, pos: usize) -> Option<usize> {
        if self.memoize {
            if let Some(&state) = self.memo.get(&(name, pos)) {
                self.cache_hits += 1;
                return match state {
                    MemoState::Active => None, // guard against direct recursion
                    MemoState::Done(res) => res,
                };
            }
            self.memo.insert((name, pos), MemoState::Active);
        }
        self.rule_calls += 1;
        let expr = &self.grammar.rules[name];
        let res = self.eval_expr(expr, pos);
        if self.memoize {
            self.memo.insert((name, pos), MemoState::Done(res));
        }
        res
    }

    fn eval_expr(&mut self, expr: &Peg, pos: usize) -> Option<usize> {
        match expr {
            Peg::Lit(s) => {
                let b = s.as_bytes();
                if self.src.get(pos..pos + b.len()) == Some(b) {
                    Some(pos + b.len())
                } else {
                    None
                }
            }
            Peg::AnyChar => {
                if pos < self.src.len() {
                    Some(pos + 1)
                } else {
                    None
                }
            }
            Peg::Rule(name) => self.eval_rule(name, pos),
            Peg::Seq(items) => {
                let mut cur = pos;
                for it in items {
                    cur = self.eval_expr(it, cur)?;
                }
                Some(cur)
            }
            Peg::Choice(alts) => {
                // Ordered choice `/`: first match wins and commits (no cross-branch backtracking).
                for alt in alts {
                    if let Some(end) = self.eval_expr(alt, pos) {
                        return Some(end);
                    }
                }
                None
            }
            Peg::Opt(inner) => Some(self.eval_expr(inner, pos).unwrap_or(pos)),
            Peg::Plus(inner) => {
                let mut cur = self.eval_expr(inner, pos)?;
                while let Some(next) = self.eval_expr(inner, cur) {
                    if next == cur {
                        break;
                    }
                    cur = next;
                }
                Some(cur)
            }
            Peg::And(inner) => self.eval_expr(inner, pos).map(|_| pos),
            Peg::Not(inner) => {
                if self.eval_expr(inner, pos).is_some() {
                    None
                } else {
                    Some(pos)
                }
            }
        }
    }
}

/// PEG grammar where ordered fallback `(Prod '+' Expr) / Prod` forces exponential
/// backtracking without Packrat memoization on deeply nested parentheses `((((id))))`.
fn expr_peg() -> PegGrammar {
    use Peg::{Choice, Lit, Not, Rule, Seq};
    let mut rules = BTreeMap::new();
    rules.insert("Top", Seq(vec![Rule("Expr"), Not(Box::new(Peg::AnyChar))]));
    rules.insert(
        "Expr",
        Choice(vec![
            Seq(vec![Rule("Prod"), Lit("+"), Rule("Expr")]),
            Rule("Prod"),
        ]),
    );
    rules.insert(
        "Prod",
        Choice(vec![
            Seq(vec![Rule("Atom"), Lit("*"), Rule("Prod")]),
            Rule("Atom"),
        ]),
    );
    rules.insert(
        "Atom",
        Choice(vec![Seq(vec![Lit("("), Rule("Expr"), Lit(")")]), Lit("id")]),
    );
    PegGrammar {
        start: "Top",
        rules,
    }
}

/// Ford's classic non-CFG PEG for `{ a^n b^n c^n | n >= 1 }` using syntactic predicates.
fn an_bn_cn_peg() -> PegGrammar {
    use Peg::{And, Lit, Not, Opt, Plus, Rule, Seq};
    let mut rules = BTreeMap::new();
    rules.insert(
        "S",
        Seq(vec![
            And(Box::new(Seq(vec![Rule("A"), Lit("c")]))),
            Plus(Box::new(Lit("a"))),
            Rule("B"),
            Not(Box::new(Peg::AnyChar)),
        ]),
    );
    rules.insert("A", Seq(vec![Lit("a"), Opt(Box::new(Rule("A"))), Lit("b")]));
    rules.insert("B", Seq(vec![Lit("b"), Opt(Box::new(Rule("B"))), Lit("c")]));
    PegGrammar { start: "S", rules }
}

fn ordered_choice_trap_peg() -> PegGrammar {
    use Peg::{Choice, Lit, Not, Seq};
    let mut rules = BTreeMap::new();
    rules.insert(
        "Trap",
        Seq(vec![
            Choice(vec![Lit("a"), Lit("ab")]),
            Not(Box::new(Peg::AnyChar)),
        ]),
    );
    PegGrammar {
        start: "Trap",
        rules,
    }
}

fn run_peg(grammar: &PegGrammar, src: &str, memoize: bool) -> (bool, usize, usize) {
    let mut runner = PegRunner::new(grammar, src, memoize);
    let ok = runner.parse() == Some(src.len());
    (ok, runner.rule_calls, runner.cache_hits)
}

fn main() {
    println!("=== Advanced Parsing (Modern): PEG and Packrat Memoization ===");

    let g_expr = expr_peg();
    let nested = format!("{}id{}", "(".repeat(10), ")".repeat(10));
    let (ok_naive, calls_naive, _) = run_peg(&g_expr, &nested, false);
    let (ok_pack, calls_pack, hits_pack) = run_peg(&g_expr, &nested, true);
    assert!(ok_naive && ok_pack);

    println!("\n1. Pathological backtracking on depth-10 nested `{nested}`:");
    println!("   Unmemoized recursive PEG calls: {calls_naive:>8} (exponential O(2^d))");
    println!(
        "   Packrat memoized PEG calls:     {calls_pack:>8} (linear, cache hits = {hits_pack})"
    );

    let g_abc = an_bn_cn_peg();
    println!("\n2. Syntactic predicates `&e` / `!e` recognizing non-CFG {{a^n b^n c^n}}:");
    for sample in ["abc", "aaabbbccc", "aaabbccc", "aabbbcc", "aabbcc!"] {
        let (ok, calls, _) = run_peg(&g_abc, sample, true);
        println!("   {sample:<12} -> matched={ok:<5} (Packrat rule evals={calls})");
    }

    let g_trap = ordered_choice_trap_peg();
    let (trap_ab, _, _) = run_peg(&g_trap, "ab", true);
    println!("\n3. Ordered choice `/` vs CFG `|`: rule `'a' / 'ab' !.` on \"ab\" -> {trap_ab} (first alt commits!)");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packrat_prevents_exponential_backtracking_on_nested_parens() {
        let g = expr_peg();
        let nested = format!("{}id+id*id{}", "(".repeat(8), ")".repeat(8));
        let (ok_naive, calls_naive, _) = run_peg(&g, &nested, false);
        let (ok_pack, calls_pack, hits) = run_peg(&g, &nested, true);
        assert!(ok_naive && ok_pack);
        assert!(
            calls_naive > calls_pack * 10,
            "naive={calls_naive}, pack={calls_pack}"
        );
        assert!(hits > 0);
    }

    #[test]
    fn syntactic_predicates_recognize_exact_an_bn_cn() {
        let g = an_bn_cn_peg();
        for n in 1..=6 {
            let valid = format!("{}{}{}", "a".repeat(n), "b".repeat(n), "c".repeat(n));
            assert!(run_peg(&g, &valid, true).0, "failed valid n={n}");
        }
        for bad in ["", "ab", "aabb", "aaabbcc", "aabbbcc", "aabbccc", "abcabc"] {
            assert!(!run_peg(&g, bad, true).0, "unexpectedly matched '{bad}'");
        }
    }

    #[test]
    fn ordered_choice_commits_to_first_matching_branch() {
        let g = ordered_choice_trap_peg();
        assert!(run_peg(&g, "a", true).0);
        // Because `'a'` matches prefix of `"ab"`, ordered choice `/` never tries `'ab'`.
        assert!(!run_peg(&g, "ab", true).0);
    }
}
