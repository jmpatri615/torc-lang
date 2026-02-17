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

use self::constraints::{BandwidthConstraint, Lifetime};
use self::edge::{Edge, EdgeId};
use self::node::{Node, NodeId, NodeKind};
use self::region::{Region, RegionId};

use crate::contract::{EffectSet, ObligationKind, ProofObligation, ProofStatus};
use crate::types::check::types_compatible;
use crate::types::{Linearity, Predicate, Type};

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

    #[error("merge conflict: duplicate {kind} id {id}")]
    MergeConflict { kind: String, id: uuid::Uuid },

    #[error("incomplete port mapping: unmapped boundary port ({node}, {port})")]
    UnmappedBoundaryPort { node: NodeId, port: usize },
}

/// Direction of a boundary edge relative to a module.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BoundaryDirection {
    /// Edge flows into the module (external source → internal target).
    In,
    /// Edge flows out of the module (internal source → external target).
    Out,
}

/// A boundary edge that crosses a module boundary.
#[derive(Debug, Clone)]
pub struct BoundaryEdge {
    /// Node inside the module.
    pub internal_node: NodeId,
    /// Port on the internal node.
    pub internal_port: usize,
    /// Node outside the module.
    pub external_node: NodeId,
    /// Port on the external node.
    pub external_port: usize,
    /// Data type flowing on this edge.
    pub data_type: Option<Type>,
    /// Direction relative to the module.
    pub direction: BoundaryDirection,
}

/// A module extracted from a graph, with boundary information.
pub struct ModuleInterface {
    /// The internal subgraph.
    pub graph: Graph,
    /// Edges flowing into the module.
    pub inputs: Vec<BoundaryEdge>,
    /// Edges flowing out of the module.
    pub outputs: Vec<BoundaryEdge>,
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

    /// Merge another graph into this one.
    ///
    /// All node, edge, and region IDs in `other` must be disjoint from `self`.
    /// Returns `MergeConflict` if any ID collides. On success, all nodes, edges,
    /// regions, and indexes from `other` are copied into `self`.
    pub fn merge(&mut self, other: &Graph) -> Result<(), GraphError> {
        // Conflict check — scan all IDs before mutating
        for id in other.nodes.keys() {
            if self.nodes.contains_key(id) {
                return Err(GraphError::MergeConflict {
                    kind: "node".to_string(),
                    id: *id,
                });
            }
        }
        for id in other.edges.keys() {
            if self.edges.contains_key(id) {
                return Err(GraphError::MergeConflict {
                    kind: "edge".to_string(),
                    id: *id,
                });
            }
        }
        for id in other.regions.keys() {
            if self.regions.contains_key(id) {
                return Err(GraphError::MergeConflict {
                    kind: "region".to_string(),
                    id: *id,
                });
            }
        }

        // Copy data
        for (id, node) in &other.nodes {
            self.nodes.insert(*id, node.clone());
        }
        for (id, edge) in &other.edges {
            self.edges.insert(*id, edge.clone());
        }
        for (id, region) in &other.regions {
            self.regions.insert(*id, region.clone());
        }

        // Merge indexes
        for (id, edges) in &other.outgoing {
            self.outgoing.entry(*id).or_default().extend(edges);
        }
        for (id, edges) in &other.incoming {
            self.incoming.entry(*id).or_default().extend(edges);
        }
        for (id, children) in &other.region_children {
            self.region_children.entry(*id).or_default().extend(children);
        }
        for (node_id, region_id) in &other.node_region {
            self.node_region.insert(*node_id, *region_id);
        }
        for (child_id, parent_id) in &other.region_parent {
            self.region_parent.insert(*child_id, *parent_id);
        }

        Ok(())
    }

    /// Remove an edge from the graph.
    ///
    /// Removes the edge from the `edges` map and from the source/target
    /// outgoing/incoming index lists.
    pub fn remove_edge(&mut self, id: EdgeId) -> Result<(), GraphError> {
        let edge = self.edges.remove(&id).ok_or(GraphError::EdgeNotFound(id))?;
        if let Some(list) = self.outgoing.get_mut(&edge.source.0) {
            list.retain(|&eid| eid != id);
        }
        if let Some(list) = self.incoming.get_mut(&edge.target.0) {
            list.retain(|&eid| eid != id);
        }
        Ok(())
    }

    /// Remove a node and all its connected edges from the graph.
    ///
    /// Also removes the node from its containing region if any.
    pub fn remove_node(&mut self, id: NodeId) -> Result<(), GraphError> {
        if !self.nodes.contains_key(&id) {
            return Err(GraphError::NodeNotFound(id));
        }

        // Collect all connected edge IDs
        let out_edges: Vec<EdgeId> = self.outgoing.get(&id).cloned().unwrap_or_default();
        let in_edges: Vec<EdgeId> = self.incoming.get(&id).cloned().unwrap_or_default();

        // Remove each edge (cleans up the other endpoint's index)
        for eid in out_edges {
            // Remove from edges map and target's incoming list
            if let Some(edge) = self.edges.remove(&eid) {
                if let Some(list) = self.incoming.get_mut(&edge.target.0) {
                    list.retain(|&e| e != eid);
                }
            }
        }
        for eid in in_edges {
            if let Some(edge) = self.edges.remove(&eid) {
                if let Some(list) = self.outgoing.get_mut(&edge.source.0) {
                    list.retain(|&e| e != eid);
                }
            }
        }

        // Remove from indexes
        self.outgoing.remove(&id);
        self.incoming.remove(&id);

        // Remove from region membership
        if let Some(region_id) = self.node_region.remove(&id) {
            if let Some(children) = self.region_children.get_mut(&region_id) {
                children.retain(|&nid| nid != id);
            }
            if let Some(region) = self.regions.get_mut(&region_id) {
                region.children.retain(|&nid| nid != id);
            }
        }

        self.nodes.remove(&id);
        Ok(())
    }

    /// Remove a region from the graph.
    ///
    /// Child nodes become regionless. Child sub-regions lose their parent.
    pub fn remove_region(&mut self, id: RegionId) -> Result<(), GraphError> {
        if !self.regions.contains_key(&id) {
            return Err(GraphError::RegionNotFound(id));
        }

        // Clear node_region for child nodes
        if let Some(children) = self.region_children.remove(&id) {
            for child_id in children {
                self.node_region.remove(&child_id);
            }
        }

        // Child sub-regions whose parent is this region: clear parent
        let child_region_ids: Vec<RegionId> = self
            .region_parent
            .iter()
            .filter(|(_, &parent)| parent == id)
            .map(|(&child, _)| child)
            .collect();
        for child_rid in child_region_ids {
            self.region_parent.remove(&child_rid);
            if let Some(region) = self.regions.get_mut(&child_rid) {
                region.parent = None;
            }
        }

        // If this region has a parent, remove from region_parent index
        self.region_parent.remove(&id);

        self.regions.remove(&id);
        Ok(())
    }

    /// Flatten a region into its parent.
    ///
    /// Child nodes and sub-regions are promoted to the parent region (if any),
    /// or become regionless (if the inlined region has no parent).
    /// Edges between inlined nodes are preserved.
    pub fn inline_region(&mut self, id: RegionId) -> Result<(), GraphError> {
        let region = self
            .regions
            .get(&id)
            .ok_or(GraphError::RegionNotFound(id))?;
        let children = region.children.clone();
        let parent = region.parent;

        // Collect child sub-regions whose parent is this region
        let child_sub_regions: Vec<RegionId> = self
            .region_parent
            .iter()
            .filter(|(_, &p)| p == id)
            .map(|(&c, _)| c)
            .collect();

        if let Some(parent_id) = parent {
            // Promote children to parent region
            for &child_id in &children {
                self.node_region.insert(child_id, parent_id);
                if let Some(parent_children) = self.region_children.get_mut(&parent_id) {
                    parent_children.push(child_id);
                }
                if let Some(parent_region) = self.regions.get_mut(&parent_id) {
                    parent_region.children.push(child_id);
                }
            }
            // Reparent child sub-regions to parent
            for child_rid in &child_sub_regions {
                self.region_parent.insert(*child_rid, parent_id);
                if let Some(region) = self.regions.get_mut(child_rid) {
                    region.parent = Some(parent_id);
                }
            }
        } else {
            // No parent: children become regionless
            for &child_id in &children {
                self.node_region.remove(&child_id);
            }
            // Sub-regions lose their parent
            for child_rid in &child_sub_regions {
                self.region_parent.remove(child_rid);
                if let Some(region) = self.regions.get_mut(child_rid) {
                    region.parent = None;
                }
            }
        }

        // Remove the inlined region itself
        self.region_children.remove(&id);
        self.region_parent.remove(&id);
        self.regions.remove(&id);
        Ok(())
    }

    /// Merge another graph into this one and create edges between specified port pairs.
    ///
    /// Each connection specifies `((src_node, src_port), (dst_node, dst_port))`.
    /// The source and target nodes may come from either graph.
    pub fn compose(
        &mut self,
        other: &Graph,
        connections: &[(edge::PortRef, edge::PortRef)],
    ) -> Result<(), GraphError> {
        self.merge(other)?;
        for &((src_node, src_port), (dst_node, dst_port)) in connections {
            let edge = Edge::new((src_node, src_port), (dst_node, dst_port));
            self.add_edge(edge)?;
        }
        Ok(())
    }

    /// Extract a module (subgraph with boundary information) for the given nodes.
    ///
    /// Returns the internal subgraph plus lists of boundary edges
    /// that cross the module boundary.
    pub fn extract_module(&self, node_ids: &HashSet<NodeId>) -> ModuleInterface {
        let graph = self.extract_subgraph(node_ids);

        let mut inputs = Vec::new();
        let mut outputs = Vec::new();

        for edge in self.edges.values() {
            let src_in = node_ids.contains(&edge.source.0);
            let tgt_in = node_ids.contains(&edge.target.0);

            match (src_in, tgt_in) {
                (true, false) => {
                    // Source inside, target outside → output
                    outputs.push(BoundaryEdge {
                        internal_node: edge.source.0,
                        internal_port: edge.source.1,
                        external_node: edge.target.0,
                        external_port: edge.target.1,
                        data_type: edge.data_type.clone(),
                        direction: BoundaryDirection::Out,
                    });
                }
                (false, true) => {
                    // Source outside, target inside → input
                    inputs.push(BoundaryEdge {
                        internal_node: edge.target.0,
                        internal_port: edge.target.1,
                        external_node: edge.source.0,
                        external_port: edge.source.1,
                        data_type: edge.data_type.clone(),
                        direction: BoundaryDirection::In,
                    });
                }
                _ => {} // Both inside or both outside — skip
            }
        }

        ModuleInterface {
            graph,
            inputs,
            outputs,
        }
    }

    /// Replace a set of nodes with a replacement graph, reconnecting boundary edges.
    ///
    /// `port_map` maps `(old_node, port)` → `(new_node, port)` for each boundary
    /// edge endpoint that falls within `old_nodes`.
    pub fn replace_subgraph(
        &mut self,
        old_nodes: &HashSet<NodeId>,
        replacement: &Graph,
        port_map: &HashMap<(NodeId, usize), (NodeId, usize)>,
    ) -> Result<(), GraphError> {
        // 1. Collect boundary edge descriptors before removing anything
        struct BoundaryDescriptor {
            external_endpoint: (NodeId, usize),
            internal_is_source: bool,
            new_endpoint: (NodeId, usize),
            data_type: Option<Type>,
            lifetime: Lifetime,
            bandwidth: Option<BandwidthConstraint>,
        }

        let mut descriptors = Vec::new();

        for edge in self.edges.values() {
            let src_in = old_nodes.contains(&edge.source.0);
            let tgt_in = old_nodes.contains(&edge.target.0);

            match (src_in, tgt_in) {
                (true, false) => {
                    // Source is internal, target is external
                    let new_ep = port_map
                        .get(&(edge.source.0, edge.source.1))
                        .ok_or(GraphError::UnmappedBoundaryPort {
                            node: edge.source.0,
                            port: edge.source.1,
                        })?;
                    descriptors.push(BoundaryDescriptor {
                        external_endpoint: edge.target,
                        internal_is_source: true,
                        new_endpoint: *new_ep,
                        data_type: edge.data_type.clone(),
                        lifetime: edge.lifetime.clone(),
                        bandwidth: edge.bandwidth.clone(),
                    });
                }
                (false, true) => {
                    // Target is internal, source is external
                    let new_ep = port_map
                        .get(&(edge.target.0, edge.target.1))
                        .ok_or(GraphError::UnmappedBoundaryPort {
                            node: edge.target.0,
                            port: edge.target.1,
                        })?;
                    descriptors.push(BoundaryDescriptor {
                        external_endpoint: edge.source,
                        internal_is_source: false,
                        new_endpoint: *new_ep,
                        data_type: edge.data_type.clone(),
                        lifetime: edge.lifetime.clone(),
                        bandwidth: edge.bandwidth.clone(),
                    });
                }
                _ => {} // Both inside or both outside
            }
        }

        // 2. Validate that all mapped endpoints exist in the replacement graph
        for desc in &descriptors {
            if !replacement.nodes.contains_key(&desc.new_endpoint.0) {
                return Err(GraphError::NodeNotFound(desc.new_endpoint.0));
            }
        }

        // 3. Remove old nodes (this removes their edges and region membership)
        let old_node_ids: Vec<NodeId> = old_nodes.iter().copied().collect();
        for id in &old_node_ids {
            self.remove_node(*id)?;
        }

        // 4. Merge replacement
        self.merge(replacement)?;

        // 5. Recreate boundary edges
        for desc in descriptors {
            let mut new_edge = if desc.internal_is_source {
                Edge::new(desc.new_endpoint, desc.external_endpoint)
            } else {
                Edge::new(desc.external_endpoint, desc.new_endpoint)
            };
            new_edge.data_type = desc.data_type;
            new_edge.lifetime = desc.lifetime;
            new_edge.bandwidth = desc.bandwidth;
            self.add_edge(new_edge)?;
        }

        Ok(())
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
    fn merge_disjoint_graphs() {
        let mut g1 = Graph::new();
        let n1 = make_literal_node();
        let n2 = make_arithmetic_node();
        let id1 = n1.id;
        let id2 = n2.id;
        g1.add_node(n1).unwrap();
        g1.add_node(n2).unwrap();
        g1.add_edge(Edge::new((id1, 0), (id2, 0))).unwrap();

        let mut g2 = Graph::new();
        let n3 = make_literal_node();
        let n4 = make_literal_node();
        let id3 = n3.id;
        let id4 = n4.id;
        g2.add_node(n3).unwrap();
        g2.add_node(n4).unwrap();
        g2.add_edge(Edge::new((id3, 0), (id4, 0))).unwrap();

        g1.merge(&g2).unwrap();
        assert_eq!(g1.node_count(), 4);
        assert_eq!(g1.edge_count(), 2);
    }

    #[test]
    fn merge_empty_into_populated() {
        let mut g1 = Graph::new();
        let n1 = make_literal_node();
        g1.add_node(n1).unwrap();

        let g2 = Graph::new();
        g1.merge(&g2).unwrap();
        assert_eq!(g1.node_count(), 1);
    }

    #[test]
    fn merge_populated_into_empty() {
        let mut g1 = Graph::new();

        let mut g2 = Graph::new();
        let n1 = make_literal_node();
        let n2 = make_literal_node();
        g2.add_node(n1).unwrap();
        g2.add_node(n2).unwrap();

        g1.merge(&g2).unwrap();
        assert_eq!(g1.node_count(), 2);
    }

    #[test]
    fn merge_conflict_detected() {
        let mut g1 = Graph::new();
        let n1 = make_literal_node();
        let shared_id = n1.id;
        g1.add_node(n1).unwrap();

        let mut g2 = Graph::new();
        let n2 = Node::with_id(shared_id, NodeKind::Literal);
        g2.add_node(n2).unwrap();

        let err = g1.merge(&g2).unwrap_err();
        assert!(matches!(err, GraphError::MergeConflict { .. }));
    }

    #[test]
    fn merge_edge_conflict_detected() {
        let mut g1 = Graph::new();
        let n1 = make_literal_node();
        let n2 = make_literal_node();
        let id1 = n1.id;
        let id2 = n2.id;
        g1.add_node(n1).unwrap();
        g1.add_node(n2).unwrap();
        let shared_edge = Edge::new((id1, 0), (id2, 0));
        let shared_edge_id = shared_edge.id;
        g1.add_edge(shared_edge).unwrap();

        let mut g2 = Graph::new();
        let n3 = make_literal_node();
        let n4 = make_literal_node();
        let id3 = n3.id;
        let id4 = n4.id;
        g2.add_node(n3).unwrap();
        g2.add_node(n4).unwrap();
        let dup_edge = Edge::with_id(shared_edge_id, (id3, 0), (id4, 0));
        g2.add_edge(dup_edge).unwrap();

        let err = g1.merge(&g2).unwrap_err();
        assert!(
            matches!(err, GraphError::MergeConflict { ref kind, .. } if kind == "edge")
        );
    }

    #[test]
    fn merge_region_conflict_detected() {
        let mut g1 = Graph::new();
        let n1 = make_literal_node();
        let id1 = n1.id;
        g1.add_node(n1).unwrap();
        let r1 = Region::new(RegionKind::Sequential, vec![id1]);
        let shared_region_id = r1.id;
        g1.add_region(r1).unwrap();

        let mut g2 = Graph::new();
        let n2 = make_literal_node();
        let id2 = n2.id;
        g2.add_node(n2).unwrap();
        let r2 = Region::with_id(shared_region_id, RegionKind::Parallel, vec![id2]);
        g2.add_region(r2).unwrap();

        let err = g1.merge(&g2).unwrap_err();
        assert!(
            matches!(err, GraphError::MergeConflict { ref kind, .. } if kind == "region")
        );
    }

    #[test]
    fn merge_preserves_region_hierarchy() {
        let mut g1 = Graph::new();
        let n1 = make_literal_node();
        g1.add_node(n1).unwrap();

        let mut g2 = Graph::new();
        let n2 = make_literal_node();
        let n3 = make_literal_node();
        let id2 = n2.id;
        let id3 = n3.id;
        g2.add_node(n2).unwrap();
        g2.add_node(n3).unwrap();

        let inner = Region::new(RegionKind::Sequential, vec![id2]);
        let inner_id = inner.id;
        g2.add_region(inner).unwrap();

        let outer = Region::new(RegionKind::Parallel, vec![id3]);
        let outer_id = outer.id;
        g2.add_region(outer).unwrap();
        g2.set_region_parent(inner_id, outer_id).unwrap();

        g1.merge(&g2).unwrap();
        assert_eq!(g1.region_count(), 2);
        assert_eq!(g1.parent_region(&inner_id), Some(&outer_id));
        assert_eq!(g1.containing_region(&id2), Some(&inner_id));
        // Merged graph should pass structural validation
        assert!(g1.validate().is_ok());
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

    #[test]
    fn remove_node_basic() {
        let mut g = Graph::new();
        let n1 = make_literal_node();
        let n2 = make_arithmetic_node();
        let n3 = make_literal_node();
        let id1 = n1.id;
        let id2 = n2.id;
        let id3 = n3.id;
        g.add_node(n1).unwrap();
        g.add_node(n2).unwrap();
        g.add_node(n3).unwrap();
        g.add_edge(Edge::new((id1, 0), (id2, 0))).unwrap();
        g.add_edge(Edge::new((id2, 0), (id3, 0))).unwrap();

        g.remove_node(id2).unwrap();

        assert_eq!(g.node_count(), 2);
        assert_eq!(g.edge_count(), 0);
        assert!(g.get_node(&id2).is_none());
        assert!(g.outgoing_edges(&id1).is_empty());
        assert!(g.incoming_edges(&id3).is_empty());
    }

    #[test]
    fn remove_node_updates_region() {
        let mut g = Graph::new();
        let n1 = make_literal_node();
        let n2 = make_literal_node();
        let id1 = n1.id;
        let id2 = n2.id;
        g.add_node(n1).unwrap();
        g.add_node(n2).unwrap();

        let r = Region::new(RegionKind::Parallel, vec![id1, id2]);
        let rid = r.id;
        g.add_region(r).unwrap();

        g.remove_node(id1).unwrap();

        assert_eq!(g.containing_region(&id1), None);
        let region = g.get_region(&rid).unwrap();
        assert_eq!(region.children.len(), 1);
        assert!(!region.children.contains(&id1));
    }

    #[test]
    fn remove_edge_basic() {
        let mut g = Graph::new();
        let n1 = make_literal_node();
        let n2 = make_arithmetic_node();
        let id1 = n1.id;
        let id2 = n2.id;
        g.add_node(n1).unwrap();
        g.add_node(n2).unwrap();
        let edge = Edge::new((id1, 0), (id2, 0));
        let eid = edge.id;
        g.add_edge(edge).unwrap();

        g.remove_edge(eid).unwrap();

        assert_eq!(g.edge_count(), 0);
        assert!(g.outgoing_edges(&id1).is_empty());
        assert!(g.incoming_edges(&id2).is_empty());
    }

    #[test]
    fn remove_region_frees_children() {
        let mut g = Graph::new();
        let n1 = make_literal_node();
        let n2 = make_literal_node();
        let id1 = n1.id;
        let id2 = n2.id;
        g.add_node(n1).unwrap();
        g.add_node(n2).unwrap();

        let r = Region::new(RegionKind::Parallel, vec![id1, id2]);
        let rid = r.id;
        g.add_region(r).unwrap();

        g.remove_region(rid).unwrap();

        assert_eq!(g.region_count(), 0);
        assert_eq!(g.containing_region(&id1), None);
        assert_eq!(g.containing_region(&id2), None);
    }

    #[test]
    fn remove_region_updates_child_regions() {
        let mut g = Graph::new();
        let n1 = make_literal_node();
        let n2 = make_literal_node();
        let id1 = n1.id;
        let id2 = n2.id;
        g.add_node(n1).unwrap();
        g.add_node(n2).unwrap();

        let parent = Region::new(RegionKind::Parallel, vec![id1]);
        let parent_id = parent.id;
        g.add_region(parent).unwrap();

        let child = Region::new(RegionKind::Sequential, vec![id2]);
        let child_id = child.id;
        g.add_region(child).unwrap();
        g.set_region_parent(child_id, parent_id).unwrap();

        g.remove_region(parent_id).unwrap();

        // Child region should lose its parent
        assert_eq!(g.parent_region(&child_id), None);
        let child_region = g.get_region(&child_id).unwrap();
        assert_eq!(child_region.parent, None);
        // n1 should be regionless now
        assert_eq!(g.containing_region(&id1), None);
    }

    #[test]
    fn inline_region_with_parent() {
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

        // Outer region with n3
        let outer = Region::new(RegionKind::Parallel, vec![id3]);
        let outer_id = outer.id;
        g.add_region(outer).unwrap();

        // Inner region with n1, n2
        let inner = Region::new(RegionKind::Sequential, vec![id1, id2]);
        let inner_id = inner.id;
        g.add_region(inner).unwrap();
        g.set_region_parent(inner_id, outer_id).unwrap();

        // Sub-region inside inner
        let sub = Region::new(RegionKind::Atomic, vec![]);
        let sub_id = sub.id;
        g.add_region(sub).unwrap();
        g.set_region_parent(sub_id, inner_id).unwrap();

        g.inline_region(inner_id).unwrap();

        // Inner region is gone
        assert!(g.get_region(&inner_id).is_none());
        // Children promoted to outer
        assert_eq!(g.containing_region(&id1), Some(&outer_id));
        assert_eq!(g.containing_region(&id2), Some(&outer_id));
        // Sub-region reparented to outer
        assert_eq!(g.parent_region(&sub_id), Some(&outer_id));
        let sub_region = g.get_region(&sub_id).unwrap();
        assert_eq!(sub_region.parent, Some(outer_id));
    }

    #[test]
    fn inline_region_without_parent() {
        let mut g = Graph::new();
        let n1 = make_literal_node();
        let id1 = n1.id;
        g.add_node(n1).unwrap();

        let r = Region::new(RegionKind::Parallel, vec![id1]);
        let rid = r.id;
        g.add_region(r).unwrap();

        g.inline_region(rid).unwrap();

        assert!(g.get_region(&rid).is_none());
        assert_eq!(g.containing_region(&id1), None);
    }

    #[test]
    fn extract_module_identifies_boundaries() {
        let mut g = Graph::new();
        let n1 = make_literal_node();
        let n2 = make_arithmetic_node();
        let n3 = make_literal_node();
        let n4 = make_literal_node();
        let id1 = n1.id;
        let id2 = n2.id;
        let id3 = n3.id;
        let id4 = n4.id;
        g.add_node(n1).unwrap();
        g.add_node(n2).unwrap();
        g.add_node(n3).unwrap();
        g.add_node(n4).unwrap();

        // n1 -> n2 -> n3, n4 -> n2
        g.add_edge(Edge::new((id1, 0), (id2, 0))).unwrap();
        g.add_edge(Edge::new((id2, 0), (id3, 0))).unwrap();
        g.add_edge(Edge::new((id4, 0), (id2, 1))).unwrap();

        // Extract module = {n2}
        let module_nodes: HashSet<NodeId> = [id2].into_iter().collect();
        let module = g.extract_module(&module_nodes);

        assert_eq!(module.graph.node_count(), 1);
        // Inputs: n1->n2, n4->n2 (2 incoming boundary edges)
        assert_eq!(module.inputs.len(), 2);
        // Outputs: n2->n3 (1 outgoing boundary edge)
        assert_eq!(module.outputs.len(), 1);
        assert_eq!(module.outputs[0].internal_node, id2);
        assert_eq!(module.outputs[0].external_node, id3);
    }

    #[test]
    fn extract_module_captures_edge_types() {
        use crate::types::Type;

        let mut g = Graph::new();
        let n1 = make_literal_node();
        let n2 = make_arithmetic_node();
        let id1 = n1.id;
        let id2 = n2.id;
        g.add_node(n1).unwrap();
        g.add_node(n2).unwrap();

        g.add_edge(Edge::typed((id1, 0), (id2, 0), Type::f32()))
            .unwrap();

        let module_nodes: HashSet<NodeId> = [id2].into_iter().collect();
        let module = g.extract_module(&module_nodes);

        assert_eq!(module.inputs.len(), 1);
        assert_eq!(module.inputs[0].data_type, Some(Type::f32()));
    }

    #[test]
    fn replace_subgraph_basic() {
        let mut g = Graph::new();
        let n1 = make_literal_node();
        let n2 = make_arithmetic_node();
        let n3 = make_literal_node();
        let id1 = n1.id;
        let id2 = n2.id;
        let id3 = n3.id;
        g.add_node(n1).unwrap();
        g.add_node(n2).unwrap();
        g.add_node(n3).unwrap();
        g.add_edge(Edge::new((id1, 0), (id2, 0))).unwrap();
        g.add_edge(Edge::new((id2, 0), (id3, 0))).unwrap();

        // Replacement: a single new node
        let mut replacement = Graph::new();
        let new_node = make_literal_node();
        let new_id = new_node.id;
        replacement.add_node(new_node).unwrap();

        let old_nodes: HashSet<NodeId> = [id2].into_iter().collect();
        let mut port_map = HashMap::new();
        port_map.insert((id2, 0), (new_id, 0)); // output boundary
        // Also need to map the input boundary: (id2, 0) as target
        // But the input edge is (id1,0)->(id2,0), so internal endpoint is (id2,0)
        // Both boundary edges reference (id2, 0) — one as source, one as target
        // port_map already has (id2, 0) -> (new_id, 0)

        g.replace_subgraph(&old_nodes, &replacement, &port_map)
            .unwrap();

        assert!(g.get_node(&id2).is_none());
        assert!(g.get_node(&new_id).is_some());
        assert_eq!(g.node_count(), 3); // n1, new_node, n3
        assert_eq!(g.edge_count(), 2); // reconnected boundary edges
    }

    #[test]
    fn replace_subgraph_preserves_edge_metadata() {
        use crate::graph::constraints::Lifetime;
        use crate::types::Type;

        let mut g = Graph::new();
        let n1 = make_literal_node();
        let n2 = make_arithmetic_node();
        let id1 = n1.id;
        let id2 = n2.id;
        g.add_node(n1).unwrap();
        g.add_node(n2).unwrap();
        let mut edge = Edge::typed((id1, 0), (id2, 0), Type::i32());
        edge.lifetime = Lifetime::Manual;
        g.add_edge(edge).unwrap();

        let mut replacement = Graph::new();
        let new_node = make_literal_node();
        let new_id = new_node.id;
        replacement.add_node(new_node).unwrap();

        let old_nodes: HashSet<NodeId> = [id2].into_iter().collect();
        let mut port_map = HashMap::new();
        port_map.insert((id2, 0), (new_id, 0));

        g.replace_subgraph(&old_nodes, &replacement, &port_map)
            .unwrap();

        let new_edge = g.edges().next().unwrap();
        assert_eq!(new_edge.data_type, Some(Type::i32()));
        assert_eq!(new_edge.lifetime, Lifetime::Manual);
    }

    #[test]
    fn replace_subgraph_unmapped_port_error() {
        let mut g = Graph::new();
        let n1 = make_literal_node();
        let n2 = make_arithmetic_node();
        let id1 = n1.id;
        let id2 = n2.id;
        g.add_node(n1).unwrap();
        g.add_node(n2).unwrap();
        g.add_edge(Edge::new((id1, 0), (id2, 0))).unwrap();

        let replacement = Graph::new();
        let old_nodes: HashSet<NodeId> = [id2].into_iter().collect();
        let port_map = HashMap::new(); // Empty — missing mapping

        let result = g.replace_subgraph(&old_nodes, &replacement, &port_map);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, GraphError::UnmappedBoundaryPort { .. }));
    }

    #[test]
    fn replace_subgraph_invalid_target_in_port_map() {
        let mut g = Graph::new();
        let n1 = make_literal_node();
        let n2 = make_arithmetic_node();
        let id1 = n1.id;
        let id2 = n2.id;
        g.add_node(n1).unwrap();
        g.add_node(n2).unwrap();
        g.add_edge(Edge::new((id1, 0), (id2, 0))).unwrap();

        let replacement = Graph::new(); // Empty — no nodes
        let old_nodes: HashSet<NodeId> = [id2].into_iter().collect();
        let fake_id = NodeId::new_v4();
        let mut port_map = HashMap::new();
        port_map.insert((id2, 0), (fake_id, 0)); // Maps to non-existent node

        let result = g.replace_subgraph(&old_nodes, &replacement, &port_map);
        assert!(result.is_err());
        // Error should be caught before any mutation occurs
        assert_eq!(g.node_count(), 2); // Graph unchanged
    }

    #[test]
    fn compose_disjoint_with_connections() {
        let mut g1 = Graph::new();
        let n1 = make_literal_node();
        let id1 = n1.id;
        g1.add_node(n1).unwrap();

        let mut g2 = Graph::new();
        let n2 = make_arithmetic_node();
        let id2 = n2.id;
        g2.add_node(n2).unwrap();

        g1.compose(&g2, &[((id1, 0), (id2, 0))]).unwrap();

        assert_eq!(g1.node_count(), 2);
        assert_eq!(g1.edge_count(), 1);
        assert_eq!(g1.outgoing_edges(&id1).len(), 1);
        assert_eq!(g1.incoming_edges(&id2).len(), 1);
    }

    #[test]
    fn compose_validates_connections() {
        let mut g1 = Graph::new();
        let n1 = make_literal_node();
        let id1 = n1.id;
        g1.add_node(n1).unwrap();

        let g2 = Graph::new();
        let fake_id = NodeId::new_v4();

        let result = g1.compose(&g2, &[((id1, 0), (fake_id, 0))]);
        assert!(result.is_err());
    }

    #[test]
    fn inline_region_preserves_edges() {
        let mut g = Graph::new();
        let n1 = make_literal_node();
        let n2 = make_arithmetic_node();
        let id1 = n1.id;
        let id2 = n2.id;
        g.add_node(n1).unwrap();
        g.add_node(n2).unwrap();
        g.add_edge(Edge::new((id1, 0), (id2, 0))).unwrap();

        let r = Region::new(RegionKind::Sequential, vec![id1, id2]);
        let rid = r.id;
        g.add_region(r).unwrap();

        g.inline_region(rid).unwrap();

        assert_eq!(g.edge_count(), 1);
        assert_eq!(g.outgoing_edges(&id1).len(), 1);
        assert_eq!(g.incoming_edges(&id2).len(), 1);
    }
}
