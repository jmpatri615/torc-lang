//! Content-addressed in-memory proof cache.

use std::collections::HashMap;

use sha2::{Digest, Sha256};
use torc_core::contract::{ProofObligation, ProofWitness};

/// Statistics about cache usage.
#[derive(Debug, Clone, Default)]
pub struct CacheStats {
    pub hits: usize,
    pub misses: usize,
    pub entries: usize,
}

/// A cached proof entry.
#[derive(Debug, Clone)]
pub struct CacheEntry {
    pub witness: ProofWitness,
    pub obligation_hash: String,
    pub timestamp: u64,
}

/// Content-addressed proof cache keyed by obligation hash.
#[derive(Debug, Clone)]
pub struct ProofCache {
    entries: HashMap<String, CacheEntry>,
    hits: usize,
    misses: usize,
}

impl ProofCache {
    /// Create a new empty cache.
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
            hits: 0,
            misses: 0,
        }
    }

    /// Look up a cached witness for the given obligation.
    pub fn lookup(&mut self, obligation: &ProofObligation) -> Option<&ProofWitness> {
        let hash = obligation_hash(obligation);
        if self.entries.contains_key(&hash) {
            self.hits += 1;
            Some(&self.entries[&hash].witness)
        } else {
            self.misses += 1;
            None
        }
    }

    /// Store a proof witness for the given obligation.
    pub fn store(&mut self, obligation: &ProofObligation, witness: ProofWitness) {
        let hash = obligation_hash(obligation);
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        self.entries.insert(
            hash.clone(),
            CacheEntry {
                witness,
                obligation_hash: hash,
                timestamp,
            },
        );
    }

    /// Invalidate (remove) a cached entry by obligation hash.
    pub fn invalidate(&mut self, obligation_hash: &str) {
        self.entries.remove(obligation_hash);
    }

    /// Return cache usage statistics.
    pub fn statistics(&self) -> CacheStats {
        CacheStats {
            hits: self.hits,
            misses: self.misses,
            entries: self.entries.len(),
        }
    }
}

impl Default for ProofCache {
    fn default() -> Self {
        Self::new()
    }
}

/// Compute a content hash for an obligation: SHA-256 of (kind, predicate, description).
pub fn obligation_hash(obligation: &ProofObligation) -> String {
    let mut hasher = Sha256::new();
    hasher.update(format!("{:?}", obligation.kind).as_bytes());
    hasher.update(format!("{:?}", obligation.predicate).as_bytes());
    hasher.update(obligation.description.as_bytes());
    format!("{:x}", hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;
    use torc_core::contract::{ObligationKind, ProofStatus, ProofWitness};
    use torc_core::types::Predicate;

    fn sample_obligation() -> ProofObligation {
        ProofObligation {
            kind: ObligationKind::Postcondition,
            predicate: Predicate::in_range("output", 0, 4095),
            description: "output in [0, 4095]".into(),
            status: ProofStatus::Pending,
            witness: None,
            waiver: None,
        }
    }

    fn sample_witness() -> ProofWitness {
        ProofWitness {
            hash: "abc123".into(),
            solver: "interval_domain".into(),
            data: vec![],
        }
    }

    #[test]
    fn store_and_retrieve() {
        let mut cache = ProofCache::new();
        let ob = sample_obligation();
        let witness = sample_witness();

        assert!(cache.lookup(&ob).is_none());
        cache.store(&ob, witness.clone());
        assert_eq!(cache.lookup(&ob).unwrap().solver, "interval_domain");
    }

    #[test]
    fn cache_hit_on_unchanged() {
        let mut cache = ProofCache::new();
        let ob = sample_obligation();
        cache.store(&ob, sample_witness());

        // Same obligation should hit
        let _ = cache.lookup(&ob);
        let _ = cache.lookup(&ob);
        let stats = cache.statistics();
        assert!(stats.hits >= 2);
        assert_eq!(stats.entries, 1);
    }

    #[test]
    fn invalidation() {
        let mut cache = ProofCache::new();
        let ob = sample_obligation();
        cache.store(&ob, sample_witness());

        let hash = obligation_hash(&ob);
        cache.invalidate(&hash);

        assert!(cache.lookup(&ob).is_none());
        assert_eq!(cache.statistics().entries, 0);
    }
}
