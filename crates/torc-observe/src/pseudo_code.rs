//! Pseudo-code view: procedural-style approximation of a graph program.

use std::collections::HashMap;

use serde_json::json;
use torc_core::graph::node::{NodeId, NodeKind};
use torc_core::graph::Graph;

use crate::error::ObserveError;
use crate::format::{
    arithmetic_op_symbol, bitwise_op_symbol, comparison_op_symbol, format_predicate,
    format_time_ns, node_display_name,
};
use crate::view::{RenderContext, View, ViewKind, ViewOutput};

/// Pseudo-code view that generates a procedural-style approximation.
pub struct PseudoCodeView;

impl View for PseudoCodeView {
    fn kind(&self) -> ViewKind {
        ViewKind::PseudoCode
    }

    fn render(&self, graph: &Graph, _ctx: &RenderContext<'_>) -> Result<ViewOutput, ObserveError> {
        let sorted = graph
            .topological_sort()
            .map_err(|e| ObserveError::GraphError {
                message: format!("topological sort failed: {e}"),
            })?;

        if sorted.is_empty() {
            return Ok(ViewOutput {
                text: "// Empty graph — no nodes to display.\n".to_string(),
                data: json!({
                    "view": "pseudo-code",
                    "statements": [],
                    "node_count": 0,
                }),
            });
        }

        // Build a map from node ID to variable name
        let mut var_names: HashMap<NodeId, String> = HashMap::new();
        for &node_id in &sorted {
            if let Some(node) = graph.get_node(&node_id) {
                var_names.insert(node_id, node_display_name(node));
            }
        }

        // Build a map from (node_id, port_index) -> producing variable name
        // For each edge, record: target_node's input port -> source variable name
        let mut input_vars: HashMap<(NodeId, usize), String> = HashMap::new();
        for edge in graph.edges() {
            let src_name = var_names
                .get(&edge.source.0)
                .cloned()
                .unwrap_or_else(|| "?".to_string());
            input_vars.insert((edge.target.0, edge.target.1), src_name);
        }

        let mut lines = Vec::new();
        let mut json_stmts = Vec::new();

        lines.push(
            "// APPROXIMATION — actual execution follows graph data dependencies".to_string(),
        );
        lines.push(String::new());

        for &node_id in &sorted {
            let node = match graph.get_node(&node_id) {
                Some(n) => n,
                None => continue,
            };

            let name = var_names.get(&node_id).cloned().unwrap_or_default();
            let input = |port: usize| -> String {
                input_vars
                    .get(&(node_id, port))
                    .cloned()
                    .unwrap_or_else(|| format!("input_{port}"))
            };

            let output_type = node
                .type_signature
                .as_ref()
                .and_then(|sig| sig.outputs.first())
                .map(|t| format!("{t}"))
                .unwrap_or_default();

            let stmt = match &node.kind {
                NodeKind::Literal => {
                    let value = node
                        .annotations
                        .get("value")
                        .cloned()
                        .unwrap_or_else(|| "?".to_string());
                    format!("let {name} = {value};")
                }

                NodeKind::Arithmetic(op) => {
                    let sym = arithmetic_op_symbol(op);
                    format!("let {name} = {} {sym} {};", input(0), input(1))
                }

                NodeKind::Bitwise(op) => {
                    let sym = bitwise_op_symbol(op);
                    if matches!(op, torc_core::graph::node::BitwiseOp::Not) {
                        format!("let {name} = {sym}{};", input(0))
                    } else {
                        format!("let {name} = {} {sym} {};", input(0), input(1))
                    }
                }

                NodeKind::Comparison(op) => {
                    let sym = comparison_op_symbol(op);
                    format!("let {name} = {} {sym} {};", input(0), input(1))
                }

                NodeKind::Select => {
                    format!(
                        "let {name} = if {} {{ {} }} else {{ {} }};",
                        input(0),
                        input(1),
                        input(2)
                    )
                }

                NodeKind::Read => {
                    let device = node
                        .annotations
                        .get("device")
                        .cloned()
                        .unwrap_or_else(|| "device".to_string());
                    format!("let {name} = read({device});")
                }

                NodeKind::Write => {
                    let device = node
                        .annotations
                        .get("device")
                        .cloned()
                        .unwrap_or_else(|| "device".to_string());
                    format!("write({device}, {});", input(0))
                }

                NodeKind::Conversion => {
                    if output_type.is_empty() {
                        format!("let {name} = convert({});", input(0))
                    } else {
                        format!("let {name} = {} as {output_type};", input(0))
                    }
                }

                NodeKind::Construct => {
                    // Collect all inputs
                    let inputs: Vec<String> = (0..4)
                        .map(&input)
                        .take_while(|s| !s.starts_with("input_"))
                        .collect();
                    format!("let {name} = construct({});", inputs.join(", "))
                }

                NodeKind::Destructure => {
                    format!("let {name} = destructure({});", input(0))
                }

                NodeKind::Index => {
                    format!("let {name} = {}[{}];", input(0), input(1))
                }

                NodeKind::Iterate => {
                    format!("let {name} = loop {{ /* iterate */ }};")
                }

                NodeKind::Recurse => {
                    format!("let {name} = recurse({});", input(0))
                }

                _ => {
                    // Generic fallback
                    let kind_name = format!("{}", node.kind);
                    let inputs: Vec<String> = (0..4)
                        .map(input)
                        .take_while(|s| !s.starts_with("input_"))
                        .collect();
                    if inputs.is_empty() {
                        format!("let {name} = {kind_name}();")
                    } else {
                        format!("let {name} = {kind_name}({});", inputs.join(", "))
                    }
                }
            };

            // Build comment with contract info
            let mut comment_parts = Vec::new();
            if !output_type.is_empty() {
                comment_parts.push(format!("-> {output_type}"));
            }
            if let Some(ref contract) = node.contract {
                if !contract.effects.is_pure() {
                    comment_parts.push(format!("{}", contract.effects));
                }
                if let Some(ref tb) = contract.time_bound {
                    if let Some(wcet) = tb.wcet_ns {
                        comment_parts.push(format!("WCET {}", format_time_ns(wcet)));
                    }
                }
                for pre in &contract.preconditions {
                    comment_parts.push(format!("pre: {}", format_predicate(pre)));
                }
            }

            let line = if comment_parts.is_empty() {
                stmt.clone()
            } else {
                format!("{stmt}  // {}", comment_parts.join(", "))
            };

            lines.push(line);

            json_stmts.push(json!({
                "node_id": node_id.to_string(),
                "name": name,
                "kind": format!("{}", node.kind),
                "statement": stmt,
                "output_type": output_type,
            }));
        }

        let text = lines.join("\n") + "\n";
        let data = json!({
            "view": "pseudo-code",
            "statements": json_stmts,
            "node_count": sorted.len(),
        });

        Ok(ViewOutput { text, data })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use torc_core::graph::edge::Edge;
    use torc_core::graph::node::{ArithmeticOp, ComparisonOp, Node, NodeKind};
    use torc_core::types::{Type, TypeSignature};

    fn empty_ctx() -> RenderContext<'static> {
        RenderContext::empty()
    }

    #[test]
    fn empty_graph() {
        let g = Graph::new();
        let view = PseudoCodeView;
        let output = view.render(&g, &empty_ctx()).unwrap();
        assert!(output.text.contains("Empty graph"));
        assert_eq!(output.data["node_count"], 0);
    }

    #[test]
    fn single_literal() {
        let mut g = Graph::new();
        let mut n =
            Node::new(NodeKind::Literal).with_type_signature(TypeSignature::source(Type::i32()));
        n.annotations.insert("name".into(), "x".into());
        n.annotations.insert("value".into(), "42".into());
        g.add_node(n).unwrap();

        let view = PseudoCodeView;
        let output = view.render(&g, &empty_ctx()).unwrap();
        assert!(output.text.contains("let x = 42;"));
        assert_eq!(output.data["node_count"], 1);
    }

    #[test]
    fn linear_chain() {
        let mut g = Graph::new();
        let mut n1 =
            Node::new(NodeKind::Literal).with_type_signature(TypeSignature::source(Type::i32()));
        n1.annotations.insert("name".into(), "a".into());
        n1.annotations.insert("value".into(), "10".into());

        let mut n2 =
            Node::new(NodeKind::Literal).with_type_signature(TypeSignature::source(Type::i32()));
        n2.annotations.insert("name".into(), "b".into());
        n2.annotations.insert("value".into(), "20".into());

        let mut n3 = Node::new(NodeKind::Arithmetic(ArithmeticOp::Add)).with_type_signature(
            TypeSignature::pure_fn(vec![Type::i32(), Type::i32()], Type::i32()),
        );
        n3.annotations.insert("name".into(), "sum".into());

        let id1 = g.add_node(n1).unwrap();
        let id2 = g.add_node(n2).unwrap();
        let id3 = g.add_node(n3).unwrap();

        g.add_edge(Edge::typed((id1, 0), (id3, 0), Type::i32()))
            .unwrap();
        g.add_edge(Edge::typed((id2, 0), (id3, 1), Type::i32()))
            .unwrap();

        let view = PseudoCodeView;
        let output = view.render(&g, &empty_ctx()).unwrap();
        assert!(output.text.contains("let a = 10;"));
        assert!(output.text.contains("let b = 20;"));
        assert!(output.text.contains("let sum = a + b;"));
    }

    #[test]
    fn contract_annotations() {
        use torc_core::contract::{Contract, EffectSet};
        use torc_core::types::Effect;

        let mut g = Graph::new();
        let mut n = Node::new(NodeKind::Read)
            .with_type_signature(TypeSignature::source(Type::f32()))
            .with_contract(
                Contract::pure_default()
                    .with_wcet(50_000, "arm")
                    .with_effects(EffectSet::from_effects(vec![Effect::IO("ADC1".into())])),
            );
        n.annotations.insert("name".into(), "sensor".into());
        n.annotations.insert("device".into(), "ADC1".into());
        g.add_node(n).unwrap();

        let view = PseudoCodeView;
        let output = view.render(&g, &empty_ctx()).unwrap();
        assert!(output.text.contains("read(ADC1)"));
        assert!(output.text.contains("IO<ADC1>"));
        assert!(output.text.contains("WCET 50.0us"));
    }

    #[test]
    fn comparison_node() {
        let mut g = Graph::new();
        let mut n1 =
            Node::new(NodeKind::Literal).with_type_signature(TypeSignature::source(Type::i32()));
        n1.annotations.insert("name".into(), "x".into());
        n1.annotations.insert("value".into(), "5".into());

        let mut n2 =
            Node::new(NodeKind::Literal).with_type_signature(TypeSignature::source(Type::i32()));
        n2.annotations.insert("name".into(), "y".into());
        n2.annotations.insert("value".into(), "10".into());

        let mut n3 = Node::new(NodeKind::Comparison(ComparisonOp::Lt)).with_type_signature(
            TypeSignature::pure_fn(vec![Type::i32(), Type::i32()], Type::Bool),
        );
        n3.annotations.insert("name".into(), "less".into());

        let id1 = g.add_node(n1).unwrap();
        let id2 = g.add_node(n2).unwrap();
        let id3 = g.add_node(n3).unwrap();

        g.add_edge(Edge::typed((id1, 0), (id3, 0), Type::i32()))
            .unwrap();
        g.add_edge(Edge::typed((id2, 0), (id3, 1), Type::i32()))
            .unwrap();

        let view = PseudoCodeView;
        let output = view.render(&g, &empty_ctx()).unwrap();
        assert!(output.text.contains("let less = x < y;"));
    }

    #[test]
    fn json_export() {
        let mut g = Graph::new();
        let mut n = Node::new(NodeKind::Literal);
        n.annotations.insert("name".into(), "x".into());
        n.annotations.insert("value".into(), "1".into());
        g.add_node(n).unwrap();

        let view = PseudoCodeView;
        let output = view.render(&g, &empty_ctx()).unwrap();
        assert_eq!(output.data["view"], "pseudo-code");
        assert_eq!(output.data["node_count"], 1);
        assert!(output.data["statements"].as_array().unwrap().len() == 1);
    }
}
