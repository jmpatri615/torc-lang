//! Materialization report aggregating all pipeline stages.

use std::fmt;

use crate::canonicalize::CanonicalizationStats;
use crate::resource::ResourceReport;
use crate::transform::TransformStats;

/// Summary report of the entire materialization pipeline.
#[derive(Debug, Clone)]
pub struct MaterializationReport {
    /// Target platform name.
    pub target: String,
    /// Total pipeline duration in milliseconds.
    pub duration_ms: u64,
    /// Canonicalization statistics.
    pub canonicalization: CanonicalizationStats,
    /// Whether verification passed.
    pub verification_passed: bool,
    /// Transform statistics from all passes.
    pub transforms: Vec<TransformStats>,
    /// Longest sequential dependency chain.
    pub schedule_depth: usize,
    /// Maximum available parallelism.
    pub max_parallelism: usize,
    /// Resource fitting report.
    pub resources: Option<ResourceReport>,
}

impl fmt::Display for MaterializationReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "=== Materialization Report ===")?;
        writeln!(f, "Target: {}", self.target)?;
        writeln!(f, "Duration: {} ms", self.duration_ms)?;
        writeln!(f)?;

        writeln!(f, "--- Canonicalization ---")?;
        writeln!(
            f,
            "  Nodes: {} -> {} ({} deduplicated)",
            self.canonicalization.initial_node_count,
            self.canonicalization.final_node_count,
            self.canonicalization.nodes_deduplicated,
        )?;
        writeln!(
            f,
            "  Regions: {} inlined, {} flattened",
            self.canonicalization.regions_inlined, self.canonicalization.regions_flattened,
        )?;

        writeln!(f)?;
        writeln!(
            f,
            "--- Verification: {} ---",
            if self.verification_passed {
                "PASSED"
            } else {
                "FAILED"
            }
        )?;

        if !self.transforms.is_empty() {
            writeln!(f)?;
            writeln!(f, "--- Transforms ({} passes) ---", self.transforms.len())?;
            for (i, stats) in self.transforms.iter().enumerate() {
                writeln!(
                    f,
                    "  Pass {}: +{} nodes, -{} nodes, +{} edges, -{} edges",
                    i, stats.nodes_added, stats.nodes_removed, stats.edges_added, stats.edges_removed,
                )?;
            }
        }

        writeln!(f)?;
        writeln!(f, "--- Schedule ---")?;
        writeln!(f, "  Sequential depth: {}", self.schedule_depth)?;
        writeln!(f, "  Max parallelism: {}", self.max_parallelism)?;

        if let Some(ref resources) = self.resources {
            writeln!(f)?;
            write!(f, "{resources}")?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn report_display() {
        let report = MaterializationReport {
            target: "linux-x86_64".into(),
            duration_ms: 42,
            canonicalization: CanonicalizationStats {
                nodes_deduplicated: 2,
                regions_flattened: 1,
                regions_inlined: 0,
                initial_node_count: 10,
                final_node_count: 8,
            },
            verification_passed: true,
            transforms: vec![TransformStats {
                nodes_added: 0,
                nodes_removed: 0,
                edges_added: 0,
                edges_removed: 0,
            }],
            schedule_depth: 5,
            max_parallelism: 3,
            resources: None,
        };

        let output = format!("{report}");
        assert!(output.contains("Materialization Report"));
        assert!(output.contains("linux-x86_64"));
        assert!(output.contains("PASSED"));
        assert!(output.contains("2 deduplicated"));
    }
}
