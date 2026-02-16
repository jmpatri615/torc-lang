//! Constraint and lifetime metadata for edges and regions.
//!
//! These types annotate edges with ownership/lifetime semantics and
//! regions with execution constraints.

use serde::{Deserialize, Serialize};

use super::region::RegionId;

/// Lifetime annotation for an edge, describing how long the data persists.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Lifetime {
    /// Data lives for the duration of the specified region.
    Region(RegionId),
    /// Data lives for the entire program execution.
    Static,
    /// Data lifetime is manually managed (e.g., via Allocate/Deallocate).
    Manual,
    /// Data lives for at most the specified number of nanoseconds.
    Bounded(u64),
}

/// Bandwidth constraint for an edge, specifying data throughput requirements.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct BandwidthConstraint {
    /// Minimum required throughput in bytes per second.
    pub min_bytes_per_sec: u64,
    /// Optional maximum throughput cap in bytes per second.
    pub max_bytes_per_sec: Option<u64>,
}

impl BandwidthConstraint {
    /// Create a bandwidth constraint with only a minimum requirement.
    pub fn min(min_bytes_per_sec: u64) -> Self {
        Self {
            min_bytes_per_sec,
            max_bytes_per_sec: None,
        }
    }

    /// Create a bandwidth constraint with both min and max bounds.
    ///
    /// # Panics
    ///
    /// Panics if `min_bytes_per_sec > max_bytes_per_sec`.
    pub fn bounded(min_bytes_per_sec: u64, max_bytes_per_sec: u64) -> Self {
        assert!(
            min_bytes_per_sec <= max_bytes_per_sec,
            "BandwidthConstraint: min ({min_bytes_per_sec}) must be <= max ({max_bytes_per_sec})"
        );
        Self {
            min_bytes_per_sec,
            max_bytes_per_sec: Some(max_bytes_per_sec),
        }
    }
}

/// Execution constraint that can be applied to a region.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Constraint {
    /// Maximum wall-clock time in nanoseconds.
    MaxTime(u64),
    /// Maximum memory usage in bytes.
    MaxMemory(usize),
    /// Maximum energy budget in microjoules.
    MaxEnergy(u64),
    /// User-defined constraint with a name and description.
    Custom { name: String, description: String },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lifetime_variants() {
        let region_id = uuid::Uuid::new_v4();
        let lifetimes = vec![
            Lifetime::Region(region_id),
            Lifetime::Static,
            Lifetime::Manual,
            Lifetime::Bounded(1_000_000),
        ];
        assert_eq!(lifetimes.len(), 4);
        assert_eq!(lifetimes[0], Lifetime::Region(region_id));
    }

    #[test]
    fn bandwidth_construction() {
        let bw_min = BandwidthConstraint::min(1_000_000);
        assert_eq!(bw_min.min_bytes_per_sec, 1_000_000);
        assert_eq!(bw_min.max_bytes_per_sec, None);

        let bw_bounded = BandwidthConstraint::bounded(1_000_000, 10_000_000);
        assert_eq!(bw_bounded.max_bytes_per_sec, Some(10_000_000));
    }

    #[test]
    #[should_panic(expected = "min")]
    fn bandwidth_bounded_rejects_min_greater_than_max() {
        BandwidthConstraint::bounded(10_000_000, 1_000_000);
    }

    #[test]
    fn constraint_variants() {
        let constraints = vec![
            Constraint::MaxTime(50_000),
            Constraint::MaxMemory(1024),
            Constraint::MaxEnergy(100),
            Constraint::Custom {
                name: "safety".into(),
                description: "ASIL-D compliant".into(),
            },
        ];
        assert_eq!(constraints.len(), 4);
    }
}
