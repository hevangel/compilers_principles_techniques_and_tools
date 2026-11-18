"""Build an attribute dependency graph, classify S-/L-attributedness, and evaluate topologically."""

from dataclasses import dataclass
from enum import Enum
import heapq
import unittest

# Attribute vertex key: (parse_node_id, attribute_name).
# Attributes prefixed with "syn_" are synthesized; others (e.g. "inh_*") are inherited.
AttrInstance = tuple[int, str]


# Fixed set of arithmetic semantic actions so dependency graphs stay inspectable.
class RuleKind(Enum):
    CONST = "Const"
    COPY_FIRST = "CopyFirst"
    ADD_TWO = "AddTwo"
    ADD_TEN_TIMES_LEFT_PLUS_RIGHT = "AddTenTimesLeftPlusRight"


# Semantic rule defining `target` from `deps` (adds edges dep -> target).
@dataclass(frozen=True)
class Rule:
    target: AttrInstance
    deps: list[AttrInstance]
    kind: RuleKind
    const_val: int = 0


# Parse-tree node links used to check whether dependency edges respect S-/L-attributed rules.
@dataclass(frozen=True)
class ParseNode:
    parent: int | None
    children: list[int]


# SDD classification hierarchy: SAttributed <= LAttributed <= GeneralAcyclic.
class SddClass(Enum):
    S_ATTRIBUTED = "SAttributed"
    L_ATTRIBUTED = "LAttributed"
    GENERAL_ACYCLIC = "GeneralAcyclic"


class CycleError(ValueError):
    def __init__(self, nodes: list[AttrInstance]) -> None:
        super().__init__(f"circular attribute dependency involving {nodes}")
        self.nodes = nodes


def classify_sdd(nodes: list[ParseNode], rules: list[Rule]) -> SddClass:
    """Classify an SDD as S-attributed, L-attributed, or general acyclic.

    - S-attributed: only `syn_*` attributes, depending only on direct children (or self).
    - L-attributed: `syn_*` depends on direct children (or self); inherited attributes
      depend only on the node's parent or siblings strictly to its left.
    """
    is_s = True
    is_l = True
    for rule in rules:
        target_node, target_attr = rule.target
        is_synth = target_attr.startswith("syn_")
        for dep_node, _ in rule.deps:
            if dep_node == target_node:
                # Same-node dependencies are allowed in both S- and L-attributed SDDs.
                continue
            target_is_parent = dep_node in nodes[target_node].children
            if not is_synth or not target_is_parent:
                is_s = False
            if is_synth:
                if not target_is_parent:
                    is_l = False
            else:
                # Inherited attribute: check parent and left-sibling restriction.
                parent = nodes[target_node].parent
                if parent is None:
                    is_l = False
                else:
                    pos = nodes[parent].children.index(target_node)
                    left_siblings = nodes[parent].children[:pos]
                    if dep_node != parent and dep_node not in left_siblings:
                        is_l = False
    if is_s:
        return SddClass.S_ATTRIBUTED
    if is_l:
        return SddClass.L_ATTRIBUTED
    return SddClass.GENERAL_ACYCLIC


def evaluate_sdd(rules: list[Rule]) -> tuple[list[AttrInstance], dict[AttrInstance, int]]:
    """Run Kahn's topological sort on the attribute dependency graph.

    Uses a min-heap over `(node_id, attr_name)` so evaluation order is deterministic,
    and raises `CycleError` if any attributes still have non-zero in-degree at the end.
    """
    rule_by_target = {r.target: r for r in rules}
    all_nodes: set[AttrInstance] = set(rule_by_target)
    for r in rules:
        all_nodes.update(r.deps)

    in_degree: dict[AttrInstance, int] = {n: 0 for n in all_nodes}
    dependents: dict[AttrInstance, list[AttrInstance]] = {}
    for r in rules:
        for dep in r.deps:
            if dep not in rule_by_target:
                raise ValueError(f"missing rule for dependency {dep}")
            in_degree[r.target] += 1
            dependents.setdefault(dep, []).append(r.target)

    # Invariant: every attribute in `heap` has all of its dependencies already evaluated.
    heap = sorted(k for k, deg in in_degree.items() if deg == 0)
    heapq.heapify(heap)
    order: list[AttrInstance] = []
    values: dict[AttrInstance, int] = {}

    while heap:
        node = heapq.heappop(heap)
        rule = rule_by_target[node]
        if rule.kind == RuleKind.CONST:
            val = rule.const_val
        elif rule.kind == RuleKind.COPY_FIRST:
            val = values[rule.deps[0]]
        elif rule.kind == RuleKind.ADD_TWO:
            val = values[rule.deps[0]] + values[rule.deps[1]]
        else:
            val = 10 * values[rule.deps[0]] + values[rule.deps[1]]
        values[node] = val
        order.append(node)
        for child in dependents.get(node, []):
            in_degree[child] -= 1
            if in_degree[child] == 0:
                heapq.heappush(heap, child)

    if len(order) != len(all_nodes):
        raise CycleError(sorted(k for k, deg in in_degree.items() if deg > 0))
    return order, values


def sample_l_attributed() -> tuple[list[ParseNode], list[Rule]]:
    """Build the 5-node digit-accumulator tree for `3` followed by `5` -> `35`."""
    nodes = [
        ParseNode(None, [1, 2]),
        ParseNode(0, []),
        ParseNode(0, [3, 4]),
        ParseNode(2, []),
        ParseNode(2, []),
    ]
    rules = [
        Rule((1, "syn_val"), [], RuleKind.CONST, const_val=3),
        Rule((2, "inh_acc"), [(1, "syn_val")], RuleKind.COPY_FIRST),
        Rule((3, "syn_val"), [], RuleKind.CONST, const_val=5),
        Rule(
            (4, "inh_acc"),
            [(2, "inh_acc"), (3, "syn_val")],
            RuleKind.ADD_TEN_TIMES_LEFT_PLUS_RIGHT,
        ),
        Rule((4, "syn_val"), [(4, "inh_acc")], RuleKind.COPY_FIRST),
        Rule((2, "syn_val"), [(4, "syn_val")], RuleKind.COPY_FIRST),
        Rule((0, "syn_val"), [(2, "syn_val")], RuleKind.COPY_FIRST),
    ]
    return nodes, rules


def main() -> None:
    nodes, rules = sample_l_attributed()
    sdd_class = classify_sdd(nodes, rules)
    order, values = evaluate_sdd(rules)
    print(f"SDD class: {sdd_class.value}")
    print("Topological evaluation order:")
    for attr in order:
        print(f"  {attr} = {values[attr]}")


class SddEvalTests(unittest.TestCase):
    def test_evaluates_l_attributed_sdd_in_topological_order(self) -> None:
        nodes, rules = sample_l_attributed()
        self.assertEqual(classify_sdd(nodes, rules), SddClass.L_ATTRIBUTED)
        order, values = evaluate_sdd(rules)
        self.assertEqual(len(order), len(rules))
        self.assertEqual(values[(0, "syn_val")], 35)
        self.assertEqual(values[(2, "inh_acc")], 3)
        self.assertEqual(values[(4, "inh_acc")], 35)

    def test_detects_cycles_and_non_l_attributed_dependencies(self) -> None:
        nodes = [ParseNode(None, [1, 2]), ParseNode(0, []), ParseNode(0, [])]
        rtl_rules = [
            Rule((2, "syn_val"), [], RuleKind.CONST, const_val=7),
            Rule((1, "inh_x"), [(2, "syn_val")], RuleKind.COPY_FIRST),
            Rule((0, "syn_val"), [(1, "inh_x"), (2, "syn_val")], RuleKind.ADD_TWO),
        ]
        self.assertEqual(classify_sdd(nodes, rtl_rules), SddClass.GENERAL_ACYCLIC)
        self.assertEqual(evaluate_sdd(rtl_rules)[1][(0, "syn_val")], 14)

        cyclic = [
            Rule((1, "syn_a"), [(2, "inh_b")], RuleKind.COPY_FIRST),
            Rule((2, "inh_b"), [(1, "syn_a")], RuleKind.COPY_FIRST),
        ]
        with self.assertRaises(CycleError):
            evaluate_sdd(cyclic)


if __name__ == "__main__":
    main()
