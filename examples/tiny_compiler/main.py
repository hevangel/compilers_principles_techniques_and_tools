"""Translate a tiny integer language into bytecode and run it on a stack machine."""

from dataclasses import dataclass
import sys
import unittest

# Signed 64-bit two's-complement bounds so Python matches Rust's wrapping i64 VM.
I64_MASK = (1 << 64) - 1
I64_MIN = -(1 << 63)
I64_MAX = (1 << 63) - 1


def wrap_i64(value: int) -> int:
    """Wrap an arbitrary-precision Python int to signed 64-bit two's-complement."""
    masked = value & I64_MASK
    return masked - (1 << 64) if masked >= (1 << 63) else masked


@dataclass(frozen=True)
class Token:
    """Lexical unit emitted by `lex` and consumed by `Compiler`."""

    kind: str
    value: object = None


def lex(source: str) -> list[Token]:
    """Scan UTF-8/ASCII source text into tokens using longest-match rules."""
    raw = source.encode("utf-8")
    tokens: list[Token] = []
    end = 0
    while end < len(raw):
        start = end
        end += 1
        byte = raw[start]
        if chr(byte).isspace() and byte < 128:
            continue
        if (65 <= byte <= 90) or (97 <= byte <= 122):
            # Consume the entire identifier before matching keywords (`let`, `emit`).
            while end < len(raw) and (
                48 <= raw[end] <= 57 or 65 <= raw[end] <= 90 or 97 <= raw[end] <= 122
            ):
                end += 1
            word = raw[start:end].decode("ascii")
            if word == "let":
                tokens.append(Token("Let"))
            elif word == "emit":
                tokens.append(Token("Emit"))
            else:
                tokens.append(Token("Name", word))
        elif 48 <= byte <= 57:
            while end < len(raw) and 48 <= raw[end] <= 57:
                end += 1
            value = int(raw[start:end].decode("ascii"))
            # Reject literals exceeding signed 64-bit max at compile time.
            if value > I64_MAX:
                raise ValueError(f"integer literal at byte {start} is too large")
            tokens.append(Token("Number", value))
        elif byte == ord("="):
            tokens.append(Token("Equal"))
        elif byte == ord("+"):
            tokens.append(Token("Plus"))
        elif byte == ord("*"):
            tokens.append(Token("Star"))
        elif byte == ord("("):
            tokens.append(Token("Open"))
        elif byte == ord(")"):
            tokens.append(Token("Close"))
        elif byte == ord(";"):
            tokens.append(Token("Semicolon"))
        else:
            raise ValueError(f"unexpected character at byte {start}")
    return tokens


@dataclass(frozen=True)
class Inst:
    """Single bytecode instruction for the stack machine."""

    op: str
    arg: int | None = None


@dataclass
class Program:
    """Compiled bytecode stream plus the number of variable slots required."""

    slots: int
    code: list[Inst]


class Compiler:
    """Single-pass recursive-descent parser, symbol table, and stack-code emitter."""

    def __init__(self, tokens: list[Token]) -> None:
        self.tokens = tokens
        self.pos = 0
        # Compile-time symbol table mapping variable names to slot indices.
        self.slots: dict[str, int] = {}
        self.code: list[Inst] = []

    def peek(self) -> Token | None:
        return self.tokens[self.pos] if self.pos < len(self.tokens) else None

    def next_token(self) -> Token | None:
        tok = self.peek()
        if tok is not None:
            self.pos += 1
        return tok

    def eat(self, kind: str) -> bool:
        """Consume the next token only if its kind matches `kind`."""
        tok = self.peek()
        if tok is not None and tok.kind == kind:
            self.pos += 1
            return True
        return False

    def expect(self, kind: str) -> None:
        if not self.eat(kind):
            raise ValueError(f"expected {kind}")

    def factor(self, depth: int) -> None:
        """Parse a primary value (number, variable, or parenthesized expr); net stack effect +1."""
        tok = self.next_token()
        if tok is None:
            raise ValueError("expected an integer, a name, or an opening parenthesis")
        if tok.kind == "Number":
            self.code.append(Inst("Push", int(tok.value)))  # type: ignore[arg-type]
        elif tok.kind == "Name":
            name = str(tok.value)
            if name not in self.slots:
                raise ValueError(f"unknown name {name}")
            self.code.append(Inst("Load", self.slots[name]))
        elif tok.kind == "Open":
            # Guard against unbounded recursion on deeply nested parentheses.
            if depth >= 64:
                raise ValueError("parentheses nested too deeply")
            self.expression(depth + 1)
            self.expect("Close")
        else:
            raise ValueError("expected an integer, a name, or an opening parenthesis")

    def term(self, depth: int) -> None:
        """Parse multiplication chains so `*` binds tighter than `+`; net stack effect +1."""
        self.factor(depth)
        while self.eat("Star"):
            self.factor(depth)
            self.code.append(Inst("Mul"))

    def expression(self, depth: int) -> None:
        """Parse left-associative addition chains; emits Add after both operands (+1 stack)."""
        self.term(depth)
        while self.eat("Plus"):
            self.term(depth)
            self.code.append(Inst("Add"))

    def statement(self) -> None:
        """Parse one `let` or `emit` statement; leaves the VM stack empty (net 0)."""
        tok = self.next_token()
        if tok is not None and tok.kind == "Let":
            name_tok = self.next_token()
            if name_tok is None or name_tok.kind != "Name":
                raise ValueError("expected a declaration name")
            name = str(name_tok.value)
            if name in self.slots:
                raise ValueError(f"duplicate declaration of {name}")
            self.expect("Equal")
            self.expression(0)
            # Register the variable only after compiling its initializer so `let a = a;` fails.
            slot = len(self.slots)
            self.slots[name] = slot
            self.code.append(Inst("Store", slot))
        elif tok is not None and tok.kind == "Emit":
            self.expression(0)
            self.code.append(Inst("Emit"))
        else:
            raise ValueError("expected let or emit")
        self.expect("Semicolon")


def compile_source(source: str) -> Program:
    """Lex and compile a full sequence of statements into a `Program`."""
    compiler = Compiler(lex(source))
    while compiler.peek() is not None:
        compiler.statement()
    return Program(slots=len(compiler.slots), code=compiler.code)


def execute(program: Program) -> list[int]:
    """Run compiled bytecode on an operand stack and fixed slot array."""
    stack: list[int] = []
    slots = [0] * program.slots
    output: list[int] = []
    for inst in program.code:
        if inst.op == "Push":
            assert inst.arg is not None
            stack.append(inst.arg)
        elif inst.op == "Load":
            assert inst.arg is not None
            stack.append(slots[inst.arg])
        elif inst.op == "Store":
            assert inst.arg is not None
            slots[inst.arg] = stack.pop()
        elif inst.op in ("Add", "Mul"):
            # Right operand sits on top because it was evaluated/pushed second.
            right = stack.pop()
            left = stack.pop()
            value = wrap_i64(left + right if inst.op == "Add" else left * right)
            stack.append(value)
        elif inst.op == "Emit":
            output.append(stack.pop())
    return output


def main() -> int:
    source = (
        sys.argv[1]
        if len(sys.argv) > 1
        else "let boxes = 4; let perbox = 6; emit boxes * perbox + 3;"
    )
    try:
        program = compile_source(source)
    except ValueError as exc:
        print(exc, file=sys.stderr)
        return 1
    for pc, inst in enumerate(program.code):
        print(f"{pc:02d} {inst}")
    print(f"Output: {execute(program)}")
    return 0


class TinyCompilerTests(unittest.TestCase):
    def test_compiles_and_executes_precedence_and_parentheses(self) -> None:
        program = compile_source("let n = 6; emit n + 2 * 3; emit (n + 2) * 3;")
        self.assertEqual(execute(program), [12, 24])
        self.assertEqual(
            execute(compile_source("emit 9223372036854775807 + 1;")),
            [I64_MIN],
        )

    def test_rejects_invalid_source(self) -> None:
        for source in [
            "let a = a;",
            "emit 2",
            "emit 2 @ 3;",
            "emit (3;",
            "let n = 1; let n = 2;",
            "emit 9223372036854775808;",
        ]:
            with self.assertRaises(ValueError, msg=source):
                compile_source(source)
        deep = f"emit {'(' * 65}1{')' * 65};"
        with self.assertRaises(ValueError):
            compile_source(deep)


if __name__ == "__main__":
    raise SystemExit(main())
