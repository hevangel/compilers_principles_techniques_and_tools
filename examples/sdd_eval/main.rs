//! Build an attribute dependency graph, classify S-/L-attributedness, and evaluate topologically.

use std::collections::{BTreeMap, BTreeSet};

// Vertex in the attribute dependency graph: (parse_node_id, attribute_name).
// Naming convention: `syn_*` denotes synthesized attributes; anything else (e.g., `inh_*`) is inherited.
type AttrInstance = (usize, String);

// Closed set of semantic actions so rules remain inspectable and deterministic.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RuleKind {
    Const(i64),
    CopyFirst,
    AddTwo,
    AddTenTimesLeftPlusRight,
}

// One semantic equation: `target = kind(deps...)`, adding dependency edges `dep -> target`.
#[derive(Clone, Debug)]
struct Rule {
    target: AttrInstance,
    deps: Vec<AttrInstance>,
    kind: RuleKind,
}

// Parse-tree skeleton used to verify whether attribute dependencies respect tree structure.
#[derive(Clone, Debug)]
struct ParseNode {
    parent: Option<usize>,
    children: Vec<usize>,
}

// Hierarchy of SDDs: SAttributed subset of LAttributed subset of GeneralAcyclic.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SddClass {
    SAttributed,
    LAttributed,
    GeneralAcyclic,
}

#[derive(Debug, PartialEq, Eq)]
enum EvalError {
    Cycle(Vec<AttrInstance>),
    UnknownDep(AttrInstance),
}

// Classify an SDD by checking every cross-node dependency edge `dep -> target`:
// - S-attributed invariant: only synthesized attributes exist, and they depend only on direct children
//   (or same-node attributes), enabling pure bottom-up LR parser evaluation.
// - L-attributed invariant: synthesized attributes depend only on direct children (or self), while
//   inherited attributes on a child may depend only on its parent or siblings strictly to its left,
//   enabling a single depth-first left-to-right evaluation pass.
fn classify_sdd(nodes: &[ParseNode], rules: &[Rule]) -> SddClass {
    let mut is_s_attributed = true;
    let mut is_l_attributed = true;

    for rule in rules {
        let (target_node, ref target_attr) = rule.target;
        let is_synth = target_attr.starts_with("syn_");
        for (dep_node, _) in &rule.deps {
            if *dep_node == target_node {
                // Intra-node edges are valid in both S- and L-attributed SDDs (assuming acyclicity).
                continue;
            }
            let target_is_parent_of_dep = nodes[target_node].children.contains(dep_node);
            if !is_synth || !target_is_parent_of_dep {
                is_s_attributed = false;
            }
            if is_synth {
                if !target_is_parent_of_dep {
                    is_l_attributed = false;
                }
            } else {
                // Inherited attribute on target_node: may depend only on parent or left siblings.
                if let Some(p) = nodes[target_node].parent {
                    let pos = nodes[p]
                        .children
                        .iter()
                        .position(|&c| c == target_node)
                        .unwrap();
                    let allowed_left_sibling = nodes[p].children[..pos].contains(dep_node);
                    if *dep_node != p && !allowed_left_sibling {
                        is_l_attributed = false;
                    }
                } else {
                    // Root inherited attributes have no parent or left siblings in the tree.
                    is_l_attributed = false;
                }
            }
        }
    }

    if is_s_attributed {
        SddClass::SAttributed
    } else if is_l_attributed {
        SddClass::LAttributed
    } else {
        SddClass::GeneralAcyclic
    }
}

// Evaluate rules using Kahn's topological sort over the attribute dependency graph.
// Invariant: an attribute enters `ready` only once `in_degree == 0`, i.e., all of its
// dependencies already have entries in `values`. Using BTreeSet makes tie-breaking deterministic.
fn evaluate_sdd(
    rules: &[Rule],
) -> Result<(Vec<AttrInstance>, BTreeMap<AttrInstance, i64>), EvalError> {
    let mut rule_by_target: BTreeMap<AttrInstance, &Rule> = BTreeMap::new();
    let mut all_nodes: BTreeSet<AttrInstance> = BTreeSet::new();
    for rule in rules {
        rule_by_target.insert(rule.target.clone(), rule);
        all_nodes.insert(rule.target.clone());
        for dep in &rule.deps {
            all_nodes.insert(dep.clone());
        }
    }

    let mut in_degree: BTreeMap<AttrInstance, usize> =
        all_nodes.iter().map(|n| (n.clone(), 0)).collect();
    let mut dependents: BTreeMap<AttrInstance, Vec<AttrInstance>> = BTreeMap::new();

    for rule in rules {
        for dep in &rule.deps {
            if !rule_by_target.contains_key(dep) {
                return Err(EvalError::UnknownDep(dep.clone()));
            }
            *in_degree.get_mut(&rule.target).unwrap() += 1;
            dependents
                .entry(dep.clone())
                .or_default()
                .push(rule.target.clone());
        }
    }

    let mut ready: BTreeSet<AttrInstance> = in_degree
        .iter()
        .filter_map(|(k, &deg)| (deg == 0).then_some(k.clone()))
        .collect();
    let mut order = Vec::new();
    let mut values: BTreeMap<AttrInstance, i64> = BTreeMap::new();

    while let Some(node) = ready.pop_first() {
        let rule = rule_by_target[&node];
        let val = match rule.kind {
            RuleKind::Const(v) => v,
            RuleKind::CopyFirst => values[&rule.deps[0]],
            RuleKind::AddTwo => values[&rule.deps[0]] + values[&rule.deps[1]],
            RuleKind::AddTenTimesLeftPlusRight => {
                10 * values[&rule.deps[0]] + values[&rule.deps[1]]
            }
        };
        values.insert(node.clone(), val);
        order.push(node.clone());
        if let Some(children) = dependents.get(&node) {
            for child in children {
                let d = in_degree.get_mut(child).unwrap();
                *d -= 1;
                if *d == 0 {
                    ready.insert(child.clone());
                }
            }
        }
    }

    // Any node that never reached in_degree 0 is part of or blocked by a dependency cycle.
    if order.len() != all_nodes.len() {
        let cycle_nodes: Vec<AttrInstance> = in_degree
            .into_iter()
            .filter_map(|(k, deg)| (deg > 0).then_some(k))
            .collect();
        return Err(EvalError::Cycle(cycle_nodes));
    }

    Ok((order, values))
}

// Parse tree for L-attributed accumulator: (node 0) -> [node 1 (digit 3), node 2 (tail)]
// where node 2 has children [node 3 (digit 5), node 4 (empty tail)]: computes 3*10 + 5 = 35.
fn sample_l_attributed() -> (Vec<ParseNode>, Vec<Rule>) {
    let nodes = vec![
        ParseNode {
            parent: None,
            children: vec![1, 2],
        }, // 0: root Number
        ParseNode {
            parent: Some(0),
            children: vec![],
        }, // 1: Digit(3)
        ParseNode {
            parent: Some(0),
            children: vec![3, 4],
        }, // 2: Tail
        ParseNode {
            parent: Some(2),
            children: vec![],
        }, // 3: Digit(5)
        ParseNode {
            parent: Some(2),
            children: vec![],
        }, // 4: EmptyTail
    ];
    let rules = vec![
        Rule {
            target: (1, "syn_val".into()),
            deps: vec![],
            kind: RuleKind::Const(3),
        },
        Rule {
            target: (2, "inh_acc".into()),
            deps: vec![(1, "syn_val".into())],
            kind: RuleKind::CopyFirst,
        },
        Rule {
            target: (3, "syn_val".into()),
            deps: vec![],
            kind: RuleKind::Const(5),
        },
        Rule {
            target: (4, "inh_acc".into()),
            deps: vec![(2, "inh_acc".into()), (3, "syn_val".into())],
            kind: RuleKind::AddTenTimesLeftPlusRight,
        },
        Rule {
            target: (4, "syn_val".into()),
            deps: vec![(4, "inh_acc".into())],
            kind: RuleKind::CopyFirst,
        },
        Rule {
            target: (2, "syn_val".into()),
            deps: vec![(4, "syn_val".into())],
            kind: RuleKind::CopyFirst,
        },
        Rule {
            target: (0, "syn_val".into()),
            deps: vec![(2, "syn_val".into())],
            kind: RuleKind::CopyFirst,
        },
    ];
    (nodes, rules)
}

fn main() {
    let (nodes, rules) = sample_l_attributed();
    let class = classify_sdd(&nodes, &rules);
    let (order, values) = evaluate_sdd(&rules).unwrap();
    println!("SDD class: {class:?}");
    println!("Topological evaluation order:");
    for attr in &order {
        println!("  ({}, {}) = {}", attr.0, attr.1, values[attr]);
    }
    let small_nodes = vec![
        ParseNode {
            parent: None,
            children: vec![1, 2],
        },
        ParseNode {
            parent: Some(0),
            children: vec![],
        },
        ParseNode {
            parent: Some(0),
            children: vec![],
        },
    ];
    let s_attr = vec![
        Rule {
            target: (1, "syn_v".into()),
            deps: vec![],
            kind: RuleKind::Const(4),
        },
        Rule {
            target: (2, "syn_v".into()),
            deps: vec![],
            kind: RuleKind::Const(6),
        },
        Rule {
            target: (0, "syn_v".into()),
            deps: vec![(1, "syn_v".into()), (2, "syn_v".into())],
            kind: RuleKind::AddTwo,
        },
    ];
    println!(
        "S-attributed sum ({:?}): {}",
        classify_sdd(&small_nodes, &s_attr),
        evaluate_sdd(&s_attr).unwrap().1[&(0, "syn_v".into())]
    );
}

#[test]
fn evaluates_l_attributed_sdd_in_topological_order() {
    let (nodes, rules) = sample_l_attributed();
    assert_eq!(classify_sdd(&nodes, &rules), SddClass::LAttributed);
    let (order, values) = evaluate_sdd(&rules).unwrap();
    assert_eq!(order.len(), rules.len());
    assert_eq!(values[&(0, "syn_val".into())], 35);
    assert_eq!(values[&(2, "inh_acc".into())], 3);
    assert_eq!(values[&(4, "inh_acc".into())], 35);
}

#[test]
fn detects_cycles_and_non_l_attributed_dependencies() {
    let nodes = vec![
        ParseNode {
            parent: None,
            children: vec![1, 2],
        },
        ParseNode {
            parent: Some(0),
            children: vec![],
        },
        ParseNode {
            parent: Some(0),
            children: vec![],
        },
    ];
    // Right-to-left dependency breaks L-attributedness but is still acyclic.
    let rtl_rules = vec![
        Rule {
            target: (2, "syn_val".into()),
            deps: vec![],
            kind: RuleKind::Const(7),
        },
        Rule {
            target: (1, "inh_x".into()),
            deps: vec![(2, "syn_val".into())],
            kind: RuleKind::CopyFirst,
        },
        Rule {
            target: (0, "syn_val".into()),
            deps: vec![(1, "inh_x".into()), (2, "syn_val".into())],
            kind: RuleKind::AddTwo,
        },
    ];
    assert_eq!(classify_sdd(&nodes, &rtl_rules), SddClass::GeneralAcyclic);
    assert_eq!(
        evaluate_sdd(&rtl_rules).unwrap().1[&(0, "syn_val".into())],
        14
    );

    // Circular SDD specification must be rejected.
    let cyclic_rules = vec![
        Rule {
            target: (1, "syn_a".into()),
            deps: vec![(2, "inh_b".into())],
            kind: RuleKind::CopyFirst,
        },
        Rule {
            target: (2, "inh_b".into()),
            deps: vec![(1, "syn_a".into())],
            kind: RuleKind::CopyFirst,
        },
    ];
    assert!(matches!(
        evaluate_sdd(&cyclic_rules),
        Err(EvalError::Cycle(_))
    ));
}
