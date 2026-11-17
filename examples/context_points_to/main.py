"""Solve k-CFA call-string points-to analysis with on-the-fly virtual dispatch discovery."""

from dataclasses import dataclass
import unittest


@dataclass(frozen=True)
class HeapSite:
    label: str
    class_type: str


def push_call_string(ctx: tuple[str, ...], site: str, k: int) -> tuple[str, ...]:
    if k == 0:
        return ()
    nxt = ctx + (site,)
    return nxt[-k:]


def solve_k_cfa(program: dict, k: int) -> dict:
    pts: dict[tuple[str, tuple[str, ...], str], set[HeapSite]] = {}
    reachable: set[tuple[str, tuple[str, ...]]] = {(program["entry"], ())}
    call_edges: set[tuple[str, tuple[str, ...], str, str, tuple[str, ...]]] = set()

    def get_pts(key: tuple[str, tuple[str, ...], str]) -> set[HeapSite]:
        return pts.setdefault(key, set())

    while True:
        changed = False
        for func_name, ctx in sorted(reachable):
            func = program["functions"][func_name]
            for stmt in func["stmts"]:
                kind = stmt[0]
                if kind == "alloc":
                    _, dst, site = stmt
                    s = get_pts((func_name, ctx, dst))
                    if site not in s:
                        s.add(site)
                        changed = True
                elif kind == "assign":
                    _, dst, src = stmt
                    src_sites = set(pts.get((func_name, ctx, src), set()))
                    d = get_pts((func_name, ctx, dst))
                    before = len(d)
                    d.update(src_sites)
                    changed |= len(d) != before
                elif kind == "call":
                    _, site, callee, args, dst = stmt
                    c_ctx = push_call_string(ctx, site, k)
                    if (callee, c_ctx) not in reachable:
                        reachable.add((callee, c_ctx))
                        changed = True
                    edge = (func_name, ctx, site, callee, c_ctx)
                    if edge not in call_edges:
                        call_edges.add(edge)
                        changed = True
                    callee_fn = program["functions"][callee]
                    for arg, param in zip(args, callee_fn["params"]):
                        p_set = get_pts((callee, c_ctx, param))
                        before = len(p_set)
                        p_set.update(pts.get((func_name, ctx, arg), set()))
                        changed |= len(p_set) != before
                    d_set = get_pts((func_name, ctx, dst))
                    before = len(d_set)
                    d_set.update(pts.get((callee, c_ctx, "$ret"), set()))
                    changed |= len(d_set) != before
                elif kind == "vcall":
                    _, site, receiver, method, args, dst = stmt
                    recv_sites = set(pts.get((func_name, ctx, receiver), set()))
                    c_ctx = push_call_string(ctx, site, k)
                    for obj in sorted(recv_sites, key=lambda h: h.label):
                        callee = program["vtable"].get((obj.class_type, method))
                        if callee is None:
                            continue
                        if (callee, c_ctx) not in reachable:
                            reachable.add((callee, c_ctx))
                            changed = True
                        edge = (func_name, ctx, site, callee, c_ctx)
                        if edge not in call_edges:
                            call_edges.add(edge)
                            changed = True
                        callee_fn = program["functions"][callee]
                        if callee_fn["params"]:
                            p0 = get_pts((callee, c_ctx, callee_fn["params"][0]))
                            if obj not in p0:
                                p0.add(obj)
                                changed = True
                        for arg, param in zip(args, callee_fn["params"][1:]):
                            p_set = get_pts((callee, c_ctx, param))
                            before = len(p_set)
                            p_set.update(pts.get((func_name, ctx, arg), set()))
                            changed |= len(p_set) != before
                        d_set = get_pts((func_name, ctx, dst))
                        before = len(d_set)
                        d_set.update(pts.get((callee, c_ctx, "$ret"), set()))
                        changed |= len(d_set) != before
                elif kind == "return":
                    _, src = stmt
                    r_set = get_pts((func_name, ctx, "$ret"))
                    before = len(r_set)
                    r_set.update(pts.get((func_name, ctx, src), set()))
                    changed |= len(r_set) != before
        if not changed:
            return {"pts": pts, "call_edges": call_edges, "reachable": reachable}


def merged_var(res: dict, func: str, var: str) -> set[str]:
    out: set[str] = set()
    for (f, _ctx, v), sites in res["pts"].items():
        if f == func and v == var:
            out.update(s.label for s in sites)
    return out


def relay_program() -> dict:
    return {
        "entry": "main",
        "vtable": {},
        "functions": {
            "main": {
                "params": [],
                "stmts": [
                    ("alloc", "a", HeapSite("alloc_A", "A")),
                    ("alloc", "b", HeapSite("alloc_B", "B")),
                    ("call", "c1", "relay", ["a"], "x"),
                    ("call", "c2", "relay", ["b"], "y"),
                ],
            },
            "relay": {
                "params": ["p"],
                "stmts": [("assign", "t", "p"), ("return", "t")],
            },
        },
    }


def virtual_dispatch_and_recursion_program() -> dict:
    return {
        "entry": "main",
        "vtable": {
            ("SubA", "apply"): "SubA::apply",
            ("SubB", "apply"): "SubB::apply",
        },
        "functions": {
            "main": {
                "params": [],
                "stmts": [
                    ("alloc", "obj_a", HeapSite("h_sub_a", "SubA")),
                    ("alloc", "payload", HeapSite("h_payload", "Data")),
                    ("vcall", "v1", "obj_a", "apply", ["payload"], "out_a"),
                    ("call", "r0", "rec", ["payload"], "out_rec"),
                ],
            },
            "SubA::apply": {"params": ["this", "arg"], "stmts": [("return", "arg")]},
            "SubB::apply": {"params": ["this", "arg"], "stmts": [("return", "this")]},
            "rec": {
                "params": ["n"],
                "stmts": [
                    ("call", "r_self", "rec", ["n"], "tmp"),
                    ("return", "n"),
                ],
            },
        },
    }


def main() -> None:
    relay = relay_program()
    for k in (0, 1):
        res = solve_k_cfa(relay, k)
        n_ctx = sum(1 for f, _ in res["reachable"] if f == "relay")
        print(
            f"relay with k={k}: x -> {sorted(merged_var(res, 'main', 'x'))}, "
            f"y -> {sorted(merged_var(res, 'main', 'y'))}, relay contexts={n_ctx}"
        )

    dyn_res = solve_k_cfa(virtual_dispatch_and_recursion_program(), 2)
    callees = sorted({e[3] for e in dyn_res["call_edges"]})
    rec_ctxs = sorted(c for f, c in dyn_res["reachable"] if f == "rec")
    print(
        f"k=2 dynamic + recursive: discovered callees={callees}, "
        f"out_a={sorted(merged_var(dyn_res, 'main', 'out_a'))}, rec_contexts={rec_ctxs}"
    )


class ContextPointsToTests(unittest.TestCase):
    def test_relay_k0_vs_k1(self) -> None:
        prog = relay_program()
        k0 = solve_k_cfa(prog, 0)
        self.assertEqual(merged_var(k0, "main", "x"), {"alloc_A", "alloc_B"})
        self.assertEqual(merged_var(k0, "main", "y"), {"alloc_A", "alloc_B"})

        k1 = solve_k_cfa(prog, 1)
        self.assertEqual(merged_var(k1, "main", "x"), {"alloc_A"})
        self.assertEqual(merged_var(k1, "main", "y"), {"alloc_B"})

    def test_virtual_dispatch_and_k2_recursion(self) -> None:
        res = solve_k_cfa(virtual_dispatch_and_recursion_program(), 2)
        self.assertEqual(merged_var(res, "main", "out_a"), {"h_payload"})
        self.assertEqual(merged_var(res, "main", "out_rec"), {"h_payload"})
        callees = {e[3] for e in res["call_edges"]}
        self.assertIn("SubA::apply", callees)
        self.assertNotIn("SubB::apply", callees)
        rec_ctxs = {c for f, c in res["reachable"] if f == "rec"}
        self.assertEqual(rec_ctxs, {("r0",), ("r0", "r_self"), ("r_self", "r_self")})


if __name__ == "__main__":
    main()
