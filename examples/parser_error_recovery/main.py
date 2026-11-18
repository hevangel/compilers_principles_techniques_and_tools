"""Chapter 4 (§4.4.5, §4.7–4.8): LL(1) Synchronization Sets and LR Error Recovery.

Demonstrates:
1. Computing `FIRST` and `FOLLOW` sets to populate an LL(1) table with explicit
   `Synch` entries alongside production entries,
2. LL(1) panic + phrase-level recovery:
   - `Synch` pops the nonterminal on stack,
   - Empty cell skips an unexpected token,
   - Missing expected delimiter (e.g., `;` before `let`/`emit`/`$`) performs virtual
     single-token insertion,
   - Error cooldown prevents cascading secondary messages until real tokens advance,
3. SLR(1) table-integrated `error` recovery (`S -> error ;`):
   - Unwinds the LR state stack to the nearest state with a valid shift on `error`,
   - Shifts the `error` pseudo-token, discards tokens until a valid follow token (`;`),
     and resumes parsing remaining statements in the same pass.
"""

from __future__ import annotations

from dataclasses import dataclass
import unittest


@dataclass(frozen=True)
class Token:
    kind: str
    lexeme: str
    pos: int


@dataclass(frozen=True)
class ParseDiagnostic:
    pos: int
    phase: str
    message: str


def tokenize(src: str) -> list[Token]:
    n = len(src)
    i = 0
    out: list[Token] = []
    while i < n:
        if src[i].isspace():
            i += 1
            continue
        start = i
        if src[i].isalpha() or src[i] == "_":
            i += 1
            while i < n and (src[i].isalnum() or src[i] == "_"):
                i += 1
            lex = src[start:i]
            kind = lex if lex in ("let", "emit") else "id"
            out.append(Token(kind, lex, start))
            continue
        if src[i].isdigit():
            i += 1
            while i < n and src[i].isdigit():
                i += 1
            out.append(Token("int", src[start:i], start))
            continue
        if src[i] in ("=", "+", "(", ")", ";"):
            ch = src[i]
            i += 1
            out.append(Token(ch, ch, start))
            continue
        out.append(Token("bad", src[i], start))
        i += 1
    out.append(Token("$", "$", n))
    return out


# ---------------------------------------------------------------------------
# Part 1: LL(1) Table with `Synch` Entries
# ---------------------------------------------------------------------------


@dataclass(frozen=True)
class Sym:
    is_nt: bool
    name: str


def nt(s: str) -> Sym:
    return Sym(True, s)


def tm(s: str) -> Sym:
    return Sym(False, s)


@dataclass(frozen=True)
class LlProd:
    lhs: str
    rhs: tuple[Sym, ...]


class Ll1Table:
    def __init__(self) -> None:
        self.prods = [
            LlProd("Program", (nt("StmtList"),)),
            LlProd("StmtList", (nt("Stmt"), nt("StmtList"))),
            LlProd("StmtList", ()),
            LlProd("Stmt", (tm("let"), tm("id"), tm("="), nt("Expr"), tm(";"))),
            LlProd("Stmt", (tm("emit"), nt("Expr"), tm(";"))),
            LlProd("Expr", (nt("Term"), nt("ExprP"))),
            LlProd("ExprP", (tm("+"), nt("Term"), nt("ExprP"))),
            LlProd("ExprP", ()),
            LlProd("Term", (tm("id"),)),
            LlProd("Term", (tm("int"),)),
            LlProd("Term", (tm("("), nt("Expr"), tm(")"))),
        ]
        nts = ["Program", "StmtList", "Stmt", "Expr", "ExprP", "Term"]
        self.first: dict[str, set[str]] = {n: set() for n in nts}
        nullable: dict[str, bool] = {n: False for n in nts}

        changed = True
        while changed:
            changed = False
            for p in self.prods:
                all_null = True
                for sym in p.rhs:
                    if not sym.is_nt:
                        if sym.name not in self.first[p.lhs]:
                            self.first[p.lhs].add(sym.name)
                            changed = True
                        all_null = False
                        break
                    for t in self.first[sym.name]:
                        if t not in self.first[p.lhs]:
                            self.first[p.lhs].add(t)
                            changed = True
                    if not nullable[sym.name]:
                        all_null = False
                        break
                if all_null and not nullable[p.lhs]:
                    nullable[p.lhs] = True
                    changed = True

        self.follow: dict[str, set[str]] = {n: set() for n in nts}
        self.follow["Program"].add("$")
        changed = True
        while changed:
            changed = False
            for p in self.prods:
                trailer = set(self.follow[p.lhs])
                for sym in reversed(p.rhs):
                    if not sym.is_nt:
                        trailer = {sym.name}
                    else:
                        before = len(self.follow[sym.name])
                        self.follow[sym.name].update(trailer)
                        if len(self.follow[sym.name]) != before:
                            changed = True
                        if nullable[sym.name]:
                            trailer.update(self.first[sym.name])
                        else:
                            trailer = set(self.first[sym.name])

        def first_of_rhs(rhs: tuple[Sym, ...]) -> tuple[set[str], bool]:
            out: set[str] = set()
            eps = True
            for sym in rhs:
                if not sym.is_nt:
                    out.add(sym.name)
                    eps = False
                    break
                out.update(self.first[sym.name])
                if not nullable[sym.name]:
                    eps = False
                    break
            return out, eps

        self.table: dict[tuple[str, str], tuple[str, int | None]] = {}
        for idx, p in enumerate(self.prods):
            fset, eps = first_of_rhs(p.rhs)
            for t in fset:
                self.table[(p.lhs, t)] = ("prod", idx)
            if eps:
                for t in self.follow[p.lhs]:
                    self.table[(p.lhs, t)] = ("prod", idx)
        for nonterm in nts:
            synchs = set(self.follow[nonterm]) | {";", "$"}
            if nonterm in ("Expr", "ExprP", "Term"):
                synchs |= {"let", "emit"}
            for t in synchs:
                self.table.setdefault((nonterm, t), ("synch", None))


def ll1_parse_with_recovery(src: str, ll: Ll1Table) -> tuple[int, list[ParseDiagnostic], list[str]]:
    tokens = tokenize(src)
    stack: list[Sym] = [tm("$"), nt("Program")]
    ip = 0
    valid_stmts = 0
    diags: list[ParseDiagnostic] = []
    actions: list[str] = []
    recovering = False
    stmt_has_error = False

    while stack:
        top = stack.pop()
        tok = tokens[ip]
        if not top.is_nt:
            exp = top.name
            if exp == "$" and tok.kind == "$":
                break
            if exp == tok.kind:
                if exp == ";" and not stmt_has_error:
                    valid_stmts += 1
                if exp == ";":
                    stmt_has_error = False
                ip += 1
                recovering = False
            else:
                stmt_has_error = True
                if not recovering:
                    diags.append(
                        ParseDiagnostic(tok.pos, "LL(1)", f"expected '{exp}', found '{tok.lexeme}'")
                    )
                    recovering = True
                if exp in (";", ")"):
                    actions.append(f"insert missing '{exp}' before '{tok.lexeme}'")
                elif exp != "$" and tok.kind != "$":
                    actions.append(f"pop mismatched terminal '{exp}'")
        else:
            nonterm = top.name
            if nonterm == "Stmt":
                stmt_has_error = False
            entry = ll.table.get((nonterm, tok.kind))
            if entry is not None and entry[0] == "prod":
                assert entry[1] is not None
                for sym in reversed(ll.prods[entry[1]].rhs):
                    stack.append(sym)
                recovering = False
            elif entry is not None and entry[0] == "synch":
                stmt_has_error = True
                if not recovering:
                    diags.append(
                        ParseDiagnostic(tok.pos, "LL(1)", f"unexpected '{tok.lexeme}' while expanding {nonterm}")
                    )
                    recovering = True
                actions.append(f"synch: pop nonterminal {nonterm} on '{tok.lexeme}'")
            else:
                stmt_has_error = True
                if not recovering:
                    diags.append(
                        ParseDiagnostic(tok.pos, "LL(1)", f"unexpected '{tok.lexeme}' in {nonterm}; scanning to synch token")
                    )
                    recovering = True
                if tok.kind == "$":
                    actions.append(f"EOF reached; aborting {nonterm}")
                else:
                    actions.append(f"skip token '{tok.lexeme}' while seeking synch for {nonterm}")
                    ip += 1
                    stack.append(nt(nonterm))

    return valid_stmts, diags, actions


# ---------------------------------------------------------------------------
# Part 2: SLR(1) Table with `error` Synchronization Production (`S -> error ;`)
# ---------------------------------------------------------------------------


@dataclass(frozen=True)
class LrProd:
    lhs: str
    rhs: tuple[str, ...]
    is_error_prod: bool = False


class SlrParser:
    def __init__(self) -> None:
        self.prods = [
            LrProd("S'", ("P",)),
            LrProd("P", ("P", "S")),
            LrProd("P", ("S",)),
            LrProd("S", ("let", "id", "=", "E", ";")),
            LrProd("S", ("emit", "E", ";")),
            LrProd("S", ("error", ";"), is_error_prod=True),
            LrProd("E", ("E", "+", "T")),
            LrProd("E", ("T",)),
            LrProd("T", ("id",)),
            LrProd("T", ("int",)),
            LrProd("T", ("(", "E", ")")),
        ]
        nts = {p.lhs for p in self.prods}
        all_syms = set(nts)
        for p in self.prods:
            all_syms.update(p.rhs)
        terminals = sorted(all_syms - nts)

        first: dict[str, set[str]] = {n: set() for n in nts}
        changed = True
        while changed:
            changed = False
            for p in self.prods:
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

        follow: dict[str, set[str]] = {n: set() for n in nts}
        follow["S'"].add("$")
        changed = True
        while changed:
            changed = False
            for p in self.prods:
                trailer = set(follow[p.lhs])
                for s in reversed(p.rhs):
                    if s not in nts:
                        trailer = {s}
                    else:
                        before = len(follow[s])
                        follow[s].update(trailer)
                        if len(follow[s]) != before:
                            changed = True
                        trailer = set(first[s])

        def closure(seed: frozenset[tuple[int, int]]) -> frozenset[tuple[int, int]]:
            cur = set(seed)
            ch = True
            while ch:
                ch = False
                for pid, dot in list(cur):
                    if dot < len(self.prods[pid].rhs):
                        sym = self.prods[pid].rhs[dot]
                        if sym in nts:
                            for npid, np in enumerate(self.prods):
                                if np.lhs == sym and (npid, 0) not in cur:
                                    cur.add((npid, 0))
                                    ch = True
            return frozenset(cur)

        def goto_items(items: frozenset[tuple[int, int]], sym: str) -> frozenset[tuple[int, int]]:
            seed = frozenset(
                (pid, dot + 1)
                for pid, dot in items
                if dot < len(self.prods[pid].rhs) and self.prods[pid].rhs[dot] == sym
            )
            return closure(seed) if seed else frozenset()

        states: list[frozenset[tuple[int, int]]] = []
        state_id: dict[frozenset[tuple[int, int]], int] = {}
        transitions: dict[tuple[int, str], int] = {}

        start = closure(frozenset({(0, 0)}))
        state_id[start] = 0
        states.append(start)

        idx = 0
        while idx < len(states):
            cur_items = states[idx]
            for sym in sorted(all_syms):
                nxt = goto_items(cur_items, sym)
                if not nxt:
                    continue
                if nxt not in state_id:
                    state_id[nxt] = len(states)
                    states.append(nxt)
                transitions[(idx, sym)] = state_id[nxt]
            idx += 1

        self.action: dict[tuple[int, str], tuple[str, int]] = {}
        self.goto: dict[tuple[int, str], int] = {}
        for sid, items in enumerate(states):
            for term in terminals:
                if (sid, term) in transitions:
                    self.action[(sid, term)] = ("shift", transitions[(sid, term)])
            for nonterm in sorted(nts):
                if (sid, nonterm) in transitions:
                    self.goto[(sid, nonterm)] = transitions[(sid, nonterm)]
            for pid, dot in items:
                if dot == len(self.prods[pid].rhs):
                    if pid == 0:
                        self.action[(sid, "$")] = ("accept", 0)
                    else:
                        for f in follow[self.prods[pid].lhs]:
                            self.action.setdefault((sid, f), ("reduce", pid))


def slr_parse_with_recovery(src: str, lr: SlrParser) -> tuple[int, list[ParseDiagnostic], list[str]]:
    tokens = tokenize(src)
    stack = [0]
    ip = 0
    valid_stmts = 0
    diags: list[ParseDiagnostic] = []
    actions: list[str] = []

    while True:
        state = stack[-1]
        tok = tokens[ip]
        act = lr.action.get((state, tok.kind))
        if act is not None and act[0] == "shift":
            stack.append(act[1])
            ip += 1
        elif act is not None and act[0] == "reduce":
            prod = lr.prods[act[1]]
            for _ in prod.rhs:
                stack.pop()
            if prod.lhs == "S" and not prod.is_error_prod:
                valid_stmts += 1
            stack.append(lr.goto[(stack[-1], prod.lhs)])
        elif act is not None and act[0] == "accept":
            break
        else:
            diags.append(ParseDiagnostic(tok.pos, "SLR", f"syntax error near '{tok.lexeme}'"))
            popped = 0
            while stack:
                top = stack[-1]
                err_act = lr.action.get((top, "error"))
                if err_act is not None and err_act[0] == "shift":
                    err_state = err_act[1]
                    stack.append(err_state)
                    actions.append(
                        f"unwound {popped} state(s) to state {top}, shifted `error` -> state {err_state}"
                    )
                    skipped = 0
                    while (
                        ip < len(tokens)
                        and tokens[ip].kind != "$"
                        and (err_state, tokens[ip].kind) not in lr.action
                    ):
                        ip += 1
                        skipped += 1
                    actions.append(f"discarded {skipped} token(s), resumed at '{tokens[ip].lexeme}'")
                    break
                stack.pop()
                popped += 1
            if not stack or tokens[ip].kind == "$":
                break

    return valid_stmts, diags, actions


def main() -> None:
    ll = Ll1Table()
    lr = SlrParser()
    broken = "let x = + 5; let = 9; emit (1 + ); emit 42;"

    print("=== LL(1) Synchronization Sets and LR Error Recovery (Ch. 4) ===")
    print(f"Sample input:\n  {broken}\n")

    ll_valid, ll_diags, ll_acts = ll1_parse_with_recovery(broken, ll)
    print(f"1. LL(1) Table-Driven Recovery (FOLLOW synch + virtual ';'):\n   valid statements recovered = {ll_valid}, diagnostics = {len(ll_diags)}")
    for d in ll_diags:
        print(f"   [diag @{d.pos}] {d.message}")
    for a in ll_acts:
        print(f"   [action] {a}")

    lr_valid, lr_diags, lr_acts = slr_parse_with_recovery(broken, lr)
    print(f"\n2. SLR(1) Error-Production Recovery (`S -> error ;`):\n   valid statements recovered = {lr_valid}, diagnostics = {len(lr_diags)}")
    for d in lr_diags:
        print(f"   [diag @{d.pos}] {d.message}")
    for a in lr_acts:
        print(f"   [action] {a}")


class ParserErrorRecoveryTests(unittest.TestCase):
    def test_ll1_recovers_missing_semicolon_and_broken_statements(self) -> None:
        ll = Ll1Table()
        src = "let a = 1 emit 2; let = 3; emit 42;"
        valid, diags, acts = ll1_parse_with_recovery(src, ll)
        self.assertGreaterEqual(valid, 2)
        self.assertTrue(diags)
        self.assertTrue(any("insert missing ';'" in a for a in acts))

    def test_ll1_and_slr_both_recover_valid_tail_statement_without_cascade(self) -> None:
        ll = Ll1Table()
        lr = SlrParser()
        src = "let x = + 5; let = 9; emit 42;"
        ll_v, ll_d, _ = ll1_parse_with_recovery(src, ll)
        lr_v, lr_d, _ = slr_parse_with_recovery(src, lr)
        self.assertGreaterEqual(ll_v, 1)
        self.assertEqual(lr_v, 1)
        self.assertLessEqual(len(ll_d), 3)
        self.assertEqual(len(lr_d), 2)

    def test_valid_input_produces_zero_diagnostics_in_both_parsers(self) -> None:
        ll = Ll1Table()
        lr = SlrParser()
        src = "let x = 1 + 2; emit (x + 3);"
        ll_v, ll_d, _ = ll1_parse_with_recovery(src, ll)
        lr_v, lr_d, _ = slr_parse_with_recovery(src, lr)
        self.assertEqual(ll_v, 2)
        self.assertEqual(lr_v, 2)
        self.assertEqual(ll_d, [])
        self.assertEqual(lr_d, [])


if __name__ == "__main__":
    main()
