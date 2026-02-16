//! Graph construction API for building Torc graphs programmatically.
//!
//! The `GraphBuilder` provides an ergonomic API for constructing Torc computation
//! graphs with validation during construction. It is designed for programmatic use
//! by AI systems and other tools.
//!
//! # Example
//!
//! ```rust
//! use torc_core::builder::GraphBuilder;
//! use torc_core::graph::node::{NodeKind, ArithmeticOp};
//! use torc_core::types::Type;
//!
//! let mut builder = GraphBuilder::new();
//!
//! // Create two literal inputs
//! let a = builder.add_literal("a");
//! let b = builder.add_literal("b");
//!
//! // Add them together
//! let sum = builder.add_arithmetic(ArithmeticOp::Add, "sum");
//!
//! // Connect inputs to the add node
//! builder.connect(a, 0, sum, 0).unwrap();
//! builder.connect(b, 0, sum, 1).unwrap();
//!
//! // Build the graph
//! let graph = builder.build().unwrap();
//! assert_eq!(graph.node_count(), 3);
//! ```

use std::collections::HashMap;

use crate::contract::Contract;
use crate::graph::constraints::{BandwidthConstraint, Constraint, Lifetime};
use crate::graph::edge::Edge;
use crate::graph::node::{ArithmeticOp, BitwiseOp, ComparisonOp, Node, NodeId, NodeKind};
use crate::graph::port::Port;
use crate::graph::region::{Region, RegionId, RegionKind};
use crate::graph::{Graph, GraphError};
use crate::provenance::Provenance;
use crate::types::{Type, TypeSignature};

/// A builder for constructing Torc computation graphs.
///
/// Provides convenience methods for creating common node types, connecting
/// nodes via edges, and organizing nodes into regions. Validates the graph
/// during construction.
pub struct GraphBuilder {
    graph: Graph,
    /// Named nodes for easier reference during construction.
    names: HashMap<String, NodeId>,
    /// Stack of open regions for nested region construction.
    /// Each entry is (kind, child_nodes, child_region_ids).
    region_stack: Vec<(RegionKind, Vec<NodeId>, Vec<RegionId>)>,
}

impl GraphBuilder {
    /// Create a new empty graph builder.
    pub fn new() -> Self {
        Self {
            graph: Graph::new(),
            names: HashMap::new(),
            region_stack: Vec::new(),
        }
    }

    /// Add a node with the given kind and optional name.
    pub fn add_node(&mut self, kind: NodeKind, name: &str) -> NodeId {
        let node = Node::new(kind);
        let id = node.id;
        self.graph
            .add_node(node)
            .expect("fresh UUID should not collide");
        if !name.is_empty() {
            self.names.insert(name.to_string(), id);
        }
        // Track node in current region if one is open
        if let Some((_kind, children, _)) = self.region_stack.last_mut() {
            children.push(id);
        }
        id
    }

    /// Add a node with a type signature.
    pub fn add_typed_node(
        &mut self,
        kind: NodeKind,
        name: &str,
        type_sig: TypeSignature,
    ) -> NodeId {
        let node = Node::new(kind).with_type_signature(type_sig);
        let id = node.id;
        self.graph
            .add_node(node)
            .expect("fresh UUID should not collide");
        if !name.is_empty() {
            self.names.insert(name.to_string(), id);
        }
        if let Some((_kind, children, _)) = self.region_stack.last_mut() {
            children.push(id);
        }
        id
    }

    /// Add a node with full metadata (type, contract, provenance).
    pub fn add_full_node(
        &mut self,
        kind: NodeKind,
        name: &str,
        type_sig: Option<TypeSignature>,
        contract: Option<Contract>,
        provenance: Option<Provenance>,
    ) -> NodeId {
        let mut node = Node::new(kind);
        node.type_signature = type_sig;
        node.contract = contract;
        node.provenance = provenance;
        let id = node.id;
        self.graph
            .add_node(node)
            .expect("fresh UUID should not collide");
        if !name.is_empty() {
            self.names.insert(name.to_string(), id);
        }
        if let Some((_kind, children, _)) = self.region_stack.last_mut() {
            children.push(id);
        }
        id
    }

    // === Convenience node constructors ===

    /// Add a literal value node.
    pub fn add_literal(&mut self, name: &str) -> NodeId {
        self.add_node(NodeKind::Literal, name)
    }

    /// Add an arithmetic operation node.
    pub fn add_arithmetic(&mut self, op: ArithmeticOp, name: &str) -> NodeId {
        self.add_node(NodeKind::Arithmetic(op), name)
    }

    /// Add a bitwise operation node.
    pub fn add_bitwise(&mut self, op: BitwiseOp, name: &str) -> NodeId {
        self.add_node(NodeKind::Bitwise(op), name)
    }

    /// Add a comparison operation node.
    pub fn add_comparison(&mut self, op: ComparisonOp, name: &str) -> NodeId {
        self.add_node(NodeKind::Comparison(op), name)
    }

    /// Add a type conversion node.
    pub fn add_conversion(&mut self, name: &str) -> NodeId {
        self.add_node(NodeKind::Conversion, name)
    }

    /// Add a select (conditional) node.
    pub fn add_select(&mut self, name: &str) -> NodeId {
        self.add_node(NodeKind::Select, name)
    }

    /// Add a construct (tuple/record builder) node.
    pub fn add_construct(&mut self, name: &str) -> NodeId {
        self.add_node(NodeKind::Construct, name)
    }

    /// Add a destructure (field extraction) node.
    pub fn add_destructure(&mut self, name: &str) -> NodeId {
        self.add_node(NodeKind::Destructure, name)
    }

    /// Add an FFI call node.
    pub fn add_ffi_call(&mut self, name: &str) -> NodeId {
        self.add_node(NodeKind::FFICall, name)
    }

    // === Edge construction ===

    /// Connect an output port of one node to an input port of another.
    pub fn connect(
        &mut self,
        from_node: NodeId,
        from_port: usize,
        to_node: NodeId,
        to_port: usize,
    ) -> Result<(), GraphError> {
        let edge = Edge::new((from_node, from_port), (to_node, to_port));
        self.graph.add_edge(edge)?;
        Ok(())
    }

    /// Connect nodes with a typed edge.
    pub fn connect_typed(
        &mut self,
        from_node: NodeId,
        from_port: usize,
        to_node: NodeId,
        to_port: usize,
        data_type: Type,
    ) -> Result<(), GraphError> {
        let edge = Edge::typed((from_node, from_port), (to_node, to_port), data_type);
        self.graph.add_edge(edge)?;
        Ok(())
    }

    /// Connect nodes with a lifetime annotation.
    pub fn connect_with_lifetime(
        &mut self,
        from_node: NodeId,
        from_port: usize,
        to_node: NodeId,
        to_port: usize,
        lifetime: Lifetime,
    ) -> Result<(), GraphError> {
        let edge = Edge::new((from_node, from_port), (to_node, to_port)).with_lifetime(lifetime);
        self.graph.add_edge(edge)?;
        Ok(())
    }

    /// Connect nodes with full edge metadata (type, lifetime, bandwidth).
    #[allow(clippy::too_many_arguments)]
    pub fn connect_full(
        &mut self,
        from_node: NodeId,
        from_port: usize,
        to_node: NodeId,
        to_port: usize,
        data_type: Option<Type>,
        lifetime: Lifetime,
        bandwidth: Option<BandwidthConstraint>,
    ) -> Result<(), GraphError> {
        let mut edge = Edge::new((from_node, from_port), (to_node, to_port));
        edge.data_type = data_type;
        edge.lifetime = lifetime;
        edge.bandwidth = bandwidth;
        self.graph.add_edge(edge)?;
        Ok(())
    }

    // === Region construction ===

    /// Begin a new region. Nodes added after this call will be collected
    /// into this region until `end_region()` is called.
    pub fn begin_region(&mut self, kind: RegionKind) {
        self.region_stack.push((kind, Vec::new(), Vec::new()));
    }

    /// End the current region, returning its ID.
    ///
    /// If regions are nested, the completed region's parent is set to the
    /// enclosing region (resolved when the parent region is itself ended).
    pub fn end_region(&mut self) -> Result<RegionId, GraphError> {
        let (kind, children, child_region_ids) = self
            .region_stack
            .pop()
            .ok_or(GraphError::RegionNotFound(uuid::Uuid::nil()))?;
        let region = Region::new(kind, children);
        let id = self.graph.add_region(region)?;

        // Set parent on all child regions that were completed inside this one
        for child_rid in child_region_ids {
            self.graph.set_region_parent(child_rid, id)?;
        }

        // If there's still a parent region on the stack, register ourselves as its child
        if let Some((_, _, parent_child_regions)) = self.region_stack.last_mut() {
            parent_child_regions.push(id);
        }

        Ok(id)
    }

    /// Add an execution constraint to a region.
    pub fn add_region_constraint(
        &mut self,
        region_id: RegionId,
        constraint: Constraint,
    ) -> Result<(), GraphError> {
        let region = self
            .graph
            .get_region_mut(&region_id)
            .ok_or(GraphError::RegionNotFound(region_id))?;
        region.constraints.push(constraint);
        Ok(())
    }

    /// Add an interface port to a region.
    pub fn add_region_port(
        &mut self,
        region_id: RegionId,
        port: Port,
    ) -> Result<(), GraphError> {
        let region = self
            .graph
            .get_region_mut(&region_id)
            .ok_or(GraphError::RegionNotFound(region_id))?;
        region.interfaces.push(port);
        Ok(())
    }

    // === Name lookup ===

    /// Look up a node ID by its assigned name.
    pub fn get_named(&self, name: &str) -> Option<NodeId> {
        self.names.get(name).copied()
    }

    // === Annotation ===

    /// Add an annotation to a node.
    pub fn annotate(&mut self, node_id: NodeId, key: &str, value: &str) -> Result<(), GraphError> {
        let node = self
            .graph
            .get_node_mut(&node_id)
            .ok_or(GraphError::NodeNotFound(node_id))?;
        node.annotations.insert(key.to_string(), value.to_string());
        Ok(())
    }

    /// Set the contract on a node.
    pub fn set_contract(&mut self, node_id: NodeId, contract: Contract) -> Result<(), GraphError> {
        let node = self
            .graph
            .get_node_mut(&node_id)
            .ok_or(GraphError::NodeNotFound(node_id))?;
        node.contract = Some(contract);
        Ok(())
    }

    /// Set the provenance on a node.
    pub fn set_provenance(
        &mut self,
        node_id: NodeId,
        provenance: Provenance,
    ) -> Result<(), GraphError> {
        let node = self
            .graph
            .get_node_mut(&node_id)
            .ok_or(GraphError::NodeNotFound(node_id))?;
        node.provenance = Some(provenance);
        Ok(())
    }

    // === Build ===

    /// Validate and return the constructed graph.
    pub fn build(self) -> Result<Graph, Vec<GraphError>> {
        self.graph.validate()?;
        Ok(self.graph)
    }

    /// Return the graph without validation (for testing or incremental construction).
    pub fn into_graph(self) -> Graph {
        self.graph
    }
}

impl Default for GraphBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simple_add_graph() {
        let mut b = GraphBuilder::new();
        let a = b.add_literal("a");
        let x = b.add_literal("b");
        let sum = b.add_arithmetic(ArithmeticOp::Add, "sum");
        b.connect(a, 0, sum, 0).unwrap();
        b.connect(x, 0, sum, 1).unwrap();

        let graph = b.build().unwrap();
        assert_eq!(graph.node_count(), 3);
        assert_eq!(graph.edge_count(), 2);
    }

    #[test]
    fn named_node_lookup() {
        let mut b = GraphBuilder::new();
        let id = b.add_literal("my_input");
        assert_eq!(b.get_named("my_input"), Some(id));
        assert_eq!(b.get_named("nonexistent"), None);
    }

    #[test]
    fn region_construction() {
        let mut b = GraphBuilder::new();
        b.begin_region(RegionKind::Parallel);
        b.add_literal("a");
        b.add_literal("b");
        let region_id = b.end_region().unwrap();

        let graph = b.build().unwrap();
        assert_eq!(graph.node_count(), 2);
        assert_eq!(graph.region_count(), 1);
        assert!(graph.get_region(&region_id).is_some());
    }

    #[test]
    fn typed_edge() {
        let mut b = GraphBuilder::new();
        let a = b.add_literal("a");
        let conv = b.add_conversion("conv");
        b.connect_typed(a, 0, conv, 0, Type::f32()).unwrap();

        let graph = b.build().unwrap();
        assert_eq!(graph.edge_count(), 1);
        let edge = graph.edges().next().unwrap();
        assert!(edge.data_type.is_some());
    }

    #[test]
    fn annotate_node() {
        let mut b = GraphBuilder::new();
        let id = b.add_literal("x");
        b.annotate(id, "safety_class", "ASIL-B").unwrap();

        let graph = b.build().unwrap();
        let node = graph.get_node(&id).unwrap();
        assert_eq!(node.annotations.get("safety_class").unwrap(), "ASIL-B");
    }

    #[test]
    fn dangling_edge_rejected() {
        let mut b = GraphBuilder::new();
        let a = b.add_literal("a");
        let fake_id = uuid::Uuid::new_v4();
        assert!(b.connect(a, 0, fake_id, 0).is_err());
    }

    #[test]
    fn contract_on_node() {
        let mut b = GraphBuilder::new();
        let id = b.add_arithmetic(ArithmeticOp::Add, "add");
        let contract = Contract::pure_default();
        b.set_contract(id, contract).unwrap();

        let graph = b.build().unwrap();
        let node = graph.get_node(&id).unwrap();
        assert!(node.contract.is_some());
    }

    #[test]
    fn clarke_transform() {
        // Build the Clarke transform from spec section 12
        let mut b = GraphBuilder::new();

        b.begin_region(RegionKind::Parallel);

        // Inputs
        let ia = b.add_literal("ia");
        let ib = b.add_literal("ib");

        // Constants
        let two = b.add_literal("two");
        let one_over_sqrt3 = b.add_literal("one_over_sqrt3");

        // i_alpha = ia (direct pass-through, but modeled as a node)
        let i_alpha = b.add_conversion("i_alpha");

        // temp = 2.0 * ib
        let mul_2_ib = b.add_arithmetic(ArithmeticOp::Mul, "mul_2_ib");

        // temp2 = ia + temp
        let add_ia = b.add_arithmetic(ArithmeticOp::Add, "add_ia_2ib");

        // i_beta = temp2 * ONE_OVER_SQRT3
        let i_beta = b.add_arithmetic(ArithmeticOp::Mul, "i_beta");

        // Connect: ia -> i_alpha
        b.connect(ia, 0, i_alpha, 0).unwrap();

        // Connect: two, ib -> mul_2_ib
        b.connect(two, 0, mul_2_ib, 0).unwrap();
        b.connect(ib, 0, mul_2_ib, 1).unwrap();

        // Connect: ia, mul_2_ib -> add
        b.connect(ia, 0, add_ia, 0).unwrap();
        b.connect(mul_2_ib, 0, add_ia, 1).unwrap();

        // Connect: add, one_over_sqrt3 -> i_beta
        b.connect(add_ia, 0, i_beta, 0).unwrap();
        b.connect(one_over_sqrt3, 0, i_beta, 1).unwrap();

        b.end_region().unwrap();

        let graph = b.build().unwrap();
        assert_eq!(graph.node_count(), 8);
        assert_eq!(graph.edge_count(), 7);

        // Verify topological ordering works (it's a DAG)
        let order = graph.topological_sort().unwrap();
        assert_eq!(order.len(), 8);
    }

    #[test]
    fn connect_with_lifetime() {
        use crate::graph::constraints::Lifetime;

        let mut b = GraphBuilder::new();
        let a = b.add_literal("a");
        let c = b.add_conversion("c");
        b.connect_with_lifetime(a, 0, c, 0, Lifetime::Manual)
            .unwrap();

        let graph = b.build().unwrap();
        let edge = graph.edges().next().unwrap();
        assert_eq!(edge.lifetime, Lifetime::Manual);
    }

    #[test]
    fn connect_full_edge() {
        use crate::graph::constraints::{BandwidthConstraint, Lifetime};

        let mut b = GraphBuilder::new();
        let a = b.add_literal("a");
        let c = b.add_conversion("c");
        b.connect_full(
            a,
            0,
            c,
            0,
            Some(Type::f32()),
            Lifetime::Bounded(1_000_000),
            Some(BandwidthConstraint::min(1_000_000)),
        )
        .unwrap();

        let graph = b.build().unwrap();
        let edge = graph.edges().next().unwrap();
        assert_eq!(edge.data_type, Some(Type::f32()));
        assert_eq!(edge.lifetime, Lifetime::Bounded(1_000_000));
        assert!(edge.bandwidth.is_some());
    }

    #[test]
    fn region_constraints_via_builder() {
        use crate::graph::constraints::Constraint;

        let mut b = GraphBuilder::new();
        b.begin_region(RegionKind::Parallel);
        b.add_literal("x");
        let rid = b.end_region().unwrap();

        b.add_region_constraint(rid, Constraint::MaxTime(50_000))
            .unwrap();
        b.add_region_constraint(rid, Constraint::MaxMemory(1024))
            .unwrap();

        let graph = b.build().unwrap();
        let region = graph.get_region(&rid).unwrap();
        assert_eq!(region.constraints.len(), 2);
    }

    #[test]
    fn region_ports_via_builder() {
        use crate::graph::port::Port;

        let mut b = GraphBuilder::new();
        b.begin_region(RegionKind::Sequential);
        b.add_literal("in");
        let rid = b.end_region().unwrap();

        b.add_region_port(rid, Port::input("x", 0, Type::f32()))
            .unwrap();
        b.add_region_port(rid, Port::output("y", 0, Type::f32()))
            .unwrap();

        let graph = b.build().unwrap();
        let region = graph.get_region(&rid).unwrap();
        assert_eq!(region.interfaces.len(), 2);
    }

    #[test]
    fn nested_region_building() {
        let mut b = GraphBuilder::new();

        // Outer region
        b.begin_region(RegionKind::Parallel);
        b.add_literal("outer_node");

        // Inner region
        b.begin_region(RegionKind::Sequential);
        b.add_literal("inner_node");
        let inner_id = b.end_region().unwrap();

        let outer_id = b.end_region().unwrap();

        let graph = b.build().unwrap();
        assert_eq!(graph.region_count(), 2);

        // Inner region's parent should be the outer region
        assert_eq!(graph.parent_region(&inner_id), Some(&outer_id));
        let inner = graph.get_region(&inner_id).unwrap();
        assert_eq!(inner.parent, Some(outer_id));

        // Outer region should list inner as a child
        let children = graph.child_regions(&outer_id);
        assert_eq!(children.len(), 1);
        assert!(children.contains(&inner_id));
    }
}
