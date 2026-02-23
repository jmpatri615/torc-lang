//! TRC binary format implementation.
//!
//! The format uses a fixed-size header with magic bytes, version, and flags,
//! followed by a JSON-serialized graph payload, terminated by a SHA-256
//! content hash for integrity verification.

use std::io::{self, Read, Write};

use sha2::{Digest, Sha256};
use thiserror::Error;

use torc_core::graph::Graph;

/// Magic bytes identifying a TRC file: "TRC\0"
pub const MAGIC: [u8; 4] = [0x54, 0x52, 0x43, 0x00];

/// Size of the fixed header (magic + version + flags + counts + payload length).
/// 4 (magic) + 3 (version) + 1 (flags) + 8*4 (counts + payload_len) = 40 bytes
const HEADER_SIZE: usize = 40;

/// Size of the trailing content hash.
const HASH_SIZE: usize = 32;

/// Errors that can occur during TRC file operations.
#[derive(Debug, Error)]
pub enum TrcError {
    #[error("invalid magic bytes: expected TRC\\0")]
    InvalidMagic,

    #[error("unsupported format version {major}.{minor}.{patch}")]
    UnsupportedVersion { major: u8, minor: u8, patch: u8 },

    #[error("content hash mismatch: file is corrupted")]
    HashMismatch,

    #[error("I/O error: {0}")]
    Io(#[from] io::Error),

    #[error("serialization error: {0}")]
    Serialization(String),

    #[error("file too small to be a valid TRC file")]
    FileTooSmall,

    #[error(
        "header count mismatch: expected {expected_nodes}n/{expected_edges}e/{expected_regions}r, \
             got {actual_nodes}n/{actual_edges}e/{actual_regions}r"
    )]
    CountMismatch {
        expected_nodes: usize,
        expected_edges: usize,
        expected_regions: usize,
        actual_nodes: usize,
        actual_edges: usize,
        actual_regions: usize,
    },
}

/// TRC format version.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TrcVersion {
    pub major: u8,
    pub minor: u8,
    pub patch: u8,
}

impl TrcVersion {
    /// The current format version.
    pub const CURRENT: TrcVersion = TrcVersion {
        major: 0,
        minor: 1,
        patch: 0,
    };

    /// Check if this version is compatible with the current implementation.
    pub fn is_compatible(&self) -> bool {
        // For now, only exact major version match.
        // Major 0 means pre-stable, so we check exact match.
        self.major == Self::CURRENT.major && self.minor <= Self::CURRENT.minor
    }
}

impl std::fmt::Display for TrcVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

/// TRC format flags.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TrcFlags {
    bits: u8,
}

impl TrcFlags {
    /// No flags set.
    pub const NONE: TrcFlags = TrcFlags { bits: 0 };

    /// Payload is compressed (reserved for future use).
    pub const COMPRESSED: u8 = 0x01;

    /// File contains proof witnesses.
    pub const HAS_PROOFS: u8 = 0x02;

    /// File contains provenance data.
    pub const HAS_PROVENANCE: u8 = 0x04;

    pub fn new(bits: u8) -> Self {
        Self { bits }
    }

    pub fn has(&self, flag: u8) -> bool {
        self.bits & flag != 0
    }

    pub fn set(&mut self, flag: u8) {
        self.bits |= flag;
    }

    pub fn bits(&self) -> u8 {
        self.bits
    }
}

/// A TRC file: header metadata + graph.
#[derive(Debug)]
pub struct TrcFile {
    /// Format version.
    pub version: TrcVersion,
    /// Format flags.
    pub flags: TrcFlags,
    /// The graph data.
    pub graph: Graph,
}

impl TrcFile {
    /// Create a new TRC file wrapping the given graph.
    pub fn new(graph: Graph) -> Self {
        let mut flags = TrcFlags::NONE;
        // Check if any nodes have provenance
        if graph.nodes().any(|n| n.provenance.is_some()) {
            flags.set(TrcFlags::HAS_PROVENANCE);
        }
        // Check if any nodes have proof witnesses
        if graph.nodes().any(|n| {
            n.contract
                .as_ref()
                .is_some_and(|c| c.proof_witness.is_some())
        }) {
            flags.set(TrcFlags::HAS_PROOFS);
        }
        Self {
            version: TrcVersion::CURRENT,
            flags,
            graph,
        }
    }

    /// Serialize to a writer in TRC binary format.
    pub fn write_to<W: Write>(&self, writer: &mut W) -> Result<(), TrcError> {
        // Serialize graph to JSON
        let payload =
            serde_json::to_vec(&self.graph).map_err(|e| TrcError::Serialization(e.to_string()))?;

        let node_count = self.graph.node_count() as u64;
        let edge_count = self.graph.edge_count() as u64;
        let region_count = self.graph.region_count() as u64;
        let payload_len = payload.len() as u64;

        // Compute content hash over header + payload
        let mut hasher = Sha256::new();

        // Write magic
        writer.write_all(&MAGIC)?;
        hasher.update(MAGIC);

        // Write version
        let version_bytes = [self.version.major, self.version.minor, self.version.patch];
        writer.write_all(&version_bytes)?;
        hasher.update(version_bytes);

        // Write flags
        writer.write_all(&[self.flags.bits()])?;
        hasher.update([self.flags.bits()]);

        // Write counts and payload length (little-endian u64)
        for val in [node_count, edge_count, region_count, payload_len] {
            let bytes = val.to_le_bytes();
            writer.write_all(&bytes)?;
            hasher.update(bytes);
        }

        // Write payload
        writer.write_all(&payload)?;
        hasher.update(&payload);

        // Write content hash
        let hash: [u8; 32] = hasher.finalize().into();
        writer.write_all(&hash)?;

        Ok(())
    }

    /// Serialize to a byte vector.
    pub fn to_bytes(&self) -> Result<Vec<u8>, TrcError> {
        let mut buf = Vec::new();
        self.write_to(&mut buf)?;
        Ok(buf)
    }

    /// Deserialize from a reader.
    pub fn read_from<R: Read>(reader: &mut R) -> Result<Self, TrcError> {
        let mut data = Vec::new();
        reader.read_to_end(&mut data)?;
        Self::from_bytes(&data)
    }

    /// Deserialize from a byte slice.
    pub fn from_bytes(data: &[u8]) -> Result<Self, TrcError> {
        if data.len() < HEADER_SIZE + HASH_SIZE {
            return Err(TrcError::FileTooSmall);
        }

        // Verify magic
        if data[0..4] != MAGIC {
            return Err(TrcError::InvalidMagic);
        }

        // Read version
        let version = TrcVersion {
            major: data[4],
            minor: data[5],
            patch: data[6],
        };
        if !version.is_compatible() {
            return Err(TrcError::UnsupportedVersion {
                major: version.major,
                minor: version.minor,
                patch: version.patch,
            });
        }

        // Read flags
        let flags = TrcFlags::new(data[7]);

        // Read counts
        let node_count = u64::from_le_bytes(data[8..16].try_into().unwrap()) as usize;
        let edge_count = u64::from_le_bytes(data[16..24].try_into().unwrap()) as usize;
        let region_count = u64::from_le_bytes(data[24..32].try_into().unwrap()) as usize;
        let payload_len = u64::from_le_bytes(data[32..40].try_into().unwrap()) as usize;

        // Verify we have enough data
        let expected_size = HEADER_SIZE + payload_len + HASH_SIZE;
        if data.len() < expected_size {
            return Err(TrcError::FileTooSmall);
        }

        // Verify content hash
        let payload_end = HEADER_SIZE + payload_len;
        let stored_hash = &data[payload_end..payload_end + HASH_SIZE];

        let mut hasher = Sha256::new();
        hasher.update(&data[..payload_end]);
        let computed_hash: [u8; 32] = hasher.finalize().into();

        if computed_hash != stored_hash {
            return Err(TrcError::HashMismatch);
        }

        // Deserialize graph from JSON payload
        let payload = &data[HEADER_SIZE..payload_end];
        let graph: Graph =
            serde_json::from_slice(payload).map_err(|e| TrcError::Serialization(e.to_string()))?;

        // Validate header counts match deserialized graph
        if graph.node_count() != node_count
            || graph.edge_count() != edge_count
            || graph.region_count() != region_count
        {
            return Err(TrcError::CountMismatch {
                expected_nodes: node_count,
                expected_edges: edge_count,
                expected_regions: region_count,
                actual_nodes: graph.node_count(),
                actual_edges: graph.edge_count(),
                actual_regions: graph.region_count(),
            });
        }

        Ok(Self {
            version,
            flags,
            graph,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use torc_core::contract::{Contract, EnergyBound, ProofWitness};
    use torc_core::graph::constraints::{BandwidthConstraint, Constraint, Lifetime};
    use torc_core::graph::edge::Edge;
    use torc_core::graph::node::{ArithmeticOp, Node, NodeKind};
    use torc_core::graph::port::Port;
    use torc_core::graph::region::{Region, RegionKind};
    use torc_core::provenance::Provenance;
    use torc_core::types::{Predicate, Type, TypeSignature};

    fn sample_graph() -> Graph {
        let mut g = Graph::new();
        let n1 = Node::new(NodeKind::Literal);
        let n2 = Node::new(NodeKind::Literal);
        let n3 = Node::new(NodeKind::Arithmetic(ArithmeticOp::Add));
        let id1 = n1.id;
        let id2 = n2.id;
        let id3 = n3.id;
        g.add_node(n1).unwrap();
        g.add_node(n2).unwrap();
        g.add_node(n3).unwrap();
        g.add_edge(Edge::new((id1, 0), (id3, 0))).unwrap();
        g.add_edge(Edge::new((id2, 0), (id3, 1))).unwrap();
        g.add_region(Region::new(RegionKind::Parallel, vec![id1, id2]))
            .unwrap();
        g
    }

    #[test]
    fn round_trip() {
        let graph = sample_graph();
        let trc = TrcFile::new(graph);

        let bytes = trc.to_bytes().unwrap();
        let loaded = TrcFile::from_bytes(&bytes).unwrap();

        assert_eq!(loaded.version, TrcVersion::CURRENT);
        assert_eq!(loaded.graph.node_count(), 3);
        assert_eq!(loaded.graph.edge_count(), 2);
        assert_eq!(loaded.graph.region_count(), 1);
    }

    #[test]
    fn empty_graph_round_trip() {
        let graph = Graph::new();
        let trc = TrcFile::new(graph);

        let bytes = trc.to_bytes().unwrap();
        let loaded = TrcFile::from_bytes(&bytes).unwrap();

        assert_eq!(loaded.graph.node_count(), 0);
        assert_eq!(loaded.graph.edge_count(), 0);
        assert_eq!(loaded.graph.region_count(), 0);
    }

    #[test]
    fn invalid_magic_rejected() {
        let graph = Graph::new();
        let trc = TrcFile::new(graph);
        let mut bytes = trc.to_bytes().unwrap();

        // Corrupt magic bytes
        bytes[0] = 0xFF;

        let result = TrcFile::from_bytes(&bytes);
        assert!(matches!(result, Err(TrcError::InvalidMagic)));
    }

    #[test]
    fn corrupted_data_rejected() {
        let graph = sample_graph();
        let trc = TrcFile::new(graph);
        let mut bytes = trc.to_bytes().unwrap();

        // Corrupt a byte in the payload area
        let mid = HEADER_SIZE + 10;
        if mid < bytes.len() - HASH_SIZE {
            bytes[mid] ^= 0xFF;
        }

        let result = TrcFile::from_bytes(&bytes);
        assert!(matches!(
            result,
            Err(TrcError::HashMismatch) | Err(TrcError::Serialization(_))
        ));
    }

    #[test]
    fn truncated_file_rejected() {
        let result = TrcFile::from_bytes(&[0x54, 0x52, 0x43, 0x00]);
        assert!(matches!(result, Err(TrcError::FileTooSmall)));
    }

    #[test]
    fn version_display() {
        assert_eq!(TrcVersion::CURRENT.to_string(), "0.1.0");
    }

    #[test]
    fn flags_operations() {
        let mut flags = TrcFlags::NONE;
        assert!(!flags.has(TrcFlags::COMPRESSED));
        assert!(!flags.has(TrcFlags::HAS_PROOFS));

        flags.set(TrcFlags::HAS_PROOFS);
        assert!(flags.has(TrcFlags::HAS_PROOFS));
        assert!(!flags.has(TrcFlags::COMPRESSED));

        flags.set(TrcFlags::COMPRESSED);
        assert!(flags.has(TrcFlags::COMPRESSED));
        assert!(flags.has(TrcFlags::HAS_PROOFS));
    }

    #[test]
    fn write_and_read_via_io() {
        let graph = sample_graph();
        let trc = TrcFile::new(graph);

        let mut buf = Vec::new();
        trc.write_to(&mut buf).unwrap();

        let loaded = TrcFile::read_from(&mut buf.as_slice()).unwrap();
        assert_eq!(loaded.graph.node_count(), 3);
    }

    /// Build a rich graph with all metadata variants for comprehensive round-trip testing.
    fn rich_sample_graph() -> Graph {
        let mut g = Graph::new();

        // n1: Literal with type signature, contract (pre/post, bounds, proof witness),
        // provenance, and annotations
        let mut contract = Contract::with_conditions(
            vec![Predicate::positive("input")],
            vec![Predicate::in_range("output", 0, 4095)],
        )
        .with_wcet(50_000, "arm-cortex-m4f")
        .with_stack(64)
        .with_no_heap();
        contract.energy_bound = Some(EnergyBound { max_uj: 100 });
        contract.proof_witness = Some(ProofWitness {
            hash: "sha256:deadbeef".to_string(),
            solver: "z3-4.12".to_string(),
            data: vec![0xCA, 0xFE],
        });

        let mut n1 = Node::new(NodeKind::Literal)
            .with_type_signature(TypeSignature::source(Type::i32()))
            .with_contract(contract)
            .with_provenance(Provenance::ai_authored(
                "claude-4.5-opus",
                "anthropic",
                "20260215",
                "ADC read node",
            ));
        n1.annotations
            .insert("safety_class".to_string(), "ASIL-D".to_string());
        n1.annotations
            .insert("opt_hint".to_string(), "vectorize".to_string());

        // n2: Arithmetic node with simple type signature
        let n2 = Node::new(NodeKind::Arithmetic(ArithmeticOp::Add)).with_type_signature(
            TypeSignature::pure_fn(vec![Type::i32(), Type::i32()], Type::i32()),
        );

        // n3: Another literal
        let n3 =
            Node::new(NodeKind::Literal).with_type_signature(TypeSignature::source(Type::f32()));

        let id1 = n1.id;
        let id2 = n2.id;
        let id3 = n3.id;
        g.add_node(n1).unwrap();
        g.add_node(n2).unwrap();
        g.add_node(n3).unwrap();

        // Typed edge with custom lifetime and bandwidth
        let e1 = Edge::typed((id1, 0), (id2, 0), Type::i32())
            .with_lifetime(Lifetime::Manual)
            .with_bandwidth(BandwidthConstraint::bounded(1_000, 1_000_000));
        g.add_edge(e1).unwrap();

        // Edge with Bounded lifetime
        let e2 = Edge::new((id3, 0), (id2, 1)).with_lifetime(Lifetime::Bounded(5_000_000));
        g.add_edge(e2).unwrap();

        // Inner region with constraints and ports
        let inner = Region::new(RegionKind::Sequential, vec![id1])
            .with_constraints(vec![
                Constraint::MaxTime(100_000),
                Constraint::MaxMemory(4096),
            ])
            .with_interfaces(vec![
                Port::input("x", 0, Type::i32()),
                Port::output("y", 0, Type::i32()),
            ]);
        let inner_id = inner.id;
        g.add_region(inner).unwrap();

        // Outer region
        let outer = Region::new(RegionKind::Parallel, vec![id2, id3]);
        let outer_id = outer.id;
        g.add_region(outer).unwrap();

        g.set_region_parent(inner_id, outer_id).unwrap();

        g
    }

    #[test]
    fn rich_graph_round_trip() {
        let graph = rich_sample_graph();
        let trc = TrcFile::new(graph);

        // Verify flags are set before serialization
        assert!(trc.flags.has(TrcFlags::HAS_PROVENANCE));
        assert!(trc.flags.has(TrcFlags::HAS_PROOFS));

        let bytes = trc.to_bytes().unwrap();
        let loaded = TrcFile::from_bytes(&bytes).unwrap();

        assert_eq!(loaded.graph.node_count(), 3);
        assert_eq!(loaded.graph.edge_count(), 2);
        assert_eq!(loaded.graph.region_count(), 2);

        // Verify flags survive round-trip
        assert!(loaded.flags.has(TrcFlags::HAS_PROVENANCE));
        assert!(loaded.flags.has(TrcFlags::HAS_PROOFS));
        assert!(!loaded.flags.has(TrcFlags::COMPRESSED));
    }

    #[test]
    fn round_trip_preserves_contracts() {
        let graph = rich_sample_graph();
        let trc = TrcFile::new(graph);
        let bytes = trc.to_bytes().unwrap();
        let loaded = TrcFile::from_bytes(&bytes).unwrap();

        // Find the node with a contract (the one with preconditions)
        let contracted_node = loaded
            .graph
            .nodes()
            .find(|n| {
                n.contract.is_some() && !n.contract.as_ref().unwrap().preconditions.is_empty()
            })
            .expect("should find contracted node");

        let c = contracted_node.contract.as_ref().unwrap();
        assert_eq!(c.preconditions.len(), 1);
        assert_eq!(c.postconditions.len(), 1);
        assert!(c.time_bound.is_some());
        assert_eq!(c.time_bound.as_ref().unwrap().wcet_ns, Some(50_000));
        assert!(c.stack_bound.is_some());
        assert_eq!(c.stack_bound.as_ref().unwrap().max_bytes, 64);
        assert!(c.memory_bound.is_some());
        assert!(c.energy_bound.is_some());
        assert_eq!(c.energy_bound.as_ref().unwrap().max_uj, 100);

        let pw = c.proof_witness.as_ref().unwrap();
        assert_eq!(pw.hash, "sha256:deadbeef");
        assert_eq!(pw.solver, "z3-4.12");
        assert_eq!(pw.data, vec![0xCA, 0xFE]);
    }

    #[test]
    fn round_trip_preserves_provenance() {
        let graph = rich_sample_graph();
        let trc = TrcFile::new(graph);
        let bytes = trc.to_bytes().unwrap();
        let loaded = TrcFile::from_bytes(&bytes).unwrap();

        let prov_node = loaded
            .graph
            .nodes()
            .find(|n| n.provenance.is_some())
            .expect("should find node with provenance");

        let p = prov_node.provenance.as_ref().unwrap();
        assert_eq!(p.creation_reason, "ADC read node");
        assert!(format!("{}", p.created_by).contains("claude-4.5-opus"));
    }

    #[test]
    fn round_trip_preserves_annotations() {
        let graph = rich_sample_graph();
        let trc = TrcFile::new(graph);
        let bytes = trc.to_bytes().unwrap();
        let loaded = TrcFile::from_bytes(&bytes).unwrap();

        let annotated = loaded
            .graph
            .nodes()
            .find(|n| !n.annotations.is_empty())
            .expect("should find annotated node");

        assert_eq!(annotated.annotations.get("safety_class").unwrap(), "ASIL-D");
        assert_eq!(annotated.annotations.get("opt_hint").unwrap(), "vectorize");
    }

    #[test]
    fn round_trip_preserves_edge_metadata() {
        let graph = rich_sample_graph();
        let trc = TrcFile::new(graph);
        let bytes = trc.to_bytes().unwrap();
        let loaded = TrcFile::from_bytes(&bytes).unwrap();

        // Find edge with bandwidth
        let bw_edge = loaded
            .graph
            .edges()
            .find(|e| e.bandwidth.is_some())
            .expect("should find edge with bandwidth");

        assert_eq!(bw_edge.data_type, Some(Type::i32()));
        assert_eq!(bw_edge.lifetime, Lifetime::Manual);
        let bw = bw_edge.bandwidth.as_ref().unwrap();
        assert_eq!(bw.min_bytes_per_sec, 1_000);
        assert_eq!(bw.max_bytes_per_sec, Some(1_000_000));

        // Find edge with Bounded lifetime
        let bounded_edge = loaded
            .graph
            .edges()
            .find(|e| matches!(e.lifetime, Lifetime::Bounded(_)))
            .expect("should find edge with bounded lifetime");

        assert_eq!(bounded_edge.lifetime, Lifetime::Bounded(5_000_000));
    }

    #[test]
    fn round_trip_preserves_nested_regions() {
        let graph = rich_sample_graph();
        let trc = TrcFile::new(graph);
        let bytes = trc.to_bytes().unwrap();
        let loaded = TrcFile::from_bytes(&bytes).unwrap();

        // Find inner region (Sequential with constraints)
        let inner = loaded
            .graph
            .regions()
            .find(|r| r.kind == RegionKind::Sequential)
            .expect("should find inner region");

        assert_eq!(inner.constraints.len(), 2);
        assert!(inner
            .constraints
            .iter()
            .any(|c| matches!(c, Constraint::MaxTime(100_000))));
        assert!(inner
            .constraints
            .iter()
            .any(|c| matches!(c, Constraint::MaxMemory(4096))));
        assert_eq!(inner.interfaces.len(), 2);
        assert!(inner.parent.is_some());

        // Verify parent link survives
        let parent_id = inner.parent.unwrap();
        let outer = loaded
            .graph
            .get_region(&parent_id)
            .expect("parent region should exist");
        assert_eq!(outer.kind, RegionKind::Parallel);
    }

    #[test]
    fn has_proofs_flag_set() {
        let mut g = Graph::new();
        let mut contract = Contract::pure_default();
        contract.proof_witness = Some(ProofWitness {
            hash: "sha256:abc".to_string(),
            solver: "z3".to_string(),
            data: vec![1, 2, 3],
        });
        let n = Node::new(NodeKind::Literal).with_contract(contract);
        g.add_node(n).unwrap();

        let trc = TrcFile::new(g);
        assert!(trc.flags.has(TrcFlags::HAS_PROOFS));
    }

    #[test]
    fn has_proofs_flag_not_set() {
        let mut g = Graph::new();
        let n = Node::new(NodeKind::Literal).with_contract(Contract::pure_default());
        g.add_node(n).unwrap();

        let trc = TrcFile::new(g);
        assert!(!trc.flags.has(TrcFlags::HAS_PROOFS));
    }

    #[test]
    fn no_flags_on_bare_graph() {
        let mut g = Graph::new();
        let n = Node::new(NodeKind::Literal);
        g.add_node(n).unwrap();

        let trc = TrcFile::new(g);
        assert_eq!(trc.flags, TrcFlags::NONE);
    }

    #[test]
    fn count_mismatch_detected() {
        let graph = sample_graph();
        let trc = TrcFile::new(graph);
        let mut bytes = trc.to_bytes().unwrap();

        // Corrupt the node_count in the header (bytes 8..16) to claim 99 nodes
        let bad_count: u64 = 99;
        bytes[8..16].copy_from_slice(&bad_count.to_le_bytes());

        // Recompute the hash so it doesn't fail on HashMismatch first
        let payload_len = u64::from_le_bytes(bytes[32..40].try_into().unwrap()) as usize;
        let payload_end = HEADER_SIZE + payload_len;

        let mut hasher = sha2::Sha256::new();
        hasher.update(&bytes[..payload_end]);
        let new_hash: [u8; 32] = hasher.finalize().into();
        bytes[payload_end..payload_end + 32].copy_from_slice(&new_hash);

        let result = TrcFile::from_bytes(&bytes);
        assert!(matches!(result, Err(TrcError::CountMismatch { .. })));
    }
}
