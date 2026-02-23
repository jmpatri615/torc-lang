//! `torc verify` — run verification engine on a graph.

use std::fs;
use std::path::Path;

use anyhow::{bail, Context, Result};
use torc_spec::bridge::decision_aware_profile;
use torc_trc::TrcFile;
use torc_verify::engine::VerificationEngine;
use torc_verify::profile::VerificationProfile;

use crate::commands::decision::load_tdg_optional;
use crate::manifest::TorcManifest;

/// Run verification on a Torc graph.
pub fn run(
    project_dir: &Path,
    manifest: Option<&TorcManifest>,
    input: Option<&str>,
    status_only: bool,
    report_format: Option<&str>,
    profile: Option<&str>,
    _incremental: bool,
) -> Result<()> {
    // Load graph
    let graph_path = match input {
        Some(path) => Path::new(path).to_path_buf(),
        None => project_dir.join("graph/main.trc"),
    };

    if !graph_path.exists() {
        bail!(
            "graph file not found: {}. Run 'torc init' to create a project.",
            graph_path.display()
        );
    }

    let bytes =
        fs::read(&graph_path).with_context(|| format!("reading {}", graph_path.display()))?;
    let trc =
        TrcFile::from_bytes(&bytes).with_context(|| format!("parsing {}", graph_path.display()))?;

    // Resolve profile (CLI flag > manifest default > development)
    let effective_profile =
        profile.or_else(|| manifest.and_then(|m| m.default_verification_profile()));
    let mut vprofile = resolve_profile(effective_profile)?;

    // Decision-aware profile adjustment
    let decision_graph = load_tdg_optional(project_dir);
    if let Some(ref dg) = decision_graph {
        use torc_spec::decision::DecisionState;

        let has_conflicted = dg.decisions().any(|d| d.state == DecisionState::Conflicted);
        if has_conflicted {
            eprintln!("warning: Decision conflict detected — verification may be incomplete");
        }

        let original_level = vprofile.level;
        vprofile = decision_aware_profile(dg, vprofile);
        if vprofile.level != original_level {
            println!(
                "note: Verification profile upgraded to {:?} due to decision state",
                vprofile.level
            );
        }
    }

    // Run verification
    let mut engine = VerificationEngine::new(vprofile);
    let report = engine.verify(&trc.graph);

    // Output
    if status_only {
        println!("Verification status:");
        println!("  Total:    {}", report.summary.total);
        println!("  Verified: {}", report.summary.verified);
        println!("  Pending:  {}", report.summary.pending);
        println!("  Waived:   {}", report.summary.waived);
        println!("  Failed:   {}", report.summary.failed);
    } else {
        match report_format {
            Some("json") => {
                let json = serde_json::json!({
                    "profile": format!("{:?}", report.profile),
                    "summary": {
                        "total": report.summary.total,
                        "verified": report.summary.verified,
                        "pending": report.summary.pending,
                        "waived": report.summary.waived,
                        "failed": report.summary.failed,
                        "cache_hits": report.summary.cache_hits,
                    },
                    "diagnostics": report.diagnostics.iter().map(|d| {
                        serde_json::json!({
                            "obligation_id": d.obligation_id,
                            "severity": format!("{}", d.severity),
                            "message": d.message,
                            "context": d.context,
                            "suggestions": d.suggestions,
                        })
                    }).collect::<Vec<_>>(),
                });
                println!("{}", serde_json::to_string_pretty(&json)?);
            }
            _ => {
                // Default: human-readable
                print!("{report}");
            }
        }
    }

    // Exit code 1 if any failed
    if report.summary.failed > 0 {
        bail!(
            "verification failed: {} obligation(s) not satisfied",
            report.summary.failed
        );
    }

    Ok(())
}

fn resolve_profile(name: Option<&str>) -> Result<VerificationProfile> {
    match name {
        Some("development") | None => Ok(VerificationProfile::development()),
        Some("integration") => Ok(VerificationProfile::integration()),
        Some("certification") => Ok(VerificationProfile::certification()),
        Some(other) => bail!(
            "unknown verification profile: '{other}'. Choose: development, integration, certification"
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_profiles() {
        assert!(resolve_profile(None).is_ok());
        assert!(resolve_profile(Some("development")).is_ok());
        assert!(resolve_profile(Some("integration")).is_ok());
        assert!(resolve_profile(Some("certification")).is_ok());
        assert!(resolve_profile(Some("unknown")).is_err());
    }

    #[test]
    fn verify_empty_graph() {
        let dir = tempfile::tempdir().unwrap();
        let graph_dir = dir.path().join("graph");
        std::fs::create_dir_all(&graph_dir).unwrap();

        // Write an empty graph
        let graph = torc_core::graph::Graph::new();
        let trc = TrcFile::new(graph);
        let bytes = trc.to_bytes().unwrap();
        std::fs::write(graph_dir.join("main.trc"), &bytes).unwrap();

        // Verify should succeed (no obligations)
        run(dir.path(), None, None, false, None, None, false).unwrap();
    }
}
