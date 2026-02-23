//! Obligation registry: collects, tracks, and updates proof obligations.

use torc_core::contract::{ObligationKind, ProofObligation, ProofStatus, ProofWitness, Waiver};
use torc_core::graph::edge::EdgeId;
use torc_core::graph::node::NodeId;
use torc_core::graph::Graph;

/// A proof obligation with tracking metadata.
#[derive(Debug, Clone)]
pub struct TrackedObligation {
    /// Sequential ID within this registry.
    pub id: u64,
    /// The underlying proof obligation.
    pub obligation: ProofObligation,
    /// Source node (if applicable).
    pub node_id: Option<NodeId>,
    /// Source edge (if applicable).
    pub edge_id: Option<EdgeId>,
}

/// Statistics about obligation statuses.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RegistryStats {
    pub total: usize,
    pub verified: usize,
    pub pending: usize,
    pub assumed: usize,
    pub waived: usize,
}

/// Collects and tracks proof obligations from graph validation.
#[derive(Debug, Clone)]
pub struct ObligationRegistry {
    obligations: Vec<TrackedObligation>,
    next_id: u64,
}

impl ObligationRegistry {
    /// Create an empty registry.
    pub fn new() -> Self {
        Self {
            obligations: Vec::new(),
            next_id: 0,
        }
    }

    /// Collect all obligations from a graph by running type and contract validation.
    pub fn collect_from_graph(graph: &Graph) -> Self {
        let mut registry = Self::new();

        // Collect type/edge obligations (may include refinement subtyping obligations)
        match graph.validate_types() {
            Ok(obligations) => {
                for ob in obligations {
                    registry.add(ob, None, None);
                }
            }
            Err(_errors) => {
                // Structural errors are handled separately by StructuralAnalyzer;
                // validate_types also calls validate_contracts internally, so
                // if it fails we still collect contract obligations directly.
                let contract_obs = graph.validate_contracts();
                for ob in contract_obs {
                    registry.add(ob, None, None);
                }
            }
        }

        registry
    }

    /// Add an obligation with optional source metadata.
    fn add(
        &mut self,
        obligation: ProofObligation,
        node_id: Option<NodeId>,
        edge_id: Option<EdgeId>,
    ) {
        let id = self.next_id;
        self.next_id += 1;
        self.obligations.push(TrackedObligation {
            id,
            obligation,
            node_id,
            edge_id,
        });
    }

    /// Iterate over all obligations.
    pub fn all(&self) -> &[TrackedObligation] {
        &self.obligations
    }

    /// Iterate over pending obligations.
    pub fn pending(&self) -> impl Iterator<Item = &TrackedObligation> {
        self.obligations
            .iter()
            .filter(|o| o.obligation.status == ProofStatus::Pending)
    }

    /// Filter obligations by kind.
    pub fn by_kind(&self, kind: ObligationKind) -> impl Iterator<Item = &TrackedObligation> {
        self.obligations
            .iter()
            .filter(move |o| o.obligation.kind == kind)
    }

    /// Update the status and optional witness of an obligation by ID.
    pub fn update_status(&mut self, id: u64, status: ProofStatus, witness: Option<ProofWitness>) {
        if let Some(tracked) = self.obligations.iter_mut().find(|o| o.id == id) {
            tracked.obligation.status = status;
            if witness.is_some() {
                tracked.obligation.witness = witness;
            }
        }
    }

    /// Apply a waiver to an obligation, setting its status to Waived.
    pub fn apply_waiver(&mut self, id: u64, waiver: Waiver) {
        if let Some(tracked) = self.obligations.iter_mut().find(|o| o.id == id) {
            tracked.obligation.status = ProofStatus::Waived;
            tracked.obligation.waiver = Some(waiver);
        }
    }

    /// Compute statistics about obligation statuses.
    pub fn statistics(&self) -> RegistryStats {
        let mut stats = RegistryStats {
            total: self.obligations.len(),
            ..Default::default()
        };
        for o in &self.obligations {
            match o.obligation.status {
                ProofStatus::Verified => stats.verified += 1,
                ProofStatus::Pending => stats.pending += 1,
                ProofStatus::Assumed => stats.assumed += 1,
                ProofStatus::Waived => stats.waived += 1,
            }
        }
        stats
    }

    /// Total number of tracked obligations.
    pub fn len(&self) -> usize {
        self.obligations.len()
    }

    /// Whether the registry is empty.
    pub fn is_empty(&self) -> bool {
        self.obligations.is_empty()
    }
}

impl Default for ObligationRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use torc_core::contract::{Contract, ObligationKind, ProofStatus};
    use torc_core::graph::edge::Edge;
    use torc_core::graph::node::{Node, NodeKind};
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

        let edge = Edge::typed((n1_id, 0), (n2_id, 0), Type::i32());
        g.add_edge(edge).unwrap();

        g
    }

    #[test]
    fn collect_from_graph() {
        let g = make_graph_with_obligations();
        let registry = ObligationRegistry::collect_from_graph(&g);
        // Should have at least the postcondition + precondition + edge-crossing obligation
        assert!(registry.len() >= 2);
    }

    #[test]
    fn filter_by_kind() {
        let g = make_graph_with_obligations();
        let registry = ObligationRegistry::collect_from_graph(&g);

        let preconditions: Vec<_> = registry.by_kind(ObligationKind::Precondition).collect();
        for p in &preconditions {
            assert_eq!(p.obligation.kind, ObligationKind::Precondition);
        }

        let postconditions: Vec<_> = registry.by_kind(ObligationKind::Postcondition).collect();
        for p in &postconditions {
            assert_eq!(p.obligation.kind, ObligationKind::Postcondition);
        }
    }

    #[test]
    fn update_status() {
        let g = make_graph_with_obligations();
        let mut registry = ObligationRegistry::collect_from_graph(&g);

        let first_id = registry.all()[0].id;
        assert_eq!(registry.all()[0].obligation.status, ProofStatus::Pending);

        registry.update_status(first_id, ProofStatus::Verified, None);
        assert_eq!(registry.all()[0].obligation.status, ProofStatus::Verified);
    }

    #[test]
    fn statistics() {
        let g = make_graph_with_obligations();
        let mut registry = ObligationRegistry::collect_from_graph(&g);
        let stats = registry.statistics();
        assert_eq!(stats.total, registry.len());
        assert_eq!(stats.pending, registry.len());

        // Verify one obligation
        let first_id = registry.all()[0].id;
        registry.update_status(first_id, ProofStatus::Verified, None);
        let stats = registry.statistics();
        assert_eq!(stats.verified, 1);
        assert_eq!(stats.pending, stats.total - 1);
    }
}
