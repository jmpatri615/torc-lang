//! Node types and the Node struct.
//!
//! A node represents a unit of computation in a Torc graph.

use std::collections::HashMap;
use std::fmt;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::contract::Contract;
use crate::provenance::Provenance;
use crate::types::TypeSignature;

/// Globally unique, content-addressed node identifier.
pub type NodeId = Uuid;

/// Arithmetic operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ArithmeticOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Pow,
}

/// Bitwise operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BitwiseOp {
    And,
    Or,
    Xor,
    Not,
    ShiftLeft,
    ShiftRight,
    Rotate,
}

/// Comparison operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ComparisonOp {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

/// Memory ordering for atomic operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MemoryOrdering {
    Relaxed,
    Acquire,
    Release,
    AcqRel,
    SeqCst,
}

/// The kind of computation a node represents.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum NodeKind {
    // Primitive computation
    Literal,
    Arithmetic(ArithmeticOp),
    Bitwise(BitwiseOp),
    Comparison(ComparisonOp),
    Conversion,

    // Data structure
    Construct,
    Destructure,
    Index,
    Slice,

    // Control flow
    Select,
    Switch,
    Iterate,
    Recurse,
    Fixpoint,

    // Effects
    Allocate,
    Deallocate,
    Read,
    Write,
    Atomic(MemoryOrdering),
    Fence(MemoryOrdering),
    Syscall,
    FFICall,

    // Meta
    Verify,
    Assume,
    Measure,
    Checkpoint,
    Annotate,

    // Probabilistic
    Sample,
    Condition,
    Expectation,
    Entropy,
    Approximate,
}

impl fmt::Display for NodeKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NodeKind::Literal => write!(f, "Literal"),
            NodeKind::Arithmetic(op) => write!(f, "Arithmetic({op:?})"),
            NodeKind::Bitwise(op) => write!(f, "Bitwise({op:?})"),
            NodeKind::Comparison(op) => write!(f, "Comparison({op:?})"),
            NodeKind::Conversion => write!(f, "Conversion"),
            NodeKind::Construct => write!(f, "Construct"),
            NodeKind::Destructure => write!(f, "Destructure"),
            NodeKind::Index => write!(f, "Index"),
            NodeKind::Slice => write!(f, "Slice"),
            NodeKind::Select => write!(f, "Select"),
            NodeKind::Switch => write!(f, "Switch"),
            NodeKind::Iterate => write!(f, "Iterate"),
            NodeKind::Recurse => write!(f, "Recurse"),
            NodeKind::Fixpoint => write!(f, "Fixpoint"),
            NodeKind::Allocate => write!(f, "Allocate"),
            NodeKind::Deallocate => write!(f, "Deallocate"),
            NodeKind::Read => write!(f, "Read"),
            NodeKind::Write => write!(f, "Write"),
            NodeKind::Atomic(ord) => write!(f, "Atomic({ord:?})"),
            NodeKind::Fence(ord) => write!(f, "Fence({ord:?})"),
            NodeKind::Syscall => write!(f, "Syscall"),
            NodeKind::FFICall => write!(f, "FFICall"),
            NodeKind::Verify => write!(f, "Verify"),
            NodeKind::Assume => write!(f, "Assume"),
            NodeKind::Measure => write!(f, "Measure"),
            NodeKind::Checkpoint => write!(f, "Checkpoint"),
            NodeKind::Annotate => write!(f, "Annotate"),
            NodeKind::Sample => write!(f, "Sample"),
            NodeKind::Condition => write!(f, "Condition"),
            NodeKind::Expectation => write!(f, "Expectation"),
            NodeKind::Entropy => write!(f, "Entropy"),
            NodeKind::Approximate => write!(f, "Approximate"),
        }
    }
}

/// A node in the Torc computation graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Node {
    /// Globally unique identifier.
    pub id: NodeId,
    /// The category of computation.
    pub kind: NodeKind,
    /// Type signature: input types and output types for this node.
    pub type_signature: Option<TypeSignature>,
    /// Behavioral contract: pre/postconditions, resource bounds, effects.
    pub contract: Option<Contract>,
    /// Provenance: who created this node, when, and why.
    pub provenance: Option<Provenance>,
    /// Extensible metadata (optimization hints, safety class, etc.).
    pub annotations: HashMap<String, String>,
}

impl Node {
    /// Create a new node with a random UUID.
    pub fn new(kind: NodeKind) -> Self {
        Self {
            id: Uuid::new_v4(),
            kind,
            type_signature: None,
            contract: None,
            provenance: None,
            annotations: HashMap::new(),
        }
    }

    /// Create a node with a specific ID (for deserialization or testing).
    pub fn with_id(id: NodeId, kind: NodeKind) -> Self {
        Self {
            id,
            kind,
            type_signature: None,
            contract: None,
            provenance: None,
            annotations: HashMap::new(),
        }
    }

    /// Attach a type signature to this node.
    pub fn with_type_signature(mut self, sig: TypeSignature) -> Self {
        self.type_signature = Some(sig);
        self
    }

    /// Attach a contract to this node.
    pub fn with_contract(mut self, contract: Contract) -> Self {
        self.contract = Some(contract);
        self
    }

    /// Attach provenance information to this node.
    pub fn with_provenance(mut self, provenance: Provenance) -> Self {
        self.provenance = Some(provenance);
        self
    }
}
