//! Chapter 4 (§4.1–4.3): Context-Free Grammars, Derivations, Ambiguity & Limits.
//!
//! Demonstrates:
//! 1. Formal CFG definition `(V_N, V_T, P, S)` and sentential-form rewriting,
//! 2. Step-by-step Leftmost (`=>_lm`) vs. Rightmost (`=>_rm`) derivations producing the
//!    exact same parse tree on an unambiguous grammar (`E -> E + T | T`, `T -> T * F | F`, `F -> id`),
//! 3. Proving ambiguity directly by enumerating ALL parse trees for a token sequence via
//!    general CFG chart recognition (`id + id * id` under `E -> E + E | E * E | id` yields 2 trees;
//!    dangling-else `if c then if c then s else s` yields 2 trees),
//! 4. Demonstrating the limits of Context-Free Grammars:
//!    - Balanced nesting `{ a^n b^n }` IS context-free (recognized by a CFG / PDA),
//!    - Cross-serial counting `{ a^n b^n c^n }` and declaration matching `{ decl x; ... use x }`
//!      are NOT context-free (rejected by pumping/stack limits, motivating Chapter 5 semantic checks).

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Sym {
    Nt(String),
    T(String),
}

impl fmt::Display for Sym {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Sym::Nt(s) | Sym::T(s) => f.write_str(s),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Prod {
    lhs: String,
    rhs: Vec<Sym>,
}

#[derive(Clone, Debug)]
struct Cfg {
    start: String,
    prods: Vec<Prod>,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum ParseTree {
    Node {
        label: String,
        children: Vec<ParseTree>,
    },
    Leaf(String),
}

impl ParseTree {
    fn format_lines(&self, indent: usize, out: &mut Vec<String>) {
        let pad = "  ".repeat(indent);
        match self {
            ParseTree::Leaf(tok) => out.push(format!("{pad}{tok}")),
            ParseTree::Node { label, children } => {
                out.push(format!("{pad}{label}"));
                if children.is_empty() {
                    out.push(format!("{pad}  ε"));
                } else {
                    for c in children {
                        c.format_lines(indent + 1, out);
                    }
                }
            }
        }
    }

    fn yield_tokens(&self, out: &mut Vec<String>) {
        match self {
            ParseTree::Leaf(t) => out.push(t.clone()),
            ParseTree::Node { children, .. } => {
                for c in children {
                    c.yield_tokens(out);
                }
            }
        }
    }
}

/// Generates leftmost (`leftmost = true`) or rightmost (`leftmost = false`) derivation steps
/// from a concrete parse tree by repeatedly expanding either the leftmost or rightmost unexpanded
/// nonterminal in the current sentential form.
fn derivation_from_tree(tree: &ParseTree, leftmost: bool) -> Vec<String> {
    enum Item<'a> {
        ExpandedLeaf(&'a str),
        Pending(&'a ParseTree),
    }

    let mut frontier: Vec<Item<'_>> = vec![Item::Pending(tree)];
    let format_frontier = |items: &[Item<'_>]| -> String {
        if items.is_empty() {
            return "ε".to_string();
        }
        items
            .iter()
            .map(|it| match it {
                Item::ExpandedLeaf(s) => (*s).to_string(),
                Item::Pending(ParseTree::Leaf(s)) => s.clone(),
                Item::Pending(ParseTree::Node { label, .. }) => label.clone(),
            })
            .collect::<Vec<_>>()
            .join(" ")
    };

    let mut steps = vec![format_frontier(&frontier)];
    loop {
        let target_idx = if leftmost {
            frontier
                .iter()
                .position(|it| matches!(it, Item::Pending(ParseTree::Node { .. })))
        } else {
            frontier
                .iter()
                .rposition(|it| matches!(it, Item::Pending(ParseTree::Node { .. })))
        };
        let Some(idx) = target_idx else {
            break;
        };
        let Item::Pending(ParseTree::Node { children, .. }) = frontier.remove(idx) else {
            unreachable!();
        };
        if children.is_empty() {
            // ε-production: removes the nonterminal from sentential form
        } else {
            let replacements: Vec<Item<'_>> = children
                .iter()
                .map(|c| match c {
                    ParseTree::Leaf(s) => Item::ExpandedLeaf(s.as_str()),
                    node @ ParseTree::Node { .. } => Item::Pending(node),
                })
                .collect();
            for (offset, item) in replacements.into_iter().enumerate() {
                frontier.insert(idx + offset, item);
            }
        }
        steps.push(format_frontier(&frontier));
    }
    steps
}

/// Enumerates ALL parse trees for `tokens` under any epsilon-free CFG (supports ambiguity & left recursion)
/// using memoized top-down / CYK-style span splitting with cycle prevention over unary rules.
fn enumerate_parse_trees(cfg: &Cfg, tokens: &[&str]) -> Vec<ParseTree> {
    type MemoKey = (String, usize, usize);
    let mut memo: BTreeMap<MemoKey, Vec<ParseTree>> = BTreeMap::new();
    let mut visiting: BTreeSet<MemoKey> = BTreeSet::new();

    fn solve_nt(
        cfg: &Cfg,
        nt: &str,
        l: usize,
        r: usize,
        tokens: &[&str],
        memo: &mut BTreeMap<MemoKey, Vec<ParseTree>>,
        visiting: &mut BTreeSet<MemoKey>,
    ) -> Vec<ParseTree> {
        let key = (nt.to_string(), l, r);
        if let Some(cached) = memo.get(&key) {
            return cached.clone();
        }
        if !visiting.insert(key.clone()) {
            return Vec::new();
        }

        let mut trees = BTreeSet::new();
        for prod in &cfg.prods {
            if prod.lhs != nt {
                continue;
            }
            for children in solve_seq(cfg, &prod.rhs, 0, l, r, tokens, memo, visiting) {
                trees.insert(ParseTree::Node {
                    label: nt.to_string(),
                    children,
                });
            }
        }
        visiting.remove(&key);
        let out: Vec<ParseTree> = trees.into_iter().collect();
        memo.insert(key, out.clone());
        out
    }

    #[allow(clippy::too_many_arguments)]
    fn solve_seq(
        cfg: &Cfg,
        rhs: &[Sym],
        idx: usize,
        l: usize,
        r: usize,
        tokens: &[&str],
        memo: &mut BTreeMap<MemoKey, Vec<ParseTree>>,
        visiting: &mut BTreeSet<MemoKey>,
    ) -> Vec<Vec<ParseTree>> {
        if idx == rhs.len() {
            return if l == r { vec![Vec::new()] } else { Vec::new() };
        }
        // Remaining epsilon-free symbols need at least (rhs.len() - idx) tokens unless ε-productions exist.
        // Here we handle exact spans (and allow 0-width ε-rules cleanly).
        match &rhs[idx] {
            Sym::T(term) => {
                if l < r && tokens[l] == term {
                    let tails = solve_seq(cfg, rhs, idx + 1, l + 1, r, tokens, memo, visiting);
                    tails
                        .into_iter()
                        .map(|mut t| {
                            let mut full = vec![ParseTree::Leaf((*term).clone())];
                            full.append(&mut t);
                            full
                        })
                        .collect()
                } else {
                    Vec::new()
                }
            }
            Sym::Nt(sym_nt) => {
                let mut res = Vec::new();
                for mid in l..=r {
                    let left_trees = solve_nt(cfg, sym_nt, l, mid, tokens, memo, visiting);
                    if left_trees.is_empty() {
                        continue;
                    }
                    let right_seqs = solve_seq(cfg, rhs, idx + 1, mid, r, tokens, memo, visiting);
                    for lt in &left_trees {
                        for rs in &right_seqs {
                            let mut combined = vec![lt.clone()];
                            combined.extend(rs.iter().cloned());
                            res.push(combined);
                        }
                    }
                }
                res
            }
        }
    }

    solve_nt(
        cfg,
        &cfg.start,
        0,
        tokens.len(),
        tokens,
        &mut memo,
        &mut visiting,
    )
}

fn nt(s: &str) -> Sym {
    Sym::Nt(s.to_string())
}

fn t(s: &str) -> Sym {
    Sym::T(s.to_string())
}

fn ambiguous_expr_cfg() -> Cfg {
    Cfg {
        start: "E".into(),
        prods: vec![
            Prod {
                lhs: "E".into(),
                rhs: vec![nt("E"), t("+"), nt("E")],
            },
            Prod {
                lhs: "E".into(),
                rhs: vec![nt("E"), t("*"), nt("E")],
            },
            Prod {
                lhs: "E".into(),
                rhs: vec![t("id")],
            },
        ],
    }
}

fn unambiguous_expr_cfg() -> Cfg {
    Cfg {
        start: "E".into(),
        prods: vec![
            Prod {
                lhs: "E".into(),
                rhs: vec![nt("E"), t("+"), nt("T")],
            },
            Prod {
                lhs: "E".into(),
                rhs: vec![nt("T")],
            },
            Prod {
                lhs: "T".into(),
                rhs: vec![nt("T"), t("*"), nt("F")],
            },
            Prod {
                lhs: "T".into(),
                rhs: vec![nt("F")],
            },
            Prod {
                lhs: "F".into(),
                rhs: vec![t("id")],
            },
        ],
    }
}

fn dangling_else_cfg() -> Cfg {
    Cfg {
        start: "S".into(),
        prods: vec![
            Prod {
                lhs: "S".into(),
                rhs: vec![t("if"), t("c"), t("then"), nt("S")],
            },
            Prod {
                lhs: "S".into(),
                rhs: vec![t("if"), t("c"), t("then"), nt("S"), t("else"), nt("S")],
            },
            Prod {
                lhs: "S".into(),
                rhs: vec![t("other")],
            },
        ],
    }
}

fn an_bn_cfg() -> Cfg {
    Cfg {
        start: "S".into(),
        prods: vec![
            Prod {
                lhs: "S".into(),
                rhs: vec![t("a"), nt("S"), t("b")],
            },
            Prod {
                lhs: "S".into(),
                rhs: vec![],
            },
        ],
    }
}

/// Checks non-context-free `{ a^n b^n c^n | n >= 1 }` directly via cross-serial counts.
fn is_an_bn_cn(s: &str) -> bool {
    let bytes = s.as_bytes();
    let n = bytes.len();
    let mut i = 0;
    while i < n && bytes[i] == b'a' {
        i += 1;
    }
    let na = i;
    let mut nb = 0;
    while i < n && bytes[i] == b'b' {
        i += 1;
        nb += 1;
    }
    let mut nc = 0;
    while i < n && bytes[i] == b'c' {
        i += 1;
        nc += 1;
    }
    i == n && na >= 1 && na == nb && nb == nc
}

fn main() {
    println!("=== CFGs, Leftmost/Rightmost Derivations, Ambiguity & Limits (Ch. 4) ===");

    let tokens = ["id", "+", "id", "*", "id"];
    let amb_trees = enumerate_parse_trees(&ambiguous_expr_cfg(), &tokens);
    println!(
        "\n1. Ambiguous grammar E -> E + E | E * E | id on {:?}: found {} parse trees!",
        tokens.join(" "),
        amb_trees.len()
    );
    for (idx, tree) in amb_trees.iter().enumerate() {
        let mut yielded = Vec::new();
        tree.yield_tokens(&mut yielded);
        assert_eq!(yielded, tokens);
        let mut lines = Vec::new();
        tree.format_lines(1, &mut lines);
        println!(
            "  Tree #{} (yield: {}):\n{}",
            idx + 1,
            yielded.join(" "),
            lines.join("\n")
        );
    }

    let unamb_trees = enumerate_parse_trees(&unambiguous_expr_cfg(), &tokens);
    println!(
        "\n2. Stratified unambiguous grammar on {:?}: found {} unique parse tree",
        tokens.join(" "),
        unamb_trees.len()
    );
    let tree = &unamb_trees[0];
    let lm = derivation_from_tree(tree, true);
    let rm = derivation_from_tree(tree, false);
    println!("  Leftmost derivation (=>_lm):");
    for step in &lm {
        println!("    =>_lm {step}");
    }
    println!("  Rightmost derivation (=>_rm):");
    for step in &rm {
        println!("    =>_rm {step}");
    }

    let else_tokens = [
        "if", "c", "then", "if", "c", "then", "other", "else", "other",
    ];
    let else_trees = enumerate_parse_trees(&dangling_else_cfg(), &else_tokens);
    println!(
        "\n3. Dangling-else ambiguity on {:?}: {} parse trees (else binds outer vs. inner if)",
        else_tokens.join(" "),
        else_trees.len()
    );

    let ab_trees = enumerate_parse_trees(&an_bn_cfg(), &["a", "a", "b", "b"]);
    println!(
        "\n4. CFG vs. non-CFG boundary: {{a^n b^n}} has {} CFG parse tree on 'aabb'; {{a^n b^n c^n}} ('aabbcc' valid? {}) exceeds one pushdown stack -> handled semantically.",
        ab_trees.len(),
        is_an_bn_cn("aabbcc")
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ambiguous_grammars_produce_multiple_distinct_parse_trees() {
        let expr_trees =
            enumerate_parse_trees(&ambiguous_expr_cfg(), &["id", "+", "id", "*", "id"]);
        assert_eq!(expr_trees.len(), 2);

        let else_trees = enumerate_parse_trees(
            &dangling_else_cfg(),
            &[
                "if", "c", "then", "if", "c", "then", "other", "else", "other",
            ],
        );
        assert_eq!(else_trees.len(), 2);
    }

    #[test]
    fn unambiguous_grammar_produces_single_tree_with_matching_lm_and_rm_yields() {
        let toks = ["id", "+", "id", "*", "id"];
        let trees = enumerate_parse_trees(&unambiguous_expr_cfg(), &toks);
        assert_eq!(trees.len(), 1);
        let mut yielded = Vec::new();
        trees[0].yield_tokens(&mut yielded);
        assert_eq!(yielded, toks);

        let lm = derivation_from_tree(&trees[0], true);
        let rm = derivation_from_tree(&trees[0], false);
        assert_eq!(lm.first().unwrap(), "E");
        assert_eq!(rm.first().unwrap(), "E");
        assert_eq!(lm.last().unwrap(), "id + id * id");
        assert_eq!(rm.last().unwrap(), "id + id * id");
        assert_ne!(lm, rm);
    }

    #[test]
    fn an_bn_is_cfg_while_an_bn_cn_requires_cross_serial_checking() {
        assert_eq!(
            enumerate_parse_trees(&an_bn_cfg(), &["a", "a", "a", "b", "b", "b"]).len(),
            1
        );
        assert!(enumerate_parse_trees(&an_bn_cfg(), &["a", "a", "b"]).is_empty());
        assert!(is_an_bn_cn("aaabbbccc"));
        assert!(!is_an_bn_cn("aaabbbcc"));
    }
}
