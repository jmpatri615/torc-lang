//! TDG file format serialization and deserialization.
//!
//! The TDG (Torc Decision Graph) format stores decision graphs as a binary
//! file with header, JSON payload, and SHA-256 integrity hash.
//!
//! Layout:
//!   [magic: 4 bytes "TDG\0"] [version_major: 1] [version_minor: 1]
//!   [flags: 1] [reserved: 1] [decision_count: u32 LE] [assumption_count: u32 LE]
//!   [payload_length: u32 LE] [json_payload: N bytes] [sha256: 32 bytes]

use sha2::{Digest, Sha256};

use crate::error::SpecError;
use crate::graph::DecisionGraph;

/// Magic bytes for TDG format: "TDG\0"
const TDG_MAGIC: [u8; 4] = [0x54, 0x44, 0x47, 0x00];

/// Current version.
const VERSION_MAJOR: u8 = 0;
const VERSION_MINOR: u8 = 1;

/// Header size (magic + version + flags + reserved + counts + payload_len).
const HEADER_SIZE: usize = 4 + 1 + 1 + 1 + 1 + 4 + 4 + 4; // 20 bytes

/// SHA-256 hash size.
const HASH_SIZE: usize = 32;

/// A TDG file: header + decision graph.
pub struct TdgFile {
    pub graph: DecisionGraph,
}

impl TdgFile {
    /// Create a new TDG file from a decision graph.
    pub fn new(graph: DecisionGraph) -> Self {
        Self { graph }
    }

    /// Serialize to bytes.
    pub fn to_bytes(&self) -> Result<Vec<u8>, SpecError> {
        let json =
            serde_json::to_vec(&self.graph).map_err(|e| SpecError::Serialization(e.to_string()))?;

        let decision_count = self.graph.decision_count() as u32;
        let assumption_count = self.graph.assumption_count() as u32;
        let payload_len = json.len() as u32;

        let mut buf = Vec::with_capacity(HEADER_SIZE + json.len() + HASH_SIZE);

        // Magic
        buf.extend_from_slice(&TDG_MAGIC);
        // Version
        buf.push(VERSION_MAJOR);
        buf.push(VERSION_MINOR);
        // Flags (reserved for future use)
        buf.push(0);
        // Reserved
        buf.push(0);
        // Decision count
        buf.extend_from_slice(&decision_count.to_le_bytes());
        // Assumption count
        buf.extend_from_slice(&assumption_count.to_le_bytes());
        // Payload length
        buf.extend_from_slice(&payload_len.to_le_bytes());
        // JSON payload
        buf.extend_from_slice(&json);

        // SHA-256 of everything so far
        let mut hasher = Sha256::new();
        hasher.update(&buf);
        let hash = hasher.finalize();
        buf.extend_from_slice(&hash);

        Ok(buf)
    }

    /// Deserialize from bytes.
    pub fn from_bytes(data: &[u8]) -> Result<Self, SpecError> {
        if data.len() < HEADER_SIZE + HASH_SIZE {
            return Err(SpecError::TooShort {
                expected: HEADER_SIZE + HASH_SIZE,
                actual: data.len(),
            });
        }

        // Check magic
        if data[0..4] != TDG_MAGIC {
            return Err(SpecError::InvalidMagic);
        }

        // Check version
        let major = data[4];
        let minor = data[5];
        if major != VERSION_MAJOR {
            return Err(SpecError::UnsupportedVersion { major, minor });
        }

        // Read counts
        let decision_count = u32::from_le_bytes([data[8], data[9], data[10], data[11]]);
        let assumption_count = u32::from_le_bytes([data[12], data[13], data[14], data[15]]);
        let payload_len = u32::from_le_bytes([data[16], data[17], data[18], data[19]]) as usize;

        let expected_total = HEADER_SIZE + payload_len + HASH_SIZE;
        if data.len() < expected_total {
            return Err(SpecError::TooShort {
                expected: expected_total,
                actual: data.len(),
            });
        }

        // Verify SHA-256
        let payload_end = HEADER_SIZE + payload_len;
        let hash_start = payload_end;
        let stored_hash = &data[hash_start..hash_start + HASH_SIZE];

        let mut hasher = Sha256::new();
        hasher.update(&data[..payload_end]);
        let computed_hash = hasher.finalize();

        if computed_hash.as_slice() != stored_hash {
            return Err(SpecError::IntegrityFailed {
                expected: hex_encode(stored_hash),
                actual: hex_encode(computed_hash.as_slice()),
            });
        }

        // Deserialize JSON payload
        let json_data = &data[HEADER_SIZE..payload_end];
        let graph: DecisionGraph = serde_json::from_slice(json_data)
            .map_err(|e| SpecError::Deserialization(e.to_string()))?;

        // Validate counts
        if graph.decision_count() != decision_count as usize {
            return Err(SpecError::Deserialization(format!(
                "decision count mismatch: header says {decision_count}, payload has {}",
                graph.decision_count()
            )));
        }
        if graph.assumption_count() != assumption_count as usize {
            return Err(SpecError::Deserialization(format!(
                "assumption count mismatch: header says {assumption_count}, payload has {}",
                graph.assumption_count()
            )));
        }

        Ok(Self { graph })
    }
}

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assumption::{Assumption, Confidence, ImpactLevel};
    use crate::decision::{Decision, DecisionState, DecisionValue};

    #[test]
    fn empty_round_trip() {
        let graph = DecisionGraph::new();
        let tdg = TdgFile::new(graph);
        let bytes = tdg.to_bytes().unwrap();
        let tdg2 = TdgFile::from_bytes(&bytes).unwrap();
        assert_eq!(tdg2.graph.decision_count(), 0);
    }

    #[test]
    fn round_trip_with_decisions() {
        let mut graph = DecisionGraph::new();
        let d = Decision::new("PWM Frequency", "performance")
            .with_description("Select the PWM switching frequency");
        graph.add_decision(d);

        let mut d2 = Decision::new("Control topology", "topology");
        d2.state = DecisionState::Committed;
        d2.value = DecisionValue::Specific("FOC".into());
        graph.add_decision(d2);

        let a = Assumption::new("Back-EMF stable", Confidence::Medium, ImpactLevel::High);
        graph.add_assumption(a);

        let tdg = TdgFile::new(graph);
        let bytes = tdg.to_bytes().unwrap();
        let tdg2 = TdgFile::from_bytes(&bytes).unwrap();

        assert_eq!(tdg2.graph.decision_count(), 2);
        assert_eq!(tdg2.graph.assumption_count(), 1);
    }

    #[test]
    fn invalid_magic() {
        let mut data = vec![0x00; 100];
        data[0..4].copy_from_slice(b"BAD\0");
        assert!(matches!(
            TdgFile::from_bytes(&data),
            Err(SpecError::InvalidMagic)
        ));
    }

    #[test]
    fn corruption_detected() {
        let graph = DecisionGraph::new();
        let tdg = TdgFile::new(graph);
        let mut bytes = tdg.to_bytes().unwrap();
        // Corrupt a byte in the payload area
        if bytes.len() > HEADER_SIZE + 2 {
            bytes[HEADER_SIZE + 1] ^= 0xFF;
        }
        assert!(TdgFile::from_bytes(&bytes).is_err());
    }

    #[test]
    fn too_short() {
        let data = vec![0x54, 0x44, 0x47, 0x00]; // just magic
        assert!(matches!(
            TdgFile::from_bytes(&data),
            Err(SpecError::TooShort { .. })
        ));
    }
}
