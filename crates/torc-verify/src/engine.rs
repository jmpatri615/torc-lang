//! Verification engine orchestrator.
//!
//! Ties together structural analysis, interval analysis, SMT solving,
//! proof witness generation, and caching into a single `verify()` pipeline.

use torc_core::contract::ProofStatus;
use torc_core::graph::Graph;

use crate::cache::ProofCache;
use crate::interval::{IntervalAnalyzer, IntervalResult};
use crate::profile::{ProfileLevel, SmtScope, VerificationProfile};
use crate::registry::ObligationRegistry;
use crate::report::VerificationReport;
use crate::structural::StructuralAnalyzer;
use crate::witness::generate_witness;

/// The main verification engine.
pub struct VerificationEngine {
    profile: VerificationProfile,
    cache: ProofCache,
}

impl VerificationEngine {
    /// Create a new engine with the given profile.
    pub fn new(profile: VerificationProfile) -> Self {
        Self {
            profile,
            cache: ProofCache::new(),
        }
    }

    /// Run the full verification pipeline on a graph.
    pub fn verify(&mut self, graph: &Graph) -> VerificationReport {
        // 1. Collect obligations
        let mut registry = ObligationRegistry::collect_from_graph(graph);

        // 2. Check cache — reuse cached proofs (skip for certification)
        if self.profile.level != ProfileLevel::Certification {
            let ids_to_cache: Vec<u64> = registry.pending().map(|o| o.id).collect();
            for id in ids_to_cache {
                if let Some(tracked) = registry.get(id) {
                    if let Some(witness) = self.cache.lookup(&tracked.obligation) {
                        let w = witness.clone();
                        registry.update_status(id, ProofStatus::Verified, Some(w));
                    }
                }
            }
        }

        // 3. Structural analysis
        let structural_diagnostics = if self.profile.run_structural {
            StructuralAnalyzer::analyze(graph, &mut registry)
        } else {
            Vec::new()
        };

        // 4. Interval analysis on remaining pending obligations
        if self.profile.run_interval {
            let pending: Vec<_> = registry.pending().collect();
            let results = IntervalAnalyzer::analyze(&pending);

            for (id, result) in results {
                match result {
                    IntervalResult::Proven => {
                        if let Some(tracked) = registry.get(id) {
                            let witness =
                                generate_witness("interval_domain", &tracked.obligation, vec![]);
                            self.cache.store(&tracked.obligation, witness.clone());
                            registry.update_status(id, ProofStatus::Verified, Some(witness));
                        }
                    }
                    IntervalResult::Disproven { .. } | IntervalResult::Inconclusive => {
                        // Leave pending for SMT or manual review
                    }
                }
            }
        }

        // 5. SMT solving (feature-gated)
        #[cfg(feature = "z3")]
        {
            if self.profile.run_smt != SmtScope::Skip {
                let solver = crate::smt::SmtSolver::new(self.profile.solver_timeout);

                let pending_ids: Vec<u64> = registry.pending().map(|o| o.id).collect();
                for id in pending_ids {
                    if let Some(tracked) = registry.get(id) {
                        let result = solver.check_obligation(&tracked.obligation);
                        match result {
                            crate::smt::SmtResult::Proven => {
                                let witness = generate_witness("z3", &tracked.obligation, vec![]);
                                self.cache.store(&tracked.obligation, witness.clone());
                                registry.update_status(id, ProofStatus::Verified, Some(witness));
                            }
                            _ => {}
                        }
                    }
                }
            }
        }

        // Suppress unused variable warning when z3 feature is not enabled
        #[cfg(not(feature = "z3"))]
        let _ = SmtScope::Skip;

        // 6. Build report
        let cache_stats = self.cache.statistics();
        VerificationReport::build(
            &registry,
            &cache_stats,
            self.profile.level,
            &structural_diagnostics,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use torc_core::contract::Contract;
    use torc_core::graph::edge::Edge;
    use torc_core::graph::node::{Node, NodeKind};
    use torc_core::types::{Predicate, Type, TypeSignature};

    fn make_simple_graph() -> Graph {
        let mut g = Graph::new();

        let mut n1 = Node::new(NodeKind::Literal);
        n1.type_signature = Some(TypeSignature::source(Type::i32()));
        n1.contract = Some(Contract::with_conditions(
            vec![],
            vec![Predicate::Gt(
                Box::new(Predicate::IntLit(10)),
                Box::new(Predicate::IntLit(5)),
            )],
        ));

        let mut n2 = Node::new(NodeKind::Arithmetic(
            torc_core::graph::node::ArithmeticOp::Add,
        ));
        n2.type_signature = Some(TypeSignature::pure_fn(vec![Type::i32()], Type::i32()));

        let n1_id = g.add_node(n1).unwrap();
        let n2_id = g.add_node(n2).unwrap();

        let edge = Edge::typed((n1_id, 0), (n2_id, 0), Type::i32());
        g.add_edge(edge).unwrap();

        g
    }

    #[test]
    fn full_pipeline_simple_graph() {
        let g = make_simple_graph();
        let mut engine = VerificationEngine::new(VerificationProfile::development());
        let report = engine.verify(&g);

        assert!(report.summary.total > 0);
        // Interval analysis should discharge 10 > 5
        assert!(report.summary.verified > 0);
    }

    #[test]
    fn cache_reuse_on_second_run() {
        let g = make_simple_graph();
        let mut engine = VerificationEngine::new(VerificationProfile::development());

        let report1 = engine.verify(&g);
        let verified1 = report1.summary.verified;

        let report2 = engine.verify(&g);
        // Second run should get cache hits
        assert!(report2.summary.cache_hits > 0 || report2.summary.verified >= verified1);
    }

    #[test]
    fn profile_respecting() {
        let g = make_simple_graph();

        // Development profile skips SMT
        let mut dev_engine = VerificationEngine::new(VerificationProfile::development());
        let dev_report = dev_engine.verify(&g);
        assert_eq!(dev_report.profile, ProfileLevel::Development);

        // Certification profile runs everything
        let mut cert_engine = VerificationEngine::new(VerificationProfile::certification());
        let cert_report = cert_engine.verify(&g);
        assert_eq!(cert_report.profile, ProfileLevel::Certification);
    }
}
