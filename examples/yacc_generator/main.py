"""Yacc-style `.y` LALR(1) parser-generator DSL compiler: parses declarations (`%token`,
`%left`, `%right`, `%nonassoc`, `%start`) and productions (`%prec`, `%empty`, `{ actions }`),
builds canonical LR(1) -> merged LALR(1) tables with Yacc conflict resolution, and runs the
generated parser + semantic action evaluator.
"""

from dataclasses import dataclass
import unittest


# ---------------------------------------------------------------------------
# 1. `.y` Grammar, Precedence & Semantic Action AST
# ---------------------------------------------------------------------------

EOF_SYM = "$"
AUG_START = "$accept"


@dataclass(frozen=True)
class PrecEntry:
    level: int
    assoc: str  # "left", "right", "nonassoc"


@dataclass(frozen=True, order=True)
class Symbol:
    is_nt: bool
    name: str


@dataclass(frozen=True)
class ActionExpr:
    kind: str  # "dollar", "int", "call"
    idx: int = 0
    val: int = 0
    name: str = ""
    args: tuple["ActionExpr", ...] = ()


@dataclass(frozen=True)
class Production:
    id: int
    lhs: str
    rhs: tuple[Symbol, ...]
    prec_override: str | None
    raw_action: str
    action: ActionExpr


@dataclass
class YaccSpec:
    start_sym: str
    terminals: set[str]
    nonterminals: set[str]
    prec_table: dict[str, PrecEntry]
    prods: list[Production]


def strip_yacc_comments(src: str) -> str:
    out: list[str] = []
    i = 0
    n = len(src)
    in_sq = False
    while i < n:
        ch = src[i]
        if in_sq:
            out.append(ch)
            if ch == "\\" and i + 1 < n:
                i += 1
                out.append(src[i])
            elif ch == "'":
                in_sq = False
            i += 1
            continue
        if ch == "'":
            in_sq = True
            out.append("'")
            i += 1
            continue
        if ch == "/" and i + 1 < n and src[i + 1] == "/":
            i += 2
            while i < n and src[i] != "\n":
                i += 1
            continue
        if ch == "/" and i + 1 < n and src[i + 1] == "*":
            i += 2
            closed = False
            while i + 1 < n:
                if src[i] == "*" and src[i + 1] == "/":
                    i += 2
                    closed = True
                    break
                if src[i] == "\n":
                    out.append("\n")
                i += 1
            if not closed:
                raise ValueError("unterminated /* ... */ comment in .y spec")
            continue
        out.append(ch)
        i += 1
    return "".join(out)


class ActionParser:
    def __init__(self, src: str, rhs_len: int) -> None:
        self.src = src
        self.pos = 0
        self.rhs_len = rhs_len

    def skip_ws(self) -> None:
        while self.pos < len(self.src) and self.src[self.pos].isspace():
            self.pos += 1

    def parse(self) -> ActionExpr:
        self.skip_ws()
        expr = self.parse_expr()
        self.skip_ws()
        if self.pos < len(self.src):
            raise ValueError(f"unexpected trailing token '{self.src[self.pos]}' in action")
        return expr

    def parse_expr(self) -> ActionExpr:
        self.skip_ws()
        if self.pos >= len(self.src):
            raise ValueError("empty semantic action")
        ch = self.src[self.pos]
        if ch == "$":
            self.pos += 1
            start = self.pos
            while self.pos < len(self.src) and self.src[self.pos].isdigit():
                self.pos += 1
            if self.pos == start:
                raise ValueError("expected positional index after '$' in action")
            idx = int(self.src[start : self.pos])
            if idx == 0 or idx > self.rhs_len:
                raise ValueError(
                    f"semantic action reference ${idx} out of bounds for rule with {self.rhs_len} symbols"
                )
            return ActionExpr("dollar", idx=idx)
        if ch == "-" or ch.isdigit():
            start = self.pos
            if ch == "-":
                self.pos += 1
            dstart = self.pos
            while self.pos < len(self.src) and self.src[self.pos].isdigit():
                self.pos += 1
            if self.pos == dstart:
                raise ValueError("invalid integer literal in semantic action")
            return ActionExpr("int", val=int(self.src[start : self.pos]))
        if ch.isalpha() or ch == "_":
            start = self.pos
            while self.pos < len(self.src) and (self.src[self.pos].isalnum() or self.src[self.pos] == "_"):
                self.pos += 1
            name = self.src[start : self.pos]
            self.skip_ws()
            if self.pos >= len(self.src) or self.src[self.pos] != "(":
                raise ValueError(f"expected '(' after action helper '{name}'")
            self.pos += 1
            args: list[ActionExpr] = []
            self.skip_ws()
            if self.pos < len(self.src) and self.src[self.pos] != ")":
                while True:
                    args.append(self.parse_expr())
                    self.skip_ws()
                    if self.pos < len(self.src) and self.src[self.pos] == ",":
                        self.pos += 1
                    elif self.pos < len(self.src) and self.src[self.pos] == ")":
                        break
                    else:
                        raise ValueError(f"expected ',' or ')' in call '{name}(...)'")
            if self.pos >= len(self.src) or self.src[self.pos] != ")":
                raise ValueError(f"unclosed call '{name}(...)'")
            self.pos += 1
            return ActionExpr("call", name=name, args=tuple(args))
        raise ValueError(f"unexpected character '{ch}' in action")


def lex_rules_section(src: str) -> list[tuple[str, str]]:
    i = 0
    n = len(src)
    out: list[tuple[str, str]] = []
    while i < n:
        ch = src[i]
        if ch.isspace():
            i += 1
            continue
        if ch == ":":
            out.append(("COLON", ":"))
            i += 1
        elif ch == "|":
            out.append(("PIPE", "|"))
            i += 1
        elif ch == ";":
            out.append(("SEMI", ";"))
            i += 1
        elif ch == "'":
            i += 1
            if i >= n:
                raise ValueError("unterminated character token in rules")
            if src[i] == "\\":
                i += 1
                if i >= n:
                    raise ValueError("dangling escape in char token")
                lit = src[i]
                i += 1
            else:
                lit = src[i]
                i += 1
            if i >= n or src[i] != "'":
                raise ValueError("expected closing '\\'' in single-char token")
            i += 1
            out.append(("CHARLIT", lit))
        elif ch == "{":
            i += 1
            start = i
            depth = 1
            while i < n and depth > 0:
                if src[i] == "{":
                    depth += 1
                elif src[i] == "}":
                    depth -= 1
                if depth > 0:
                    i += 1
            if depth != 0:
                raise ValueError("unclosed '{ ... }' semantic action in rules")
            body = src[start:i].strip()
            i += 1
            out.append(("ACTION", body))
        elif ch == "%":
            start = i
            i += 1
            while i < n and (src[i].isalnum() or src[i] == "_"):
                i += 1
            word = src[start:i]
            if word == "%prec":
                out.append(("PREC", word))
            elif word == "%empty":
                out.append(("EMPTY", word))
            else:
                raise ValueError(f"unexpected directive in rules section: {word}")
        elif ch.isalpha() or ch == "_":
            start = i
            i += 1
            while i < n and (src[i].isalnum() or src[i] == "_"):
                i += 1
            out.append(("IDENT", src[start:i]))
        else:
            raise ValueError(f"unexpected character '{ch}' in .y rules section")
    return out


def parse_decl_token(tok: str) -> str:
    if tok.startswith("'") and tok.endswith("'") and len(tok) >= 3:
        inner = tok[1:-1]
        if len(inner) != 1:
            raise ValueError(f"multi-char literal token unsupported: {tok}")
        return inner
    return tok


def parse_yacc_spec(raw_src: str) -> YaccSpec:
    cleaned = strip_yacc_comments(raw_src)
    sections: list[list[str]] = [[]]
    for line in cleaned.splitlines():
        if line.strip() == "%%":
            sections.append([])
        else:
            sections[-1].append(line)
    if len(sections) < 2:
        raise ValueError("Yacc specification requires at least one '%%' section divider")

    declared_tokens: set[str] = set()
    prec_table: dict[str, PrecEntry] = {}
    start_opt: str | None = None
    prec_level = 0

    words = "\n".join(sections[0]).split()
    idx = 0
    while idx < len(words):
        d = words[idx]
        idx += 1
        if d == "%token":
            while idx < len(words) and not words[idx].startswith("%"):
                declared_tokens.add(parse_decl_token(words[idx]))
                idx += 1
        elif d in ("%left", "%right", "%nonassoc"):
            prec_level += 1
            assoc = "left" if d == "%left" else "right" if d == "%right" else "nonassoc"
            while idx < len(words) and not words[idx].startswith("%"):
                name = parse_decl_token(words[idx])
                declared_tokens.add(name)
                prec_table[name] = PrecEntry(prec_level, assoc)
                idx += 1
        elif d == "%start":
            if idx >= len(words) or words[idx].startswith("%"):
                raise ValueError("expected symbol name after %start")
            start_opt = words[idx]
            idx += 1
        else:
            raise ValueError(f"unknown Yacc declaration directive: {d}")

    rtoks = lex_rules_section("\n".join(sections[1]))
    raw_prods: list[tuple[str, list[str], str | None, str | None]] = []
    nonterminals: set[str] = set()
    terminals = set(declared_tokens)

    p = 0
    first_lhs: str | None = None
    while p < len(rtoks):
        kind, lhs = rtoks[p]
        if kind != "IDENT":
            raise ValueError("expected nonterminal LHS at start of rule group")
        if first_lhs is None:
            first_lhs = lhs
        nonterminals.add(lhs)
        p += 1
        if p >= len(rtoks) or rtoks[p][0] != "COLON":
            raise ValueError(f"expected ':' after LHS '{lhs}'")
        p += 1

        while True:
            rhs_names: list[str] = []
            prec_override: str | None = None
            action_opt: str | None = None

            while p < len(rtoks) and rtoks[p][0] not in ("PIPE", "SEMI"):
                tk, tv = rtoks[p]
                if tk == "EMPTY":
                    p += 1
                elif tk == "IDENT":
                    rhs_names.append(tv)
                    p += 1
                elif tk == "CHARLIT":
                    terminals.add(tv)
                    rhs_names.append(tv)
                    p += 1
                elif tk == "PREC":
                    p += 1
                    if p >= len(rtoks) or rtoks[p][0] not in ("IDENT", "CHARLIT"):
                        raise ValueError("expected terminal after %prec")
                    if rtoks[p][0] == "CHARLIT":
                        terminals.add(rtoks[p][1])
                    prec_override = rtoks[p][1]
                    p += 1
                elif tk == "ACTION":
                    action_opt = tv
                    p += 1
                else:
                    raise ValueError(f"unexpected token '{tv}' inside rule '{lhs}'")

            raw_prods.append((lhs, rhs_names, prec_override, action_opt))
            if p < len(rtoks) and rtoks[p][0] == "PIPE":
                p += 1
            elif p < len(rtoks) and rtoks[p][0] == "SEMI":
                p += 1
                break
            else:
                raise ValueError(f"expected '|' or ';' closing rule '{lhs}'")

    if not raw_prods:
        raise ValueError("Yacc rules section has no productions")

    start_sym = start_opt if start_opt is not None else first_lhs
    assert start_sym is not None
    if start_sym not in nonterminals:
        raise ValueError(f"start symbol '{start_sym}' has no defining rule")

    prods: list[Production] = []
    for pid, (lhs, rhs_names, prec_ov, act_opt) in enumerate(raw_prods):
        rhs_syms: list[Symbol] = []
        for s in rhs_names:
            if s in nonterminals:
                rhs_syms.append(Symbol(True, s))
            else:
                terminals.add(s)
                rhs_syms.append(Symbol(False, s))
        raw_act = act_opt if act_opt is not None else ("0" if not rhs_syms else "$1")
        action = ActionParser(raw_act, len(rhs_syms)).parse()
        prods.append(Production(pid, lhs, tuple(rhs_syms), prec_ov, raw_act, action))

    return YaccSpec(start_sym, terminals, nonterminals, prec_table, prods)


# ---------------------------------------------------------------------------
# 2. Canonical LR(1) -> Merged LALR(1) + Precedence Conflict Resolution
# ---------------------------------------------------------------------------

@dataclass(frozen=True, order=True)
class Lr1Item:
    prod_id: int
    dot: int
    lookahead: str


@dataclass(frozen=True)
class ConflictRecord:
    kind: str  # "resolved_sr", "unresolved_sr", "unresolved_rr"
    state: int
    term: str
    detail: str


@dataclass
class CompiledParser:
    spec: YaccSpec
    prods: list[Production]
    lr1_state_count: int
    lalr_state_count: int
    action_table: dict[tuple[int, str], tuple[str, int]]
    goto_table: dict[tuple[int, str], int]
    conflicts: list[ConflictRecord]


def compute_first_sets(
    prods: list[Production], terminals: set[str], nonterminals: set[str]
) -> dict[str, tuple[set[str], bool]]:
    first: dict[str, tuple[set[str], bool]] = {t: ({t}, False) for t in terminals}
    first[EOF_SYM] = ({EOF_SYM}, False)
    for nt in nonterminals:
        first[nt] = (set(), False)
    first[AUG_START] = (set(), False)

    while True:
        changed = False
        for prod in prods:
            all_null = True
            to_add: set[str] = set()
            for sym in prod.rhs:
                fset, nullable = first[sym.name]
                to_add.update(fset)
                if not nullable:
                    all_null = False
                    break
            lhs_set, lhs_null = first[prod.lhs]
            before = len(lhs_set)
            lhs_set.update(to_add)
            if len(lhs_set) > before:
                changed = True
            if all_null and not lhs_null:
                first[prod.lhs] = (lhs_set, True)
                changed = True
        if not changed:
            break
    return first


def first_of_seq(
    seq: tuple[Symbol, ...], trail: str, first: dict[str, tuple[set[str], bool]]
) -> set[str]:
    out: set[str] = set()
    all_null = True
    for sym in seq:
        fset, nullable = first[sym.name]
        out.update(fset)
        if not nullable:
            all_null = False
            break
    if all_null:
        out.add(trail)
    return out


def closure_lr1(
    seed: set[Lr1Item],
    prods: list[Production],
    first: dict[str, tuple[set[str], bool]],
) -> frozenset[Lr1Item]:
    items = set(seed)
    work = list(seed)
    while work:
        it = work.pop()
        prod = prods[it.prod_id]
        if it.dot < len(prod.rhs) and prod.rhs[it.dot].is_nt:
            nt = prod.rhs[it.dot].name
            las = first_of_seq(prod.rhs[it.dot + 1 :], it.lookahead, first)
            for p in prods:
                if p.lhs == nt:
                    for la in las:
                        cand = Lr1Item(p.id, 0, la)
                        if cand not in items:
                            items.add(cand)
                            work.append(cand)
    return frozenset(items)


def goto_lr1(
    items: frozenset[Lr1Item],
    sym: Symbol,
    prods: list[Production],
    first: dict[str, tuple[set[str], bool]],
) -> frozenset[Lr1Item]:
    seed = {
        Lr1Item(it.prod_id, it.dot + 1, it.lookahead)
        for it in items
        if it.dot < len(prods[it.prod_id].rhs) and prods[it.prod_id].rhs[it.dot] == sym
    }
    return closure_lr1(seed, prods, first) if seed else frozenset()


def production_prec(prod: Production, prec_table: dict[str, PrecEntry]) -> PrecEntry | None:
    if prod.prec_override is not None:
        return prec_table.get(prod.prec_override)
    last_t: str | None = None
    for sym in prod.rhs:
        if not sym.is_nt:
            last_t = sym.name
    return prec_table.get(last_t) if last_t is not None else None


def compile_yacc_spec(raw_src: str) -> CompiledParser:
    spec = parse_yacc_spec(raw_src)
    prods: list[Production] = [
        Production(
            0,
            AUG_START,
            (Symbol(True, spec.start_sym),),
            None,
            "$1",
            ActionExpr("dollar", idx=1),
        )
    ]
    for p in spec.prods:
        prods.append(
            Production(p.id + 1, p.lhs, p.rhs, p.prec_override, p.raw_action, p.action)
        )

    first = compute_first_sets(prods, spec.terminals, spec.nonterminals)
    all_symbols = [Symbol(False, t) for t in sorted(spec.terminals)] + [
        Symbol(True, nt) for nt in sorted(spec.nonterminals)
    ]

    init = closure_lr1({Lr1Item(0, 0, EOF_SYM)}, prods, first)
    lr1_states: list[frozenset[Lr1Item]] = [init]
    lr1_index: dict[frozenset[Lr1Item], int] = {init: 0}
    lr1_edges: dict[tuple[int, Symbol], int] = {}

    q_idx = 0
    while q_idx < len(lr1_states):
        cur = lr1_states[q_idx]
        for sym in all_symbols:
            nxt = goto_lr1(cur, sym, prods, first)
            if not nxt:
                continue
            tgt = lr1_index.get(nxt)
            if tgt is None:
                tgt = len(lr1_states)
                lr1_states.append(nxt)
                lr1_index[nxt] = tgt
            lr1_edges[(q_idx, sym)] = tgt
        q_idx += 1

    # Merge LR(1) states sharing the same LR(0) core into LALR(1) states.
    core_to_lalr: dict[frozenset[tuple[int, int]], int] = {}
    lr1_to_lalr: list[int] = [0] * len(lr1_states)
    lalr_sets: list[set[Lr1Item]] = []
    for sid, st in enumerate(lr1_states):
        core = frozenset((it.prod_id, it.dot) for it in st)
        lid = core_to_lalr.get(core)
        if lid is None:
            lid = len(lalr_sets)
            core_to_lalr[core] = lid
            lalr_sets.append(set())
        lr1_to_lalr[sid] = lid
        lalr_sets[lid].update(st)

    lalr_edges: dict[tuple[int, Symbol], int] = {
        (lr1_to_lalr[src], sym): lr1_to_lalr[dst] for (src, sym), dst in lr1_edges.items()
    }

    action_table: dict[tuple[int, str], tuple[str, int]] = {}
    goto_table: dict[tuple[int, str], int] = {
        (src, sym.name): dst for (src, sym), dst in lalr_edges.items() if sym.is_nt
    }
    conflicts: list[ConflictRecord] = []
    all_terms = sorted(spec.terminals | {EOF_SYM})

    for state_id, st in enumerate(lalr_sets):
        for term in all_terms:
            shift_target = (
                lalr_edges.get((state_id, Symbol(False, term))) if term != EOF_SYM else None
            )
            reduces = sorted(
                {
                    it.prod_id
                    for it in st
                    if it.dot == len(prods[it.prod_id].rhs)
                    and it.lookahead == term
                    and it.prod_id != 0
                }
            )
            has_accept = any(
                it.prod_id == 0 and it.dot == 1 and it.lookahead == term for it in st
            )

            chosen: tuple[str, int] | None = None
            if has_accept and not reduces and shift_target is None:
                chosen = ("accept", 0)
            elif reduces:
                chosen_red = reduces[0]
                for other in reduces[1:]:
                    conflicts.append(
                        ConflictRecord(
                            "unresolved_rr",
                            state_id,
                            term,
                            f"reduce r{chosen_red} ({prods[chosen_red].lhs}) vs reduce r{other} "
                            f"({prods[other].lhs}) -> picked earlier rule r{chosen_red}",
                        )
                    )
                if shift_target is not None:
                    rp = production_prec(prods[chosen_red], spec.prec_table)
                    tp = spec.prec_table.get(term)
                    if rp is not None and tp is not None:
                        if rp.level > tp.level:
                            act = ("reduce", chosen_red)
                            why = (
                                f"rule r{chosen_red} prec {rp.level} > '{term}' prec {tp.level} -> reduce"
                            )
                        elif rp.level < tp.level:
                            act = ("shift", shift_target)
                            why = (
                                f"rule r{chosen_red} prec {rp.level} < '{term}' prec {tp.level} -> shift {shift_target}"
                            )
                        elif tp.assoc == "left":
                            act = ("reduce", chosen_red)
                            why = f"equal prec {rp.level} (%left) -> reduce"
                        elif tp.assoc == "right":
                            act = ("shift", shift_target)
                            why = f"equal prec {rp.level} (%right) -> shift {shift_target}"
                        else:
                            act = ("nonassoc", 0)
                            why = f"equal prec {rp.level} (%nonassoc) -> error"
                        conflicts.append(ConflictRecord("resolved_sr", state_id, term, why))
                        chosen = act
                    else:
                        conflicts.append(
                            ConflictRecord(
                                "unresolved_sr",
                                state_id,
                                term,
                                f"shift {shift_target} vs reduce r{chosen_red} (no prec pair) -> default shift {shift_target}",
                            )
                        )
                        chosen = ("shift", shift_target)
                else:
                    chosen = ("reduce", chosen_red)
            elif shift_target is not None:
                chosen = ("shift", shift_target)

            if chosen is not None:
                action_table[(state_id, term)] = chosen

    return CompiledParser(
        spec=spec,
        prods=prods,
        lr1_state_count=len(lr1_states),
        lalr_state_count=len(lalr_sets),
        action_table=action_table,
        goto_table=goto_table,
        conflicts=conflicts,
    )


# ---------------------------------------------------------------------------
# 3. Generated Parser Driver & Semantic Action Evaluator
# ---------------------------------------------------------------------------

def tokenize_program(src: str, declared_terms: set[str]) -> list[tuple[str, str]]:
    i = 0
    n = len(src)
    out: list[tuple[str, str]] = []
    while i < n:
        ch = src[i]
        if ch.isspace():
            i += 1
            continue
        if ch.isdigit():
            start = i
            while i < n and src[i].isdigit():
                i += 1
            out.append(("NUM", src[start:i]))
            continue
        if ch.isalpha() or ch == "_":
            start = i
            while i < n and (src[i].isalnum() or src[i] == "_"):
                i += 1
            word = src[start:i]
            out.append((word if word in declared_terms else "ID", word))
            continue
        if ch in declared_terms:
            out.append((ch, ch))
            i += 1
            continue
        raise ValueError(f"unexpected input character '{ch}'")
    out.append((EOF_SYM, EOF_SYM))
    return out


def as_int(v: int | str, env: dict[str, int]) -> int:
    if isinstance(v, int):
        return v
    if v in env:
        return env[v]
    raise ValueError(f"undefined identifier '{v}'")


def eval_action(expr: ActionExpr, args: list[int | str], env: dict[str, int]) -> int | str:
    if expr.kind == "dollar":
        return args[expr.idx - 1]
    if expr.kind == "int":
        return expr.val
    vals = [eval_action(a, args, env) for a in expr.args]
    name = expr.name
    if name == "num" and len(vals) == 1 and isinstance(vals[0], int):
        return vals[0]
    if name == "var" and len(vals) == 1 and isinstance(vals[0], str):
        if vals[0] not in env:
            raise ValueError(f"undefined variable '{vals[0]}'")
        return env[vals[0]]
    if name == "neg" and len(vals) == 1:
        return -as_int(vals[0], env)
    if name in ("add", "sub", "mul", "div", "lt") and len(vals) == 2:
        a, b = as_int(vals[0], env), as_int(vals[1], env)
        if name == "add":
            return a + b
        if name == "sub":
            return a - b
        if name == "mul":
            return a * b
        if name == "div":
            if b == 0:
                raise ValueError("division by zero in semantic action")
            return a // b
        return 1 if a < b else 0
    if name == "assign" and len(vals) == 2:
        if not isinstance(vals[0], str):
            raise ValueError(f"expected identifier name, found integer {vals[0]}")
        val = as_int(vals[1], env)
        env[vals[0]] = val
        return val
    if name == "seq" and len(vals) == 2:
        return vals[1]
    raise ValueError(f"unknown semantic action call '{name}' with {len(vals)} args")


def parse_with_yacc(compiled: CompiledParser, src: str) -> tuple[int | str, dict[str, int], int]:
    tokens = tokenize_program(src, compiled.spec.terminals)
    state_stack: list[int] = [0]
    val_stack: list[int | str] = []
    cursor = 0
    env: dict[str, int] = {}
    reductions = 0

    while True:
        state = state_stack[-1]
        kind, lexeme = tokens[cursor]
        act = compiled.action_table.get((state, kind))
        if act is None:
            expected = sorted(t for (s, t) in compiled.action_table if s == state)
            raise ValueError(
                f"syntax error at token '{lexeme}' ('{kind}') in state {state}; expected one of {expected}"
            )
        op, arg = act
        if op == "shift":
            val_stack.append(int(lexeme) if kind == "NUM" else lexeme)
            state_stack.append(arg)
            cursor += 1
        elif op == "reduce":
            prod = compiled.prods[arg]
            k = len(prod.rhs)
            rhs_vals = val_stack[len(val_stack) - k :]
            del val_stack[len(val_stack) - k :]
            del state_stack[len(state_stack) - k :]
            res = eval_action(prod.action, rhs_vals, env)
            goto_st = compiled.goto_table.get((state_stack[-1], prod.lhs))
            if goto_st is None:
                raise ValueError(f"missing GOTO for nonterminal '{prod.lhs}' from state {state_stack[-1]}")
            val_stack.append(res)
            state_stack.append(goto_st)
            reductions += 1
        elif op == "accept":
            return (val_stack[-1] if val_stack else 0), env, reductions
        else:
            raise ValueError(f"syntax error: non-associative operator '{lexeme}' cannot be chained")


# ---------------------------------------------------------------------------
# 4. Worked Demo & Tests
# ---------------------------------------------------------------------------

SAMPLE_YACC_SPEC = r"""
%token ID NUM
%left ';'
%nonassoc '<'
%left '+' '-'
%left '*' '/'
%right UMINUS
%start stmts

%%
stmts : stmts ';' stmt        { seq($1, $3) }
      | stmt                  { $1 }
      ;

stmt  : ID '=' expr           { assign($1, $3) }
      | expr                  { $1 }
      ;

expr  : expr '+' expr         { add($1, $3) }
      | expr '-' expr         { sub($1, $3) }
      | expr '*' expr         { mul($1, $3) }
      | expr '/' expr         { div($1, $3) }
      | expr '<' expr         { lt($1, $3) }
      | '-' expr %prec UMINUS { neg($2) }
      | '(' expr ')'          { $2 }
      | ID                    { var($1) }
      | NUM                   { num($1) }
      ;
%%
"""

SAMPLE_PROGRAM = "x = 2 + 3 * -4; x * 2 + 1"


def main() -> None:
    compiled = compile_yacc_spec(SAMPLE_YACC_SPEC)
    resolved_cnt = sum(1 for c in compiled.conflicts if c.kind == "resolved_sr")
    unresolved_cnt = len(compiled.conflicts) - resolved_cnt
    print(
        f"Yacc LALR(1) table built: {len(compiled.spec.prods)} rules, "
        f"{compiled.lr1_state_count} canonical LR(1) states -> "
        f"{compiled.lalr_state_count} merged LALR(1) states"
    )
    print(
        f"Conflicts: {resolved_cnt} S/R resolved by %left/%right/%nonassoc/%prec, "
        f"{unresolved_cnt} unresolved"
    )
    for c in compiled.conflicts[:6]:
        print(f"  [state {c.state} on '{c.term}'] {c.detail}")

    val, env, reductions = parse_with_yacc(compiled, SAMPLE_PROGRAM)
    print(f"\nParsed & evaluated {SAMPLE_PROGRAM!r}:")
    print(f"  reductions = {reductions}")
    print(f"  env        = {env}")
    print(f"  result     = {val}")


class YaccGeneratorTests(unittest.TestCase):
    def test_resolves_operator_ambiguity_and_evaluates_actions(self) -> None:
        compiled = compile_yacc_spec(SAMPLE_YACC_SPEC)
        unresolved = [c for c in compiled.conflicts if c.kind != "resolved_sr"]
        self.assertEqual(unresolved, [])
        self.assertTrue(any(c.kind == "resolved_sr" for c in compiled.conflicts))

        val, env, _ = parse_with_yacc(compiled, SAMPLE_PROGRAM)
        self.assertEqual(env.get("x"), -10)
        self.assertEqual(val, -19)

        val2, _, _ = parse_with_yacc(compiled, "10 - 3 - 2; 4 < 9")
        self.assertEqual(val2, 1)
        val3, _, _ = parse_with_yacc(compiled, "10 - 3 - 2")
        self.assertEqual(val3, 5)

    def test_nonassoc_rejects_chaining_and_unannotated_grammar_warns(self) -> None:
        compiled = compile_yacc_spec(SAMPLE_YACC_SPEC)
        with self.assertRaisesRegex(ValueError, "non-associative"):
            parse_with_yacc(compiled, "1 < 2 < 3")

        unannotated = """
%token NUM
%%
expr : expr '+' expr { add($1, $3) }
     | expr '*' expr { mul($1, $3) }
     | NUM           { num($1) }
     ;
%%
"""
        weak = compile_yacc_spec(unannotated)
        self.assertTrue(any(c.kind == "unresolved_sr" for c in weak.conflicts))

        rr_spec = """
%token A
%%
s : x A { $1 }
  | y A { $1 }
  ;
x : %empty { 1 } ;
y : %empty { 2 } ;
%%
"""
        rr = compile_yacc_spec(rr_spec)
        self.assertTrue(any(c.kind == "unresolved_rr" for c in rr.conflicts))
        val, _, _ = parse_with_yacc(rr, "A")
        self.assertEqual(val, 1)


if __name__ == "__main__":
    main()
