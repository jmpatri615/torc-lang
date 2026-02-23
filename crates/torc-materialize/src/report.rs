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
    /// Whether code generation was enabled.
    pub codegen_enabled: bool,
    /// Code size in bytes (if codegen was run).
    pub code_size_bytes: Option<u64>,
    /// Optimization profile used (if codegen was run).
    pub optimization_profile: Option<String>,
    /// Whether post-materialization verification passed.
    pub post_verify_passed: Option<bool>,
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
                    i,
                    stats.nodes_added,
                    stats.nodes_removed,
                    stats.edges_added,
                    stats.edges_removed,
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

        if self.codegen_enabled {
            writeln!(f)?;
            writeln!(f, "--- Code Generation ---")?;
            if let Some(profile) = &self.optimization_profile {
                writeln!(f, "  Optimization: {profile}")?;
            }
            if let Some(size) = self.code_size_bytes {
                writeln!(f, "  Code size: {size} bytes")?;
            }
            if let Some(passed) = self.post_verify_passed {
                writeln!(
                    f,
                    "  Post-verify: {}",
                    if passed { "PASSED" } else { "FAILED" }
                )?;
            }
        }

        Ok(())
    }
}

impl MaterializationReport {
    /// Format a compact spec-style report combining verification summary, resources, and artifact path.
    pub fn format_spec_report(
        &self,
        verification_summary: &str,
        artifact_path: Option<&str>,
    ) -> String {
        let mut out = String::new();
        out.push_str(verification_summary);
        out.push('\n');
        if let Some(ref resources) = self.resources {
            out.push_str(&resources.format_spec_style());
            out.push('\n');
        }
        if let Some(path) = artifact_path {
            out.push_str(&format!("Artifact: {path}"));
            out.push('\n');
        }
        out.truncate(out.trim_end_matches('\n').len());
        out
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
            codegen_enabled: false,
            code_size_bytes: None,
            optimization_profile: None,
            post_verify_passed: None,
        };

        let output = format!("{report}");
        assert!(output.contains("Materialization Report"));
        assert!(output.contains("linux-x86_64"));
        assert!(output.contains("PASSED"));
        assert!(output.contains("2 deduplicated"));
    }

    #[test]
    fn format_spec_report_full() {
        use crate::resource::{ResourceReport, ResourceUsage};

        let report = MaterializationReport {
            target: "stm32f407-discovery".into(),
            duration_ms: 15,
            canonicalization: CanonicalizationStats {
                nodes_deduplicated: 0,
                regions_flattened: 0,
                regions_inlined: 0,
                initial_node_count: 18,
                final_node_count: 18,
            },
            verification_passed: true,
            transforms: vec![],
            schedule_depth: 8,
            max_parallelism: 2,
            resources: Some(ResourceReport {
                flash: ResourceUsage {
                    name: "flash".into(),
                    used: 31_244,
                    available: 524_288,
                    percent: 6.0,
                },
                ram: ResourceUsage {
                    name: "ram".into(),
                    used: 2_108,
                    available: 131_072,
                    percent: 1.6,
                },
                stack: Some(ResourceUsage {
                    name: "stack".into(),
                    used: 892,
                    available: 4_096,
                    percent: 21.8,
                }),
                all_fit: true,
                violations: vec![],
            }),
            codegen_enabled: false,
            code_size_bytes: None,
            optimization_profile: None,
            post_verify_passed: None,
        };

        let verify_summary = "Verification: 42/42 obligations verified (0 waived)";
        let output = report.format_spec_report(verify_summary, Some("/tmp/out/main.elf"));
        assert!(output.contains("Verification: 42/42"));
        assert!(output.contains("Resources:"));
        assert!(output.contains("Flash:"));
        assert!(output.contains("Artifact: /tmp/out/main.elf"));
    }
}
