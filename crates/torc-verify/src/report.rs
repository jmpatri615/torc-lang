//! Verification report with summary statistics and diagnostics.

use std::collections::HashMap;
use std::fmt;

use torc_core::contract::ProofStatus;

use crate::cache::CacheStats;
use crate::profile::ProfileLevel;
use crate::registry::ObligationRegistry;
use crate::structural::StructuralDiagnostic;

/// Severity level for diagnostics.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
    Info,
}

impl fmt::Display for Severity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Severity::Error => write!(f, "ERROR"),
            Severity::Warning => write!(f, "WARN"),
            Severity::Info => write!(f, "INFO"),
        }
    }
}

/// A diagnostic message about a specific obligation.
#[derive(Debug, Clone)]
pub struct Diagnostic {
    pub obligation_id: u64,
    pub severity: Severity,
    pub message: String,
    pub context: String,
    pub counterexample: Option<HashMap<String, String>>,
    pub suggestions: Vec<String>,
}

/// Summary statistics for a verification run.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReportSummary {
    pub total: usize,
    pub verified: usize,
    pub pending: usize,
    pub waived: usize,
    pub failed: usize,
    pub cache_hits: usize,
}

/// The complete verification report.
#[derive(Debug, Clone)]
pub struct VerificationReport {
    pub summary: ReportSummary,
    pub diagnostics: Vec<Diagnostic>,
    pub profile: ProfileLevel,
}

impl VerificationReport {
    /// Build a report from a registry, cache statistics, and structural diagnostics.
    pub fn build(
        registry: &ObligationRegistry,
        cache_stats: &CacheStats,
        profile: ProfileLevel,
        structural_diagnostics: &[StructuralDiagnostic],
    ) -> Self {
        let reg_stats = registry.statistics();

        let summary = ReportSummary {
            total: reg_stats.total,
            verified: reg_stats.verified,
            pending: reg_stats.pending,
            waived: reg_stats.waived,
            failed: reg_stats.assumed,
            cache_hits: cache_stats.hits,
        };

        let mut diagnostics = Vec::new();

        // Convert structural diagnostics to report diagnostics
        for sd in structural_diagnostics {
            let severity = match sd.severity {
                crate::structural::Severity::Error => Severity::Error,
                crate::structural::Severity::Warning => Severity::Warning,
            };
            let mut suggestions = Vec::new();
            if let Some(ref s) = sd.suggestion {
                suggestions.push(s.clone());
            }
            diagnostics.push(Diagnostic {
                obligation_id: 0,
                severity,
                message: sd.message.clone(),
                context: sd
                    .node_id
                    .map(|id| format!("node {id}"))
                    .unwrap_or_default(),
                counterexample: None,
                suggestions,
            });
        }

        // Generate diagnostics for non-verified obligations
        for tracked in registry.all() {
            match tracked.obligation.status {
                ProofStatus::Pending => {
                    let suggestions = suggest_for_kind(&tracked.obligation.kind);
                    diagnostics.push(Diagnostic {
                        obligation_id: tracked.id,
                        severity: Severity::Warning,
                        message: format!(
                            "obligation remains pending: {}",
                            tracked.obligation.description
                        ),
                        context: format!("{:?}", tracked.obligation.kind),
                        counterexample: None,
                        suggestions,
                    });
                }
                ProofStatus::Assumed => {
                    diagnostics.push(Diagnostic {
                        obligation_id: tracked.id,
                        severity: Severity::Info,
                        message: format!(
                            "obligation assumed without proof: {}",
                            tracked.obligation.description
                        ),
                        context: format!("{:?}", tracked.obligation.kind),
                        counterexample: None,
                        suggestions: vec!["Provide proof or waiver with justification".into()],
                    });
                }
                ProofStatus::Waived => {
                    diagnostics.push(Diagnostic {
                        obligation_id: tracked.id,
                        severity: Severity::Info,
                        message: format!("obligation waived: {}", tracked.obligation.description),
                        context: format!("{:?}", tracked.obligation.kind),
                        counterexample: None,
                        suggestions: vec![],
                    });
                }
                ProofStatus::Verified => {}
            }
        }

        Self {
            summary,
            diagnostics,
            profile,
        }
    }
}

/// Generate suggestions based on obligation kind.
fn suggest_for_kind(kind: &torc_core::contract::ObligationKind) -> Vec<String> {
    use torc_core::contract::ObligationKind;
    match kind {
        ObligationKind::TypeRefinement => vec![
            "Add clamping: clamp(output, lo, hi)".into(),
            "Waive obligation (requires justification)".into(),
        ],
        ObligationKind::Precondition => vec![
            "Strengthen precondition on predecessor".into(),
            "Waive obligation (requires justification)".into(),
        ],
        ObligationKind::Postcondition => vec![
            "Weaken postcondition or add guard".into(),
            "Waive obligation (requires justification)".into(),
        ],
        ObligationKind::ResourceBound => vec![
            "Optimize implementation to meet bound".into(),
            "Relax resource bound if safe".into(),
            "Waive obligation (requires justification)".into(),
        ],
        ObligationKind::Linearity => vec![
            "Ensure linear values are consumed exactly once".into(),
            "Waive obligation (requires justification)".into(),
        ],
        ObligationKind::Termination => vec![
            "Provide a ranking function or variant".into(),
            "Waive obligation (requires justification)".into(),
        ],
    }
}

impl fmt::Display for VerificationReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "=== Verification Report ({:?}) ===", self.profile)?;
        writeln!(
            f,
            "Total: {} | Verified: {} | Pending: {} | Waived: {} | Failed: {} | Cache hits: {}",
            self.summary.total,
            self.summary.verified,
            self.summary.pending,
            self.summary.waived,
            self.summary.failed,
            self.summary.cache_hits,
        )?;

        if self.diagnostics.is_empty() {
            writeln!(f, "No diagnostics.")?;
        } else {
            writeln!(f, "--- Diagnostics ---")?;
            for diag in &self.diagnostics {
                writeln!(
                    f,
                    "[{}] #{}: {} ({})",
                    diag.severity, diag.obligation_id, diag.message, diag.context
                )?;
                if let Some(ref ce) = diag.counterexample {
                    writeln!(f, "  Counterexample: {ce:?}")?;
                }
                for s in &diag.suggestions {
                    writeln!(f, "  Suggestion: {s}")?;
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use torc_core::contract::{Contract, ProofStatus};
    use torc_core::graph::edge::Edge;
    use torc_core::graph::node::{Node, NodeKind};
    use torc_core::graph::Graph;
    use torc_core::types::{Predicate, Type, TypeSignature};

    fn make_graph_with_obligations() -> Graph {
        let mut g = Graph::new();
        let mut n1 = Node::new(NodeKind::Literal);
        n1.type_signature = Some(TypeSignature::source(Type::i32()));
        n1.contract = Some(Contract::with_conditions(
            vec![],
            vec![Predicate::positive("output")],
        ));
        let mut n2 = Node::new(NodeKind::Arithmetic(
            torc_core::graph::node::ArithmeticOp::Add,
        ));
        n2.type_signature = Some(TypeSignature::pure_fn(vec![Type::i32()], Type::i32()));
        n2.contract = Some(Contract::with_conditions(
            vec![Predicate::positive("input")],
            vec![],
        ));
        let n1_id = g.add_node(n1).unwrap();
        let n2_id = g.add_node(n2).unwrap();
        g.add_edge(Edge::typed((n1_id, 0), (n2_id, 0), Type::i32()))
            .unwrap();
        g
    }

    #[test]
    fn summary_statistics_correct() {
        let g = make_graph_with_obligations();
        let mut registry = crate::registry::ObligationRegistry::collect_from_graph(&g);

        // Verify one obligation
        if let Some(first) = registry.all().first() {
            let id = first.id;
            registry.update_status(id, ProofStatus::Verified, None);
        }

        let cache_stats = CacheStats::default();
        let report =
            VerificationReport::build(&registry, &cache_stats, ProfileLevel::Development, &[]);

        assert_eq!(report.summary.total, registry.len());
        assert_eq!(report.summary.verified, 1);
        assert_eq!(report.summary.pending, registry.len() - 1);
        assert_eq!(report.profile, ProfileLevel::Development);
    }

    #[test]
    fn diagnostic_formatting() {
        let g = make_graph_with_obligations();
        let registry = crate::registry::ObligationRegistry::collect_from_graph(&g);
        let cache_stats = CacheStats::default();
        let report =
            VerificationReport::build(&registry, &cache_stats, ProfileLevel::Development, &[]);

        let output = format!("{report}");
        assert!(output.contains("Verification Report"));
        assert!(output.contains("Total:"));
    }

    #[test]
    fn suggestion_generation() {
        use torc_core::contract::ObligationKind;

        let suggestions = suggest_for_kind(&ObligationKind::TypeRefinement);
        assert!(suggestions.iter().any(|s| s.contains("clamp")));

        let suggestions = suggest_for_kind(&ObligationKind::Precondition);
        assert!(suggestions.iter().any(|s| s.contains("Strengthen")));

        let suggestions = suggest_for_kind(&ObligationKind::Postcondition);
        assert!(suggestions.iter().any(|s| s.contains("Weaken")));

        let suggestions = suggest_for_kind(&ObligationKind::Termination);
        assert!(suggestions.iter().any(|s| s.contains("ranking")));
    }
}
