"""Chapters 3–5 Integration: `.l` + `.y` Pipeline Constructing a Typed AST.

Unlike `yacc_generator` (which evaluates integer expressions eagerly during reductions),
this lesson connects a compiled `.l` scanner directly to a compiled `.y` LALR(1)
parser whose semantic actions construct a typed, span-annotated AST for downstream stages:
  1. Compile a `.l` lexical spec into a multi-rule scanner emitting `(kind, yylval, Span)`,
  2. Compile a `.y` grammar spec (with `%left`, `%prec`, and constructor actions)
     into LALR(1) tables,
  3. Parse source tokens into an AST (`AstProgram`) preserving exact merged `Span`s,
  4. Hand the AST to Chapter 5 semantic analysis (undeclared variable check with caret
     diagnostics) and Chapter 6 three-address code (TAC) lowering + execution.
"""

from __future__ import annotations

from dataclasses import dataclass
import unittest


@dataclass(frozen=True)
class Span:
    start: int
    end: int
    line: int
    col: int

    def merge(self, other: Span) -> Span:
        if self.start <= other.start:
            return Span(self.start, max(self.end, other.end), self.line, self.col)
        return Span(other.start, max(self.end, other.end), other.line, other.col)


@dataclass(frozen=True)
class Token:
    kind: str
    lexeme: str
    span: Span


@dataclass(frozen=True)
class Diagnostic:
    span: Span
    message: str


class PipelineError(Exception):
    def __init__(self, diag: Diagnostic) -> None:
        super().__init__(diag.message)
        self.diag = diag


def format_diagnostic(src: str, diag: Diagnostic) -> str:
    lines = src.splitlines()
    line_str = lines[diag.span.line - 1] if 0 <= diag.span.line - 1 < len(lines) else ""
    pad = " " * max(0, diag.span.col - 1)
    avail = max(1, len(line_str) - max(0, diag.span.col - 1))
    width = min(avail, max(1, diag.span.end - diag.span.start))
    carets = "^" * width
    return (
        f"error at line {diag.span.line}, col {diag.span.col}: {diag.message}\n"
        f"  | {line_str}\n"
        f"  | {pad}{carets}"
    )


# ---------------------------------------------------------------------------
# 2. Mini .l Specification Compiler & Scanner
# ---------------------------------------------------------------------------


def compile_lex_spec(spec: str) -> list[tuple[str, str | None]]:
    rules: list[tuple[str, str | None]] = []
    in_rules = False
    for raw in spec.splitlines():
        line = raw.strip()
        if not line or (not in_rules and line.startswith("//")):
            continue
        if line == "%%":
            in_rules = not in_rules
            continue
        if not in_rules:
            continue
        pat, action = line.rsplit(maxsplit=1)
        rules.append((pat.strip(), None if action == "SKIP" else action))
    if not rules:
        raise ValueError("empty .l rule section")
    return rules


def _match_rule_len(pattern: str, src: str, pos: int) -> int:
    n = len(src)
    if pattern == r"[ \t\r\n]+":
        k = pos
        while k < n and src[k].isspace():
            k += 1
        return k - pos
    if pattern == r"//[^\n]*":
        if src.startswith("//", pos):
            k = pos + 2
            while k < n and src[k] != "\n":
                k += 1
            return k - pos
        return 0
    if pattern == "[a-zA-Z_][a-zA-Z0-9_]*":
        if pos < n and (src[pos].isalpha() or src[pos] == "_"):
            k = pos + 1
            while k < n and (src[k].isalnum() or src[k] == "_"):
                k += 1
            return k - pos
        return 0
    if pattern == "[0-9]+":
        k = pos
        while k < n and src[k].isdigit():
            k += 1
        return k - pos
    return len(pattern) if src.startswith(pattern, pos) else 0


def lex_source(rules: list[tuple[str, str | None]], src: str) -> list[Token]:
    n = len(src)
    i = 0
    line = 1
    line_start = 0
    out: list[Token] = []

    while i < n:
        best: tuple[int, int] | None = None
        for idx, (pat, _) in enumerate(rules):
            mlen = _match_rule_len(pat, src, i)
            if mlen > 0 and (best is None or mlen > best[0]):
                best = (mlen, idx)
        col = i - line_start + 1
        if best is None:
            raise PipelineError(
                Diagnostic(Span(i, i + 1, line, col), f"lexical error: unexpected character '{src[i]}'")
            )
        mlen, ridx = best
        end = i + mlen
        kind = rules[ridx][1]
        if kind is not None:
            out.append(Token(kind, src[i:end], Span(i, end, line, col)))
        for k in range(i, end):
            if src[k] == "\n":
                line += 1
                line_start = k + 1
        i = end

    out.append(Token("$", "$", Span(n, n, line, max(1, n - line_start + 1))))
    return out


# ---------------------------------------------------------------------------
# 3. Typed AST Nodes & .y AST Constructor Actions
# ---------------------------------------------------------------------------


@dataclass(frozen=True)
class IntExpr:
    val: int
    span: Span


@dataclass(frozen=True)
class VarExpr:
    name: str
    span: Span


@dataclass(frozen=True)
class NegExpr:
    inner: AstExpr
    span: Span


@dataclass(frozen=True)
class BinExpr:
    op: str
    left: AstExpr
    right: AstExpr
    span: Span


AstExpr = IntExpr | VarExpr | NegExpr | BinExpr


@dataclass(frozen=True)
class LetStmt:
    name: str
    expr: AstExpr
    span: Span


@dataclass(frozen=True)
class EmitStmt:
    expr: AstExpr
    span: Span


AstStmt = LetStmt | EmitStmt


@dataclass(frozen=True)
class AstProgram:
    stmts: tuple[AstStmt, ...]


def _val_span(v: object) -> Span:
    if isinstance(v, Token):
        return v.span
    if isinstance(v, (IntExpr, VarExpr, NegExpr, BinExpr, LetStmt, EmitStmt)):
        return v.span
    if isinstance(v, AstProgram) and v.stmts:
        return v.stmts[0].span.merge(v.stmts[-1].span)
    return Span(0, 0, 1, 1)


# ---------------------------------------------------------------------------
# 4. Mini .y Compiler -> LALR(1) Table & AST Builder
# ---------------------------------------------------------------------------


@dataclass(frozen=True)
class YaccProd:
    lhs: str
    rhs: tuple[str, ...]
    prec_sym: str | None
    action: str


class CompiledParser:
    def __init__(
        self,
        prods: list[YaccProd],
        action: dict[tuple[int, str], tuple[str, int]],
        goto: dict[tuple[int, str], int],
    ) -> None:
        self.prods = prods
        self.action = action
        self.goto = goto


def compile_yacc_spec(spec: str) -> CompiledParser:
    prec: dict[str, tuple[int, str]] = {}
    prods: list[YaccProd] = []
    level = 0
    in_rules = False
    start_nt: str | None = None

    for raw in spec.splitlines():
        line = raw.strip()
        if not line or line.startswith("//"):
            continue
        if line == "%%":
            in_rules = not in_rules
            continue
        if not in_rules:
            parts = line.split()
            if parts[0] in ("%left", "%right"):
                level += 1
                assoc = "left" if parts[0] == "%left" else "right"
                for tok in parts[1:]:
                    prec[tok] = (level, assoc)
            elif parts[0] == "%start":
                start_nt = parts[1]
            continue

        head_rhs, act_part = line.split("{", 1)
        action = act_part.rstrip().removesuffix("}").strip()
        lhs_raw, rhs_raw = head_rhs.split(":", 1)
        lhs = lhs_raw.strip()
        if start_nt is None:
            start_nt = lhs
        tokens = rhs_raw.split()
        rhs: list[str] = []
        prec_sym: str | None = None
        i = 0
        while i < len(tokens):
            if tokens[i] == "%prec":
                prec_sym = tokens[i + 1]
                i += 2
            else:
                rhs.append(tokens[i])
                i += 1
        prods.append(YaccProd(lhs, tuple(rhs), prec_sym, action))

    assert start_nt is not None
    full_prods = [YaccProd("S'", (start_nt,), None, "$1"), *prods]
    nts = {p.lhs for p in full_prods}
    all_syms = set(nts)
    for p in full_prods:
        all_syms.update(p.rhs)

    first: dict[str, set[str]] = {n: set() for n in nts}
    changed = True
    while changed:
        changed = False
        for p in full_prods:
            if p.rhs:
                s = p.rhs[0]
                if s not in nts:
                    if s not in first[p.lhs]:
                        first[p.lhs].add(s)
                        changed = True
                else:
                    before = len(first[p.lhs])
                    first[p.lhs].update(first[s])
                    if len(first[p.lhs]) != before:
                        changed = True

    def closure1(seed: frozenset[tuple[int, int, str]]) -> frozenset[tuple[int, int, str]]:
        cur = set(seed)
        ch = True
        while ch:
            ch = False
            for pid, dot, la in list(cur):
                if dot < len(full_prods[pid].rhs):
                    sym = full_prods[pid].rhs[dot]
                    if sym in nts:
                        if dot + 1 < len(full_prods[pid].rhs):
                            beta = full_prods[pid].rhs[dot + 1]
                            follow_la = {beta} if beta not in nts else first[beta]
                        else:
                            follow_la = {la}
                        for npid, np in enumerate(full_prods):
                            if np.lhs == sym:
                                for b in follow_la:
                                    if (npid, 0, b) not in cur:
                                        cur.add((npid, 0, b))
                                        ch = True
        return frozenset(cur)

    lr1_states: list[frozenset[tuple[int, int, str]]] = []
    lr1_id: dict[frozenset[tuple[int, int, str]], int] = {}
    lr1_trans: dict[tuple[int, str], int] = {}

    s0 = closure1(frozenset({(0, 0, "$")}))
    lr1_id[s0] = 0
    lr1_states.append(s0)

    idx = 0
    while idx < len(lr1_states):
        items = lr1_states[idx]
        for sym in sorted(all_syms):
            seed = frozenset(
                (pid, dot + 1, la)
                for pid, dot, la in items
                if dot < len(full_prods[pid].rhs) and full_prods[pid].rhs[dot] == sym
            )
            if not seed:
                continue
            target = closure1(seed)
            if target not in lr1_id:
                lr1_id[target] = len(lr1_states)
                lr1_states.append(target)
            lr1_trans[(idx, sym)] = lr1_id[target]
        idx += 1

    core_to_lalr: dict[frozenset[tuple[int, int]], int] = {}
    lalr_states: list[set[tuple[int, int, str]]] = []
    map_state = [0] * len(lr1_states)
    for sid, st in enumerate(lr1_states):
        core = frozenset((p, d) for p, d, _ in st)
        if core in core_to_lalr:
            lid = core_to_lalr[core]
            lalr_states[lid].update(st)
        else:
            lid = len(lalr_states)
            core_to_lalr[core] = lid
            lalr_states.append(set(st))
        map_state[sid] = lid

    action_tbl: dict[tuple[int, str], tuple[str, int]] = {}
    goto_tbl: dict[tuple[int, str], int] = {}
    for (old_sid, sym), tgt in lr1_trans.items():
        lid = map_state[old_sid]
        tlid = map_state[tgt]
        if sym in nts:
            goto_tbl[(lid, sym)] = tlid
        else:
            action_tbl[(lid, sym)] = ("shift", tlid)

    def prod_prec(pid: int) -> tuple[int, str] | None:
        p = full_prods[pid]
        if p.prec_sym is not None:
            return prec.get(p.prec_sym)
        for s in reversed(p.rhs):
            if s in prec:
                return prec[s]
        return None

    for lid, items in enumerate(lalr_states):
        for pid, dot, la in items:
            if dot != len(full_prods[pid].rhs):
                continue
            if pid == 0 and la == "$":
                action_tbl[(lid, "$")] = ("accept", 0)
                continue
            key = (lid, la)
            if key not in action_tbl:
                action_tbl[key] = ("reduce", pid)
            elif action_tbl[key][0] == "shift":
                pp = prod_prec(pid)
                tp = prec.get(la)
                assert pp is not None and tp is not None
                if pp[0] > tp[0] or (pp[0] == tp[0] and pp[1] == "left"):
                    action_tbl[key] = ("reduce", pid)

    return CompiledParser(full_prods, action_tbl, goto_tbl)


def _eval_yacc_action(action: str, args: list[object]) -> object:
    span = _val_span(args[0]).merge(_val_span(args[-1]))
    if action == "$1":
        return args[0]
    if action == "ProgSingle($1)":
        stmt = args[0]
        assert isinstance(stmt, (LetStmt, EmitStmt))
        return AstProgram((stmt,))
    if action == "ProgAppend($1,$2)":
        prog, stmt = args
        assert isinstance(prog, AstProgram) and isinstance(stmt, (LetStmt, EmitStmt))
        return AstProgram((*prog.stmts, stmt))
    if action == "LetStmt($2,$4)":
        tok_id, expr = args[1], args[3]
        assert isinstance(tok_id, Token) and isinstance(expr, (IntExpr, VarExpr, NegExpr, BinExpr))
        return LetStmt(tok_id.lexeme, expr, span)
    if action == "EmitStmt($2)":
        expr = args[1]
        assert isinstance(expr, (IntExpr, VarExpr, NegExpr, BinExpr))
        return EmitStmt(expr, span)
    if action == "Bin($1,$2,$3)":
        left, op_tok, right = args
        assert isinstance(op_tok, Token)
        return BinExpr(op_tok.lexeme, left, right, span)  # type: ignore[arg-type]
    if action == "Neg($2)":
        return NegExpr(args[1], span)  # type: ignore[arg-type]
    if action == "Group($2)":
        inner = args[1]
        if isinstance(inner, IntExpr):
            return IntExpr(inner.val, span)
        if isinstance(inner, VarExpr):
            return VarExpr(inner.name, span)
        if isinstance(inner, NegExpr):
            return NegExpr(inner.inner, span)
        assert isinstance(inner, BinExpr)
        return BinExpr(inner.op, inner.left, inner.right, span)
    if action == "IdExpr($1)":
        tok = args[0]
        assert isinstance(tok, Token)
        return VarExpr(tok.lexeme, tok.span)
    if action == "NumExpr($1)":
        tok = args[0]
        assert isinstance(tok, Token)
        return IntExpr(int(tok.lexeme), tok.span)
    raise ValueError(f"unknown .y action '{action}'")


def parse_to_ast(parser: CompiledParser, tokens: list[Token]) -> AstProgram:
    states = [0]
    vals: list[object] = []
    ip = 0
    while True:
        s = states[-1]
        tok = tokens[ip]
        act = parser.action.get((s, tok.kind))
        if act is None:
            raise PipelineError(Diagnostic(tok.span, f"syntax error near '{tok.lexeme}'"))
        if act[0] == "shift":
            states.append(act[1])
            vals.append(tok)
            ip += 1
        elif act[0] == "reduce":
            prod = parser.prods[act[1]]
            k = len(prod.rhs)
            rhs_vals = vals[-k:]
            del vals[-k:]
            del states[-k:]
            vals.append(_eval_yacc_action(prod.action, rhs_vals))
            states.append(parser.goto[(states[-1], prod.lhs)])
        else:
            root = vals.pop()
            assert isinstance(root, AstProgram)
            return root


# ---------------------------------------------------------------------------
# 5. Chapter 5 Semantic Check & Chapter 6 TAC Lowering
# ---------------------------------------------------------------------------


def check_semantics(prog: AstProgram) -> None:
    declared: set[str] = set()

    def check_expr(e: AstExpr) -> None:
        if isinstance(e, VarExpr) and e.name not in declared:
            raise PipelineError(Diagnostic(e.span, f"use of undeclared variable '{e.name}'"))
        if isinstance(e, NegExpr):
            check_expr(e.inner)
        if isinstance(e, BinExpr):
            check_expr(e.left)
            check_expr(e.right)
            if e.op == "/" and isinstance(e.right, IntExpr) and e.right.val == 0:
                raise PipelineError(Diagnostic(e.span, "constant division by zero"))

    for stmt in prog.stmts:
        check_expr(stmt.expr)
        if isinstance(stmt, LetStmt):
            declared.add(stmt.name)


def lower_to_tac(prog: AstProgram) -> list[tuple[str, ...]]:
    temp = 0
    code: list[tuple[str, ...]] = []

    def lower_expr(e: AstExpr) -> str:
        nonlocal temp
        if isinstance(e, IntExpr):
            dst = f"t{temp}"
            temp += 1
            code.append(("const", dst, str(e.val)))
            return dst
        if isinstance(e, VarExpr):
            return e.name
        if isinstance(e, NegExpr):
            s = lower_expr(e.inner)
            dst = f"t{temp}"
            temp += 1
            code.append(("neg", dst, s))
            return dst
        l = lower_expr(e.left)
        r = lower_expr(e.right)
        dst = f"t{temp}"
        temp += 1
        code.append((e.op, dst, l, r))
        return dst

    for stmt in prog.stmts:
        src = lower_expr(stmt.expr)
        if isinstance(stmt, LetStmt):
            code.append(("copy", stmt.name, src))
        else:
            code.append(("emit", src))
    return code


def eval_tac(code: list[tuple[str, ...]]) -> list[int]:
    env: dict[str, int] = {}
    out: list[int] = []
    for instr in code:
        op = instr[0]
        if op == "const":
            env[instr[1]] = int(instr[2])
        elif op == "copy":
            env[instr[1]] = env[instr[2]]
        elif op == "neg":
            env[instr[1]] = -env[instr[2]]
        elif op in ("+", "-", "*", "/"):
            a, b = env[instr[2]], env[instr[3]]
            if op == "+":
                env[instr[1]] = a + b
            elif op == "-":
                env[instr[1]] = a - b
            elif op == "*":
                env[instr[1]] = a * b
            else:
                if b == 0:
                    raise ZeroDivisionError("runtime division by zero")
                env[instr[1]] = int(a / b)
        elif op == "emit":
            out.append(env[instr[1]])
    return out


LEX_SPEC = r"""
%%
[ \t\r\n]+                SKIP
//[^\n]*                  SKIP
let                       LET
emit                      EMIT
[a-zA-Z_][a-zA-Z0-9_]*    ID
[0-9]+                    NUM
=                         =
+                         +
-                         -
*                         *
/                         /
(                         (
)                         )
;                         ;
%%
"""

YACC_SPEC = r"""
%left + -
%left * /
%right UMINUS
%start Program
%%
Program : Stmt               { ProgSingle($1) }
Program : Program Stmt       { ProgAppend($1,$2) }
Stmt    : LET ID = Expr ;    { LetStmt($2,$4) }
Stmt    : EMIT Expr ;        { EmitStmt($2) }
Expr    : Expr + Expr        { Bin($1,$2,$3) }
Expr    : Expr - Expr        { Bin($1,$2,$3) }
Expr    : Expr * Expr        { Bin($1,$2,$3) }
Expr    : Expr / Expr        { Bin($1,$2,$3) }
Expr    : - Expr %prec UMINUS { Neg($2) }
Expr    : ( Expr )           { Group($2) }
Expr    : ID                 { IdExpr($1) }
Expr    : NUM                { NumExpr($1) }
%%
"""


def run_pipeline(src: str) -> tuple[AstProgram, list[tuple[str, ...]], list[int]]:
    rules = compile_lex_spec(LEX_SPEC)
    parser = compile_yacc_spec(YACC_SPEC)
    tokens = lex_source(rules, src)
    ast = parse_to_ast(parser, tokens)
    check_semantics(ast)
    tac = lower_to_tac(ast)
    return ast, tac, eval_tac(tac)


def main() -> None:
    src = (
        "// Integrated .l + .y -> AST -> SemCheck -> TAC\n"
        "let price = 6;\n"
        "let total = (price + 2) * 3 - -1;\n"
        "emit total;\n"
    )
    ast, tac, out = run_pipeline(src)
    print("=== Integrated .l + .y -> AST -> Semantic Check -> TAC (Ch. 3–5) ===")
    print(f"Constructed AST statements: {len(ast.stmts)}")
    for stmt in ast.stmts:
        print(f"  {stmt}")
    print(f"\nLowered TAC ({len(tac)} instructions) -> output {out}")

    bad = "let x = 5;\nemit x + ghost;\n"
    try:
        run_pipeline(bad)
    except PipelineError as exc:
        print(f"\nSemantic diagnostic preserved across .l -> .y -> AST:\n{format_diagnostic(bad, exc.diag)}")


class LexYaccAstTests(unittest.TestCase):
    def test_lex_plus_yacc_builds_typed_ast_and_executes_tac(self) -> None:
        _, _, out = run_pipeline("let a = 2 + 3 * 4; emit -a + (10 / 2);")
        self.assertEqual(out, [-9])

    def test_longest_match_keeps_letter_as_id_not_let(self) -> None:
        _, _, out = run_pipeline("let letter = 7; emit letter + 3;")
        self.assertEqual(out, [10])

    def test_preserves_spans_from_lex_through_yacc_to_semantic_diagnostic(self) -> None:
        src = "let a = 1;\nemit a + missing;\n"
        with self.assertRaises(PipelineError) as ctx:
            run_pipeline(src)
        self.assertEqual(ctx.exception.diag.span.line, 2)
        self.assertEqual(ctx.exception.diag.span.col, 10)
        rendered = format_diagnostic(src, ctx.exception.diag)
        self.assertIn("^^^^^^^", rendered)


if __name__ == "__main__":
    main()
