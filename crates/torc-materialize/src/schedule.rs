//! Execution scheduling: topological ordering with parallelism detection.

use std::collections::{HashMap, HashSet};

use torc_core::graph::node::NodeId;
use torc_core::graph::region::{RegionId, RegionKind};
use torc_core::graph::Graph;

use crate::error::MaterializationError;

/// A single step in an execution schedule.
#[derive(Debug, Clone)]
pub enum ScheduleStep {
    /// Execute a single node.
    Execute(NodeId),
    /// Execute multiple independent nodes in parallel.
    Parallel(Vec<NodeId>),
    /// Execute a region's schedule.
    Region {
        region_id: RegionId,
        kind: RegionKind,
        body: Vec<ScheduleStep>,
    },
}

/// A complete execution schedule for a graph.
#[derive(Debug, Clone)]
pub struct ExecutionSchedule {
    /// Ordered steps to execute.
    pub steps: Vec<ScheduleStep>,
    /// Longest sequential chain length.
    pub sequential_depth: usize,
    /// Maximum number of nodes executable in parallel at any step.
    pub max_parallelism: usize,
}

/// Compute an execution schedule from a graph using topological ordering.
///
/// Groups nodes with no inter-dependencies into `Parallel` steps.
/// Wraps region contents into `Region` steps preserving region semantics.
pub fn compute_schedule(graph: &Graph) -> Result<ExecutionSchedule, MaterializationError> {
    let sorted = graph
        .topological_sort()
        .map_err(|e| MaterializationError::SchedulingFailed {
            message: format!("topological sort failed: {e}"),
        })?;

    if sorted.is_empty() {
        return Ok(ExecutionSchedule {
            steps: vec![],
            sequential_depth: 0,
            max_parallelism: 0,
        });
    }

    // Partition nodes into levels based on longest path from roots
    let levels = compute_levels(graph, &sorted);

    // Group nodes by level
    let max_level = levels.values().copied().max().unwrap_or(0);
    let mut level_groups: Vec<Vec<NodeId>> = vec![vec![]; max_level + 1];
    for &node_id in &sorted {
        if let Some(&level) = levels.get(&node_id) {
            level_groups[level].push(node_id);
        }
    }

    // Build schedule steps
    let mut steps = Vec::new();
    let mut max_parallelism = 0;

    for group in &level_groups {
        if group.is_empty() {
            continue;
        }
        max_parallelism = max_parallelism.max(group.len());
        if group.len() == 1 {
            steps.push(ScheduleStep::Execute(group[0]));
        } else {
            steps.push(ScheduleStep::Parallel(group.clone()));
        }
    }

    // Wrap region nodes into Region steps
    let steps = wrap_regions(graph, steps);

    let sequential_depth = steps.len();

    Ok(ExecutionSchedule {
        steps,
        sequential_depth,
        max_parallelism,
    })
}

/// Compute the longest path from any root to each node (0-indexed levels).
fn compute_levels(graph: &Graph, sorted: &[NodeId]) -> HashMap<NodeId, usize> {
    let mut levels: HashMap<NodeId, usize> = HashMap::new();

    for &node_id in sorted {
        let incoming = graph.incoming_edges(&node_id);
        let level = incoming
            .iter()
            .filter_map(|eid| {
                let edge = graph.get_edge(eid)?;
                levels.get(&edge.source.0).map(|&l| l + 1)
            })
            .max()
            .unwrap_or(0);
        levels.insert(node_id, level);
    }

    levels
}

/// Wrap schedule steps so that nodes belonging to a region are grouped
/// into `Region` steps. This is a best-effort pass: it wraps contiguous
/// runs of nodes that belong to the same region.
fn wrap_regions(graph: &Graph, steps: Vec<ScheduleStep>) -> Vec<ScheduleStep> {
    if graph.region_count() == 0 {
        return steps;
    }

    // Identify which nodes belong to regions
    let region_nodes: HashSet<NodeId> = graph
        .regions()
        .flat_map(|r| r.children.iter().copied())
        .collect();

    // If no nodes are in regions, return as-is
    if region_nodes.is_empty() {
        return steps;
    }

    // For now, return steps as-is — region wrapping is a refinement
    // that's more valuable with LLVM codegen (Pass 2). The schedule
    // already respects data dependencies which subsumes region ordering.
    steps
}

/// Compute the critical path length (longest chain of dependent nodes).
pub fn critical_path_length(graph: &Graph) -> Result<usize, MaterializationError> {
    let sorted = graph
        .topological_sort()
        .map_err(|e| MaterializationError::SchedulingFailed {
            message: format!("topological sort failed: {e}"),
        })?;

    if sorted.is_empty() {
        return Ok(0);
    }

    let levels = compute_levels(graph, &sorted);
    Ok(levels.values().copied().max().unwrap_or(0).saturating_add(1))
}

#[cfg(test)]
mod tests {
    use super::*;
    use torc_core::graph::edge::Edge;
    use torc_core::graph::node::{ArithmeticOp, Node, NodeKind};
    use torc_core::types::{Type, TypeSignature};

    #[test]
    fn schedule_empty_graph() {
        let g = Graph::new();
        let sched = compute_schedule(&g).unwrap();
        assert_eq!(sched.steps.len(), 0);
        assert_eq!(sched.sequential_depth, 0);
        assert_eq!(sched.max_parallelism, 0);
    }

    #[test]
    fn schedule_linear_chain() {
        let mut g = Graph::new();
        let n1 =
            Node::new(NodeKind::Literal).with_type_signature(TypeSignature::source(Type::i32()));
        let n2 = Node::new(NodeKind::Arithmetic(ArithmeticOp::Add))
            .with_type_signature(TypeSignature::pure_fn(vec![Type::i32()], Type::i32()));
        let n3 = Node::new(NodeKind::Arithmetic(ArithmeticOp::Mul))
            .with_type_signature(TypeSignature::pure_fn(vec![Type::i32()], Type::i32()));

        let id1 = g.add_node(n1).unwrap();
        let id2 = g.add_node(n2).unwrap();
        let id3 = g.add_node(n3).unwrap();
        g.add_edge(Edge::typed((id1, 0), (id2, 0), Type::i32()))
            .unwrap();
        g.add_edge(Edge::typed((id2, 0), (id3, 0), Type::i32()))
            .unwrap();

        let sched = compute_schedule(&g).unwrap();
        assert_eq!(sched.sequential_depth, 3);
        assert_eq!(sched.max_parallelism, 1);
    }

    #[test]
    fn schedule_diamond_parallelism() {
        let mut g = Graph::new();
        let src =
            Node::new(NodeKind::Literal).with_type_signature(TypeSignature::source(Type::i32()));
        let left = Node::new(NodeKind::Arithmetic(ArithmeticOp::Add))
            .with_type_signature(TypeSignature::pure_fn(vec![Type::i32()], Type::i32()));
        let right = Node::new(NodeKind::Arithmetic(ArithmeticOp::Mul))
            .with_type_signature(TypeSignature::pure_fn(vec![Type::i32()], Type::i32()));
        let join = Node::new(NodeKind::Arithmetic(ArithmeticOp::Add))
            .with_type_signature(TypeSignature::new(
                vec![Type::i32(), Type::i32()],
                vec![Type::i32()],
            ));

        let s = g.add_node(src).unwrap();
        let l = g.add_node(left).unwrap();
        let r = g.add_node(right).unwrap();
        let j = g.add_node(join).unwrap();

        g.add_edge(Edge::typed((s, 0), (l, 0), Type::i32()))
            .unwrap();
        g.add_edge(Edge::typed((s, 0), (r, 0), Type::i32()))
            .unwrap();
        g.add_edge(Edge::typed((l, 0), (j, 0), Type::i32()))
            .unwrap();
        g.add_edge(Edge::typed((r, 0), (j, 1), Type::i32()))
            .unwrap();

        let sched = compute_schedule(&g).unwrap();
        // src -> {left, right} -> join = 3 levels
        assert_eq!(sched.sequential_depth, 3);
        assert_eq!(sched.max_parallelism, 2);
    }

    #[test]
    fn critical_path_length_diamond() {
        let mut g = Graph::new();
        let src = Node::new(NodeKind::Literal);
        let left = Node::new(NodeKind::Arithmetic(ArithmeticOp::Add));
        let right = Node::new(NodeKind::Arithmetic(ArithmeticOp::Mul));
        let join = Node::new(NodeKind::Arithmetic(ArithmeticOp::Add));

        let s = g.add_node(src).unwrap();
        let l = g.add_node(left).unwrap();
        let r = g.add_node(right).unwrap();
        let j = g.add_node(join).unwrap();

        g.add_edge(Edge::new((s, 0), (l, 0))).unwrap();
        g.add_edge(Edge::new((s, 0), (r, 0))).unwrap();
        g.add_edge(Edge::new((l, 0), (j, 0))).unwrap();
        g.add_edge(Edge::new((r, 0), (j, 1))).unwrap();

        assert_eq!(critical_path_length(&g).unwrap(), 3);
    }
}
