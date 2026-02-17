//! Contract view: tabular summary of node contracts.

use serde_json::json;
use torc_core::graph::Graph;

use crate::error::ObserveError;
use crate::format::{format_bytes, format_predicate, format_time_ns, node_display_name};
use crate::view::{RenderContext, View, ViewKind, ViewOutput};

/// Contract view that renders a tabular summary of node contracts.
pub struct ContractView;

impl View for ContractView {
    fn kind(&self) -> ViewKind {
        ViewKind::Contract
    }

    fn render(&self, graph: &Graph, _ctx: &RenderContext<'_>) -> Result<ViewOutput, ObserveError> {
        // Collect nodes that have contracts
        let mut rows = Vec::new();
        for node in graph.nodes() {
            if let Some(ref contract) = node.contract {
                let name = node_display_name(node);
                let kind = format!("{}", node.kind);

                // Key contracts: preconditions + postconditions
                let mut predicates = Vec::new();
                for pre in &contract.preconditions {
                    predicates.push(format!("pre: {}", format_predicate(pre)));
                }
                for post in &contract.postconditions {
                    predicates.push(format!("post: {}", format_predicate(post)));
                }

                // Effects
                let effects = format!("{}", contract.effects);

                // Resource bounds
                let mut bounds = Vec::new();
                if let Some(ref tb) = contract.time_bound {
                    if let Some(wcet) = tb.wcet_ns {
                        bounds.push(format!("WCET: {}", format_time_ns(wcet)));
                    }
                }
                if let Some(ref sb) = contract.stack_bound {
                    bounds.push(format!("stack: {}", format_bytes(sb.max_bytes)));
                }
                if let Some(ref mb) = contract.memory_bound {
                    if let Some(peak) = mb.peak_bytes {
                        bounds.push(format!("heap: {}", format_bytes(peak)));
                    }
                }

                // Failure modes — include in predicates column (spec puts them in Key Contracts)
                let failure_modes: Vec<String> = contract
                    .failure_modes
                    .iter()
                    .map(|fm| format!("{}: {}", fm.name, fm.recovery))
                    .collect();
                for fm in &contract.failure_modes {
                    predicates.push(format!("failure: {}", fm.name));
                    predicates.push(format!("  recovery: {}", fm.recovery));
                }

                let proof_status = format!("{}", contract.proof_status);

                rows.push(ContractRow {
                    name,
                    kind,
                    predicates,
                    effects,
                    bounds,
                    failure_modes,
                    proof_status,
                });
            }
        }

        // Sort by name for stable output
        rows.sort_by(|a, b| a.name.cmp(&b.name));

        if rows.is_empty() {
            return Ok(ViewOutput {
                text: "No contracts defined in this graph.\n".to_string(),
                data: json!({
                    "view": "contracts",
                    "contracts": [],
                    "total": 0,
                }),
            });
        }

        // Compute column widths
        let name_w = rows.iter().map(|r| r.name.len()).max().unwrap_or(4).max(4);
        let kind_w = rows.iter().map(|r| r.kind.len()).max().unwrap_or(4).max(4);
        let pred_w = rows
            .iter()
            .map(|r| {
                if r.predicates.is_empty() {
                    1 // "—"
                } else {
                    r.predicates.join("; ").len()
                }
            })
            .max()
            .unwrap_or(10)
            .max(10);
        let eff_w = rows.iter().map(|r| r.effects.len()).max().unwrap_or(7).max(7);
        let bounds_w = rows
            .iter()
            .map(|r| {
                if r.bounds.is_empty() {
                    1 // "—"
                } else {
                    r.bounds.join("; ").len()
                }
            })
            .max()
            .unwrap_or(8)
            .max(8);
        let status_w = rows
            .iter()
            .map(|r| r.proof_status.len())
            .max()
            .unwrap_or(6)
            .max(6);

        // Render table
        let mut text = String::new();
        text.push_str("=== Contract Summary ===\n\n");

        // Header
        let header = format!(
            "┌{:─<nw$}┬{:─<kw$}┬{:─<pw$}┬{:─<ew$}┬{:─<bw$}┬{:─<sw$}┐",
            "",
            "",
            "",
            "",
            "",
            "",
            nw = name_w + 2,
            kw = kind_w + 2,
            pw = pred_w + 2,
            ew = eff_w + 2,
            bw = bounds_w + 2,
            sw = status_w + 2,
        );
        text.push_str(&header);
        text.push('\n');

        text.push_str(&format!(
            "│ {:<nw$} │ {:<kw$} │ {:<pw$} │ {:<ew$} │ {:<bw$} │ {:<sw$} │",
            "Node",
            "Kind",
            "Contracts",
            "Effects",
            "Bounds",
            "Status",
            nw = name_w,
            kw = kind_w,
            pw = pred_w,
            ew = eff_w,
            bw = bounds_w,
            sw = status_w,
        ));
        text.push('\n');

        let separator = format!(
            "├{:─<nw$}┼{:─<kw$}┼{:─<pw$}┼{:─<ew$}┼{:─<bw$}┼{:─<sw$}┤",
            "",
            "",
            "",
            "",
            "",
            "",
            nw = name_w + 2,
            kw = kind_w + 2,
            pw = pred_w + 2,
            ew = eff_w + 2,
            bw = bounds_w + 2,
            sw = status_w + 2,
        );
        text.push_str(&separator);
        text.push('\n');

        // Rows
        for row in &rows {
            let pred_str = if row.predicates.is_empty() {
                "-".to_string()
            } else {
                row.predicates.join("; ")
            };
            let bounds_str = if row.bounds.is_empty() {
                "-".to_string()
            } else {
                row.bounds.join("; ")
            };

            text.push_str(&format!(
                "│ {:<nw$} │ {:<kw$} │ {:<pw$} │ {:<ew$} │ {:<bw$} │ {:<sw$} │",
                row.name,
                row.kind,
                pred_str,
                row.effects,
                bounds_str,
                row.proof_status,
                nw = name_w,
                kw = kind_w,
                pw = pred_w,
                ew = eff_w,
                bw = bounds_w,
                sw = status_w,
            ));
            text.push('\n');
        }

        // Footer
        let footer = format!(
            "└{:─<nw$}┴{:─<kw$}┴{:─<pw$}┴{:─<ew$}┴{:─<bw$}┴{:─<sw$}┘",
            "",
            "",
            "",
            "",
            "",
            "",
            nw = name_w + 2,
            kw = kind_w + 2,
            pw = pred_w + 2,
            ew = eff_w + 2,
            bw = bounds_w + 2,
            sw = status_w + 2,
        );
        text.push_str(&footer);
        text.push('\n');

        // Summary
        let verified = rows.iter().filter(|r| r.proof_status == "Verified").count();
        let pending = rows.iter().filter(|r| r.proof_status == "Pending").count();
        let assumed = rows.iter().filter(|r| r.proof_status == "Assumed").count();
        let waived = rows.iter().filter(|r| r.proof_status == "Waived").count();

        text.push_str(&format!(
            "\n{} contracts: {} verified, {} pending, {} assumed, {} waived\n",
            rows.len(),
            verified,
            pending,
            assumed,
            waived,
        ));

        // JSON
        let json_contracts: Vec<_> = rows
            .iter()
            .map(|r| {
                json!({
                    "name": r.name,
                    "kind": r.kind,
                    "predicates": r.predicates,
                    "effects": r.effects,
                    "bounds": r.bounds,
                    "failure_modes": r.failure_modes,
                    "proof_status": r.proof_status,
                })
            })
            .collect();

        let data = json!({
            "view": "contracts",
            "contracts": json_contracts,
            "total": rows.len(),
            "verified": verified,
            "pending": pending,
            "assumed": assumed,
            "waived": waived,
        });

        Ok(ViewOutput { text, data })
    }
}

struct ContractRow {
    name: String,
    kind: String,
    predicates: Vec<String>,
    effects: String,
    bounds: Vec<String>,
    failure_modes: Vec<String>,
    proof_status: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use torc_core::contract::{Contract, EffectSet};
    use torc_core::graph::node::{Node, NodeKind};
    use torc_core::types::{Effect, Predicate, Type, TypeSignature};

    fn empty_ctx() -> RenderContext<'static> {
        RenderContext::empty()
    }

    #[test]
    fn empty_graph() {
        let g = Graph::new();
        let view = ContractView;
        let output = view.render(&g, &empty_ctx()).unwrap();
        assert!(output.text.contains("No contracts"));
        assert_eq!(output.data["total"], 0);
    }

    #[test]
    fn single_contract() {
        let mut g = Graph::new();
        let mut n = Node::new(NodeKind::Literal)
            .with_type_signature(TypeSignature::source(Type::i32()))
            .with_contract(Contract::with_conditions(
                vec![Predicate::positive("input")],
                vec![Predicate::in_range("output", 0, 4095)],
            ));
        n.annotations.insert("name".into(), "adc_read".into());
        g.add_node(n).unwrap();

        let view = ContractView;
        let output = view.render(&g, &empty_ctx()).unwrap();
        assert!(output.text.contains("adc_read"));
        assert!(output.text.contains("Pending"));
        assert_eq!(output.data["total"], 1);
    }

    #[test]
    fn proof_status_display() {
        let mut g = Graph::new();
        let mut contract = Contract::pure_default()
            .with_wcet(50_000, "arm")
            .with_stack(256);
        contract.proof_status = torc_core::contract::ProofStatus::Verified;

        let mut n = Node::new(NodeKind::Read)
            .with_type_signature(TypeSignature::source(Type::f32()))
            .with_contract(contract);
        n.annotations.insert("name".into(), "sensor".into());
        g.add_node(n).unwrap();

        let view = ContractView;
        let output = view.render(&g, &empty_ctx()).unwrap();
        assert!(output.text.contains("Verified"));
        assert!(output.text.contains("WCET: 50.0us"));
        assert!(output.text.contains("stack: 256 B"));
    }

    #[test]
    fn resource_bounds() {
        let mut g = Graph::new();
        let mut n = Node::new(NodeKind::Literal)
            .with_type_signature(TypeSignature::source(Type::i32()))
            .with_contract(
                Contract::pure_default()
                    .with_wcet(100_000, "test")
                    .with_stack(512)
                    .with_no_heap(),
            );
        n.annotations.insert("name".into(), "node".into());
        g.add_node(n).unwrap();

        let view = ContractView;
        let output = view.render(&g, &empty_ctx()).unwrap();
        assert!(output.text.contains("WCET: 100.0us"));
        assert!(output.text.contains("stack: 512 B"));
        assert!(output.text.contains("heap: 0 B"));
    }

    #[test]
    fn json_export() {
        let mut g = Graph::new();
        let mut n = Node::new(NodeKind::Literal)
            .with_type_signature(TypeSignature::source(Type::i32()))
            .with_contract(Contract::pure_default().with_effects(
                EffectSet::from_effects(vec![Effect::IO("UART".into())]),
            ));
        n.annotations.insert("name".into(), "uart_write".into());
        g.add_node(n).unwrap();

        let view = ContractView;
        let output = view.render(&g, &empty_ctx()).unwrap();
        assert_eq!(output.data["view"], "contracts");
        let contracts = output.data["contracts"].as_array().unwrap();
        assert_eq!(contracts.len(), 1);
        assert_eq!(contracts[0]["effects"], "IO<UART>");
    }

    #[test]
    fn multi_predicate_column_width() {
        use torc_core::contract::Contract;

        let mut g = Graph::new();
        let mut n = Node::new(NodeKind::Literal)
            .with_type_signature(TypeSignature::source(Type::i32()))
            .with_contract(Contract::with_conditions(
                vec![
                    Predicate::positive("input"),
                    Predicate::in_range("input", 0, 4095),
                ],
                vec![Predicate::in_range("output", 0, 255)],
            ));
        n.annotations.insert("name".into(), "adc".into());
        g.add_node(n).unwrap();

        let view = ContractView;
        let output = view.render(&g, &empty_ctx()).unwrap();
        // Table lines should all have the same length (no overflow from joined predicates)
        let table_lines: Vec<&str> = output
            .text
            .lines()
            .filter(|l| l.starts_with('│'))
            .collect();
        assert!(!table_lines.is_empty());
        let first_len = table_lines[0].len();
        for line in &table_lines {
            assert_eq!(line.len(), first_len, "Table row has inconsistent width: {line}");
        }
    }

    #[test]
    fn failure_modes_in_text() {
        use torc_core::contract::{Contract, FailureMode, RecoveryStrategy};

        let mut g = Graph::new();
        let mut contract = Contract::pure_default()
            .with_wcet(45_000, "arm")
            .with_effects(EffectSet::from_effects(vec![Effect::IO("CAN1".into())]));
        contract.failure_modes.push(FailureMode {
            name: "CAN_BUS_OFF".into(),
            description: "CAN bus off condition".into(),
            recovery: RecoveryStrategy::Retry(3),
        });

        let mut n = Node::new(NodeKind::Write)
            .with_type_signature(TypeSignature::source(Type::Unit))
            .with_contract(contract);
        n.annotations.insert("name".into(), "can_transmit".into());
        g.add_node(n).unwrap();

        let view = ContractView;
        let output = view.render(&g, &empty_ctx()).unwrap();
        assert!(output.text.contains("failure: CAN_BUS_OFF"));
        assert!(output.text.contains("recovery: retry(3)"));
        // Also verify JSON still has failure_modes
        let contracts = output.data["contracts"].as_array().unwrap();
        assert!(!contracts[0]["failure_modes"].as_array().unwrap().is_empty());
    }
}
