//! `torc build` — load graph, materialize, emit artifacts.

use std::fs;
use std::path::Path;

use anyhow::{bail, Context, Result};
use torc_materialize::gate::GateConfig;
use torc_materialize::pipeline::PipelineConfig;
use torc_materialize::transform::TransformRegistry;
use torc_materialize::{materialize, MaterializationReport};
use torc_targets::Platform;
use torc_trc::TrcFile;

use torc_spec::bridge::decision_aware_profile;

use crate::commands::decision::load_tdg_optional;
use crate::manifest::{resolve_target, TorcManifest};

/// Run the build pipeline.
#[allow(clippy::too_many_arguments)]
pub fn run(
    project_dir: &Path,
    manifest: Option<&TorcManifest>,
    input: Option<&str>,
    target: Option<&str>,
    all_targets: bool,
    release: bool,
    profile: Option<&str>,
    emit: Option<&str>,
    check_resources: bool,
) -> Result<()> {
    // Resolve targets
    let platforms = resolve_platforms(target, all_targets, manifest)?;

    // Decision readiness check + gate profile adjustment
    let decision_graph = load_tdg_optional(project_dir);
    let gate_profile = if let Some(ref dg) = decision_graph {
        use torc_spec::bridge::{check_materialization_readiness, Severity};

        if let Err(issues) = check_materialization_readiness(dg) {
            let has_errors = issues.iter().any(|i| i.severity == Severity::Error);
            for issue in &issues {
                match issue.severity {
                    Severity::Error => eprintln!("error: {}", issue.message),
                    Severity::Warning => eprintln!("warning: {}", issue.message),
                }
            }
            if has_errors {
                bail!("Build blocked by decision conflicts. Resolve them with `torc decision` before building.");
            }
        }

        // Upgrade gate verification profile based on decision state
        Some(decision_aware_profile(
            dg,
            torc_verify::profile::VerificationProfile::development(),
        ))
    } else {
        None
    };

    for platform in &platforms {
        build_for_target(
            project_dir,
            manifest,
            input,
            platform.clone(),
            release,
            profile,
            emit,
            check_resources,
            gate_profile.as_ref(),
        )?;
    }

    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn build_for_target(
    project_dir: &Path,
    _manifest: Option<&TorcManifest>,
    input: Option<&str>,
    platform: Platform,
    release: bool,
    profile: Option<&str>,
    emit: Option<&str>,
    check_resources: bool,
    gate_profile: Option<&torc_verify::profile::VerificationProfile>,
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

    println!("Target: {}", platform.name);

    // Build gate config, optionally with decision-adjusted profile
    let gate = match gate_profile {
        Some(vp) => GateConfig {
            profile: vp.clone(),
            strict: false,
            max_waivers: None,
        },
        None => GateConfig::development(),
    };

    // Resolve emit mode
    let emit_mode = emit.unwrap_or("graph-stats");

    // Check resources only mode
    if check_resources {
        return run_check_resources(trc.graph, platform, gate);
    }

    // Graph stats only (no codegen needed)
    if emit_mode == "graph-stats" {
        return run_graph_stats(trc.graph, platform, gate);
    }

    // LLVM codegen modes
    #[cfg(feature = "llvm")]
    match emit_mode {
        "llvm-ir" | "object" | "executable" => {
            run_codegen(
                trc.graph,
                platform,
                emit_mode,
                release,
                profile,
                project_dir,
                gate,
            )
        }
        other => bail!(
            "unknown emit mode: '{other}'. Choose: graph-stats, llvm-ir, object, executable"
        ),
    }
    #[cfg(not(feature = "llvm"))]
    {
        let _ = (release, profile, gate);
        bail!(
            "LLVM code generation is not available.\n\
             Rebuild torc with LLVM support:\n  \
             LLVM_SYS_181_PREFIX=/usr/lib/llvm-18 cargo build -p torc --features llvm\n\
             Requested emit mode: {emit_mode}"
        );
    }
}

fn resolve_platforms(
    target: Option<&str>,
    all_targets: bool,
    manifest: Option<&TorcManifest>,
) -> Result<Vec<Platform>> {
    // --target flag takes precedence (single target)
    if let Some(name) = target {
        let platform = resolve_target(name).ok_or_else(|| {
            anyhow::anyhow!(
                "unknown target: '{name}'. Use 'torc target list' to see available targets."
            )
        })?;
        return Ok(vec![platform]);
    }

    // --all-targets: build for every platform in manifest.targets.platforms
    if all_targets {
        if let Some(manifest) = manifest {
            if let Some(ref targets) = manifest.targets {
                let mut platforms = Vec::new();
                // Include default target
                if let Some(ref default_name) = targets.default {
                    if let Some(p) = resolve_target(default_name) {
                        platforms.push(p);
                    }
                }
                // Include per-platform entries
                for name in targets.platforms.keys() {
                    if let Some(p) = resolve_target(name) {
                        if !platforms.iter().any(|existing| existing.name == p.name) {
                            platforms.push(p);
                        }
                    }
                }
                if platforms.is_empty() {
                    bail!("--all-targets: no resolvable targets found in torc.toml");
                }
                return Ok(platforms);
            }
        }
        bail!("--all-targets requires a torc.toml with [targets] section");
    }

    // Manifest default
    if let Some(manifest) = manifest {
        if let Some(name) = manifest.default_target() {
            if let Some(platform) = resolve_target(name) {
                return Ok(vec![platform]);
            }
        }
    }

    // Fallback
    Ok(vec![Platform::generic_linux_x86_64()])
}

fn run_check_resources(graph: torc_core::graph::Graph, platform: Platform, gate: GateConfig) -> Result<()> {
    let config = PipelineConfig {
        platform,
        gate,
        transforms: TransformRegistry::new(),
        enforce_resource_fit: false,
        #[cfg(feature = "llvm")]
        codegen: None,
    };

    let output = materialize(graph, config)?;
    if let Some(ref resources) = output.report.resources {
        print!("{resources}");
    }
    Ok(())
}

fn run_graph_stats(graph: torc_core::graph::Graph, platform: Platform, gate: GateConfig) -> Result<()> {
    let config = PipelineConfig {
        platform,
        gate,
        transforms: TransformRegistry::new(),
        enforce_resource_fit: false,
        #[cfg(feature = "llvm")]
        codegen: None,
    };

    let output = materialize(graph, config)?;
    print_report(&output.report);
    Ok(())
}

#[cfg(feature = "llvm")]
fn run_codegen(
    graph: torc_core::graph::Graph,
    platform: Platform,
    emit_mode: &str,
    release: bool,
    profile: Option<&str>,
    project_dir: &Path,
    gate: GateConfig,
) -> Result<()> {
    use torc_materialize::codegen::{CodegenConfig, EmitTarget};

    let emit_target = match emit_mode {
        "llvm-ir" => EmitTarget::LlvmIr,
        "object" => EmitTarget::ObjectFile,
        "executable" => EmitTarget::Executable,
        _ => unreachable!(),
    };

    let optimization = resolve_optimization(release, profile)?;

    let output_dir = project_dir.join("out");
    fs::create_dir_all(&output_dir).context("creating out/ directory")?;

    let config = PipelineConfig {
        platform,
        gate,
        transforms: TransformRegistry::new(),
        enforce_resource_fit: false,
        codegen: Some(CodegenConfig {
            target: emit_target,
            optimization,
            output_dir: output_dir.clone(),
            function_name: "main".to_string(),
        }),
    };

    let output = materialize(graph, config)?;
    print_report(&output.report);

    if let Some(ref artifact) = output.artifact {
        if let Some(ref path) = artifact.executable_path {
            println!("Executable: {}", path.display());
        }
        if let Some(ref path) = artifact.object_path {
            println!("Object:     {}", path.display());
        }
        if artifact.llvm_ir.is_some() {
            println!("LLVM IR:    (emitted to stdout)");
        }
    }

    Ok(())
}

#[cfg(feature = "llvm")]
fn resolve_optimization(
    release: bool,
    profile: Option<&str>,
) -> Result<torc_materialize::codegen::profile::OptimizationProfile> {
    use torc_materialize::codegen::profile::OptimizationProfile;

    if let Some(name) = profile {
        return match name {
            "debug" => Ok(OptimizationProfile::Debug),
            "balanced" => Ok(OptimizationProfile::Balanced),
            "throughput" => Ok(OptimizationProfile::Throughput),
            "minimal-size" => Ok(OptimizationProfile::MinimalSize),
            "deterministic-timing" => Ok(OptimizationProfile::DeterministicTiming),
            other => bail!("unknown optimization profile: '{other}'. Choose: debug, balanced, throughput, minimal-size, deterministic-timing"),
        };
    }

    if release {
        Ok(OptimizationProfile::Throughput)
    } else {
        Ok(OptimizationProfile::Debug)
    }
}

fn print_report(report: &MaterializationReport) {
    print!("{report}");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_platforms_cli_flag() {
        let platforms = resolve_platforms(Some("linux-x86_64"), false, None).unwrap();
        assert_eq!(platforms.len(), 1);
        assert_eq!(platforms[0].name, "linux-x86_64");
    }

    #[test]
    fn resolve_platforms_manifest_default() {
        let manifest = TorcManifest::from_str(
            r#"
[project]
name = "test"
[targets]
default = "stm32f407-discovery"
"#,
        )
        .unwrap();
        let platforms = resolve_platforms(None, false, Some(&manifest)).unwrap();
        assert_eq!(platforms.len(), 1);
        assert_eq!(platforms[0].name, "stm32f407-discovery");
    }

    #[test]
    fn resolve_platforms_fallback() {
        let platforms = resolve_platforms(None, false, None).unwrap();
        assert_eq!(platforms.len(), 1);
        assert_eq!(platforms[0].name, "linux-x86_64");
    }

    #[test]
    fn resolve_platforms_unknown() {
        assert!(resolve_platforms(Some("nonexistent"), false, None).is_err());
    }

    #[test]
    fn resolve_platforms_all_targets() {
        let manifest = TorcManifest::from_str(
            r#"
[project]
name = "multi-target"
[targets]
default = "linux-x86_64"
[targets.platforms.stm32f407-discovery]
optimization = "minimal-size"
"#,
        )
        .unwrap();
        let platforms = resolve_platforms(None, true, Some(&manifest)).unwrap();
        assert!(platforms.len() >= 2);
        assert!(platforms.iter().any(|p| p.name == "linux-x86_64"));
        assert!(platforms.iter().any(|p| p.name == "stm32f407-discovery"));
    }

    #[test]
    fn resolve_platforms_all_targets_no_manifest() {
        assert!(resolve_platforms(None, true, None).is_err());
    }

    #[cfg(feature = "llvm")]
    #[test]
    fn resolve_optimization_profiles() {
        use torc_materialize::codegen::profile::OptimizationProfile;

        assert!(matches!(
            resolve_optimization(false, None).unwrap(),
            OptimizationProfile::Debug
        ));
        assert!(matches!(
            resolve_optimization(true, None).unwrap(),
            OptimizationProfile::Throughput
        ));
        assert!(matches!(
            resolve_optimization(false, Some("balanced")).unwrap(),
            OptimizationProfile::Balanced
        ));
        assert!(resolve_optimization(false, Some("unknown")).is_err());
    }
}
