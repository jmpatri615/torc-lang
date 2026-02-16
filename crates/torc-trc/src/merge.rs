//! TRC-level graph merging.
//!
//! Merges two TRC files by combining their graphs and recomputing flags.

use thiserror::Error;

use torc_core::graph::GraphError;

use crate::format::{TrcError, TrcFile};

/// Errors that can occur during TRC file merging.
#[derive(Debug, Error)]
pub enum MergeError {
    #[error("TRC format error: {0}")]
    Trc(#[from] TrcError),

    #[error("graph merge error: {0}")]
    Graph(#[from] GraphError),
}

/// Merge two TRC files into a new one.
///
/// Takes ownership of `base` and merges `other`'s graph into it.
/// The result has flags recomputed from the combined graph.
pub fn merge_trc_files(base: TrcFile, other: &TrcFile) -> Result<TrcFile, MergeError> {
    let mut graph = base.graph;
    graph.merge(&other.graph)?;
    Ok(TrcFile::new(graph))
}

#[cfg(test)]
mod tests {
    use super::*;
    use torc_core::contract::{Contract, ProofWitness};
    use torc_core::graph::edge::Edge;
    use torc_core::graph::node::{Node, NodeKind};
    use torc_core::graph::Graph;
    use torc_core::provenance::Provenance;

    use crate::format::TrcFlags;

    #[test]
    fn merge_two_trc_files() {
        let mut g1 = Graph::new();
        let n1 = Node::new(NodeKind::Literal);
        let n2 = Node::new(NodeKind::Literal);
        let id1 = n1.id;
        let id2 = n2.id;
        g1.add_node(n1).unwrap();
        g1.add_node(n2).unwrap();
        g1.add_edge(Edge::new((id1, 0), (id2, 0))).unwrap();

        let mut g2 = Graph::new();
        let n3 = Node::new(NodeKind::Literal);
        g2.add_node(n3).unwrap();

        let trc1 = TrcFile::new(g1);
        let trc2 = TrcFile::new(g2);

        let merged = merge_trc_files(trc1, &trc2).unwrap();
        assert_eq!(merged.graph.node_count(), 3);
        assert_eq!(merged.graph.edge_count(), 1);
    }

    #[test]
    fn merge_recomputes_flags() {
        // g1 has provenance
        let mut g1 = Graph::new();
        let n1 = Node::new(NodeKind::Literal).with_provenance(Provenance::ai_authored(
            "claude",
            "anthropic",
            "v1",
            "test",
        ));
        g1.add_node(n1).unwrap();

        // g2 has proof witness
        let mut g2 = Graph::new();
        let mut contract = Contract::pure_default();
        contract.proof_witness = Some(ProofWitness {
            hash: "sha256:abc".to_string(),
            solver: "z3".to_string(),
            data: vec![1, 2, 3],
        });
        let n2 = Node::new(NodeKind::Literal).with_contract(contract);
        g2.add_node(n2).unwrap();

        let trc1 = TrcFile::new(g1);
        let trc2 = TrcFile::new(g2);

        let merged = merge_trc_files(trc1, &trc2).unwrap();
        assert!(merged.flags.has(TrcFlags::HAS_PROVENANCE));
        assert!(merged.flags.has(TrcFlags::HAS_PROOFS));
    }
}
