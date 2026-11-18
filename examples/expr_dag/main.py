"""Build expression DAGs via value-number hash-consing, algebraic folding, and alias kills."""

from dataclasses import dataclass
from enum import Enum
import unittest

I64_MASK = (1 << 64) - 1


# Two's-complement 64-bit wrapping helper so constant folding matches Rust's i64 semantics.
def wrap_i64(value: int) -> int:
    masked = value & I64_MASK
    return masked - (1 << 64) if masked >= (1 << 63) else masked


class Op(Enum):
    ADD = "Add"
    SUB = "Sub"
    MUL = "Mul"


# Source-level expression AST nodes before value-number deduplication.
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


# Basic-block statements; `PtrKill` models an opaque pointer write that could alias any array.
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
    """Hash-consing DAG builder with algebraic folding and per-array alias epochs.

    Invariants:
    - `nodes` and `table` maintain a 1-to-1 bijection from canonical node tuples to integer IDs.
    - `env` maps each variable name to the DAG node ID of its current value.
    - `array_epoch[A]` (falling back to `global_epoch`) increments on stores so stale cached
      `("ArrayLoad", A, idx, epoch)` nodes are never reused after a potential alias write.
    """

    def __init__(self) -> None:
        self.nodes: list[tuple[object, ...]] = []
        self.table: dict[tuple[object, ...], int] = {}
        self.env: dict[str, int] = {}
        self.array_epoch: dict[str, int] = {}
        self.global_epoch = 0

    def intern(self, key: tuple[object, ...]) -> int:
        """Return the existing value number for `key`, or allocate a new DAG node ID."""
        if key in self.table:
            return self.table[key]
        idx = len(self.nodes)
        self.nodes.append(key)
        self.table[key] = idx
        return idx

    def leaf(self, name: str) -> int:
        """Return the current value number for variable `name`, creating a Leaf on first use."""
        if name in self.env:
            return self.env[name]
        idx = self.intern(("Leaf", name))
        self.env[name] = idx
        return idx

    def lower_ast(self, ast: ExprAst) -> int:
        """Lower an expression AST bottom-up into canonical value-numbered DAG nodes."""
        if isinstance(ast, Var):
            return self.leaf(ast.name)
        if isinstance(ast, Const):
            return self.intern(("Const", ast.val))
        left = self.lower_ast(ast.left)
        right = self.lower_ast(ast.right)
        return self.make_bin(ast.op, left, right)

    def make_bin(self, op: Op, left: int, right: int) -> int:
        """Fold constants, apply algebraic identities, and canonicalize commutative operands."""
        lnode = self.nodes[left]
        rnode = self.nodes[right]
        if lnode[0] == "Const" and rnode[0] == "Const":
            a, b = int(lnode[1]), int(rnode[1])  # type: ignore[arg-type]
            if op == Op.ADD:
                return self.intern(("Const", wrap_i64(a + b)))
            if op == Op.SUB:
                return self.intern(("Const", wrap_i64(a - b)))
            return self.intern(("Const", wrap_i64(a * b)))

        # Canonicalize commutative ops (+, *) by placing the smaller value number first so
        # `a + b` and `b + a` map to the exact same tuple key.
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
        """Emit TAC instructions in post-order once per shared DAG node."""
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
        """Return the TAC operand name holding `node_id`, loading constants on demand."""
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
        """Translate basic-block statements to TAC while deduplicating loads within an epoch."""
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
                    # Same array, index value, and epoch: reuse previous load via Copy.
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
                # Advance array's epoch so future reads cannot reuse pre-store cached values.
                self.array_epoch[stmt.array] = self.array_epoch.get(stmt.array, self.global_epoch) + 1
            else:
                # PtrKill invalidates all arrays conservatively by bumping global_epoch.
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
