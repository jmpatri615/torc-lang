//! Dataflow view: text-based level-grouped graph rendering.

use std::collections::HashMap;

use serde_json::json;
use torc_core::graph::node::NodeId;
use torc_core::graph::Graph;

use crate::error::ObserveError;
use crate::format::node_display_name;
use crate::view::{RenderContext, View, ViewKind, ViewOutput};

/// Dataflow view showing level-grouped node layout.
pub struct DataflowView;

impl View for DataflowView {
    fn kind(&self) -> ViewKind {
        ViewKind::Dataflow
    }

    fn render(&self, graph: &Graph, _ctx: &RenderContext<'_>) -> Result<ViewOutput, ObserveError> {
        let sorted = graph.topological_sort().map_err(|e| ObserveError::GraphError {
            message: format!("topological sort failed: {e}"),
        })?;

        if sorted.is_empty() {
            return Ok(ViewOutput {
                text: "Empty graph — no dataflow to display.\n".to_string(),
                data: json!({
                    "view": "dataflow",
                    "levels": [],
                    "summary": {
                        "node_count": 0,
                        "edge_count": 0,
                        "region_count": 0,
                        "depth": 0,
                        "max_parallelism": 0,
                    },
                }),
            });
        }

        // Compute levels (longest path from roots)
        let levels = compute_levels(graph, &sorted);
        let max_level = levels.values().copied().max().unwrap_or(0);

        // Group nodes by level
        let mut level_groups: Vec<Vec<NodeId>> = vec![vec![]; max_level + 1];
        for &node_id in &sorted {
            if let Some(&level) = levels.get(&node_id) {
                level_groups[level].push(node_id);
            }
        }

        let mut max_parallelism = 0;
        let mut text = String::new();
        let mut json_levels = Vec::new();

        text.push_str("=== Dataflow Graph ===\n\n");

        for (level_idx, group) in level_groups.iter().enumerate() {
            if group.is_empty() {
                continue;
            }
            max_parallelism = max_parallelism.max(group.len());

            let parallel_marker = if group.len() > 1 {
                format!(" ({} parallel)", group.len())
            } else {
                String::new()
            };

            text.push_str(&format!("Level {level_idx}{parallel_marker}:\n"));

            let mut json_nodes = Vec::new();

            for &node_id in group {
                if let Some(node) = graph.get_node(&node_id) {
                    let name = node_display_name(node);
                    let kind = format!("{}", node.kind);

                    // Input types from type signature
                    let input_types = node
                        .type_signature
                        .as_ref()
                        .map(|sig| {
                            sig.inputs
                                .iter()
                                .map(|t| format!("{t}"))
                                .collect::<Vec<_>>()
                                .join(", ")
                        })
                        .unwrap_or_default();

                    // Output type
                    let output_type = node
                        .type_signature
                        .as_ref()
                        .map(|sig| {
                            sig.outputs
                                .iter()
                                .map(|t| format!("{t}"))
                                .collect::<Vec<_>>()
                                .join(", ")
                        })
                        .unwrap_or_default();

                    let type_info = if input_types.is_empty() && output_type.is_empty() {
                        String::new()
                    } else if input_types.is_empty() {
                        format!(" -> {output_type}")
                    } else {
                        format!(" ({input_types}) -> {output_type}")
                    };

                    text.push_str(&format!("  [{kind}] {name}{type_info}\n"));

                    json_nodes.push(json!({
                        "node_id": node_id.to_string(),
                        "name": name,
                        "kind": kind,
                        "input_types": input_types,
                        "output_type": output_type,
                    }));
                }
            }

            json_levels.push(json!({
                "level": level_idx,
                "nodes": json_nodes,
                "parallelism": group.len(),
            }));

            text.push('\n');
        }

        // Summary
        let depth = max_level + 1;
        text.push_str("--- Summary ---\n");
        text.push_str(&format!("  Nodes:           {}\n", graph.node_count()));
        text.push_str(&format!("  Edges:           {}\n", graph.edge_count()));
        text.push_str(&format!("  Regions:         {}\n", graph.region_count()));
        text.push_str(&format!("  Depth (levels):  {depth}\n"));
        text.push_str(&format!("  Max parallelism: {max_parallelism}\n"));

        let data = json!({
            "view": "dataflow",
            "levels": json_levels,
            "summary": {
                "node_count": graph.node_count(),
                "edge_count": graph.edge_count(),
                "region_count": graph.region_count(),
                "depth": depth,
                "max_parallelism": max_parallelism,
            },
        });

        Ok(ViewOutput { text, data })
    }
}

/// Compute the longest path from any root to each node (0-indexed levels).
///
/// Reimplemented locally because `schedule::compute_levels` is private.
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

#[cfg(test)]
mod tests {
    use super::*;
    use torc_core::graph::edge::Edge;
    use torc_core::graph::node::{ArithmeticOp, Node, NodeKind};
    use torc_core::types::{Type, TypeSignature};

    fn empty_ctx() -> RenderContext<'static> {
        RenderContext::empty()
    }

    #[test]
    fn empty_graph() {
        let g = Graph::new();
        let view = DataflowView;
        let output = view.render(&g, &empty_ctx()).unwrap();
        assert!(output.text.contains("Empty graph"));
        assert_eq!(output.data["summary"]["node_count"], 0);
        assert_eq!(output.data["summary"]["depth"], 0);
    }

    #[test]
    fn diamond_parallelism() {
        let mut g = Graph::new();
        let mut src = Node::new(NodeKind::Literal)
            .with_type_signature(TypeSignature::source(Type::i32()));
        src.annotations.insert("name".into(), "src".into());

        let mut left = Node::new(NodeKind::Arithmetic(ArithmeticOp::Add))
            .with_type_signature(TypeSignature::pure_fn(vec![Type::i32()], Type::i32()));
        left.annotations.insert("name".into(), "left".into());

        let mut right = Node::new(NodeKind::Arithmetic(ArithmeticOp::Mul))
            .with_type_signature(TypeSignature::pure_fn(vec![Type::i32()], Type::i32()));
        right.annotations.insert("name".into(), "right".into());

        let mut join = Node::new(NodeKind::Arithmetic(ArithmeticOp::Add))
            .with_type_signature(TypeSignature::new(
                vec![Type::i32(), Type::i32()],
                vec![Type::i32()],
            ));
        join.annotations.insert("name".into(), "join".into());

        let s = g.add_node(src).unwrap();
        let l = g.add_node(left).unwrap();
        let r = g.add_node(right).unwrap();
        let j = g.add_node(join).unwrap();

        g.add_edge(Edge::typed((s, 0), (l, 0), Type::i32())).unwrap();
        g.add_edge(Edge::typed((s, 0), (r, 0), Type::i32())).unwrap();
        g.add_edge(Edge::typed((l, 0), (j, 0), Type::i32())).unwrap();
        g.add_edge(Edge::typed((r, 0), (j, 1), Type::i32())).unwrap();

        let view = DataflowView;
        let output = view.render(&g, &empty_ctx()).unwrap();

        // Check structure
        assert!(output.text.contains("Level 0"));
        assert!(output.text.contains("Level 1"));
        assert!(output.text.contains("2 parallel"));
        assert!(output.text.contains("Level 2"));

        // Check summary
        assert_eq!(output.data["summary"]["node_count"], 4);
        assert_eq!(output.data["summary"]["edge_count"], 4);
        assert_eq!(output.data["summary"]["depth"], 3);
        assert_eq!(output.data["summary"]["max_parallelism"], 2);
    }

    #[test]
    fn linear_chain_depth() {
        let mut g = Graph::new();
        let n1 = Node::new(NodeKind::Literal)
            .with_type_signature(TypeSignature::source(Type::i32()));
        let n2 = Node::new(NodeKind::Arithmetic(ArithmeticOp::Add))
            .with_type_signature(TypeSignature::pure_fn(vec![Type::i32()], Type::i32()));
        let n3 = Node::new(NodeKind::Arithmetic(ArithmeticOp::Mul))
            .with_type_signature(TypeSignature::pure_fn(vec![Type::i32()], Type::i32()));

        let id1 = g.add_node(n1).unwrap();
        let id2 = g.add_node(n2).unwrap();
        let id3 = g.add_node(n3).unwrap();

        g.add_edge(Edge::typed((id1, 0), (id2, 0), Type::i32())).unwrap();
        g.add_edge(Edge::typed((id2, 0), (id3, 0), Type::i32())).unwrap();

        let view = DataflowView;
        let output = view.render(&g, &empty_ctx()).unwrap();
        assert_eq!(output.data["summary"]["depth"], 3);
        assert_eq!(output.data["summary"]["max_parallelism"], 1);
    }

    #[test]
    fn json_structure() {
        let mut g = Graph::new();
        let mut n = Node::new(NodeKind::Literal);
        n.annotations.insert("name".into(), "x".into());
        g.add_node(n).unwrap();

        let view = DataflowView;
        let output = view.render(&g, &empty_ctx()).unwrap();
        assert_eq!(output.data["view"], "dataflow");
        let levels = output.data["levels"].as_array().unwrap();
        assert_eq!(levels.len(), 1);
        assert_eq!(levels[0]["level"], 0);
        assert_eq!(levels[0]["nodes"][0]["name"], "x");
    }

    #[test]
    fn type_info_display() {
        let mut g = Graph::new();
        let mut n = Node::new(NodeKind::Arithmetic(ArithmeticOp::Add))
            .with_type_signature(TypeSignature::pure_fn(
                vec![Type::i32(), Type::i32()],
                Type::i32(),
            ));
        n.annotations.insert("name".into(), "sum".into());
        g.add_node(n).unwrap();

        let view = DataflowView;
        let output = view.render(&g, &empty_ctx()).unwrap();
        assert!(output.text.contains("(i32, i32) -> i32"));
    }
}
