//! Core graph data structures: nodes, edges, regions, and the graph container.
//!
//! A Torc program is a directed acyclic graph (DAG) at the expression level,
//! with explicit cycle structures for iteration and recursion. The graph
//! consists of nodes (computation), edges (data dependencies), and regions
//! (scope boundaries).

pub mod constraints;
pub mod edge;
pub mod node;
pub mod port;
pub mod region;

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use std::collections::HashSet;

use self::edge::{Edge, EdgeId};
use self::node::{Node, NodeId, NodeKind};
use self::region::{Region, RegionId};

use crate::contract::{EffectSet, ObligationKind, ProofObligation, ProofStatus};
use crate::types::check::types_compatible;
use crate::types::{Linearity, Predicate};

/// Errors that can occur during graph construction or validation.
#[derive(Debug, Error)]
pub enum GraphError {
    #[error("node not found: {0}")]
    NodeNotFound(NodeId),

    #[error("edge not found: {0}")]
    EdgeNotFound(EdgeId),

    #[error("region not found: {0}")]
    RegionNotFound(RegionId),

    #[error("dangling edge: source node {src} or target node {dst} not in graph")]
    DanglingEdge { src: NodeId, dst: NodeId },

    #[error("port index {port} out of range for node {node}")]
    PortOutOfRange { node: NodeId, port: usize },

    #[error("duplicate node id: {0}")]
    DuplicateNode(NodeId),

    #[error("duplicate edge id: {0}")]
    DuplicateEdge(EdgeId),

    #[error("cycle detected involving node {0}")]
    CycleDetected(NodeId),

    #[error("node {child} not contained in its declared region {region}")]
    RegionContainment { child: NodeId, region: RegionId },

    #[error("duplicate child node {child} in region {region}")]
    DuplicateRegionChild { child: NodeId, region: RegionId },

    #[error("linearity violation: {kind:?} value at node {node} port {port} has {consumers} consumer(s)")]
    LinearityViolation {
        node: NodeId,
        port: usize,
        kind: crate::types::Linearity,
        consumers: usize,
    },

    #[error("effect violation: node {node} declares {declared} but depends on {required}")]
    EffectViolation {
        node: NodeId,
        declared: String,
        required: String,
    },

    #[error("type mismatch on edge {edge}: expected {expected}, found {found}")]
    TypeMismatch {
        edge: EdgeId,
        expected: String,
        found: String,
    },
}

/// The core graph container for a Torc program.
///
/// Stores nodes, edges, and regions with efficient lookup by ID.
/// Provides topological ordering, subgraph extraction, and validation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Graph {
    nodes: HashMap<NodeId, Node>,
    edges: HashMap<EdgeId, Edge>,
    regions: HashMap<RegionId, Region>,

    /// Index: node -> outgoing edges (edges where this node is the source)
    outgoing: HashMap<NodeId, Vec<EdgeId>>,
    /// Index: node -> incoming edges (edges where this node is the target)
    incoming: HashMap<NodeId, Vec<EdgeId>>,
    /// Index: region -> child nodes
    region_children: HashMap<RegionId, Vec<NodeId>>,
    /// Index: node -> containing region (if any)
    node_region: HashMap<NodeId, RegionId>,
    /// Index: child region -> parent region
    region_parent: HashMap<RegionId, RegionId>,
}

impl Graph {
    /// Create a new empty graph.
    pub fn new() -> Self {
        Self {
            nodes: HashMap::new(),
            edges: HashMap::new(),
            regions: HashMap::new(),
            outgoing: HashMap::new(),
            incoming: HashMap::new(),
            region_children: HashMap::new(),
            node_region: HashMap::new(),
            region_parent: HashMap::new(),
        }
    }

    /// Insert a node into the graph.
    pub fn add_node(&mut self, node: Node) -> Result<NodeId, GraphError> {
        let id = node.id;
        if self.nodes.contains_key(&id) {
            return Err(GraphError::DuplicateNode(id));
        }
        self.outgoing.entry(id).or_default();
        self.incoming.entry(id).or_default();
        self.nodes.insert(id, node);
        Ok(id)
    }

    /// Insert an edge into the graph. Both source and target nodes must exist.
    pub fn add_edge(&mut self, edge: Edge) -> Result<EdgeId, GraphError> {
        let id = edge.id;
        if self.edges.contains_key(&id) {
            return Err(GraphError::DuplicateEdge(id));
        }
        let source_node = edge.source.0;
        let target_node = edge.target.0;
        if !self.nodes.contains_key(&source_node) || !self.nodes.contains_key(&target_node) {
            return Err(GraphError::DanglingEdge {
                src: source_node,
                dst: target_node,
            });
        }
        self.outgoing.entry(source_node).or_default().push(id);
        self.incoming.entry(target_node).or_default().push(id);
        self.edges.insert(id, edge);
        Ok(id)
    }

    /// Insert a region into the graph.
    pub fn add_region(&mut self, region: Region) -> Result<RegionId, GraphError> {
        let id = region.id;
        // Validate that all child nodes exist and are unique
        let mut seen = HashSet::new();
        for child_id in &region.children {
            if !self.nodes.contains_key(child_id) {
                return Err(GraphError::NodeNotFound(*child_id));
            }
            if !seen.insert(*child_id) {
                return Err(GraphError::DuplicateRegionChild {
                    child: *child_id,
                    region: id,
                });
            }
        }
        for child_id in &region.children {
            self.node_region.insert(*child_id, id);
        }
        self.region_children.insert(id, region.children.clone());
        if let Some(parent_id) = region.parent {
            self.region_parent.insert(id, parent_id);
        }
        self.regions.insert(id, region);
        Ok(id)
    }

    /// Look up a node by ID.
    pub fn get_node(&self, id: &NodeId) -> Option<&Node> {
        self.nodes.get(id)
    }

    /// Look up a node by ID (mutable).
    pub fn get_node_mut(&mut self, id: &NodeId) -> Option<&mut Node> {
        self.nodes.get_mut(id)
    }

    /// Look up an edge by ID.
    pub fn get_edge(&self, id: &EdgeId) -> Option<&Edge> {
        self.edges.get(id)
    }

    /// Look up a region by ID.
    pub fn get_region(&self, id: &RegionId) -> Option<&Region> {
        self.regions.get(id)
    }

    /// Get all outgoing edges from a node.
    pub fn outgoing_edges(&self, node_id: &NodeId) -> &[EdgeId] {
        self.outgoing
            .get(node_id)
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }

    /// Get all incoming edges to a node.
    pub fn incoming_edges(&self, node_id: &NodeId) -> &[EdgeId] {
        self.incoming
            .get(node_id)
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }

    /// Get the region containing a node, if any.
    pub fn containing_region(&self, node_id: &NodeId) -> Option<&RegionId> {
        self.node_region.get(node_id)
    }

    /// Look up a region by ID (mutable).
    pub fn get_region_mut(&mut self, id: &RegionId) -> Option<&mut Region> {
        self.regions.get_mut(id)
    }

    /// Set the parent of a child region, updating both the Region and the index.
    pub fn set_region_parent(
        &mut self,
        child: RegionId,
        parent: RegionId,
    ) -> Result<(), GraphError> {
        if !self.regions.contains_key(&child) {
            return Err(GraphError::RegionNotFound(child));
        }
        if !self.regions.contains_key(&parent) {
            return Err(GraphError::RegionNotFound(parent));
        }
        self.region_parent.insert(child, parent);
        if let Some(region) = self.regions.get_mut(&child) {
            region.parent = Some(parent);
        }
        Ok(())
    }

    /// Get the parent region of a given region.
    pub fn parent_region(&self, region_id: &RegionId) -> Option<&RegionId> {
        self.region_parent.get(region_id)
    }

    /// Get all child regions of a given region.
    pub fn child_regions(&self, region_id: &RegionId) -> Vec<RegionId> {
        self.region_parent
            .iter()
            .filter(|(_, parent)| *parent == region_id)
            .map(|(child, _)| *child)
            .collect()
    }

    /// Extract a subgraph containing the given nodes, their internal edges,
    /// and any regions fully contained within the node set.
    pub fn extract_subgraph(&self, node_ids: &HashSet<NodeId>) -> Graph {
        let mut sub = Graph::new();

        // Copy selected nodes
        for id in node_ids {
            if let Some(node) = self.nodes.get(id) {
                sub.nodes.insert(*id, node.clone());
                sub.outgoing.entry(*id).or_default();
                sub.incoming.entry(*id).or_default();
            }
        }

        // Copy edges where both endpoints are in the set
        for edge in self.edges.values() {
            if node_ids.contains(&edge.source.0) && node_ids.contains(&edge.target.0) {
                sub.outgoing
                    .entry(edge.source.0)
                    .or_default()
                    .push(edge.id);
                sub.incoming
                    .entry(edge.target.0)
                    .or_default()
                    .push(edge.id);
                sub.edges.insert(edge.id, edge.clone());
            }
        }

        // Identify which regions are fully contained in the node set
        let included_regions: HashSet<RegionId> = self
            .regions
            .values()
            .filter(|r| !r.children.is_empty() && r.children.iter().all(|c| node_ids.contains(c)))
            .map(|r| r.id)
            .collect();

        // Copy included regions, dropping parent references to excluded regions
        for region in self.regions.values() {
            if included_regions.contains(&region.id) {
                let mut cloned = region.clone();
                if let Some(parent_id) = cloned.parent {
                    if !included_regions.contains(&parent_id) {
                        cloned.parent = None;
                    }
                }
                sub.region_children
                    .insert(cloned.id, cloned.children.clone());
                for child_id in &cloned.children {
                    sub.node_region.insert(*child_id, cloned.id);
                }
                if let Some(parent_id) = cloned.parent {
                    sub.region_parent.insert(cloned.id, parent_id);
                }
                sub.regions.insert(cloned.id, cloned);
            }
        }

        sub
    }

    /// Validate that edge port indices are consistent with node type signatures.
    ///
    /// For each edge, checks that:
    /// - The source node's output port index is within its TypeSignature outputs count
    /// - The target node's input port index is within its TypeSignature inputs count
    ///
    /// Nodes without type signatures are skipped.
    pub fn validate_port_types(&self) -> Result<(), Vec<GraphError>> {
        let mut errors = Vec::new();

        for edge in self.edges.values() {
            // Check source port against source node's outputs
            if let Some(source_node) = self.nodes.get(&edge.source.0) {
                if let Some(ref sig) = source_node.type_signature {
                    if edge.source.1 >= sig.outputs.len() {
                        errors.push(GraphError::PortOutOfRange {
                            node: edge.source.0,
                            port: edge.source.1,
                        });
                    }
                }
            }

            // Check target port against target node's inputs
            if let Some(target_node) = self.nodes.get(&edge.target.0) {
                if let Some(ref sig) = target_node.type_signature {
                    if edge.target.1 >= sig.inputs.len() {
                        errors.push(GraphError::PortOutOfRange {
                            node: edge.target.0,
                            port: edge.target.1,
                        });
                    }
                }
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    /// Return the total number of nodes.
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// Return the total number of edges.
    pub fn edge_count(&self) -> usize {
        self.edges.len()
    }

    /// Return the total number of regions.
    pub fn region_count(&self) -> usize {
        self.regions.len()
    }

    /// Iterate over all nodes.
    pub fn nodes(&self) -> impl Iterator<Item = &Node> {
        self.nodes.values()
    }

    /// Iterate over all edges.
    pub fn edges(&self) -> impl Iterator<Item = &Edge> {
        self.edges.values()
    }

    /// Iterate over all regions.
    pub fn regions(&self) -> impl Iterator<Item = &Region> {
        self.regions.values()
    }

    /// Check if a node kind is allowed to have back-edges (cycles).
    fn is_cycle_exempt(kind: &NodeKind) -> bool {
        matches!(
            kind,
            NodeKind::Iterate | NodeKind::Recurse | NodeKind::Fixpoint
        )
    }

    /// Compute a topological ordering of the nodes.
    ///
    /// Returns an ordered list of node IDs such that for every edge (u, v),
    /// u appears before v in the list. Cycles through Iterate, Recurse, or
    /// Fixpoint nodes are allowed — when the standard algorithm gets stuck
    /// on such a cycle, the exempt node is forced through (treating its
    /// remaining incoming edges as back-edges). Cycles that don't pass
    /// through any exempt node are reported as errors.
    pub fn topological_sort(&self) -> Result<Vec<NodeId>, GraphError> {
        let mut in_degree: HashMap<NodeId, usize> = HashMap::new();
        for id in self.nodes.keys() {
            in_degree.insert(*id, 0);
        }
        for edge in self.edges.values() {
            *in_degree.entry(edge.target.0).or_default() += 1;
        }

        let mut queue: Vec<NodeId> = in_degree
            .iter()
            .filter(|(_, &deg)| deg == 0)
            .map(|(id, _)| *id)
            .collect();
        queue.sort(); // Deterministic ordering

        let mut result = Vec::with_capacity(self.nodes.len());

        loop {
            // Standard Kahn's algorithm pass
            while let Some(node_id) = queue.pop() {
                result.push(node_id);
                for edge_id in self.outgoing_edges(&node_id) {
                    if let Some(edge) = self.edges.get(edge_id) {
                        let target = edge.target.0;
                        if let Some(deg) = in_degree.get_mut(&target) {
                            if *deg > 0 {
                                *deg -= 1;
                                if *deg == 0 {
                                    queue.push(target);
                                }
                            }
                        }
                    }
                }
                queue.sort(); // Keep deterministic
            }

            if result.len() == self.nodes.len() {
                break;
            }

            // Stuck on a cycle. Force one cycle-exempt node through,
            // treating its remaining incoming edges as back-edges.
            let mut exempt_remaining: Vec<NodeId> = in_degree
                .iter()
                .filter(|(_, &deg)| deg > 0)
                .filter(|(id, _)| {
                    self.nodes
                        .get(id)
                        .is_some_and(|n| Self::is_cycle_exempt(&n.kind))
                })
                .map(|(id, _)| *id)
                .collect();
            exempt_remaining.sort(); // Deterministic pick

            if let Some(&exempt_id) = exempt_remaining.first() {
                if let Some(deg) = in_degree.get_mut(&exempt_id) {
                    *deg = 0;
                }
                queue.push(exempt_id);
                queue.sort();
            } else {
                // No exempt node available — genuine cycle error
                let cycle_node = *in_degree
                    .iter()
                    .find(|(_, &deg)| deg > 0)
                    .map(|(id, _)| id)
                    .expect("cycle must involve at least one node");
                return Err(GraphError::CycleDetected(cycle_node));
            }
        }

        Ok(result)
    }

    /// Validate that linear/affine values have the correct number of consumers.
    ///
    /// - `Linear` / `Unique`: exactly 1 consumer
    /// - `Affine`: 0 or 1 consumers
    /// - Others: any count (skip)
    pub fn validate_linearity(&self) -> Result<(), Vec<GraphError>> {
        let mut errors = Vec::new();

        for node in self.nodes.values() {
            let sig = match &node.type_signature {
                Some(s) => s,
                None => continue,
            };

            for (port_idx, output_type) in sig.outputs.iter().enumerate() {
                let lin = match output_type.linearity() {
                    Some(l) => l,
                    None => continue,
                };

                // Count outgoing edges from this (node, port_index) pair
                let consumers = self
                    .outgoing_edges(&node.id)
                    .iter()
                    .filter_map(|eid| self.edges.get(eid))
                    .filter(|e| e.source.0 == node.id && e.source.1 == port_idx)
                    .count();

                match lin {
                    Linearity::Linear | Linearity::Unique => {
                        if consumers != 1 {
                            errors.push(GraphError::LinearityViolation {
                                node: node.id,
                                port: port_idx,
                                kind: lin,
                                consumers,
                            });
                        }
                    }
                    Linearity::Affine => {
                        if consumers > 1 {
                            errors.push(GraphError::LinearityViolation {
                                node: node.id,
                                port: port_idx,
                                kind: lin,
                                consumers,
                            });
                        }
                    }
                    Linearity::Shared | Linearity::Counted | Linearity::Unrestricted => {}
                }
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    /// Validate that each node's declared effects are a superset of its predecessors' effects.
    ///
    /// For each node, gathers effects from all predecessor nodes (via incoming edges).
    /// Verifies that the node's own declared effects are a superset of the union.
    pub fn validate_effects(&self) -> Result<(), Vec<GraphError>> {
        let mut errors = Vec::new();

        for node in self.nodes.values() {
            let node_effects = match &node.contract {
                Some(c) => &c.effects,
                None => continue,
            };

            // Gather effects from all predecessor nodes
            let mut required = EffectSet::empty();
            for edge_id in self.incoming_edges(&node.id) {
                if let Some(edge) = self.edges.get(edge_id) {
                    if let Some(pred_node) = self.nodes.get(&edge.source.0) {
                        if let Some(pred_contract) = &pred_node.contract {
                            required.merge(&pred_contract.effects);
                        }
                    }
                }
            }

            // If required effects are pure, nothing to check
            if required.is_pure() {
                continue;
            }

            // Check that node's declared effects are a superset
            for effect in &required.effects {
                if !node_effects.has_effect(effect) {
                    errors.push(GraphError::EffectViolation {
                        node: node.id,
                        declared: format!("{node_effects}"),
                        required: format!("{required}"),
                    });
                    break; // One error per node is sufficient
                }
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    /// Validate that edge source/target types are compatible.
    ///
    /// Returns any proof obligations generated by refinement subtyping.
    /// Skips edges where either node lacks a TypeSignature.
    pub fn validate_edge_types(&self) -> Result<Vec<ProofObligation>, Vec<GraphError>> {
        let mut errors = Vec::new();
        let mut obligations = Vec::new();

        for edge in self.edges.values() {
            let source_type = self
                .nodes
                .get(&edge.source.0)
                .and_then(|n| n.type_signature.as_ref())
                .and_then(|sig| sig.outputs.get(edge.source.1));

            let target_type = self
                .nodes
                .get(&edge.target.0)
                .and_then(|n| n.type_signature.as_ref())
                .and_then(|sig| sig.inputs.get(edge.target.1));

            let (src_ty, tgt_ty) = match (source_type, target_type) {
                (Some(s), Some(t)) => (s, t),
                _ => continue,
            };

            match types_compatible(src_ty, tgt_ty) {
                Ok(obs) => obligations.extend(obs),
                Err(_) => {
                    errors.push(GraphError::TypeMismatch {
                        edge: edge.id,
                        expected: format!("{tgt_ty}"),
                        found: format!("{src_ty}"),
                    });
                }
            }
        }

        if errors.is_empty() {
            Ok(obligations)
        } else {
            Err(errors)
        }
    }

    /// Validate contracts and generate proof obligations.
    ///
    /// Performs three kinds of obligation generation:
    /// 1. Per-node: calls `contract.generate_obligations()` for each contracted node
    /// 2. Edge-crossing: for edges where both source and target have contracts,
    ///    generates an implication obligation (postcondition => precondition)
    /// 3. Termination: for Iterate/Recurse/Fixpoint nodes, generates a termination obligation
    pub fn validate_contracts(&self) -> Vec<ProofObligation> {
        let mut obligations = Vec::new();

        // A. Per-node obligations
        for node in self.nodes.values() {
            if let Some(ref contract) = node.contract {
                obligations.extend(contract.generate_obligations());
            }
        }

        // B. Edge-crossing obligations
        for edge in self.edges.values() {
            let src_contract = self
                .nodes
                .get(&edge.source.0)
                .and_then(|n| n.contract.as_ref());
            let tgt_contract = self
                .nodes
                .get(&edge.target.0)
                .and_then(|n| n.contract.as_ref());

            if let (Some(src_c), Some(tgt_c)) = (src_contract, tgt_contract) {
                // For each postcondition of source and precondition of target,
                // generate an implication obligation: post => pre
                for post in &src_c.postconditions {
                    for pre in &tgt_c.preconditions {
                        obligations.push(ProofObligation {
                            kind: ObligationKind::Precondition,
                            predicate: Predicate::Implies(
                                Box::new(post.clone()),
                                Box::new(pre.clone()),
                            ),
                            description: "edge-crossing: postcondition of source implies precondition of target".to_string(),
                            status: ProofStatus::Pending,
                            witness: None,
                            waiver: None,
                        });
                    }
                }
            }
        }

        // C. Termination obligations
        for node in self.nodes.values() {
            if matches!(
                node.kind,
                NodeKind::Iterate | NodeKind::Recurse | NodeKind::Fixpoint
            ) {
                obligations.push(ProofObligation {
                    kind: ObligationKind::Termination,
                    predicate: Predicate::BoolLit(true),
                    description: format!("{} node must terminate", node.kind),
                    status: ProofStatus::Pending,
                    witness: None,
                    waiver: None,
                });
            }
        }

        obligations
    }

    /// Run all type-related validation checks.
    ///
    /// Combines consistency (edge type compatibility), linearity validation,
    /// effect propagation checks, and contract validation. Returns proof
    /// obligations from refinement subtyping and contract generation.
    /// Structural validation (`validate()`) should be run separately.
    pub fn validate_types(&self) -> Result<Vec<ProofObligation>, Vec<GraphError>> {
        let mut all_errors = Vec::new();
        let mut all_obligations = Vec::new();

        if let Err(errs) = self.validate_linearity() {
            all_errors.extend(errs);
        }

        if let Err(errs) = self.validate_effects() {
            all_errors.extend(errs);
        }

        match self.validate_edge_types() {
            Ok(obs) => all_obligations.extend(obs),
            Err(errs) => all_errors.extend(errs),
        }

        all_obligations.extend(self.validate_contracts());

        if all_errors.is_empty() {
            Ok(all_obligations)
        } else {
            Err(all_errors)
        }
    }

    /// Validate graph well-formedness.
    ///
    /// Checks:
    /// - No dangling edges (source and target nodes exist)
    /// - Region containment consistency
    /// - Port index validity against type signatures
    /// - Parent region existence
    pub fn validate(&self) -> Result<(), Vec<GraphError>> {
        let mut errors = Vec::new();

        // Check for dangling edges
        for edge in self.edges.values() {
            if !self.nodes.contains_key(&edge.source.0) || !self.nodes.contains_key(&edge.target.0)
            {
                errors.push(GraphError::DanglingEdge {
                    src: edge.source.0,
                    dst: edge.target.0,
                });
            }
        }

        // Check region containment consistency
        for region in self.regions.values() {
            for child_id in &region.children {
                if !self.nodes.contains_key(child_id) {
                    errors.push(GraphError::RegionContainment {
                        child: *child_id,
                        region: region.id,
                    });
                }
            }
        }

        // Check port indices against type signatures
        if let Err(port_errors) = self.validate_port_types() {
            errors.extend(port_errors);
        }

        // Check parent region existence
        for region in self.regions.values() {
            if let Some(parent_id) = region.parent {
                if !self.regions.contains_key(&parent_id) {
                    errors.push(GraphError::RegionNotFound(parent_id));
                }
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
}

impl Default for Graph {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::edge::Edge;
    use crate::graph::node::{Node, NodeKind};
    use crate::graph::region::{Region, RegionKind};

    fn make_literal_node() -> Node {
        Node::new(NodeKind::Literal)
    }

    fn make_arithmetic_node() -> Node {
        Node::new(NodeKind::Arithmetic(node::ArithmeticOp::Add))
    }

    #[test]
    fn empty_graph() {
        let g = Graph::new();
        assert_eq!(g.node_count(), 0);
        assert_eq!(g.edge_count(), 0);
        assert_eq!(g.region_count(), 0);
        assert!(g.validate().is_ok());
    }

    #[test]
    fn add_nodes_and_edges() {
        let mut g = Graph::new();
        let n1 = make_literal_node();
        let n2 = make_arithmetic_node();
        let id1 = n1.id;
        let id2 = n2.id;
        g.add_node(n1).unwrap();
        g.add_node(n2).unwrap();

        let edge = Edge::new((id1, 0), (id2, 0));
        g.add_edge(edge).unwrap();

        assert_eq!(g.node_count(), 2);
        assert_eq!(g.edge_count(), 1);
        assert_eq!(g.outgoing_edges(&id1).len(), 1);
        assert_eq!(g.incoming_edges(&id2).len(), 1);
        assert!(g.validate().is_ok());
    }

    #[test]
    fn dangling_edge_rejected() {
        let mut g = Graph::new();
        let n1 = make_literal_node();
        let id1 = n1.id;
        g.add_node(n1).unwrap();

        let fake_id = NodeId::new_v4();
        let edge = Edge::new((id1, 0), (fake_id, 0));
        assert!(g.add_edge(edge).is_err());
    }

    #[test]
    fn topological_sort_simple() {
        let mut g = Graph::new();
        let n1 = make_literal_node();
        let n2 = make_literal_node();
        let n3 = make_arithmetic_node();
        let id1 = n1.id;
        let id2 = n2.id;
        let id3 = n3.id;

        g.add_node(n1).unwrap();
        g.add_node(n2).unwrap();
        g.add_node(n3).unwrap();

        g.add_edge(Edge::new((id1, 0), (id3, 0))).unwrap();
        g.add_edge(Edge::new((id2, 0), (id3, 1))).unwrap();

        let order = g.topological_sort().unwrap();
        assert_eq!(order.len(), 3);

        // n3 must come after both n1 and n2
        let pos1 = order.iter().position(|&id| id == id1).unwrap();
        let pos2 = order.iter().position(|&id| id == id2).unwrap();
        let pos3 = order.iter().position(|&id| id == id3).unwrap();
        assert!(pos1 < pos3);
        assert!(pos2 < pos3);
    }

    #[test]
    fn region_containment() {
        let mut g = Graph::new();
        let n1 = make_literal_node();
        let n2 = make_arithmetic_node();
        let id1 = n1.id;
        let id2 = n2.id;
        g.add_node(n1).unwrap();
        g.add_node(n2).unwrap();

        let region = Region::new(RegionKind::Parallel, vec![id1, id2]);
        let region_id = region.id;
        g.add_region(region).unwrap();

        assert_eq!(g.containing_region(&id1), Some(&region_id));
        assert_eq!(g.containing_region(&id2), Some(&region_id));
    }

    #[test]
    fn duplicate_node_rejected() {
        let mut g = Graph::new();
        let n = make_literal_node();
        let n_clone = n.clone();
        g.add_node(n).unwrap();
        assert!(g.add_node(n_clone).is_err());
    }

    #[test]
    fn parent_region_tracking() {
        let mut g = Graph::new();
        let n1 = make_literal_node();
        let n2 = make_literal_node();
        let id1 = n1.id;
        let id2 = n2.id;
        g.add_node(n1).unwrap();
        g.add_node(n2).unwrap();

        let child_region = Region::new(RegionKind::Sequential, vec![id1]);
        let child_id = child_region.id;
        g.add_region(child_region).unwrap();

        let parent_region = Region::new(RegionKind::Parallel, vec![id2]);
        let parent_id = parent_region.id;
        g.add_region(parent_region).unwrap();

        g.set_region_parent(child_id, parent_id).unwrap();
        assert_eq!(g.parent_region(&child_id), Some(&parent_id));
        assert_eq!(g.parent_region(&parent_id), None);
    }

    #[test]
    fn child_regions() {
        let mut g = Graph::new();
        let n1 = make_literal_node();
        let n2 = make_literal_node();
        let n3 = make_literal_node();
        let id1 = n1.id;
        let id2 = n2.id;
        let id3 = n3.id;
        g.add_node(n1).unwrap();
        g.add_node(n2).unwrap();
        g.add_node(n3).unwrap();

        let parent = Region::new(RegionKind::Parallel, vec![id1]);
        let parent_id = parent.id;
        g.add_region(parent).unwrap();

        let child1 = Region::new(RegionKind::Sequential, vec![id2]);
        let child1_id = child1.id;
        g.add_region(child1).unwrap();
        g.set_region_parent(child1_id, parent_id).unwrap();

        let child2 = Region::new(RegionKind::Atomic, vec![id3]);
        let child2_id = child2.id;
        g.add_region(child2).unwrap();
        g.set_region_parent(child2_id, parent_id).unwrap();

        let children = g.child_regions(&parent_id);
        assert_eq!(children.len(), 2);
        assert!(children.contains(&child1_id));
        assert!(children.contains(&child2_id));
    }

    #[test]
    fn subgraph_extraction() {
        let mut g = Graph::new();
        let n1 = make_literal_node();
        let n2 = make_literal_node();
        let n3 = make_arithmetic_node();
        let n4 = make_literal_node();
        let id1 = n1.id;
        let id2 = n2.id;
        let id3 = n3.id;
        let id4 = n4.id;
        g.add_node(n1).unwrap();
        g.add_node(n2).unwrap();
        g.add_node(n3).unwrap();
        g.add_node(n4).unwrap();

        g.add_edge(Edge::new((id1, 0), (id3, 0))).unwrap();
        g.add_edge(Edge::new((id2, 0), (id3, 1))).unwrap();
        g.add_edge(Edge::new((id3, 0), (id4, 0))).unwrap();

        // Extract subgraph with just n1, n2, n3
        let subset: HashSet<NodeId> = [id1, id2, id3].into_iter().collect();
        let sub = g.extract_subgraph(&subset);

        assert_eq!(sub.node_count(), 3);
        // Only edges internal to the subset: n1->n3, n2->n3 (not n3->n4)
        assert_eq!(sub.edge_count(), 2);
    }

    #[test]
    fn subgraph_with_regions() {
        let mut g = Graph::new();
        let n1 = make_literal_node();
        let n2 = make_literal_node();
        let n3 = make_literal_node();
        let id1 = n1.id;
        let id2 = n2.id;
        let id3 = n3.id;
        g.add_node(n1).unwrap();
        g.add_node(n2).unwrap();
        g.add_node(n3).unwrap();

        // Region containing n1, n2
        let r = Region::new(RegionKind::Parallel, vec![id1, id2]);
        let rid = r.id;
        g.add_region(r).unwrap();

        // Extract with n1, n2 — region should be included
        let subset: HashSet<NodeId> = [id1, id2].into_iter().collect();
        let sub = g.extract_subgraph(&subset);
        assert_eq!(sub.region_count(), 1);
        assert!(sub.get_region(&rid).is_some());

        // Extract with n1, n3 — region should NOT be included (n2 missing)
        let subset2: HashSet<NodeId> = [id1, id3].into_iter().collect();
        let sub2 = g.extract_subgraph(&subset2);
        assert_eq!(sub2.region_count(), 0);
    }

    #[test]
    fn port_validation_pass() {
        use crate::types::{Type, TypeSignature};

        let mut g = Graph::new();
        let n1 = Node::new(NodeKind::Literal)
            .with_type_signature(TypeSignature::source(Type::i32()));
        let n2 = Node::new(NodeKind::Arithmetic(node::ArithmeticOp::Add))
            .with_type_signature(TypeSignature::pure_fn(
                vec![Type::i32(), Type::i32()],
                Type::i32(),
            ));
        let id1 = n1.id;
        let id2 = n2.id;
        g.add_node(n1).unwrap();
        g.add_node(n2).unwrap();

        // source port 0 (within 1 output), target port 0 (within 2 inputs)
        g.add_edge(Edge::new((id1, 0), (id2, 0))).unwrap();
        assert!(g.validate_port_types().is_ok());
    }

    #[test]
    fn port_validation_fail() {
        use crate::types::{Type, TypeSignature};

        let mut g = Graph::new();
        let n1 = Node::new(NodeKind::Literal)
            .with_type_signature(TypeSignature::source(Type::i32()));
        let n2 = Node::new(NodeKind::Arithmetic(node::ArithmeticOp::Add))
            .with_type_signature(TypeSignature::pure_fn(
                vec![Type::i32(), Type::i32()],
                Type::i32(),
            ));
        let id1 = n1.id;
        let id2 = n2.id;
        g.add_node(n1).unwrap();
        g.add_node(n2).unwrap();

        // source port 5 is out of range (only 1 output)
        g.add_edge(Edge::new((id1, 5), (id2, 0))).unwrap();
        let result = g.validate_port_types();
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(errors.iter().any(|e| matches!(e, GraphError::PortOutOfRange { node, port } if *node == id1 && *port == 5)));
    }

    #[test]
    fn topological_sort_with_iterate() {
        // Iterate node should allow back-edges without cycle error
        let mut g = Graph::new();
        let n1 = Node::new(NodeKind::Literal);
        let n2 = Node::new(NodeKind::Iterate);
        let n3 = Node::new(NodeKind::Arithmetic(node::ArithmeticOp::Add));
        let id1 = n1.id;
        let id2 = n2.id;
        let id3 = n3.id;
        g.add_node(n1).unwrap();
        g.add_node(n2).unwrap();
        g.add_node(n3).unwrap();

        // n1 -> n2 -> n3 -> n2 (back-edge to Iterate)
        g.add_edge(Edge::new((id1, 0), (id2, 0))).unwrap();
        g.add_edge(Edge::new((id2, 0), (id3, 0))).unwrap();
        g.add_edge(Edge::new((id3, 0), (id2, 1))).unwrap(); // back-edge

        let order = g.topological_sort().unwrap();
        assert_eq!(order.len(), 3);
    }

    #[test]
    fn topological_sort_illegal_cycle() {
        // Non-exempt nodes in a cycle should still error
        let mut g = Graph::new();
        let n1 = Node::new(NodeKind::Arithmetic(node::ArithmeticOp::Add));
        let n2 = Node::new(NodeKind::Arithmetic(node::ArithmeticOp::Mul));
        let id1 = n1.id;
        let id2 = n2.id;
        g.add_node(n1).unwrap();
        g.add_node(n2).unwrap();

        // n1 -> n2 -> n1 (cycle, neither is exempt)
        g.add_edge(Edge::new((id1, 0), (id2, 0))).unwrap();
        g.add_edge(Edge::new((id2, 0), (id1, 0))).unwrap();

        assert!(g.topological_sort().is_err());
    }

    #[test]
    fn duplicate_region_children_rejected() {
        let mut g = Graph::new();
        let n1 = make_literal_node();
        let id1 = n1.id;
        g.add_node(n1).unwrap();

        let region = Region::new(RegionKind::Parallel, vec![id1, id1]);
        let result = g.add_region(region);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, GraphError::DuplicateRegionChild { .. }));
    }

    #[test]
    fn topological_sort_iterate_preserves_forward_deps() {
        // Iterate node has a forward dependency (A) and a back-edge (from B).
        // The sort must schedule A before Iterate.
        let mut g = Graph::new();
        let a = Node::new(NodeKind::Literal);
        let iter_node = Node::new(NodeKind::Iterate);
        let b = Node::new(NodeKind::Arithmetic(node::ArithmeticOp::Add));
        let a_id = a.id;
        let iter_id = iter_node.id;
        let b_id = b.id;

        g.add_node(a).unwrap();
        g.add_node(iter_node).unwrap();
        g.add_node(b).unwrap();

        g.add_edge(Edge::new((a_id, 0), (iter_id, 0))).unwrap(); // forward
        g.add_edge(Edge::new((iter_id, 0), (b_id, 0))).unwrap(); // forward
        g.add_edge(Edge::new((b_id, 0), (iter_id, 1))).unwrap(); // back-edge

        let order = g.topological_sort().unwrap();
        assert_eq!(order.len(), 3);

        let pos_a = order.iter().position(|&id| id == a_id).unwrap();
        let pos_iter = order.iter().position(|&id| id == iter_id).unwrap();
        assert!(
            pos_a < pos_iter,
            "forward dependency: A must come before Iterate"
        );
    }

    #[test]
    fn subgraph_drops_stale_parent() {
        let mut g = Graph::new();
        let n1 = make_literal_node();
        let n2 = make_literal_node();
        let n3 = make_literal_node();
        let id1 = n1.id;
        let id2 = n2.id;
        g.add_node(n1).unwrap();
        g.add_node(n2).unwrap();
        g.add_node(n3).unwrap();

        // inner region (n1), outer region (n2), unrelated node n3
        let inner = Region::new(RegionKind::Sequential, vec![id1]);
        let inner_id = inner.id;
        g.add_region(inner).unwrap();

        let outer = Region::new(RegionKind::Parallel, vec![id2]);
        let outer_id = outer.id;
        g.add_region(outer).unwrap();

        g.set_region_parent(inner_id, outer_id).unwrap();

        // Extract only n1 — inner region is included but outer is NOT
        let subset: HashSet<NodeId> = [id1].into_iter().collect();
        let sub = g.extract_subgraph(&subset);

        assert_eq!(sub.region_count(), 1);
        let inner_sub = sub.get_region(&inner_id).unwrap();
        // Parent reference must be cleared (outer isn't in the subgraph)
        assert_eq!(inner_sub.parent, None);
        assert_eq!(sub.parent_region(&inner_id), None);
        // Subgraph must pass validation (no dangling parent)
        assert!(sub.validate().is_ok());
    }

    #[test]
    fn linearity_linear_exactly_one() {
        use crate::types::{Linearity, Type, TypeSignature};

        let mut g = Graph::new();
        let n1 = Node::new(NodeKind::Literal)
            .with_type_signature(TypeSignature::source(Type::i32().with_linearity(Linearity::Linear)));
        let n2 = Node::new(NodeKind::Arithmetic(node::ArithmeticOp::Add))
            .with_type_signature(TypeSignature::pure_fn(vec![Type::i32()], Type::i32()));
        let id1 = n1.id;
        let id2 = n2.id;
        g.add_node(n1).unwrap();
        g.add_node(n2).unwrap();
        g.add_edge(Edge::new((id1, 0), (id2, 0))).unwrap();

        // Linear with exactly 1 consumer: OK
        assert!(g.validate_linearity().is_ok());
    }

    #[test]
    fn linearity_linear_zero_consumers() {
        use crate::types::{Linearity, Type, TypeSignature};

        let mut g = Graph::new();
        // Linear value with no consumers: violation (must be used exactly once)
        let n1 = Node::new(NodeKind::Literal)
            .with_type_signature(TypeSignature::source(Type::i32().with_linearity(Linearity::Linear)));
        g.add_node(n1).unwrap();

        let result = g.validate_linearity();
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(errors.iter().any(|e| matches!(e, GraphError::LinearityViolation { consumers: 0, .. })));
    }

    #[test]
    fn linearity_linear_two_consumers() {
        use crate::types::{Linearity, Type, TypeSignature};

        let mut g = Graph::new();
        let n1 = Node::new(NodeKind::Literal)
            .with_type_signature(TypeSignature::source(Type::i32().with_linearity(Linearity::Linear)));
        let n2 = Node::new(NodeKind::Arithmetic(node::ArithmeticOp::Add))
            .with_type_signature(TypeSignature::pure_fn(vec![Type::i32()], Type::i32()));
        let n3 = Node::new(NodeKind::Arithmetic(node::ArithmeticOp::Mul))
            .with_type_signature(TypeSignature::pure_fn(vec![Type::i32()], Type::i32()));
        let id1 = n1.id;
        let id2 = n2.id;
        let id3 = n3.id;
        g.add_node(n1).unwrap();
        g.add_node(n2).unwrap();
        g.add_node(n3).unwrap();
        g.add_edge(Edge::new((id1, 0), (id2, 0))).unwrap();
        g.add_edge(Edge::new((id1, 0), (id3, 0))).unwrap();

        // Linear with 2 consumers: violation
        let result = g.validate_linearity();
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(errors.iter().any(|e| matches!(e, GraphError::LinearityViolation { consumers: 2, .. })));
    }

    #[test]
    fn linearity_affine_zero_ok() {
        use crate::types::{Linearity, Type, TypeSignature};

        let mut g = Graph::new();
        let n1 = Node::new(NodeKind::Literal)
            .with_type_signature(TypeSignature::source(Type::i32().with_linearity(Linearity::Affine)));
        g.add_node(n1).unwrap();

        // Affine with 0 consumers: OK (may be dropped)
        assert!(g.validate_linearity().is_ok());
    }

    #[test]
    fn linearity_affine_two_fails() {
        use crate::types::{Linearity, Type, TypeSignature};

        let mut g = Graph::new();
        let n1 = Node::new(NodeKind::Literal)
            .with_type_signature(TypeSignature::source(Type::i32().with_linearity(Linearity::Affine)));
        let n2 = Node::new(NodeKind::Arithmetic(node::ArithmeticOp::Add))
            .with_type_signature(TypeSignature::pure_fn(vec![Type::i32()], Type::i32()));
        let n3 = Node::new(NodeKind::Arithmetic(node::ArithmeticOp::Mul))
            .with_type_signature(TypeSignature::pure_fn(vec![Type::i32()], Type::i32()));
        let id1 = n1.id;
        let id2 = n2.id;
        let id3 = n3.id;
        g.add_node(n1).unwrap();
        g.add_node(n2).unwrap();
        g.add_node(n3).unwrap();
        g.add_edge(Edge::new((id1, 0), (id2, 0))).unwrap();
        g.add_edge(Edge::new((id1, 0), (id3, 0))).unwrap();

        // Affine with 2 consumers: violation
        assert!(g.validate_linearity().is_err());
    }

    #[test]
    fn effect_propagation_pass() {
        use crate::contract::{Contract, EffectSet};
        use crate::types::{Effect, Type, TypeSignature};

        let mut g = Graph::new();
        // Pure predecessor
        let n1 = Node::new(NodeKind::Literal)
            .with_type_signature(TypeSignature::source(Type::i32()))
            .with_contract(Contract::pure_default());
        // IO node depending on pure node: should pass
        let n2 = Node::new(NodeKind::Arithmetic(node::ArithmeticOp::Add))
            .with_type_signature(TypeSignature::pure_fn(vec![Type::i32()], Type::i32()))
            .with_contract(
                Contract::pure_default()
                    .with_effects(EffectSet::from_effects(vec![Effect::IO("UART1".into())])),
            );
        let id1 = n1.id;
        let id2 = n2.id;
        g.add_node(n1).unwrap();
        g.add_node(n2).unwrap();
        g.add_edge(Edge::new((id1, 0), (id2, 0))).unwrap();

        assert!(g.validate_effects().is_ok());
    }

    #[test]
    fn effect_propagation_violation() {
        use crate::contract::{Contract, EffectSet};
        use crate::types::{Effect, Type, TypeSignature};

        let mut g = Graph::new();
        // IO predecessor
        let n1 = Node::new(NodeKind::Literal)
            .with_type_signature(TypeSignature::source(Type::i32()))
            .with_contract(
                Contract::pure_default()
                    .with_effects(EffectSet::from_effects(vec![Effect::IO("UART1".into())])),
            );
        // Pure node depending on IO node: violation
        let n2 = Node::new(NodeKind::Arithmetic(node::ArithmeticOp::Add))
            .with_type_signature(TypeSignature::pure_fn(vec![Type::i32()], Type::i32()))
            .with_contract(Contract::pure_default());
        let id1 = n1.id;
        let id2 = n2.id;
        g.add_node(n1).unwrap();
        g.add_node(n2).unwrap();
        g.add_edge(Edge::new((id1, 0), (id2, 0))).unwrap();

        let result = g.validate_effects();
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(errors.iter().any(|e| matches!(e, GraphError::EffectViolation { .. })));
    }

    #[test]
    fn edge_type_compatibility_pass() {
        use crate::types::{Type, TypeSignature};

        let mut g = Graph::new();
        let n1 = Node::new(NodeKind::Literal)
            .with_type_signature(TypeSignature::source(Type::i32()));
        let n2 = Node::new(NodeKind::Arithmetic(node::ArithmeticOp::Add))
            .with_type_signature(TypeSignature::pure_fn(vec![Type::i32(), Type::i32()], Type::i32()));
        let id1 = n1.id;
        let id2 = n2.id;
        g.add_node(n1).unwrap();
        g.add_node(n2).unwrap();
        g.add_edge(Edge::new((id1, 0), (id2, 0))).unwrap();

        let result = g.validate_edge_types();
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[test]
    fn edge_type_compatibility_fail() {
        use crate::types::{Type, TypeSignature};

        let mut g = Graph::new();
        // Source outputs f64
        let n1 = Node::new(NodeKind::Literal)
            .with_type_signature(TypeSignature::source(Type::f64()));
        // Target expects i32 input
        let n2 = Node::new(NodeKind::Arithmetic(node::ArithmeticOp::Add))
            .with_type_signature(TypeSignature::pure_fn(vec![Type::i32()], Type::i32()));
        let id1 = n1.id;
        let id2 = n2.id;
        g.add_node(n1).unwrap();
        g.add_node(n2).unwrap();
        g.add_edge(Edge::new((id1, 0), (id2, 0))).unwrap();

        let result = g.validate_edge_types();
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(errors.iter().any(|e| matches!(e, GraphError::TypeMismatch { .. })));
    }

    #[test]
    fn validate_types_combined() {
        use crate::types::{Type, TypeSignature};

        let mut g = Graph::new();
        let n1 = Node::new(NodeKind::Literal)
            .with_type_signature(TypeSignature::source(Type::i32()));
        let n2 = Node::new(NodeKind::Arithmetic(node::ArithmeticOp::Add))
            .with_type_signature(TypeSignature::pure_fn(vec![Type::i32(), Type::i32()], Type::i32()));
        let id1 = n1.id;
        let id2 = n2.id;
        g.add_node(n1).unwrap();
        g.add_node(n2).unwrap();
        g.add_edge(Edge::new((id1, 0), (id2, 0))).unwrap();

        // validate_types should pass and return no obligations
        let result = g.validate_types();
        assert!(result.is_ok());
    }

    #[test]
    fn validate_does_not_check_cycles() {
        // validate() checks structural integrity but NOT the DAG property;
        // use topological_sort() for cycle detection.
        let mut g = Graph::new();
        let n1 = Node::new(NodeKind::Arithmetic(node::ArithmeticOp::Add));
        let n2 = Node::new(NodeKind::Arithmetic(node::ArithmeticOp::Mul));
        let id1 = n1.id;
        let id2 = n2.id;
        g.add_node(n1).unwrap();
        g.add_node(n2).unwrap();

        g.add_edge(Edge::new((id1, 0), (id2, 0))).unwrap();
        g.add_edge(Edge::new((id2, 0), (id1, 0))).unwrap();

        // validate() passes — it doesn't check for cycles
        assert!(g.validate().is_ok());
        // topological_sort() catches the cycle
        assert!(g.topological_sort().is_err());
    }

    #[test]
    fn port_validation_skips_untyped_nodes() {
        // Nodes without type signatures are skipped during port validation
        let mut g = Graph::new();
        let n1 = Node::new(NodeKind::Literal); // no type signature
        let n2 = Node::new(NodeKind::Arithmetic(node::ArithmeticOp::Add)); // no type signature
        let id1 = n1.id;
        let id2 = n2.id;
        g.add_node(n1).unwrap();
        g.add_node(n2).unwrap();

        // Port 99 would be out of range with type signatures,
        // but validation passes because nodes are untyped
        g.add_edge(Edge::new((id1, 99), (id2, 99))).unwrap();
        assert!(g.validate_port_types().is_ok());
    }

    #[test]
    fn nested_regions() {
        let mut g = Graph::new();
        let n1 = make_literal_node();
        let n2 = make_literal_node();
        let id1 = n1.id;
        let id2 = n2.id;
        g.add_node(n1).unwrap();
        g.add_node(n2).unwrap();

        let inner = Region::new(RegionKind::Sequential, vec![id1]);
        let inner_id = inner.id;
        g.add_region(inner).unwrap();

        let outer = Region::new(RegionKind::Parallel, vec![id2]);
        let outer_id = outer.id;
        g.add_region(outer).unwrap();

        g.set_region_parent(inner_id, outer_id).unwrap();

        // Validate passes — parent exists
        assert!(g.validate().is_ok());
        assert_eq!(g.parent_region(&inner_id), Some(&outer_id));

        // Region.parent field is also updated
        let inner_region = g.get_region(&inner_id).unwrap();
        assert_eq!(inner_region.parent, Some(outer_id));
    }

    #[test]
    fn edge_crossing_obligation() {
        use crate::contract::Contract;
        use crate::types::{Predicate, Type, TypeSignature};

        let mut g = Graph::new();
        let n1 = Node::new(NodeKind::Literal)
            .with_type_signature(TypeSignature::source(Type::i32()))
            .with_contract(Contract::with_conditions(
                vec![],
                vec![Predicate::positive("output")],
            ));
        let n2 = Node::new(NodeKind::Arithmetic(node::ArithmeticOp::Add))
            .with_type_signature(TypeSignature::pure_fn(vec![Type::i32()], Type::i32()))
            .with_contract(Contract::with_conditions(
                vec![Predicate::positive("input")],
                vec![],
            ));
        let id1 = n1.id;
        let id2 = n2.id;
        g.add_node(n1).unwrap();
        g.add_node(n2).unwrap();
        g.add_edge(Edge::new((id1, 0), (id2, 0))).unwrap();

        let obs = g.validate_contracts();
        // Should have: 1 postcondition from n1 + 1 precondition from n2 + 1 edge-crossing implication
        let edge_crossing = obs
            .iter()
            .filter(|o| matches!(&o.predicate, Predicate::Implies(..)))
            .count();
        assert_eq!(edge_crossing, 1);
    }

    #[test]
    fn termination_obligation_iterate() {
        use crate::contract::ObligationKind;

        let mut g = Graph::new();
        let n = Node::new(NodeKind::Iterate);
        g.add_node(n).unwrap();

        let obs = g.validate_contracts();
        assert_eq!(obs.len(), 1);
        assert_eq!(obs[0].kind, ObligationKind::Termination);
        assert!(obs[0].description.contains("Iterate"));
    }

    #[test]
    fn termination_obligation_recurse() {
        use crate::contract::ObligationKind;

        let mut g = Graph::new();
        let n = Node::new(NodeKind::Recurse);
        g.add_node(n).unwrap();

        let obs = g.validate_contracts();
        assert_eq!(obs.len(), 1);
        assert_eq!(obs[0].kind, ObligationKind::Termination);
        assert!(obs[0].description.contains("Recurse"));
    }

    #[test]
    fn termination_obligation_fixpoint() {
        use crate::contract::ObligationKind;

        let mut g = Graph::new();
        let n = Node::new(NodeKind::Fixpoint);
        g.add_node(n).unwrap();

        let obs = g.validate_contracts();
        assert_eq!(obs.len(), 1);
        assert_eq!(obs[0].kind, ObligationKind::Termination);
        assert!(obs[0].description.contains("Fixpoint"));
    }

    #[test]
    fn validate_contracts_integration() {
        use crate::contract::{Contract, ObligationKind};
        use crate::types::{Predicate, Type, TypeSignature};

        let mut g = Graph::new();
        // Literal with postcondition
        let n1 = Node::new(NodeKind::Literal)
            .with_type_signature(TypeSignature::source(Type::i32()))
            .with_contract(Contract::with_conditions(
                vec![],
                vec![Predicate::positive("output")],
            ));
        // Iterate node with contract
        let n2 = Node::new(NodeKind::Iterate)
            .with_type_signature(TypeSignature::pure_fn(vec![Type::i32()], Type::i32()))
            .with_contract(Contract::with_conditions(
                vec![Predicate::positive("input")],
                vec![],
            ));
        let id1 = n1.id;
        let id2 = n2.id;
        g.add_node(n1).unwrap();
        g.add_node(n2).unwrap();
        g.add_edge(Edge::new((id1, 0), (id2, 0))).unwrap();

        let obs = g.validate_contracts();

        // Expected obligations:
        // - n1 postcondition (1)
        // - n2 precondition (1)
        // - edge-crossing implication (1)
        // - termination for Iterate (1)
        let postconds = obs
            .iter()
            .filter(|o| o.kind == ObligationKind::Postcondition)
            .count();
        let preconds = obs
            .iter()
            .filter(|o| o.kind == ObligationKind::Precondition)
            .count();
        let terminations = obs
            .iter()
            .filter(|o| o.kind == ObligationKind::Termination)
            .count();

        assert_eq!(postconds, 1);
        // 1 direct precondition + 1 edge-crossing implication
        assert_eq!(preconds, 2);
        assert_eq!(terminations, 1);
        assert_eq!(obs.len(), 4);
    }
}
