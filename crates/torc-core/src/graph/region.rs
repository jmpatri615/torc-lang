//! Region types defining scope, lifetime, and execution constraints.
//!
//! A region is a subgraph boundary that groups nodes and defines
//! execution semantics (parallel, sequential, conditional, etc.).

use std::fmt;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::constraints::Constraint;
use super::node::NodeId;
use super::port::Port;

/// Globally unique region identifier.
pub type RegionId = Uuid;

/// The execution semantics of a region.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RegionKind {
    /// Nodes execute in a data-dependency-determined order.
    Sequential,
    /// All nodes are eligible for concurrent execution.
    Parallel,
    /// Nodes execute conditionally based on a guard.
    Conditional,
    /// Nodes execute iteratively with a termination condition.
    Iterative,
    /// All operations within are atomic (no interleaving).
    Atomic,
}

impl fmt::Display for RegionKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RegionKind::Sequential => write!(f, "sequential"),
            RegionKind::Parallel => write!(f, "parallel"),
            RegionKind::Conditional => write!(f, "conditional"),
            RegionKind::Iterative => write!(f, "iterative"),
            RegionKind::Atomic => write!(f, "atomic"),
        }
    }
}

/// A region in the Torc computation graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Region {
    /// Unique region identifier.
    pub id: RegionId,
    /// Execution semantics.
    pub kind: RegionKind,
    /// Nodes contained in this region.
    pub children: Vec<NodeId>,
    /// Execution constraints applied to this region.
    pub constraints: Vec<Constraint>,
    /// Typed interface ports for this region.
    pub interfaces: Vec<Port>,
    /// Parent region, if this region is nested.
    pub parent: Option<RegionId>,
}

impl Region {
    /// Create a new region with a random UUID.
    pub fn new(kind: RegionKind, children: Vec<NodeId>) -> Self {
        Self {
            id: Uuid::new_v4(),
            kind,
            children,
            constraints: Vec::new(),
            interfaces: Vec::new(),
            parent: None,
        }
    }

    /// Create a region with a specific ID.
    pub fn with_id(id: RegionId, kind: RegionKind, children: Vec<NodeId>) -> Self {
        Self {
            id,
            kind,
            children,
            constraints: Vec::new(),
            interfaces: Vec::new(),
            parent: None,
        }
    }

    /// Set the execution constraints on this region.
    pub fn with_constraints(mut self, constraints: Vec<Constraint>) -> Self {
        self.constraints = constraints;
        self
    }

    /// Set the interface ports on this region.
    pub fn with_interfaces(mut self, interfaces: Vec<Port>) -> Self {
        self.interfaces = interfaces;
        self
    }

    /// Set the parent region.
    pub fn with_parent(mut self, parent: RegionId) -> Self {
        self.parent = Some(parent);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::port::{Port, PortDirection};
    use crate::types::Type;

    #[test]
    fn region_with_constraints_and_interfaces() {
        let parent_id = Uuid::new_v4();
        let region = Region::new(RegionKind::Parallel, vec![])
            .with_constraints(vec![
                Constraint::MaxTime(100_000),
                Constraint::MaxMemory(4096),
            ])
            .with_interfaces(vec![
                Port::input("x", 0, Type::f32()),
                Port::output("y", 0, Type::f32()),
            ])
            .with_parent(parent_id);

        assert_eq!(region.constraints.len(), 2);
        assert_eq!(region.interfaces.len(), 2);
        assert_eq!(region.parent, Some(parent_id));
        assert_eq!(region.interfaces[0].direction, PortDirection::Input);
        assert_eq!(region.interfaces[1].direction, PortDirection::Output);
    }
}
