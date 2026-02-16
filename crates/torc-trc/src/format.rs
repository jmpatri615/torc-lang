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
        let _node_count = u64::from_le_bytes(data[8..16].try_into().unwrap());
        let _edge_count = u64::from_le_bytes(data[16..24].try_into().unwrap());
        let _region_count = u64::from_le_bytes(data[24..32].try_into().unwrap());
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
    use torc_core::graph::edge::Edge;
    use torc_core::graph::node::{ArithmeticOp, Node, NodeKind};
    use torc_core::graph::region::{Region, RegionKind};

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
}
