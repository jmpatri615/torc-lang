//! Content-addressed integrity verification.
//!
//! Every artifact in the registry is content-addressed via SHA-256.
//! Publishing is append-only: a version, once published, cannot be modified.

use sha2::{Digest, Sha256};

/// A content hash (SHA-256 hex digest).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ContentHash(pub String);

impl ContentHash {
    /// Compute the SHA-256 hash of the given data.
    pub fn compute(data: &[u8]) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(data);
        let result = hasher.finalize();
        ContentHash(hex_encode(&result))
    }

    /// Get the hex string representation.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Verify that the given data matches this hash.
    pub fn verify(&self, data: &[u8]) -> bool {
        ContentHash::compute(data) == *self
    }
}

impl std::fmt::Display for ContentHash {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Encode bytes as lowercase hex string.
fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// An integrity record for a published artifact.
#[derive(Debug, Clone)]
pub struct IntegrityRecord {
    /// Module name.
    pub name: String,
    /// Module version.
    pub version: String,
    /// SHA-256 of the .trc binary.
    pub trc_hash: ContentHash,
    /// SHA-256 of the manifest TOML.
    pub manifest_hash: ContentHash,
}

impl IntegrityRecord {
    /// Create a new integrity record from raw data.
    pub fn from_data(name: &str, version: &str, trc_data: &[u8], manifest_data: &[u8]) -> Self {
        IntegrityRecord {
            name: name.to_string(),
            version: version.to_string(),
            trc_hash: ContentHash::compute(trc_data),
            manifest_hash: ContentHash::compute(manifest_data),
        }
    }

    /// Verify the TRC data matches the expected hash.
    pub fn verify_trc(&self, data: &[u8]) -> bool {
        self.trc_hash.verify(data)
    }

    /// Verify the manifest data matches the expected hash.
    pub fn verify_manifest(&self, data: &[u8]) -> bool {
        self.manifest_hash.verify(data)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_deterministic() {
        let data = b"hello world";
        let h1 = ContentHash::compute(data);
        let h2 = ContentHash::compute(data);
        assert_eq!(h1, h2);
    }

    #[test]
    fn hash_differs_for_different_data() {
        let h1 = ContentHash::compute(b"hello");
        let h2 = ContentHash::compute(b"world");
        assert_ne!(h1, h2);
    }

    #[test]
    fn hash_verify() {
        let data = b"test data";
        let hash = ContentHash::compute(data);
        assert!(hash.verify(data));
        assert!(!hash.verify(b"tampered data"));
    }

    #[test]
    fn hash_format() {
        let hash = ContentHash::compute(b"");
        // SHA-256 of empty is well-known
        assert_eq!(
            hash.as_str(),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn integrity_record_verify() {
        let trc_data = b"binary graph data";
        let manifest_data = b"[module]\nname = \"test\"\n";
        let record = IntegrityRecord::from_data("test", "1.0.0", trc_data, manifest_data);

        assert!(record.verify_trc(trc_data));
        assert!(record.verify_manifest(manifest_data));
        assert!(!record.verify_trc(b"wrong data"));
        assert!(!record.verify_manifest(b"wrong manifest"));
    }

    #[test]
    fn display_impl() {
        let hash = ContentHash::compute(b"test");
        let s = format!("{hash}");
        assert_eq!(s.len(), 64); // SHA-256 hex is 64 chars
    }
}
