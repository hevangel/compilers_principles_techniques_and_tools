"""Build expression DAGs via value-number hash-consing, algebraic folding, and alias kills."""

from dataclasses import dataclass
from enum import Enum
import unittest

I64_MASK = (1 << 64) - 1


def wrap_i64(value: int) -> int:
    masked = value & I64_MASK
    return masked - (1 << 64) if masked >= (1 << 63) else masked


class Op(Enum):
    ADD = "Add"
    SUB = "Sub"
    MUL = "Mul"


@dataclass(frozen=True)
class Var:
    name: str


@dataclass(frozen=True)
class Const:
    val: int


@dataclass(frozen=True)
class Bin:
    op: Op
    left: "ExprAst"
    right: "ExprAst"


ExprAst = Var | Const | Bin


@dataclass(frozen=True)
class Assign:
    dst: str
    rhs: ExprAst


@dataclass(frozen=True)
class ArrayRead:
    dst: str
    array: str
    index: ExprAst


@dataclass(frozen=True)
class ArrayWrite:
    array: str
    index: ExprAst
    value: ExprAst


@dataclass(frozen=True)
class PtrKill:
    pass


BbStmt = Assign | ArrayRead | ArrayWrite | PtrKill


class DagBuilder:
    def __init__(self) -> None:
        self.nodes: list[tuple[object, ...]] = []
        self.table: dict[tuple[object, ...], int] = {}
        self.env: dict[str, int] = {}
        self.array_epoch: dict[str, int] = {}
        self.global_epoch = 0

    def intern(self, key: tuple[object, ...]) -> int:
        if key in self.table:
            return self.table[key]
        idx = len(self.nodes)
        self.nodes.append(key)
        self.table[key] = idx
        return idx

    def leaf(self, name: str) -> int:
        if name in self.env:
            return self.env[name]
        idx = self.intern(("Leaf", name))
        self.env[name] = idx
        return idx

    def lower_ast(self, ast: ExprAst) -> int:
        if isinstance(ast, Var):
            return self.leaf(ast.name)
        if isinstance(ast, Const):
            return self.intern(("Const", ast.val))
        left = self.lower_ast(ast.left)
        right = self.lower_ast(ast.right)
        return self.make_bin(ast.op, left, right)

    def make_bin(self, op: Op, left: int, right: int) -> int:
        lnode = self.nodes[left]
        rnode = self.nodes[right]
        if lnode[0] == "Const" and rnode[0] == "Const":
            a, b = int(lnode[1]), int(rnode[1])  # type: ignore[arg-type]
            if op == Op.ADD:
                return self.intern(("Const", wrap_i64(a + b)))
            if op == Op.SUB:
                return self.intern(("Const", wrap_i64(a - b)))
            return self.intern(("Const", wrap_i64(a * b)))

        if op == Op.ADD:
            if lnode == ("Const", 0):
                return right
            if rnode == ("Const", 0):
                return left
            if left > right:
                left, right = right, left
        elif op == Op.MUL:
            if lnode == ("Const", 0) or rnode == ("Const", 0):
                return self.intern(("Const", 0))
            if lnode == ("Const", 1):
                return right
            if rnode == ("Const", 1):
                return left
            if left > right:
                left, right = right, left
        elif op == Op.SUB:
            if rnode == ("Const", 0):
                return left
            if left == right:
                return self.intern(("Const", 0))

        return self.intern(("Bin", op, left, right))

    def _materialize(
        self,
        node_id: int,
        computed: dict[int, str],
        counter: list[int],
        tac: list[tuple[object, ...]],
    ) -> None:
        if node_id in computed:
            return
        node = self.nodes[node_id]
        tag = node[0]
        if tag in ("Leaf", "ArrayLoad"):
            return
        if tag == "Const":
            t = f"t{counter[0]}"
            counter[0] += 1
            tac.append(("LoadConst", t, node[1]))
            computed[node_id] = t
            return
        assert tag == "Bin"
        _, op, left, right = node
        assert isinstance(left, int) and isinstance(right, int)
        self._materialize(left, computed, counter, tac)
        self._materialize(right, computed, counter, tac)
        ln = self.nodes[left][1] if self.nodes[left][0] == "Leaf" else computed[left]
        rn = self.nodes[right][1] if self.nodes[right][0] == "Leaf" else computed[right]
        t = f"t{counter[0]}"
        counter[0] += 1
        tac.append(("Bin", t, op, ln, rn))
        computed[node_id] = t

    def _name_of(
        self,
        node_id: int,
        computed: dict[int, str],
        counter: list[int],
        tac: list[tuple[object, ...]],
    ) -> str:
        if node_id in computed:
            return computed[node_id]
        node = self.nodes[node_id]
        if node[0] == "Leaf":
            return str(node[1])
        assert node[0] == "Const"
        t = f"t{counter[0]}"
        counter[0] += 1
        tac.append(("LoadConst", t, node[1]))
        computed[node_id] = t
        return t

    def process_block(self, stmts: list[BbStmt]) -> list[tuple[object, ...]]:
        tac: list[tuple[object, ...]] = []
        computed: dict[int, str] = {}
        counter = [0]
        for stmt in stmts:
            if isinstance(stmt, Assign):
                nid = self.lower_ast(stmt.rhs)
                self._materialize(nid, computed, counter, tac)
                src = self._name_of(nid, computed, counter, tac)
                if src != stmt.dst:
                    tac.append(("Copy", stmt.dst, src))
                self.env[stmt.dst] = nid
            elif isinstance(stmt, ArrayRead):
                idx_id = self.lower_ast(stmt.index)
                self._materialize(idx_id, computed, counter, tac)
                epoch = self.array_epoch.get(stmt.array, self.global_epoch)
                key = ("ArrayLoad", stmt.array, idx_id, epoch)
                existed = key in self.table
                load_id = self.intern(key)
                if not existed:
                    idx_name = self._name_of(idx_id, computed, counter, tac)
                    tac.append(("ArrayLoad", stmt.dst, stmt.array, idx_name))
                    computed[load_id] = stmt.dst
                else:
                    tac.append(("Copy", stmt.dst, computed[load_id]))
                self.env[stmt.dst] = load_id
            elif isinstance(stmt, ArrayWrite):
                idx_id = self.lower_ast(stmt.index)
                val_id = self.lower_ast(stmt.value)
                self._materialize(idx_id, computed, counter, tac)
                self._materialize(val_id, computed, counter, tac)
                idx_name = self._name_of(idx_id, computed, counter, tac)
                val_name = self._name_of(val_id, computed, counter, tac)
                tac.append(("ArrayStore", stmt.array, idx_name, val_name))
                self.array_epoch[stmt.array] = self.array_epoch.get(stmt.array, self.global_epoch) + 1
            else:
                self.global_epoch += 1
                self.array_epoch.clear()
        return tac


def main() -> None:
    expr = Bin(
        Op.ADD,
        Bin(Op.ADD, Var("a"), Var("b")),
        Bin(Op.MUL, Bin(Op.ADD, Var("a"), Var("b")), Bin(Op.ADD, Var("b"), Var("a"))),
    )
    dag = DagBuilder()
    root = dag.lower_ast(expr)
    print(f"DAG nodes for (a+b) + (a+b)*(b+a) (root=#{root}):")
    for idx, node in enumerate(dag.nodes):
        print(f"  #{idx}: {node}")
    stmts: list[BbStmt] = [
        ArrayRead("x", "A", Var("i")),
        ArrayRead("y", "A", Var("i")),
        ArrayWrite("A", Var("j"), Const(99)),
        ArrayRead("z", "A", Var("i")),
    ]
    print("Reassembled TAC with store kill:")
    for inst in DagBuilder().process_block(stmts):
        print(f"  {inst}")


class ExprDagTests(unittest.TestCase):
    def test_hash_conses_commutative_and_algebraic_expressions(self) -> None:
        expr = Bin(
            Op.ADD,
            Bin(Op.ADD, Var("a"), Var("b")),
            Bin(Op.MUL, Bin(Op.ADD, Var("a"), Var("b")), Bin(Op.ADD, Var("b"), Var("a"))),
        )
        dag = DagBuilder()
        root = dag.lower_ast(expr)
        self.assertEqual(len(dag.nodes), 5)
        self.assertEqual(root, 4)

        identity = Bin(
            Op.ADD,
            Bin(Op.MUL, Bin(Op.ADD, Var("x"), Const(0)), Const(1)),
            Bin(Op.SUB, Var("x"), Var("x")),
        )
        dag2 = DagBuilder()
        self.assertEqual(dag2.nodes[dag2.lower_ast(identity)], ("Leaf", "x"))

    def test_array_store_invalidates_stale_load_value_number(self) -> None:
        stmts: list[BbStmt] = [
            ArrayRead("x", "A", Var("i")),
            ArrayRead("y", "A", Var("i")),
            ArrayWrite("A", Var("j"), Const(42)),
            ArrayRead("z", "A", Var("i")),
        ]
        tac = DagBuilder().process_block(stmts)
        self.assertEqual(tac[0][0], "ArrayLoad")
        self.assertEqual(tac[1], ("Copy", "y", "x"))
        self.assertEqual(tac[3][0], "ArrayStore")
        self.assertEqual(tac[4][0], "ArrayLoad")
        self.assertEqual(tac[4][1], "z")


if __name__ == "__main__":
    main()
