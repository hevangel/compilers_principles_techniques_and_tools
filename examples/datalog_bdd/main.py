"""Evaluate semi-naive Datalog points-to rules and solve relational fixed points with an ROBDD."""

from dataclasses import dataclass
import unittest


@dataclass(frozen=True)
class Term:
    is_var: bool
    val: int


@dataclass(frozen=True)
class Atom:
    pred: str
    terms: tuple[Term, ...]


@dataclass(frozen=True)
class Rule:
    head: Atom
    body: tuple[Atom, ...]


def v(idx: int) -> Term:
    return Term(True, idx)


def match_atom(atom: Atom, tup: tuple[int, ...], env: dict[int, int]) -> bool:
    if len(atom.terms) != len(tup):
        return False
    added: list[int] = []
    for term, val in zip(atom.terms, tup):
        if not term.is_var:
            if term.val != val:
                for k in added:
                    del env[k]
                return False
        else:
            if term.val in env:
                if env[term.val] != val:
                    for k in added:
                        del env[k]
                    return False
            else:
                env[term.val] = val
                added.append(term.val)
    return True


def instantiate_head(head: Atom, env: dict[int, int]) -> tuple[int, ...]:
    return tuple(env[t.val] if t.is_var else t.val for t in head.terms)


def join_rule_slot(
    rule: Rule,
    delta_slot: int | None,
    db: dict[str, set[tuple[int, ...]]],
    deltas: dict[str, set[tuple[int, ...]]],
    out_tuples: set[tuple[int, ...]],
    counts: list[int],
) -> None:
    def dfs(idx: int, env: dict[int, int]) -> None:
        if idx == len(rule.body):
            counts[0] += 1
            out_tuples.add(instantiate_head(rule.head, env))
            return
        atom = rule.body[idx]
        source = deltas if delta_slot == idx else db
        for tup in sorted(source.get(atom.pred, set())):
            snap = dict(env)
            if match_atom(atom, tup, env):
                dfs(idx + 1, env)
            env.clear()
            env.update(snap)

    dfs(0, {})


def evaluate_naive(
    edb: dict[str, set[tuple[int, ...]]], rules: list[Rule]
) -> tuple[dict[str, set[tuple[int, ...]]], tuple[int, int]]:
    db = {k: set(val) for k, val in edb.items()}
    for r in rules:
        db.setdefault(r.head.pred, set())
    rounds = 0
    counts = [0]
    while True:
        rounds += 1
        new_facts: dict[str, set[tuple[int, ...]]] = {}
        for rule in rules:
            derived: set[tuple[int, ...]] = set()
            join_rule_slot(rule, None, db, db, derived, counts)
            new_facts.setdefault(rule.head.pred, set()).update(derived)
        changed = False
        for pred, tuples in new_facts.items():
            before = len(db[pred])
            db[pred].update(tuples)
            if len(db[pred]) != before:
                changed = True
        if not changed:
            return db, (rounds, counts[0])


def evaluate_semi_naive(
    edb: dict[str, set[tuple[int, ...]]], rules: list[Rule]
) -> tuple[dict[str, set[tuple[int, ...]]], tuple[int, int]]:
    db = {k: set(val) for k, val in edb.items()}
    idb_preds = {r.head.pred for r in rules}
    for p in idb_preds:
        db.setdefault(p, set())
    rounds = 1
    counts = [0]
    deltas: dict[str, set[tuple[int, ...]]] = {}
    for rule in rules:
        if all(a.pred not in idb_preds for a in rule.body):
            derived: set[tuple[int, ...]] = set()
            join_rule_slot(rule, None, db, db, derived, counts)
            deltas.setdefault(rule.head.pred, set()).update(derived - db[rule.head.pred])
    for pred, tuples in deltas.items():
        db[pred].update(tuples)

    while any(deltas.values()):
        rounds += 1
        next_deltas: dict[str, set[tuple[int, ...]]] = {}
        for rule in rules:
            derived: set[tuple[int, ...]] = set()
            for slot, atom in enumerate(rule.body):
                if atom.pred in idb_preds and deltas.get(atom.pred):
                    join_rule_slot(rule, slot, db, deltas, derived, counts)
            diff = derived - db[rule.head.pred]
            if diff:
                next_deltas.setdefault(rule.head.pred, set()).update(diff)
        for pred, tuples in next_deltas.items():
            db[pred].update(tuples)
        deltas = next_deltas
    return db, (rounds, counts[0])


class Robdd:
    ZERO = 0
    ONE = 1

    def __init__(self) -> None:
        self.nodes: list[tuple[int, int, int] | None] = [None, None]
        self.unique: dict[tuple[int, int, int], int] = {}
        self.cache: dict[tuple[str, int, int], int] = {}

    def mk(self, var: int, low: int, high: int) -> int:
        if low == high:
            return low
        key = (var, low, high)
        if key in self.unique:
            return self.unique[key]
        idx = len(self.nodes)
        self.nodes.append(key)
        self.unique[key] = idx
        return idx

    def var(self, var_id: int) -> int:
        return self.mk(var_id, self.ZERO, self.ONE)

    def neg(self, u: int) -> int:
        return self.apply("xor", u, self.ONE)

    def ite(self, c: int, t: int, e: int) -> int:
        return self.apply("or", self.apply("and", c, t), self.apply("and", self.neg(c), e))

    def _var_of(self, u: int) -> int:
        node = self.nodes[u]
        return 10**9 if node is None else node[0]

    def _step(self, u: int, top: int, val: bool) -> int:
        node = self.nodes[u]
        if node is not None and node[0] == top:
            return node[2] if val else node[1]
        return u

    def apply(self, op: str, u: int, v_node: int) -> int:
        if u <= 1 and v_node <= 1:
            bu, bv = u == 1, v_node == 1
            res = (bu and bv) if op == "and" else (bu or bv) if op == "or" else (bu ^ bv)
            return self.ONE if res else self.ZERO
        key = (op, min(u, v_node), max(u, v_node))
        if key in self.cache:
            return self.cache[key]
        top = min(self._var_of(u), self._var_of(v_node))
        low = self.apply(op, self._step(u, top, False), self._step(v_node, top, False))
        high = self.apply(op, self._step(u, top, True), self._step(v_node, top, True))
        res = self.mk(top, low, high)
        self.cache[key] = res
        return res

    def restrict(self, u: int, target: int, val: bool) -> int:
        node = self.nodes[u]
        if node is None or node[0] > target:
            return u
        var, low, high = node
        if var == target:
            return high if val else low
        return self.mk(var, self.restrict(low, target, val), self.restrict(high, target, val))

    def exists(self, var_ids: list[int], u: int) -> int:
        for vid in var_ids:
            u = self.apply("or", self.restrict(u, vid, False), self.restrict(u, vid, True))
        return u

    def substitute(self, u: int, mapping: dict[int, int]) -> int:
        node = self.nodes[u]
        if node is None:
            return u
        var, low, high = node
        nl = self.substitute(low, mapping)
        nh = self.substitute(high, mapping)
        return self.ite(self.var(mapping.get(var, var)), nh, nl)

    def encode_pair(self, x_vars: list[int], y_vars: list[int], x: int, y: int) -> int:
        acc = self.ONE
        for bit, vx in enumerate(x_vars):
            lit = self.var(vx)
            acc = self.apply("and", acc, lit if (x >> bit) & 1 else self.neg(lit))
        for bit, vy in enumerate(y_vars):
            lit = self.var(vy)
            acc = self.apply("and", acc, lit if (y >> bit) & 1 else self.neg(lit))
        return acc

    def decode_pairs(self, u: int, x_vars: list[int], y_vars: list[int]) -> set[tuple[int, int]]:
        dom = 1 << len(x_vars)
        out: set[tuple[int, int]] = set()
        for x in range(dom):
            for y in range(dom):
                cur = u
                while self.nodes[cur] is not None:
                    var, low, high = self.nodes[cur]
                    if var in x_vars:
                        bit_on = bool((x >> x_vars.index(var)) & 1)
                    elif var in y_vars:
                        bit_on = bool((y >> y_vars.index(var)) & 1)
                    else:
                        bit_on = False
                    cur = high if bit_on else low
                if cur == self.ONE:
                    out.add((x, y))
        return out


def points_to_example() -> tuple[dict[str, set[tuple[int, ...]]], list[Rule]]:
    edb = {
        "Alloc": {(1, 10), (2, 11)},
        "Move": {(3, 1), (4, 3)},
        "Store": {(1, 100, 2)},
        "Load": {(5, 4, 100)},
        "Edge": {(0, 1), (1, 2), (2, 3), (3, 4), (1, 4)},
    }
    rules = [
        Rule(Atom("Pts", (v(0), v(1))), (Atom("Alloc", (v(0), v(1))),)),
        Rule(
            Atom("Pts", (v(0), v(1))),
            (Atom("Move", (v(0), v(2))), Atom("Pts", (v(2), v(1)))),
        ),
        Rule(
            Atom("HeapPts", (v(0), v(1), v(2))),
            (
                Atom("Store", (v(3), v(1), v(4))),
                Atom("Pts", (v(3), v(0))),
                Atom("Pts", (v(4), v(2))),
            ),
        ),
        Rule(
            Atom("Pts", (v(0), v(1))),
            (
                Atom("Load", (v(0), v(2), v(3))),
                Atom("Pts", (v(2), v(4))),
                Atom("HeapPts", (v(4), v(3), v(1))),
            ),
        ),
        Rule(Atom("Path", (v(0), v(1))), (Atom("Edge", (v(0), v(1))),)),
        Rule(
            Atom("Path", (v(0), v(2))),
            (Atom("Edge", (v(0), v(1))), Atom("Path", (v(1), v(2)))),
        ),
        Rule(
            Atom("ReachFrom0", (v(0),)),
            (Atom("Path", (Term(False, 0), v(0))),),
        ),
    ]
    return edb, rules


def solve_transitive_closure_bdd(
    edges: set[tuple[int, int]]
) -> tuple[Robdd, int, set[tuple[int, int]]]:
    bdd = Robdd()
    x_vars, y_vars, z_vars = [0, 1, 2], [3, 4, 5], [6, 7, 8]
    edge_bdd = bdd.ZERO
    for u, val in sorted(edges):
        edge_bdd = bdd.apply("or", edge_bdd, bdd.encode_pair(x_vars, y_vars, u, val))

    y_to_z = dict(zip(y_vars, z_vars))
    x_to_z = dict(zip(x_vars, z_vars))
    path_xy = edge_bdd
    edge_xz = bdd.substitute(edge_bdd, y_to_z)
    while True:
        path_zy = bdd.substitute(path_xy, x_to_z)
        composed = bdd.exists(z_vars, bdd.apply("and", edge_xz, path_zy))
        next_xy = bdd.apply("or", path_xy, composed)
        if next_xy == path_xy:
            return bdd, path_xy, bdd.decode_pairs(path_xy, x_vars, y_vars)
        path_xy = next_xy


def main() -> None:
    edb, rules = points_to_example()
    naive_db, naive_stats = evaluate_naive(edb, rules)
    semi_db, semi_stats = evaluate_semi_naive(edb, rules)
    print(
        f"Datalog points-to + closure: naive={naive_stats}, "
        f"semi_naive={semi_stats}, exact_match={naive_db == semi_db}"
    )
    print(f"  Pts={sorted(semi_db['Pts'])}")

    bdd, root, bdd_paths = solve_transitive_closure_bdd(edb["Edge"])
    dl_paths = set(semi_db["Path"])
    a, b = bdd.var(0), bdd.var(1)
    ab, ba = bdd.apply("and", a, b), bdd.apply("and", b, a)
    print(
        f"ROBDD canonical (a&b == b&a): {ab == ba} (id={ab}), "
        f"Path root={root}, BDD==Datalog: {bdd_paths == dl_paths}"
    )


class DatalogBddTests(unittest.TestCase):
    def test_semi_naive_matches_naive_with_fewer_derivations(self) -> None:
        edb, rules = points_to_example()
        naive_db, naive_stats = evaluate_naive(edb, rules)
        semi_db, semi_stats = evaluate_semi_naive(edb, rules)
        self.assertEqual(naive_db, semi_db)
        self.assertLess(semi_stats[1], naive_stats[1])
        self.assertIn((5, 11), semi_db["Pts"])

    def test_robdd_canonical_and_closure_parity(self) -> None:
        bdd = Robdd()
        a, b = bdd.var(0), bdd.var(1)
        self.assertEqual(bdd.apply("and", a, b), bdd.apply("and", b, a))
        lhs = bdd.apply("and", a, b)
        rhs = bdd.apply("and", bdd.neg(a), b)
        self.assertEqual(bdd.apply("or", lhs, rhs), b)

        edb, rules = points_to_example()
        dl_db, _ = evaluate_semi_naive(edb, rules)
        _, _, bdd_paths = solve_transitive_closure_bdd(edb["Edge"])
        self.assertEqual(bdd_paths, set(dl_db["Path"]))


if __name__ == "__main__":
    main()
