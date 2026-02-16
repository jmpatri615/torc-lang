//! Port model for region interfaces.
//!
//! Ports define the typed inputs and outputs of a region, allowing
//! regions to have well-defined interfaces for data flow.

use serde::{Deserialize, Serialize};

use crate::types::Type;

/// Direction of a port on a region interface.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PortDirection {
    /// Data flows into the region.
    Input,
    /// Data flows out of the region.
    Output,
}

/// A typed port on a region interface.
///
/// Each port has a name, direction, positional index, and a type
/// describing the data that flows through it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Port {
    /// Human-readable name for this port.
    pub name: String,
    /// Whether this port is an input or output.
    pub direction: PortDirection,
    /// Positional index among ports of the same direction.
    pub index: usize,
    /// The type of data flowing through this port.
    pub port_type: Type,
}

impl Port {
    /// Create an input port.
    pub fn input(name: impl Into<String>, index: usize, port_type: Type) -> Self {
        Self {
            name: name.into(),
            direction: PortDirection::Input,
            index,
            port_type,
        }
    }

    /// Create an output port.
    pub fn output(name: impl Into<String>, index: usize, port_type: Type) -> Self {
        Self {
            name: name.into(),
            direction: PortDirection::Output,
            index,
            port_type,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn input_port_creation() {
        let port = Port::input("x", 0, Type::f32());
        assert_eq!(port.name, "x");
        assert_eq!(port.direction, PortDirection::Input);
        assert_eq!(port.index, 0);
        assert_eq!(port.port_type, Type::f32());
    }

    #[test]
    fn output_port_creation() {
        let port = Port::output("result", 0, Type::i32());
        assert_eq!(port.name, "result");
        assert_eq!(port.direction, PortDirection::Output);
        assert_eq!(port.index, 0);
        assert_eq!(port.port_type, Type::i32());
    }

    #[test]
    fn direction_variants() {
        assert_ne!(PortDirection::Input, PortDirection::Output);
    }
}
