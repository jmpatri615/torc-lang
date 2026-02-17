//! Structural analysis pass: wraps existing graph validations.

use torc_core::contract::{ObligationKind, ProofStatus};
use torc_core::graph::node::NodeId;
use torc_core::graph::Graph;

use crate::registry::ObligationRegistry;
use crate::witness::generate_witness;

/// Severity of a structural diagnostic.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
}

/// A diagnostic produced by structural analysis.
#[derive(Debug, Clone)]
pub struct StructuralDiagnostic {
    pub severity: Severity,
    pub message: String,
    pub node_id: Option<NodeId>,
    pub suggestion: Option<String>,
}

/// Structural analyzer wrapping existing graph validations.
pub struct StructuralAnalyzer;

impl StructuralAnalyzer {
    /// Run structural analysis on a graph, discharging linearity obligations
    /// and producing diagnostics for violations.
    pub fn analyze(graph: &Graph, registry: &mut ObligationRegistry) -> Vec<StructuralDiagnostic> {
        let mut diagnostics = Vec::new();

        // Run structural well-formedness validation
        if let Err(errors) = graph.validate() {
            for err in errors {
                diagnostics.push(StructuralDiagnostic {
                    severity: Severity::Error,
                    message: err.to_string(),
                    node_id: None,
                    suggestion: Some("Fix structural well-formedness errors".into()),
                });
            }
        }

        // Run linearity validation
        let linearity_result = graph.validate_linearity();
        if let Err(ref errors) = linearity_result {
            for err in errors {
                diagnostics.push(StructuralDiagnostic {
                    severity: Severity::Error,
                    message: err.to_string(),
                    node_id: None,
                    suggestion: Some("Ensure linear values are consumed exactly once".into()),
                });
            }
        }

        // Run effect validation
        if let Err(errors) = graph.validate_effects() {
            for err in errors {
                diagnostics.push(StructuralDiagnostic {
                    severity: Severity::Error,
                    message: err.to_string(),
                    node_id: None,
                    suggestion: Some(
                        "Declare required effects on the consuming node".into(),
                    ),
                });
            }
        }

        // Discharge Linearity obligations structurally:
        // If linearity validation passed, mark all Linearity obligations as Verified.
        if linearity_result.is_ok() {
            let linearity_ids: Vec<u64> = registry
                .pending()
                .filter(|o| o.obligation.kind == ObligationKind::Linearity)
                .map(|o| o.id)
                .collect();

            for id in linearity_ids {
                if let Some(tracked) = registry.all().iter().find(|o| o.id == id) {
                    let witness =
                        generate_witness("structural_analysis", &tracked.obligation, vec![]);
                    registry.update_status(id, ProofStatus::Verified, Some(witness));
                }
            }
        }

        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use torc_core::contract::{Contract, ObligationKind, ProofStatus};
    use torc_core::graph::edge::Edge;
    use torc_core::graph::node::{Node, NodeKind};
    use torc_core::types::{Type, TypeSignature};

    #[test]
    fn linearity_obligations_discharged() {
        // Build a graph with a linear value consumed exactly once → linearity passes
        let mut g = Graph::new();

        let mut n1 = Node::new(NodeKind::Literal);
        n1.type_signature = Some(TypeSignature::source(Type::i32().linear()));
        let n1_id = g.add_node(n1).unwrap();

        let mut n2 = Node::new(NodeKind::Arithmetic(
            torc_core::graph::node::ArithmeticOp::Add,
        ));
        n2.type_signature = Some(TypeSignature::pure_fn(vec![Type::i32()], Type::i32()));
        let n2_id = g.add_node(n2).unwrap();

        let edge = Edge::typed((n1_id, 0), (n2_id, 0), Type::i32().linear());
        g.add_edge(edge).unwrap();

        let mut registry = ObligationRegistry::collect_from_graph(&g);
        let diagnostics = StructuralAnalyzer::analyze(&g, &mut registry);

        // No structural errors
        assert!(diagnostics.is_empty());

        // Any linearity obligations should be discharged
        for o in registry
            .all()
            .iter()
            .filter(|o| o.obligation.kind == ObligationKind::Linearity)
        {
            assert_eq!(o.obligation.status, ProofStatus::Verified);
        }
    }

    #[test]
    fn effect_violations_reported() {
        use torc_core::contract::EffectSet;
        use torc_core::types::Effect;

        let mut g = Graph::new();

        // Source node with IO effect
        let mut n1 = Node::new(NodeKind::Literal);
        n1.type_signature = Some(TypeSignature::source(Type::i32()));
        n1.contract = Some(
            Contract::pure_default()
                .with_effects(EffectSet::from_effects(vec![Effect::IO("ADC".into())])),
        );
        let n1_id = g.add_node(n1).unwrap();

        // Target node declares Pure but depends on IO
        let mut n2 = Node::new(NodeKind::Arithmetic(
            torc_core::graph::node::ArithmeticOp::Add,
        ));
        n2.type_signature = Some(TypeSignature::pure_fn(vec![Type::i32()], Type::i32()));
        n2.contract = Some(Contract::pure_default());
        let n2_id = g.add_node(n2).unwrap();

        let edge = Edge::typed((n1_id, 0), (n2_id, 0), Type::i32());
        g.add_edge(edge).unwrap();

        let mut registry = ObligationRegistry::collect_from_graph(&g);
        let diagnostics = StructuralAnalyzer::analyze(&g, &mut registry);

        // Should have at least one effect violation diagnostic
        assert!(diagnostics.iter().any(|d| d.severity == Severity::Error
            && d.message.contains("effect violation")));
    }

    #[test]
    fn wellformedness_errors_diagnosed() {
        use torc_core::graph::region::{Region, RegionKind};

        let mut g = Graph::new();

        // Create a region with a non-existent parent → validate() will error
        let mut region = Region::new(RegionKind::Sequential, vec![]);
        region.parent = Some(uuid::Uuid::new_v4()); // fake parent
        g.add_region(region).unwrap();

        let mut registry = ObligationRegistry::collect_from_graph(&g);
        let diagnostics = StructuralAnalyzer::analyze(&g, &mut registry);

        assert!(!diagnostics.is_empty());
        assert!(diagnostics
            .iter()
            .any(|d| d.severity == Severity::Error));
    }
}
