//! Graph canonicalization: deduplication, trivial inlining, region flattening.

use std::collections::HashMap;

use torc_core::graph::node::NodeId;
use torc_core::graph::region::RegionKind;
use torc_core::graph::Graph;
use torc_core::hash::{content_hash, ContentHash};

use crate::error::MaterializationError;

/// Statistics about canonicalization transformations applied.
#[derive(Debug, Clone, Default)]
pub struct CanonicalizationStats {
    /// Number of duplicate nodes removed.
    pub nodes_deduplicated: usize,
    /// Number of nested same-kind regions merged into their parent.
    pub regions_flattened: usize,
    /// Number of single-node regions inlined.
    pub regions_inlined: usize,
    /// Node count before canonicalization.
    pub initial_node_count: usize,
    /// Node count after canonicalization.
    pub final_node_count: usize,
}

/// Compute a content hash for a node based on its semantic content
/// (kind, type_signature, contract) — NOT the UUID.
fn node_content_hash(graph: &Graph, node_id: &NodeId) -> Option<ContentHash> {
    let node = graph.get_node(node_id)?;
    let hashable = (&node.kind, &node.type_signature, &node.contract);
    Some(content_hash(&hashable))
}

/// Find groups of nodes with identical content hashes, within the same region.
/// Nodes in different regions are NOT deduplicated (region membership is semantic).
fn find_duplicate_groups(graph: &Graph) -> Vec<Vec<NodeId>> {
    let mut groups: HashMap<(Option<uuid::Uuid>, ContentHash), Vec<NodeId>> = HashMap::new();

    for node in graph.nodes() {
        let region = graph.containing_region(&node.id).copied();
        if let Some(hash) = node_content_hash(graph, &node.id) {
            groups.entry((region, hash)).or_default().push(node.id);
        }
    }

    groups
        .into_values()
        .filter(|group| group.len() > 1)
        .collect()
}

/// Deduplicate nodes with identical content hashes within the same region.
/// Rewires edges from duplicates to the canonical (first) node.
fn deduplicate_nodes(graph: &mut Graph) -> Result<usize, MaterializationError> {
    let groups = find_duplicate_groups(graph);
    let mut count = 0;

    for group in groups {
        let canonical = group[0];
        for &dup in &group[1..] {
            // Rewire incoming edges of dup to point to canonical.
            // Content hash covers type_signature, so port counts are identical.
            let incoming: Vec<_> = graph.incoming_edges(&dup).to_vec();
            for edge_id in incoming {
                if let Some(edge) = graph.get_edge(&edge_id) {
                    let mut new_edge = edge.clone();
                    new_edge.target.0 = canonical;
                    new_edge.id = uuid::Uuid::new_v4();
                    graph.remove_edge(edge_id)?;
                    graph.add_edge(new_edge)?;
                }
            }

            // Rewire outgoing edges of dup to come from canonical
            let outgoing: Vec<_> = graph.outgoing_edges(&dup).to_vec();
            for edge_id in outgoing {
                if let Some(edge) = graph.get_edge(&edge_id) {
                    let mut new_edge = edge.clone();
                    new_edge.source.0 = canonical;
                    new_edge.id = uuid::Uuid::new_v4();
                    graph.remove_edge(edge_id)?;
                    graph.add_edge(new_edge)?;
                }
            }

            graph.remove_node(dup)?;
            count += 1;
        }
    }

    Ok(count)
}

/// Inline trivial regions: regions containing a single node with no constraints.
fn inline_trivial_regions(graph: &mut Graph) -> Result<usize, MaterializationError> {
    let mut count = 0;

    let trivial_regions: Vec<_> = graph
        .regions()
        .filter(|region| region.children.len() == 1 && region.constraints.is_empty())
        .map(|region| region.id)
        .collect();

    for region_id in trivial_regions {
        graph.remove_region(region_id)?;
        count += 1;
    }

    Ok(count)
}

/// Flatten nested same-kind regions: Sequential-in-Sequential, Parallel-in-Parallel.
/// Only flattens when the inner region has no constraints.
fn flatten_regions(graph: &mut Graph) -> Result<usize, MaterializationError> {
    let mut count = 0;

    loop {
        // Find a candidate pair: child region nested in parent of same kind,
        // child has no constraints.
        let candidate = graph.regions().find_map(|child_region| {
            if !child_region.constraints.is_empty() {
                return None;
            }
            let parent_id = graph.parent_region(&child_region.id)?;
            let parent_region = graph.get_region(parent_id)?;
            let flattenable = matches!(
                (parent_region.kind, child_region.kind),
                (RegionKind::Sequential, RegionKind::Sequential)
                    | (RegionKind::Parallel, RegionKind::Parallel)
            );
            if flattenable {
                Some(child_region.id)
            } else {
                None
            }
        });

        match candidate {
            Some(child_id) => {
                graph.inline_region(child_id)?;
                count += 1;
            }
            None => break,
        }
    }

    Ok(count)
}

/// Canonicalize a graph: deduplicate nodes, inline trivial regions, flatten nesting.
pub fn canonicalize(
    mut graph: Graph,
) -> Result<(Graph, CanonicalizationStats), MaterializationError> {
    let initial_node_count = graph.node_count();

    let nodes_deduplicated = deduplicate_nodes(&mut graph)?;
    let regions_inlined = inline_trivial_regions(&mut graph)?;
    let regions_flattened = flatten_regions(&mut graph)?;

    let final_node_count = graph.node_count();

    let stats = CanonicalizationStats {
        nodes_deduplicated,
        regions_flattened,
        regions_inlined,
        initial_node_count,
        final_node_count,
    };

    Ok((graph, stats))
}

#[cfg(test)]
mod tests {
    use super::*;
    use torc_core::graph::edge::Edge;
    use torc_core::graph::node::{ArithmeticOp, Node, NodeKind};
    use torc_core::graph::region::Region;
    use torc_core::types::{Type, TypeSignature};

    #[test]
    fn canonicalize_empty_graph() {
        let g = Graph::new();
        let (result, stats) = canonicalize(g).unwrap();
        assert_eq!(stats.initial_node_count, 0);
        assert_eq!(stats.final_node_count, 0);
        assert_eq!(result.node_count(), 0);
    }

    #[test]
    fn canonicalize_no_duplicates() {
        let mut g = Graph::new();
        let n1 =
            Node::new(NodeKind::Literal).with_type_signature(TypeSignature::source(Type::i32()));
        let n2 = Node::new(NodeKind::Arithmetic(ArithmeticOp::Add))
            .with_type_signature(TypeSignature::pure_fn(vec![Type::i32()], Type::i32()));
        let id1 = g.add_node(n1).unwrap();
        let id2 = g.add_node(n2).unwrap();
        g.add_edge(Edge::typed((id1, 0), (id2, 0), Type::i32()))
            .unwrap();

        let (result, stats) = canonicalize(g).unwrap();
        assert_eq!(stats.nodes_deduplicated, 0);
        assert_eq!(result.node_count(), 2);
    }

    #[test]
    fn deduplicate_identical_nodes() {
        let mut g = Graph::new();
        let n1 =
            Node::new(NodeKind::Literal).with_type_signature(TypeSignature::source(Type::i32()));
        let n2 =
            Node::new(NodeKind::Literal).with_type_signature(TypeSignature::source(Type::i32()));

        let consumer = Node::new(NodeKind::Arithmetic(ArithmeticOp::Add))
            .with_type_signature(TypeSignature::new(
                vec![Type::i32(), Type::i32()],
                vec![Type::i32()],
            ));

        let id1 = g.add_node(n1).unwrap();
        let id2 = g.add_node(n2).unwrap();
        let cid = g.add_node(consumer).unwrap();
        g.add_edge(Edge::typed((id1, 0), (cid, 0), Type::i32()))
            .unwrap();
        g.add_edge(Edge::typed((id2, 0), (cid, 1), Type::i32()))
            .unwrap();

        let (result, stats) = canonicalize(g).unwrap();
        assert_eq!(stats.nodes_deduplicated, 1);
        assert_eq!(result.node_count(), 2);
    }

    #[test]
    fn inline_trivial_region() {
        let mut g = Graph::new();
        let n1 = Node::new(NodeKind::Literal);
        let id1 = g.add_node(n1).unwrap();
        let region = Region::new(RegionKind::Sequential, vec![id1]);
        g.add_region(region).unwrap();

        let (result, stats) = canonicalize(g).unwrap();
        assert_eq!(stats.regions_inlined, 1);
        assert_eq!(result.region_count(), 0);
    }

    #[test]
    fn flatten_nested_sequential_regions() {
        let mut g = Graph::new();
        let n1 = Node::new(NodeKind::Literal);
        let n2 = Node::new(NodeKind::Arithmetic(ArithmeticOp::Add));
        let n3 = Node::new(NodeKind::Arithmetic(ArithmeticOp::Mul));
        let n4 = Node::new(NodeKind::Conversion);
        let id1 = g.add_node(n1).unwrap();
        let id2 = g.add_node(n2).unwrap();
        let id3 = g.add_node(n3).unwrap();
        let id4 = g.add_node(n4).unwrap();

        // Inner sequential region with 2 children (not trivial)
        let inner = Region::new(RegionKind::Sequential, vec![id1, id2]);
        let inner_id = g.add_region(inner).unwrap();

        // Outer sequential region with 2 children (not trivial)
        let outer = Region::new(RegionKind::Sequential, vec![id3, id4]);
        let outer_id = g.add_region(outer).unwrap();

        g.set_region_parent(inner_id, outer_id).unwrap();

        let (_result, stats) = canonicalize(g).unwrap();
        assert_eq!(stats.regions_flattened, 1);
    }

    #[test]
    fn no_flatten_different_kind_regions() {
        let mut g = Graph::new();
        let n1 = Node::new(NodeKind::Literal);
        let id1 = g.add_node(n1).unwrap();

        let inner = Region::new(RegionKind::Parallel, vec![id1]);
        let inner_id = g.add_region(inner).unwrap();

        let outer = Region::new(RegionKind::Sequential, vec![]);
        let outer_id = g.add_region(outer).unwrap();

        g.set_region_parent(inner_id, outer_id).unwrap();

        let (_result, stats) = canonicalize(g).unwrap();
        assert_eq!(stats.regions_flattened, 0);
    }
}
