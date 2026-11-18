"""Modern Extension: Generalized LR (GLR) Parsing, GSS & SPPF Forests.

Explicitly labeled as a modern parsing extension building directly on Chapter 4 LR tables
(Masaru Tomita 1985; Bison `%glr-parser`, Tree-sitter):
1. Keeps ALL shift/reduce and reduce/reduce conflicts inside `ACTION[state, sym] -> list[Action]`
   for ambiguous grammars like `E -> E + E | E * E | ( E ) | id`,
2. Runs Tomita's breadth-first GLR algorithm over a Graph-Structured Stack (GSS) that
   forks on conflicts and merges heads reaching identical LR states at the same input offset,
3. Records all derivations in a compact Shared Packed Parse Forest (SPPF) keyed by
   `(symbol, start, end)`, sharing common sub-derivations instead of copying trees,
4. Enumerates all Catalan `C_n` distinct parse trees cleanly (`id + id + id + id` -> 5 trees).
"""

from __future__ import annotations

from collections import deque
from dataclasses import dataclass
import unittest


@dataclass(frozen=True)
class LrProd:
    lhs: str
    rhs: tuple[str, ...]


class GlrTable:
    def __init__(self) -> None:
        self.prods = [
            LrProd("S'", ("E",)),
            LrProd("E", ("E", "+", "E")),
            LrProd("E", ("E", "*", "E")),
            LrProd("E", ("(", "E", ")")),
            LrProd("E", ("id",)),
        ]
        nts = {p.lhs for p in self.prods}
        all_syms = set(nts)
        for p in self.prods:
            all_syms.update(p.rhs)
        terminals = sorted((all_syms - nts) | {"$"})

        first: dict[str, set[str]] = {n: set() for n in nts}
        ch = True
        while ch:
            ch = False
            for p in self.prods:
                s = p.rhs[0]
                if s not in nts:
                    if s not in first[p.lhs]:
                        first[p.lhs].add(s)
                        ch = True
                else:
                    before = len(first[p.lhs])
                    first[p.lhs].update(first[s])
                    if len(first[p.lhs]) != before:
                        ch = True

        follow: dict[str, set[str]] = {n: set() for n in nts}
        follow["S'"].add("$")
        ch = True
        while ch:
            ch = False
            for p in self.prods:
                trailer = set(follow[p.lhs])
                for s in reversed(p.rhs):
                    if s not in nts:
                        trailer = {s}
                    else:
                        before = len(follow[s])
                        follow[s].update(trailer)
                        if len(follow[s]) != before:
                            ch = True
                        trailer = set(first[s])

        def closure(seed: frozenset[tuple[int, int]]) -> frozenset[tuple[int, int]]:
            cur = set(seed)
            changed = True
            while changed:
                changed = False
                for pid, dot in list(cur):
                    if dot < len(self.prods[pid].rhs):
                        sym = self.prods[pid].rhs[dot]
                        if sym in nts:
                            for npid, np in enumerate(self.prods):
                                if np.lhs == sym and (npid, 0) not in cur:
                                    cur.add((npid, 0))
                                    changed = True
            return frozenset(cur)

        states: list[frozenset[tuple[int, int]]] = []
        state_id: dict[frozenset[tuple[int, int]], int] = {}
        trans: dict[tuple[int, str], int] = {}
        s0 = closure(frozenset({(0, 0)}))
        state_id[s0] = 0
        states.append(s0)

        idx = 0
        while idx < len(states):
            cur = states[idx]
            for sym in sorted(all_syms):
                seed = frozenset(
                    (pid, dot + 1)
                    for pid, dot in cur
                    if dot < len(self.prods[pid].rhs) and self.prods[pid].rhs[dot] == sym
                )
                if not seed:
                    continue
                tgt = closure(seed)
                if tgt not in state_id:
                    state_id[tgt] = len(states)
                    states.append(tgt)
                trans[(idx, sym)] = state_id[tgt]
            idx += 1

        action_sets: dict[tuple[int, str], set[tuple[str, int]]] = {}
        self.goto: dict[tuple[int, str], int] = {}
        for sid, items in enumerate(states):
            for term in terminals:
                if (sid, term) in trans:
                    action_sets.setdefault((sid, term), set()).add(("shift", trans[(sid, term)]))
            for nt in sorted(nts):
                if (sid, nt) in trans:
                    self.goto[(sid, nt)] = trans[(sid, nt)]
            for pid, dot in items:
                if dot == len(self.prods[pid].rhs):
                    if pid == 0:
                        action_sets.setdefault((sid, "$"), set()).add(("accept", 0))
                    else:
                        for f in follow[self.prods[pid].lhs]:
                            action_sets.setdefault((sid, f), set()).add(("reduce", pid))
        self.action: dict[tuple[int, str], list[tuple[str, int]]] = {
            k: sorted(v) for k, v in action_sets.items()
        }


@dataclass
class SpfNode:
    start: int
    end: int
    is_term: bool
    label: str
    packs: set[tuple[int, ...]]


@dataclass
class GssNode:
    state: int
    pos: int
    parents: list[tuple[int, int]]


def tokenize_glr(src: str) -> list[tuple[str, str]]:
    i = 0
    n = len(src)
    out: list[tuple[str, str]] = []
    while i < n:
        if src[i].isspace():
            i += 1
            continue
        if src[i].isalpha() or src[i] == "_":
            start = i
            i += 1
            while i < n and (src[i].isalnum() or src[i] == "_"):
                i += 1
            out.append(("id", src[start:i]))
            continue
        if src[i] in ("+", "*", "(", ")"):
            out.append((src[i], src[i]))
            i += 1
            continue
        raise ValueError(f"unexpected character '{src[i]}'")
    out.append(("$", "$"))
    return out


def glr_parse(src: str, table: GlrTable) -> tuple[int | None, list[SpfNode], int, int, int]:
    tokens = tokenize_glr(src)
    sppf: list[SpfNode] = []
    nt_index: dict[tuple[str, int, int], int] = {}
    term_index: dict[tuple[str, int, int], int] = {}

    def get_nt(label: str, start: int, end: int, pack: tuple[int, ...]) -> int:
        key = (label, start, end)
        if key not in nt_index:
            nt_index[key] = len(sppf)
            sppf.append(SpfNode(start, end, False, label, set()))
        nid = nt_index[key]
        sppf[nid].packs.add(pack)
        return nid

    def get_term(lex: str, start: int, end: int) -> int:
        key = (lex, start, end)
        if key not in term_index:
            term_index[key] = len(sppf)
            sppf.append(SpfNode(start, end, True, lex, set()))
        return term_index[key]

    gss: list[GssNode] = [GssNode(0, 0, [])]
    gss_merges = 0
    conflict_forks = 0
    frontier = [0]
    root_sppf: int | None = None

    def collect_paths(node_id: int, remaining: int) -> list[tuple[int, list[int]]]:
        if remaining == 0:
            return [(node_id, [])]
        out: list[tuple[int, list[int]]] = []
        for parent, edge_spf in gss[node_id].parents:
            for anc, suffix in collect_paths(parent, remaining - 1):
                out.append((anc, [*suffix, edge_spf]))
        return out

    for pos, (tok_kind, tok_lex) in enumerate(tokens):
        if not frontier:
            raise ValueError(f"syntax error near '{tok_lex}' at token {pos}")
        at_pos = {gss[gid].state: gid for gid in frontier}
        reduce_q = deque(frontier)
        queued: set[tuple[int, int, int, tuple[int, ...]]] = set()

        for gid in frontier:
            acts = table.action.get((gss[gid].state, tok_kind), [])
            if len(acts) > 1:
                conflict_forks += len(acts) - 1

        while reduce_q:
            gid = reduce_q.popleft()
            acts = table.action.get((gss[gid].state, tok_kind), [])
            for kind, arg in acts:
                if kind == "accept":
                    if gss[gid].parents:
                        root_sppf = gss[gid].parents[0][1]
                elif kind == "reduce":
                    prod = table.prods[arg]
                    for anc_id, children_spf in collect_paths(gid, len(prod.rhs)):
                        c_tup = tuple(children_spf)
                        if (gid, arg, anc_id, c_tup) in queued:
                            continue
                        queued.add((gid, arg, anc_id, c_tup))
                        next_state = table.goto[(gss[anc_id].state, prod.lhs)]
                        spf_id = get_nt(prod.lhs, gss[anc_id].pos, pos, c_tup)
                        if next_state in at_pos:
                            egid = at_pos[next_state]
                            if (anc_id, spf_id) not in gss[egid].parents:
                                gss[egid].parents.append((anc_id, spf_id))
                                gss_merges += 1
                                reduce_q.append(egid)
                                reduce_q.extend(at_pos.values())
                        else:
                            ngid = len(gss)
                            gss.append(GssNode(next_state, pos, [(anc_id, spf_id)]))
                            at_pos[next_state] = ngid
                            reduce_q.append(ngid)

        if tok_kind == "$":
            break

        next_at_pos: dict[int, int] = {}
        next_frontier: list[int] = []
        leaf_spf = get_term(tok_lex, pos, pos + 1)
        for gid in list(at_pos.values()):
            for kind, next_state in table.action.get((gss[gid].state, tok_kind), []):
                if kind == "shift":
                    if next_state in next_at_pos:
                        egid = next_at_pos[next_state]
                        if (gid, leaf_spf) not in gss[egid].parents:
                            gss[egid].parents.append((gid, leaf_spf))
                            gss_merges += 1
                    else:
                        ngid = len(gss)
                        gss.append(GssNode(next_state, pos + 1, [(gid, leaf_spf)]))
                        next_at_pos[next_state] = ngid
                        next_frontier.append(ngid)
        frontier = next_frontier

    return root_sppf, sppf, len(gss), gss_merges, conflict_forks


def enumerate_sppf_exprs(sppf: list[SpfNode], node_id: int) -> list[str]:
    node = sppf[node_id]
    if node.is_term:
        return [node.label]
    out: set[str] = set()
    for children in node.packs:
        if len(children) == 1:
            out.update(enumerate_sppf_exprs(sppf, children[0]))
        elif len(children) == 3:
            mid = sppf[children[1]].label
            if mid in ("+", "*"):
                for l in enumerate_sppf_exprs(sppf, children[0]):
                    for r in enumerate_sppf_exprs(sppf, children[2]):
                        out.add(f"({l} {mid} {r})")
            else:
                for inner in enumerate_sppf_exprs(sppf, children[1]):
                    out.add(f"({inner})")
    return sorted(out)


def main() -> None:
    table = GlrTable()
    conflict_cells = sum(1 for v in table.action.values() if len(v) > 1)
    print("=== Advanced Parsing (Modern): Tomita GLR Parsing, GSS & SPPF Forests ===")
    print(f"Ambiguous LR grammar `E -> E + E | E * E | ( E ) | id` has {conflict_cells} S/R conflict cells (kept intact!).")
    for sample in ("id + id * id", "id + id + id + id"):
        root, sppf, gss_n, merges, forks = glr_parse(sample, table)
        assert root is not None
        trees = enumerate_sppf_exprs(sppf, root)
        print(
            f"\nInput: {sample!r}\n  SPPF nodes = {len(sppf)}, GSS nodes = {gss_n}, "
            f"merges = {merges}, conflict forks = {forks}, unpacked ASTs = {len(trees)}"
        )
        for i, t in enumerate(trees, 1):
            print(f"    AST #{i}: {t}")


class GlrParsingTests(unittest.TestCase):
    def test_glr_parses_ambiguous_precedence_into_two_sppf_trees(self) -> None:
        table = GlrTable()
        root, sppf, _, _, _ = glr_parse("a + b * c", table)
        assert root is not None
        self.assertEqual(enumerate_sppf_exprs(sppf, root), ["((a + b) * c)", "(a + (b * c))"])

    def test_glr_produces_catalan_number_of_trees_with_sppf_sharing(self) -> None:
        table = GlrTable()
        root4, sppf4, _, merges4, _ = glr_parse("a + b + c + d", table)
        assert root4 is not None
        self.assertEqual(len(enumerate_sppf_exprs(sppf4, root4)), 5)
        self.assertGreater(merges4, 0)

        root5, sppf5, _, _, _ = glr_parse("a + b + c + d + e", table)
        assert root5 is not None
        self.assertEqual(len(enumerate_sppf_exprs(sppf5, root5)), 14)
        self.assertLess(len(sppf5), 40)

    def test_glr_rejects_unmatched_tokens(self) -> None:
        table = GlrTable()
        root, _, _, _, _ = glr_parse("(a + b", table)
        self.assertIsNone(root)
        with self.assertRaises(ValueError):
            glr_parse("a + * b", table)


if __name__ == "__main__":
    main()
