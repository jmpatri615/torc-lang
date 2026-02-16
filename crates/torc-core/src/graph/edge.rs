//! Edge types representing data dependencies between nodes.
//!
//! An edge connects an output port of one node to an input port of another,
//! representing a data dependency.

use std::fmt;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::constraints::{BandwidthConstraint, Lifetime};
use super::node::NodeId;
use crate::types::Type;

/// Globally unique edge identifier.
pub type EdgeId = Uuid;

/// A port reference: (node ID, port index).
pub type PortRef = (NodeId, usize);

/// An edge in the Torc computation graph.
///
/// Represents a data dependency from a source node's output port
/// to a target node's input port, carrying a typed value.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Edge {
    /// Unique edge identifier.
    pub id: EdgeId,
    /// Output port of the producing node: (NodeId, port_index).
    pub source: PortRef,
    /// Input port of the consuming node: (NodeId, port_index).
    pub target: PortRef,
    /// The type of data flowing along this edge.
    pub data_type: Option<Type>,
    /// Lifetime annotation for the data on this edge.
    pub lifetime: Lifetime,
    /// Optional bandwidth constraint for this edge.
    pub bandwidth: Option<BandwidthConstraint>,
}

impl Edge {
    /// Create a new edge with a random UUID.
    pub fn new(source: PortRef, target: PortRef) -> Self {
        Self {
            id: Uuid::new_v4(),
            source,
            target,
            data_type: None,
            lifetime: Lifetime::Static,
            bandwidth: None,
        }
    }

    /// Create an edge with a specific ID.
    pub fn with_id(id: EdgeId, source: PortRef, target: PortRef) -> Self {
        Self {
            id,
            source,
            target,
            data_type: None,
            lifetime: Lifetime::Static,
            bandwidth: None,
        }
    }

    /// Create a typed edge.
    pub fn typed(source: PortRef, target: PortRef, data_type: Type) -> Self {
        Self {
            id: Uuid::new_v4(),
            source,
            target,
            data_type: Some(data_type),
            lifetime: Lifetime::Static,
            bandwidth: None,
        }
    }

    /// Set the lifetime annotation on this edge.
    pub fn with_lifetime(mut self, lifetime: Lifetime) -> Self {
        self.lifetime = lifetime;
        self
    }

    /// Set the bandwidth constraint on this edge.
    pub fn with_bandwidth(mut self, bandwidth: BandwidthConstraint) -> Self {
        self.bandwidth = Some(bandwidth);
        self
    }
}

impl fmt::Display for Edge {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Edge({} port {} -> {} port {})",
            self.source.0, self.source.1, self.target.0, self.target.1,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edge_with_lifetime() {
        let src = (Uuid::new_v4(), 0);
        let dst = (Uuid::new_v4(), 0);
        let edge = Edge::new(src, dst).with_lifetime(Lifetime::Manual);
        assert_eq!(edge.lifetime, Lifetime::Manual);
        assert!(edge.bandwidth.is_none());
    }

    #[test]
    fn edge_with_bandwidth() {
        let src = (Uuid::new_v4(), 0);
        let dst = (Uuid::new_v4(), 0);
        let bw = BandwidthConstraint::bounded(1_000, 1_000_000);
        let edge = Edge::new(src, dst).with_bandwidth(bw);
        assert_eq!(edge.lifetime, Lifetime::Static); // default
        assert!(edge.bandwidth.is_some());
        let bw = edge.bandwidth.unwrap();
        assert_eq!(bw.min_bytes_per_sec, 1_000);
        assert_eq!(bw.max_bytes_per_sec, Some(1_000_000));
    }
}
