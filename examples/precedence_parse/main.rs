//! Resolve shift/reduce ambiguities via precedence, associativity, and shift-beats-reduce.

use std::collections::BTreeMap;
use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Op {
    Lt,
    Add,
    Sub,
    Mul,
    Pow,
    UnaryNeg,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Assoc {
    Left,
    Right,
    Neither,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Token {
    Id(String),
    Op(char),
    LParen,
    RParen,
    If,
    Then,
    Else,
    Stmt(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Expr {
    Id(String),
    UnaryNeg(Box<Expr>),
    Binary(char, Box<Expr>, Box<Expr>),
}

impl fmt::Display for Expr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Expr::Id(name) => write!(f, "{name}"),
            Expr::UnaryNeg(inner) => write!(f, "(-{inner})"),
            Expr::Binary(op, l, r) => write!(f, "({l} {op} {r})"),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Stmt {
    Leaf(String),
    If {
        cond: String,
        then_branch: Box<Stmt>,
        else_branch: Option<Box<Stmt>>,
    },
}

impl fmt::Display for Stmt {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Stmt::Leaf(s) => write!(f, "{s}"),
            Stmt::If {
                cond,
                then_branch,
                else_branch: None,
            } => write!(f, "(if {cond} then {then_branch})"),
            Stmt::If {
                cond,
                then_branch,
                else_branch: Some(eb),
            } => write!(f, "(if {cond} then {then_branch} else {eb})"),
        }
    }
}

fn default_prec_table() -> BTreeMap<Op, (u8, Assoc)> {
    BTreeMap::from([
        (Op::Lt, (1, Assoc::Neither)),
        (Op::Add, (2, Assoc::Left)),
        (Op::Sub, (2, Assoc::Left)),
        (Op::Mul, (3, Assoc::Left)),
        (Op::UnaryNeg, (4, Assoc::Right)), // %prec override above '*'
        (Op::Pow, (5, Assoc::Right)),
    ])
}

fn char_to_binary_op(ch: char) -> Result<Op, String> {
    match ch {
        '<' => Ok(Op::Lt),
        '+' => Ok(Op::Add),
        '-' => Ok(Op::Sub),
        '*' => Ok(Op::Mul),
        '^' => Ok(Op::Pow),
        _ => Err(format!("unknown operator {ch:?}")),
    }
}

#[derive(Clone, Copy, Debug)]
enum PendingOp {
    Binary(char, Op),
    UnaryNeg,
    LParen,
}

fn should_reduce(
    top: Op,
    lookahead: Op,
    table: &BTreeMap<Op, (u8, Assoc)>,
) -> Result<bool, String> {
    let (p_top, a_top) = table[&top];
    let (p_la, _) = table[&lookahead];
    if p_top > p_la {
        Ok(true)
    } else if p_top < p_la {
        Ok(false)
    } else {
        match a_top {
            Assoc::Left => Ok(true),
            Assoc::Right => Ok(false),
            Assoc::Neither => Err(format!(
                "non-associative operator conflict: {top:?} vs {lookahead:?}"
            )),
        }
    }
}

fn reduce_top(values: &mut Vec<Expr>, ops: &mut Vec<PendingOp>) -> Result<(), String> {
    match ops.pop() {
        Some(PendingOp::UnaryNeg) => {
            let v = values.pop().ok_or("missing operand for unary '-'")?;
            values.push(Expr::UnaryNeg(Box::new(v)));
            Ok(())
        }
        Some(PendingOp::Binary(ch, _)) => {
            let r = values.pop().ok_or("missing right operand")?;
            let l = values.pop().ok_or("missing left operand")?;
            values.push(Expr::Binary(ch, Box::new(l), Box::new(r)));
            Ok(())
        }
        Some(PendingOp::LParen) => Err("unclosed '('".into()),
        None => Err("operator stack underflow".into()),
    }
}

fn parse_expr(tokens: &[Token], table: &BTreeMap<Op, (u8, Assoc)>) -> Result<Expr, String> {
    let mut values: Vec<Expr> = Vec::new();
    let mut ops: Vec<PendingOp> = Vec::new();
    let mut expect_operand = true;

    for tok in tokens {
        match tok {
            Token::Id(name) => {
                if !expect_operand {
                    return Err(format!("unexpected identifier {name:?}"));
                }
                values.push(Expr::Id(name.clone()));
                expect_operand = false;
            }
            Token::LParen => {
                if !expect_operand {
                    return Err("unexpected '(' after operand".into());
                }
                ops.push(PendingOp::LParen);
            }
            Token::RParen => {
                if expect_operand {
                    return Err("unexpected ')'".into());
                }
                while !matches!(ops.last(), Some(PendingOp::LParen) | None) {
                    reduce_top(&mut values, &mut ops)?;
                }
                if !matches!(ops.pop(), Some(PendingOp::LParen)) {
                    return Err("unmatched ')'".into());
                }
                expect_operand = false;
            }
            Token::Op('-') if expect_operand => {
                // %prec UnaryNeg
                ops.push(PendingOp::UnaryNeg);
            }
            Token::Op(ch) => {
                if expect_operand {
                    return Err(format!("unexpected binary operator {ch:?}"));
                }
                let la_op = char_to_binary_op(*ch)?;
                while let Some(&top) = ops.last() {
                    let top_op = match top {
                        PendingOp::LParen => break,
                        PendingOp::UnaryNeg => Op::UnaryNeg,
                        PendingOp::Binary(_, op) => op,
                    };
                    if should_reduce(top_op, la_op, table)? {
                        reduce_top(&mut values, &mut ops)?;
                    } else {
                        break;
                    }
                }
                ops.push(PendingOp::Binary(*ch, la_op));
                expect_operand = true;
            }
            _ => return Err("unexpected statement token in expression".into()),
        }
    }
    if expect_operand {
        return Err("unexpected end of expression".into());
    }
    while !ops.is_empty() {
        reduce_top(&mut values, &mut ops)?;
    }
    if values.len() != 1 {
        return Err("expression did not reduce to a single root".into());
    }
    Ok(values.pop().unwrap())
}

fn tokenize_expr(input: &str) -> Result<Vec<Token>, String> {
    let mut out = Vec::new();
    let bytes = input.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let ch = bytes[i] as char;
        if ch.is_ascii_whitespace() {
            i += 1;
        } else if ch.is_ascii_alphabetic() {
            let start = i;
            while i < bytes.len() && (bytes[i] as char).is_ascii_alphanumeric() {
                i += 1;
            }
            out.push(Token::Id(input[start..i].to_string()));
        } else if ch == '(' {
            out.push(Token::LParen);
            i += 1;
        } else if ch == ')' {
            out.push(Token::RParen);
            i += 1;
        } else if matches!(ch, '+' | '-' | '*' | '^' | '<') {
            out.push(Token::Op(ch));
            i += 1;
        } else {
            return Err(format!("invalid character {ch:?}"));
        }
    }
    Ok(out)
}

// Explicit shift-reduce parser for dangling-else statements:
// Shift beats reduce on lookahead `Else` when top has an unclosed `if c then S`.
fn parse_if_stmts(tokens: &[Token]) -> Result<Stmt, String> {
    #[derive(Debug)]
    enum StackItem {
        IfHeader(String),
        ElseMarker(String, Stmt),
        Reduced(Stmt),
    }

    let mut stack: Vec<StackItem> = Vec::new();
    let mut pos = 0;

    loop {
        let lookahead = tokens.get(pos);

        // Try shift/reduce resolution when top is Reduced(stmt).
        if matches!(stack.last(), Some(StackItem::Reduced(_))) {
            let len = stack.len();
            if len >= 2 {
                match &stack[len - 2] {
                    StackItem::ElseMarker(_, _) => {
                        let StackItem::Reduced(else_s) = stack.pop().unwrap() else {
                            unreachable!()
                        };
                        let StackItem::ElseMarker(cond, then_s) = stack.pop().unwrap() else {
                            unreachable!()
                        };
                        stack.push(StackItem::Reduced(Stmt::If {
                            cond,
                            then_branch: Box::new(then_s),
                            else_branch: Some(Box::new(else_s)),
                        }));
                        continue;
                    }
                    StackItem::IfHeader(_) => {
                        // Shift beats reduce on `Else`: bind `else` to the nearest `if`.
                        if matches!(lookahead, Some(Token::Else)) {
                            let StackItem::Reduced(then_s) = stack.pop().unwrap() else {
                                unreachable!()
                            };
                            let StackItem::IfHeader(cond) = stack.pop().unwrap() else {
                                unreachable!()
                            };
                            stack.push(StackItem::ElseMarker(cond, then_s));
                            pos += 1;
                            continue;
                        }
                        let StackItem::Reduced(then_s) = stack.pop().unwrap() else {
                            unreachable!()
                        };
                        let StackItem::IfHeader(cond) = stack.pop().unwrap() else {
                            unreachable!()
                        };
                        stack.push(StackItem::Reduced(Stmt::If {
                            cond,
                            then_branch: Box::new(then_s),
                            else_branch: None,
                        }));
                        continue;
                    }
                    StackItem::Reduced(_) => {
                        return Err("adjacent statements without separator".into())
                    }
                }
            }
        }

        match lookahead {
            Some(Token::If) => {
                let Some(Token::Id(cond)) = tokens.get(pos + 1) else {
                    return Err("expected condition identifier after 'if'".into());
                };
                if !matches!(tokens.get(pos + 2), Some(Token::Then)) {
                    return Err("expected 'then'".into());
                }
                stack.push(StackItem::IfHeader(cond.clone()));
                pos += 3;
            }
            Some(Token::Stmt(name)) => {
                stack.push(StackItem::Reduced(Stmt::Leaf(name.clone())));
                pos += 1;
            }
            None => break,
            Some(tok) => return Err(format!("unexpected token {tok:?}")),
        }
    }

    if stack.len() == 1 {
        if let Some(StackItem::Reduced(stmt)) = stack.pop() {
            return Ok(stmt);
        }
    }
    Err("incomplete statement".into())
}

fn main() {
    let table = default_prec_table();
    for src in ["id + id * id", "id ^ id ^ id", "id - id - id", "-id * id"] {
        let ast = parse_expr(&tokenize_expr(src).unwrap(), &table).unwrap();
        println!("{src:16} => {ast}");
    }
    let dangling = [
        Token::If,
        Token::Id("c1".into()),
        Token::Then,
        Token::If,
        Token::Id("c2".into()),
        Token::Then,
        Token::Stmt("s1".into()),
        Token::Else,
        Token::Stmt("s2".into()),
    ];
    println!("dangling-else    => {}", parse_if_stmts(&dangling).unwrap());
}

#[test]
fn resolves_precedence_associativity_and_unary_prec() {
    let table = default_prec_table();
    assert_eq!(
        parse_expr(&tokenize_expr("id + id * id").unwrap(), &table)
            .unwrap()
            .to_string(),
        "(id + (id * id))"
    );
    assert_eq!(
        parse_expr(&tokenize_expr("id ^ id ^ id").unwrap(), &table)
            .unwrap()
            .to_string(),
        "(id ^ (id ^ id))"
    );
    assert_eq!(
        parse_expr(&tokenize_expr("id - id - id").unwrap(), &table)
            .unwrap()
            .to_string(),
        "((id - id) - id)"
    );
    assert_eq!(
        parse_expr(&tokenize_expr("-id * id").unwrap(), &table)
            .unwrap()
            .to_string(),
        "((-id) * id)"
    );
    assert_eq!(
        parse_expr(&tokenize_expr("-id ^ id").unwrap(), &table)
            .unwrap()
            .to_string(),
        "(-(id ^ id))"
    );
    assert!(parse_expr(&tokenize_expr("a < b < c").unwrap(), &table).is_err());
}

#[test]
fn resolves_dangling_else_by_shifting_else() {
    let tokens = [
        Token::If,
        Token::Id("c1".into()),
        Token::Then,
        Token::If,
        Token::Id("c2".into()),
        Token::Then,
        Token::Stmt("s1".into()),
        Token::Else,
        Token::Stmt("s2".into()),
    ];
    assert_eq!(
        parse_if_stmts(&tokens).unwrap().to_string(),
        "(if c1 then (if c2 then s1 else s2))"
    );
}
