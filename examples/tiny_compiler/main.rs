//! Translate a tiny integer language into bytecode and run it on a stack machine.

use std::collections::BTreeMap;
use std::process::ExitCode;

/// Lexical unit produced by `lex` and consumed by `Compiler`.
/// `Name` borrows directly from the source string to avoid extra allocations.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Token<'a> {
    Let,
    Emit,
    Name(&'a str),
    Number(i64),
    Equal,
    Plus,
    Star,
    Open,
    Close,
    Semicolon,
}

// The scanner groups source bytes into the tokens the parser will consume.
fn lex(source: &str) -> Result<Vec<Token<'_>>, String> {
    let bytes = source.as_bytes();
    let mut tokens = Vec::new();
    let mut end = 0;
    while end < bytes.len() {
        let start = end;
        end += 1;
        let token = match bytes[start] {
            byte if byte.is_ascii_whitespace() => continue,
            byte if byte.is_ascii_alphabetic() => {
                // Longest-match: consume all trailing alphanumeric bytes before
                // checking whether the whole lexeme is a reserved keyword.
                while bytes.get(end).is_some_and(u8::is_ascii_alphanumeric) {
                    end += 1;
                }
                // Recognize a keyword only after scanning the whole word so `letter` is Name, not Let.
                match &source[start..end] {
                    "let" => Token::Let,
                    "emit" => Token::Emit,
                    name => Token::Name(name),
                }
            }
            byte if byte.is_ascii_digit() => {
                while bytes.get(end).is_some_and(u8::is_ascii_digit) {
                    end += 1;
                }
                // Validate signed 64-bit range during lexing rather than failing silently.
                let value = source[start..end]
                    .parse()
                    .map_err(|_| format!("integer literal at byte {start} is too large"))?;
                Token::Number(value)
            }
            b'=' => Token::Equal,
            b'+' => Token::Plus,
            b'*' => Token::Star,
            b'(' => Token::Open,
            b')' => Token::Close,
            b';' => Token::Semicolon,
            _ => return Err(format!("unexpected character at byte {start}")),
        };
        tokens.push(token);
    }
    Ok(tokens)
}

/// Target instruction set for the stack virtual machine.
/// Operands are passed via an implicit LIFO value stack; variables live in indexed slots.
#[derive(Debug)]
enum Inst {
    Push(i64),
    Load(usize),
    Store(usize),
    Add,
    Mul,
    Emit,
}

/// Compiled bytecode plus the number of variable slots the VM must allocate.
#[derive(Debug)]
struct Program {
    slots: usize,
    code: Vec<Inst>,
}

// Name-to-slot bindings are compile-time data. Bytecode carries slot numbers.
struct Compiler<'a> {
    tokens: std::iter::Peekable<std::vec::IntoIter<Token<'a>>>,
    /// Maps each declared variable name to its unique runtime slot index (0, 1, ...).
    slots: BTreeMap<&'a str, usize>,
    code: Vec<Inst>,
}

impl Compiler<'_> {
    /// Consume `token` only if it matches the next lookahead token.
    fn eat(&mut self, token: Token<'_>) -> bool {
        if self.tokens.peek() == Some(&token) {
            self.tokens.next();
            true
        } else {
            false
        }
    }

    /// Require `token` next or abort compilation with a syntax error.
    fn expect(&mut self, token: Token<'_>) -> Result<(), String> {
        if self.eat(token) {
            Ok(())
        } else {
            Err(format!("expected {token:?}"))
        }
    }

    // A factor emits code that leaves exactly one value on the operand stack.
    // Grammar: factor -> Number | Name | '(' expression ')'
    fn factor(&mut self, depth: usize) -> Result<(), String> {
        match self.tokens.next() {
            Some(Token::Number(value)) => self.code.push(Inst::Push(value)),
            Some(Token::Name(name)) => {
                let slot = self
                    .slots
                    .get(name)
                    .ok_or_else(|| format!("unknown name {name}"))?;
                self.code.push(Inst::Load(*slot));
            }
            // Enforce a bounded recursion depth so deeply nested parentheses cannot overflow the call stack.
            Some(Token::Open) if depth < 64 => {
                self.expression(depth + 1)?;
                self.expect(Token::Close)?;
            }
            Some(Token::Open) => return Err("parentheses nested too deeply".into()),
            _ => return Err("expected an integer, a name, or an opening parenthesis".into()),
        }
        Ok(())
    }

    // Consume an entire chain of products before returning to the addition rule.
    // Grammar: term -> factor ( '*' factor )*
    // Invariant: net stack effect is +1.
    fn term(&mut self, depth: usize) -> Result<(), String> {
        self.factor(depth)?;
        while self.eat(Token::Star) {
            self.factor(depth)?;
            self.code.push(Inst::Mul);
        }
        Ok(())
    }

    // Emit each operation after both operands so the VM can pop them from its stack.
    // Grammar: expression -> term ( '+' term )*
    // Using a loop instead of left recursion preserves left-associativity without looping forever.
    fn expression(&mut self, depth: usize) -> Result<(), String> {
        self.term(depth)?;
        while self.eat(Token::Plus) {
            self.term(depth)?;
            self.code.push(Inst::Add);
        }
        Ok(())
    }

    // Grammar: statement -> 'let' Name '=' expression ';' | 'emit' expression ';'
    // Invariant: net stack effect is 0 across every complete statement.
    fn statement(&mut self) -> Result<(), String> {
        match self.tokens.next() {
            Some(Token::Let) => {
                let Some(Token::Name(name)) = self.tokens.next() else {
                    return Err("expected a declaration name".into());
                };
                if self.slots.contains_key(name) {
                    return Err(format!("duplicate declaration of {name}"));
                }
                self.expect(Token::Equal)?;
                self.expression(0)?;
                // Introduce the name after compiling its initializer, so `let a = a;` is rejected.
                let slot = self.slots.len();
                self.slots.insert(name, slot);
                self.code.push(Inst::Store(slot));
            }
            Some(Token::Emit) => {
                self.expression(0)?;
                self.code.push(Inst::Emit);
            }
            _ => return Err("expected let or emit".into()),
        }
        self.expect(Token::Semicolon)
    }
}

/// Single-pass compiler: lexes source, parses every statement until EOF, and returns bytecode.
fn compile(source: &str) -> Result<Program, String> {
    let mut compiler = Compiler {
        tokens: lex(source)?.into_iter().peekable(),
        slots: BTreeMap::new(),
        code: Vec::new(),
    };
    while compiler.tokens.peek().is_some() {
        compiler.statement()?;
    }
    Ok(Program {
        slots: compiler.slots.len(),
        code: compiler.code,
    })
}

// The operand stack holds temporary results. Slots hold the values of declared names.
fn execute(program: &Program) -> Vec<i64> {
    let mut stack = Vec::new();
    let mut slots = vec![0; program.slots];
    let mut output = Vec::new();
    for inst in &program.code {
        match *inst {
            Inst::Push(value) => stack.push(value),
            Inst::Load(slot) => stack.push(slots[slot]),
            Inst::Store(slot) => slots[slot] = stack.pop().unwrap(),
            Inst::Add | Inst::Mul => {
                // The right operand was pushed last, so it is popped first.
                // Wrapping arithmetic matches two's-complement 64-bit hardware semantics.
                let right = stack.pop().unwrap();
                let left = stack.pop().unwrap();
                let value = match inst {
                    Inst::Add => left.wrapping_add(right),
                    Inst::Mul => left.wrapping_mul(right),
                    _ => unreachable!(),
                };
                stack.push(value);
            }
            Inst::Emit => output.push(stack.pop().unwrap()),
        }
    }
    output
}

fn main() -> ExitCode {
    let source = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "let boxes = 4; let perbox = 6; emit boxes * perbox + 3;".into());
    match compile(&source) {
        Ok(program) => {
            for (pc, inst) in program.code.iter().enumerate() {
                println!("{pc:02} {inst:?}");
            }
            println!("Output: {:?}", execute(&program));
            ExitCode::SUCCESS
        }
        Err(message) => {
            eprintln!("{message}");
            ExitCode::FAILURE
        }
    }
}

#[test]
fn compiles_and_executes_precedence_and_parentheses() {
    let program = compile("let n = 6; emit n + 2 * 3; emit (n + 2) * 3;").unwrap();
    assert_eq!(execute(&program), [12, 24]);
    assert_eq!(
        execute(&compile("emit 9223372036854775807 + 1;").unwrap()),
        [i64::MIN]
    );
}

#[test]
fn rejects_invalid_source() {
    for source in [
        "let a = a;",
        "emit 2",
        "emit 2 @ 3;",
        "emit (3;",
        "let n = 1; let n = 2;",
        "emit 9223372036854775808;",
    ] {
        assert!(compile(source).is_err(), "{source}");
    }
    let deep = format!("emit {}1{};", "(".repeat(65), ")".repeat(65));
    assert!(compile(&deep).is_err());
}
