//! Verification gate: decides whether a graph passes verification for materialization.

use torc_core::graph::Graph;
use torc_verify::engine::VerificationEngine;
use torc_verify::profile::VerificationProfile;
use torc_verify::report::VerificationReport;

use crate::error::MaterializationError;

/// The gate's decision on whether materialization may proceed.
#[derive(Debug)]
pub enum GateDecision {
    /// All obligations verified (or waived, within limits).
    Pass {
        report: VerificationReport,
        waived: usize,
    },
    /// Materialization must stop.
    Halt {
        failed: usize,
        pending: usize,
        report: VerificationReport,
    },
}

/// Configuration for the verification gate.
#[derive(Debug, Clone)]
pub struct GateConfig {
    /// Verification profile to use.
    pub profile: VerificationProfile,
    /// If true, pending obligations are treated as blocking (halt).
    /// If false, only failed obligations block.
    pub strict: bool,
    /// Maximum number of waivers allowed before halting.
    pub max_waivers: Option<usize>,
}

impl GateConfig {
    /// Development gate: lenient, pending obligations allowed.
    pub fn development() -> Self {
        Self {
            profile: VerificationProfile::development(),
            strict: false,
            max_waivers: None,
        }
    }

    /// Strict gate: pending obligations block, no waivers allowed.
    pub fn strict() -> Self {
        Self {
            profile: VerificationProfile::certification(),
            strict: true,
            max_waivers: Some(0),
        }
    }
}

/// Run the verification gate, returning the decision (which owns the report).
pub fn verification_gate(graph: &Graph, config: &GateConfig) -> GateDecision {
    let mut engine = VerificationEngine::new(config.profile.clone());
    let report = engine.verify(graph);

    let failed = report.summary.failed;
    let pending = report.summary.pending;
    let waived = report.summary.waived;

    // Check waiver limit
    if let Some(max) = config.max_waivers {
        if waived > max {
            return GateDecision::Halt {
                failed,
                pending,
                report,
            };
        }
    }

    // Check for failures
    if failed > 0 {
        return GateDecision::Halt {
            failed,
            pending,
            report,
        };
    }

    // In strict mode, pending obligations also block
    if config.strict && pending > 0 {
        return GateDecision::Halt {
            failed,
            pending,
            report,
        };
    }

    GateDecision::Pass { report, waived }
}

/// Run the verification gate and return an error if it halts.
pub fn gate_or_halt(
    graph: &Graph,
    config: &GateConfig,
) -> Result<VerificationReport, MaterializationError> {
    match verification_gate(graph, config) {
        GateDecision::Pass { report, .. } => Ok(report),
        GateDecision::Halt {
            failed, pending, ..
        } => Err(MaterializationError::VerificationFailed { failed, pending }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use torc_core::graph::edge::Edge;
    use torc_core::graph::node::{ArithmeticOp, Node, NodeKind};
    use torc_core::types::{Type, TypeSignature};

    fn make_clean_graph() -> Graph {
        let mut g = Graph::new();
        let n1 =
            Node::new(NodeKind::Literal).with_type_signature(TypeSignature::source(Type::i32()));
        let n2 = Node::new(NodeKind::Arithmetic(ArithmeticOp::Add))
            .with_type_signature(TypeSignature::pure_fn(vec![Type::i32()], Type::i32()));
        let id1 = g.add_node(n1).unwrap();
        let id2 = g.add_node(n2).unwrap();
        g.add_edge(Edge::typed((id1, 0), (id2, 0), Type::i32()))
            .unwrap();
        g
    }

    #[test]
    fn development_gate_passes_clean_graph() {
        let g = make_clean_graph();
        let config = GateConfig::development();
        let decision = verification_gate(&g, &config);
        assert!(matches!(decision, GateDecision::Pass { .. }));
    }

    #[test]
    fn gate_or_halt_returns_report_on_pass() {
        let g = make_clean_graph();
        let config = GateConfig::development();
        let result = gate_or_halt(&g, &config);
        assert!(result.is_ok());
    }

    #[test]
    fn empty_graph_passes_gate() {
        let g = Graph::new();
        let config = GateConfig::strict();
        let decision = verification_gate(&g, &config);
        assert!(matches!(decision, GateDecision::Pass { .. }));
    }

    #[test]
    fn strict_gate_blocks_unprovable_obligation() {
        use torc_core::contract::Contract;
        use torc_core::types::Predicate;

        let mut g = Graph::new();
        let mut n1 =
            Node::new(NodeKind::Literal).with_type_signature(TypeSignature::source(Type::i32()));
        // `output > 0` cannot be statically verified for an unconstrained literal
        n1.contract = Some(Contract::with_conditions(
            vec![],
            vec![Predicate::positive("output")],
        ));
        g.add_node(n1).unwrap();

        // Development mode: lenient, should pass even with pending obligations
        let dev_config = GateConfig::development();
        let dev_decision = verification_gate(&g, &dev_config);
        assert!(
            matches!(dev_decision, GateDecision::Pass { .. }),
            "development gate should pass with pending obligations"
        );

        // Strict mode: pending obligations should block
        let strict_config = GateConfig::strict();
        let strict_decision = verification_gate(&g, &strict_config);
        assert!(
            matches!(strict_decision, GateDecision::Halt { pending, .. } if pending > 0),
            "strict gate should halt on pending obligations"
        );
    }
}
