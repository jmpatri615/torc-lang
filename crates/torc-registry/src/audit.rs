//! Dependency auditing for safety-critical supply chains.
//!
//! Inspects all resolved dependencies and produces an audit report with
//! safety levels, verification coverage, and license information.

use crate::client::RegistryBackend;
use crate::error::Result;
use crate::resolution::ResolutionResult;

/// An audit report for the dependency tree.
#[derive(Debug, Clone)]
pub struct AuditReport {
    /// Individual module audit entries.
    pub entries: Vec<AuditEntry>,
    /// Overall summary.
    pub summary: AuditSummary,
}

/// Audit information for a single module.
#[derive(Debug, Clone)]
pub struct AuditEntry {
    /// Module name.
    pub name: String,
    /// Module version.
    pub version: String,
    /// Safety integrity level (e.g., "ASIL-D", "QM").
    pub integrity_level: Option<String>,
    /// Verification coverage percentage.
    pub verification_coverage: Option<u8>,
    /// License identifier.
    pub license: Option<String>,
    /// Whether the module has author information.
    pub has_authors: bool,
    /// Issues found during audit.
    pub issues: Vec<String>,
}

/// Summary of the audit.
#[derive(Debug, Clone)]
pub struct AuditSummary {
    /// Total number of dependencies audited.
    pub total: usize,
    /// Number with safety certification.
    pub certified: usize,
    /// Number with 100% verification coverage.
    pub fully_verified: usize,
    /// Number with any issues.
    pub with_issues: usize,
    /// Overall audit passed (no blocking issues).
    pub passed: bool,
}

/// Run an audit on the resolved dependencies.
pub fn audit(
    resolution: &ResolutionResult,
    backend: &dyn RegistryBackend,
) -> Result<AuditReport> {
    let mut entries = Vec::new();

    for lock_entry in &resolution.lock {
        let version = lock_entry.version.clone();
        let pkg = backend.fetch(&lock_entry.name, &version)?;

        let mut issues = Vec::new();

        let integrity_level = pkg
            .manifest
            .module
            .safety
            .as_ref()
            .and_then(|s| s.max_integrity_level.clone());

        let verification_coverage = pkg
            .manifest
            .module
            .safety
            .as_ref()
            .and_then(|s| s.verification_coverage);

        let license = pkg.manifest.module.license.clone();
        let has_authors = !pkg.manifest.module.authors.is_empty();

        // Check for common issues
        if license.is_none() {
            issues.push("no license specified".to_string());
        }

        if !has_authors {
            issues.push("no authors specified".to_string());
        }

        if verification_coverage.is_none() {
            issues.push("no verification coverage reported".to_string());
        } else if let Some(cov) = verification_coverage {
            if cov < 100 {
                issues.push(format!("verification coverage is {cov}% (not 100%)"));
            }
        }

        if integrity_level.is_none() {
            issues.push("no safety integrity level declared".to_string());
        }

        entries.push(AuditEntry {
            name: lock_entry.name.clone(),
            version: version.to_string(),
            integrity_level,
            verification_coverage,
            license,
            has_authors,
            issues,
        });
    }

    let total = entries.len();
    let certified = entries
        .iter()
        .filter(|e| e.integrity_level.is_some())
        .count();
    let fully_verified = entries
        .iter()
        .filter(|e| e.verification_coverage == Some(100))
        .count();
    let with_issues = entries.iter().filter(|e| !e.issues.is_empty()).count();
    let passed = entries
        .iter()
        .all(|e| e.issues.iter().all(|i| !i.contains("no license")));

    let summary = AuditSummary {
        total,
        certified,
        fully_verified,
        with_issues,
        passed,
    };

    Ok(AuditReport { entries, summary })
}

/// Format an audit report as a human-readable string.
pub fn format_report(report: &AuditReport) -> String {
    let mut out = String::new();

    for entry in &report.entries {
        let level = entry
            .integrity_level
            .as_deref()
            .unwrap_or("unspecified");
        let coverage = entry
            .verification_coverage
            .map(|c| format!("{c}%"))
            .unwrap_or_else(|| "N/A".to_string());
        let license = entry.license.as_deref().unwrap_or("none");

        out.push_str(&format!(
            "  {} v{}: {} certified, {} verified, license: {}\n",
            entry.name, entry.version, level, coverage, license
        ));

        for issue in &entry.issues {
            out.push_str(&format!("    ! {issue}\n"));
        }
    }

    out.push('\n');
    out.push_str(&format!(
        "Summary: {} dependencies, {} certified, {} fully verified, {} with issues\n",
        report.summary.total,
        report.summary.certified,
        report.summary.fully_verified,
        report.summary.with_issues
    ));

    if report.summary.passed {
        out.push_str("Audit: PASSED\n");
    } else {
        out.push_str("Audit: ISSUES FOUND\n");
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::{LocalRegistry, ModulePackage, RegistryBackend};
    use crate::integrity::ContentHash;
    use crate::module_manifest::ModuleManifest;
    use crate::resolution::{LockEntry, ResolutionResult};

    fn publish_module(registry: &LocalRegistry, _name: &str, _version: &str, manifest_str: &str) {
        let manifest_bytes = manifest_str.as_bytes().to_vec();
        let manifest = ModuleManifest::parse(manifest_str).unwrap();
        let trc_bytes = b"trc data".to_vec();
        let trc_hash = ContentHash::compute(&trc_bytes);
        let pkg = ModulePackage {
            manifest,
            manifest_bytes,
            trc_bytes,
            trc_hash,
        };
        registry.publish(&pkg).unwrap();
    }

    #[test]
    fn audit_fully_certified() {
        let dir = tempfile::tempdir().unwrap();
        let registry = LocalRegistry::new(dir.path().to_path_buf());

        let manifest = r#"
[module]
name = "safe-mod"
version = "1.0.0"
license = "MIT"
authors = ["test"]

[module.safety]
max-integrity-level = "ASIL-D"
verification-coverage = 100
"#;
        publish_module(&registry, "safe-mod", "1.0.0", manifest);

        let resolution = ResolutionResult {
            tree: Vec::new(),
            lock: vec![LockEntry {
                name: "safe-mod".to_string(),
                version: semver::Version::new(1, 0, 0),
                trc_hash: None,
            }],
        };

        let report = audit(&resolution, &registry).unwrap();
        assert_eq!(report.summary.total, 1);
        assert_eq!(report.summary.certified, 1);
        assert_eq!(report.summary.fully_verified, 1);
        assert!(report.summary.passed);
    }

    #[test]
    fn audit_missing_fields() {
        let dir = tempfile::tempdir().unwrap();
        let registry = LocalRegistry::new(dir.path().to_path_buf());

        let manifest = "[module]\nname = \"bare\"\nversion = \"1.0.0\"\n";
        publish_module(&registry, "bare", "1.0.0", manifest);

        let resolution = ResolutionResult {
            tree: Vec::new(),
            lock: vec![LockEntry {
                name: "bare".to_string(),
                version: semver::Version::new(1, 0, 0),
                trc_hash: None,
            }],
        };

        let report = audit(&resolution, &registry).unwrap();
        assert_eq!(report.summary.with_issues, 1);
        assert!(!report.summary.passed);

        let entry = &report.entries[0];
        assert!(entry.issues.iter().any(|i| i.contains("no license")));
    }

    #[test]
    fn format_report_output() {
        let report = AuditReport {
            entries: vec![AuditEntry {
                name: "test".to_string(),
                version: "1.0.0".to_string(),
                integrity_level: Some("ASIL-B".to_string()),
                verification_coverage: Some(95),
                license: Some("MIT".to_string()),
                has_authors: true,
                issues: vec!["verification coverage is 95% (not 100%)".to_string()],
            }],
            summary: AuditSummary {
                total: 1,
                certified: 1,
                fully_verified: 0,
                with_issues: 1,
                passed: true,
            },
        };

        let output = format_report(&report);
        assert!(output.contains("test v1.0.0"));
        assert!(output.contains("ASIL-B"));
        assert!(output.contains("95%"));
        assert!(output.contains("PASSED"));
    }
}
