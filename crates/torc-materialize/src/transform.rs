//! Transform traits and registry for graph lowering and optimization.

use std::collections::HashMap;
use std::fmt;

use torc_core::graph::node::{NodeId, NodeKind};
use torc_core::graph::Graph;
use torc_targets::Platform;

/// Result of lowering a single node to a replacement subgraph.
#[derive(Debug)]
pub struct LoweringResult {
    /// The replacement subgraph.
    pub replacement: Graph,
    /// Map from original port indices to replacement node ports:
    /// (original_port_index) -> (replacement_node_id, replacement_port_index).
    pub port_map: HashMap<usize, (NodeId, usize)>,
}

/// Trait for lowering specific node kinds to target-specific subgraphs.
///
/// Object-safe so lowerings can be stored in `Box<dyn NodeLowering>`.
pub trait NodeLowering: fmt::Debug + Send + Sync {
    /// Which node kinds this lowering can handle.
    fn supported_kinds(&self) -> Vec<NodeKind>;

    /// Whether this lowering applies to a specific node in context.
    fn applies_to(&self, kind: &NodeKind, platform: &Platform) -> bool;

    /// Lower a node to a replacement subgraph.
    fn lower(
        &self,
        node_id: NodeId,
        graph: &Graph,
        platform: &Platform,
    ) -> Result<LoweringResult, String>;
}

/// Statistics from applying a graph transform pass.
#[derive(Debug, Clone, Default)]
pub struct TransformStats {
    pub nodes_added: usize,
    pub nodes_removed: usize,
    pub edges_added: usize,
    pub edges_removed: usize,
}

/// Trait for whole-graph transformation passes.
///
/// Object-safe so transforms can be stored in `Box<dyn GraphTransform>`.
pub trait GraphTransform: fmt::Debug + Send + Sync {
    /// Human-readable name of this transform.
    fn name(&self) -> &str;

    /// Apply the transform to the graph, returning statistics.
    fn apply(&self, graph: &mut Graph, platform: &Platform) -> TransformStats;
}

/// Registry of available lowerings and transforms.
#[derive(Debug, Default)]
pub struct TransformRegistry {
    lowerings: Vec<Box<dyn NodeLowering>>,
    transforms: Vec<Box<dyn GraphTransform>>,
}

impl TransformRegistry {
    /// Create an empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a node lowering.
    pub fn register_lowering(&mut self, lowering: Box<dyn NodeLowering>) {
        self.lowerings.push(lowering);
    }

    /// Register a graph transform.
    pub fn register_transform(&mut self, transform: Box<dyn GraphTransform>) {
        self.transforms.push(transform);
    }

    /// Get all registered lowerings.
    pub fn lowerings(&self) -> &[Box<dyn NodeLowering>] {
        &self.lowerings
    }

    /// Get all registered transforms.
    pub fn transforms(&self) -> &[Box<dyn GraphTransform>] {
        &self.transforms
    }

    /// Apply all registered transforms in order.
    pub fn apply_all(&self, graph: &mut Graph, platform: &Platform) -> Vec<TransformStats> {
        self.transforms
            .iter()
            .map(|t| t.apply(graph, platform))
            .collect()
    }
}

/// A no-op transform for testing.
#[derive(Debug)]
pub struct IdentityTransform;

impl GraphTransform for IdentityTransform {
    fn name(&self) -> &str {
        "identity"
    }

    fn apply(&self, _graph: &mut Graph, _platform: &Platform) -> TransformStats {
        TransformStats::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_transform_no_changes() {
        let mut g = Graph::new();
        let platform = Platform::generic_linux_x86_64();
        let t = IdentityTransform;
        let stats = t.apply(&mut g, &platform);
        assert_eq!(stats.nodes_added, 0);
        assert_eq!(stats.nodes_removed, 0);
    }

    #[test]
    fn registry_operations() {
        let mut registry = TransformRegistry::new();
        assert_eq!(registry.transforms().len(), 0);

        registry.register_transform(Box::new(IdentityTransform));
        assert_eq!(registry.transforms().len(), 1);

        let mut g = Graph::new();
        let platform = Platform::generic_linux_x86_64();
        let stats = registry.apply_all(&mut g, &platform);
        assert_eq!(stats.len(), 1);
    }

    #[test]
    fn transform_stats_default() {
        let stats = TransformStats::default();
        assert_eq!(stats.nodes_added, 0);
        assert_eq!(stats.edges_added, 0);
    }
}
