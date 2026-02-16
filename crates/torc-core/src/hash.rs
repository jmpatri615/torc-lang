//! Content-addressed hashing for Torc graph elements.
//!
//! Every node, edge, and graph can be content-addressed via SHA-256.
//! The hash covers the semantic content (kind, type, contract) but not
//! the randomly-assigned UUID, allowing identical computations to be
//! deduplicated.

use serde::Serialize;
use sha2::{Digest, Sha256};

/// A 32-byte SHA-256 content hash.
pub type ContentHash = [u8; 32];

/// Compute the SHA-256 content hash of any serializable value.
pub fn content_hash<T: Serialize>(value: &T) -> ContentHash {
    let json = serde_json::to_vec(value).expect("serialization should not fail");
    let mut hasher = Sha256::new();
    hasher.update(&json);
    hasher.finalize().into()
}

/// Format a content hash as a hex string.
pub fn hash_hex(hash: &ContentHash) -> String {
    hash.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic_hash() {
        let h1 = content_hash(&"hello world");
        let h2 = content_hash(&"hello world");
        assert_eq!(h1, h2);
    }

    #[test]
    fn different_inputs_different_hash() {
        let h1 = content_hash(&"hello");
        let h2 = content_hash(&"world");
        assert_ne!(h1, h2);
    }

    #[test]
    fn hash_hex_format() {
        let h = content_hash(&42u32);
        let hex = hash_hex(&h);
        assert_eq!(hex.len(), 64); // 32 bytes * 2 hex chars each
    }
}
