//! Torc CLI — unified command-line interface for the Torc programming language.

mod commands;
mod manifest;

use std::path::{Path, PathBuf};
use std::process;

use clap::{Parser, Subcommand};

use manifest::TorcManifest;

#[derive(Parser)]
#[command(name = "torc", version, about = "The Torc Programming Language")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Create a new Torc project
    Init {
        /// Project name
        name: String,
    },
    /// Build a Torc graph into target artifacts
    Build {
        /// Target platform (e.g., linux-x86_64, stm32f407-discovery)
        #[arg(long)]
        target: Option<String>,
        /// Build for all targets defined in torc.toml
        #[arg(long)]
        all_targets: bool,
        /// Build in release mode (optimization: throughput)
        #[arg(long)]
        release: bool,
        /// Optimization profile (debug, balanced, throughput, minimal-size, deterministic-timing)
        #[arg(long)]
        profile: Option<String>,
        /// Emit mode (graph-stats, llvm-ir, object, executable)
        #[arg(long)]
        emit: Option<String>,
        /// Check resource constraints without codegen
        #[arg(long)]
        check_resources: bool,
        /// Input .trc file (default: graph/main.trc)
        #[arg(long)]
        input: Option<String>,
    },
    /// Run verification on a Torc graph
    Verify {
        /// Input .trc file (default: graph/main.trc)
        #[arg(long)]
        input: Option<String>,
        /// Print summary status only
        #[arg(long)]
        status: bool,
        /// Report format (human, json)
        #[arg(long)]
        report: Option<String>,
        /// Verification profile (development, integration, certification)
        #[arg(long)]
        profile: Option<String>,
        /// Enable incremental verification
        #[arg(long)]
        incremental: bool,
    },
    /// Inspect a Torc graph
    Inspect {
        /// View mode (pseudo-code, contracts, resources, dataflow, provenance)
        #[arg(long)]
        view: Option<String>,
        /// Input .trc file (default: graph/main.trc)
        #[arg(long)]
        input: Option<String>,
        /// Output format (text, json)
        #[arg(long)]
        export: Option<String>,
        /// Target platform (needed for resource budget view)
        #[arg(long)]
        target: Option<String>,
    },
    /// Manage target platforms
    Target {
        #[command(subcommand)]
        action: TargetAction,
    },
    /// Check toolchain and project status
    Doctor {
        /// Check a specific target
        #[arg(long)]
        target: Option<String>,
    },
    /// Remove build artifacts
    Clean {
        /// Also remove cached proofs
        #[arg(long)]
        proofs: bool,
    },
}

#[derive(Subcommand)]
enum TargetAction {
    /// List available target platforms
    List,
    /// Show details of a target platform
    Describe {
        /// Platform name
        name: String,
    },
    /// Add a custom target (not yet implemented)
    Add {
        /// Platform name
        name: String,
    },
    /// Validate a target definition (not yet implemented)
    Validate {
        /// Platform name
        name: String,
    },
}

fn main() {
    let cli = Cli::parse();

    let result = run(cli);
    if let Err(e) = result {
        eprintln!("error: {e:#}");
        process::exit(1);
    }
}

fn run(cli: Cli) -> anyhow::Result<()> {
    let cwd = std::env::current_dir()?;

    match cli.command {
        Commands::Init { name } => commands::init::run(&name),

        Commands::Build {
            target,
            all_targets,
            release,
            profile,
            emit,
            check_resources,
            input,
        } => {
            let (manifest, project_dir) = load_manifest_optional(&cwd)?;
            let project_dir = project_dir.unwrap_or(cwd);
            commands::build::run(
                &project_dir,
                manifest.as_ref(),
                input.as_deref(),
                target.as_deref(),
                all_targets,
                release,
                profile.as_deref(),
                emit.as_deref(),
                check_resources,
            )
        }

        Commands::Verify {
            input,
            status,
            report,
            profile,
            incremental,
        } => {
            let (manifest, project_dir) = load_manifest_optional(&cwd)?;
            let project_dir = project_dir.unwrap_or(cwd);
            commands::verify::run(
                &project_dir,
                manifest.as_ref(),
                input.as_deref(),
                status,
                report.as_deref(),
                profile.as_deref(),
                incremental,
            )
        }

        Commands::Inspect {
            view,
            input,
            export,
            target,
        } => {
            let (_, project_dir) = load_manifest_optional(&cwd)?;
            let project_dir = project_dir.unwrap_or(cwd);
            commands::inspect::run(
                &project_dir,
                view.as_deref(),
                input.as_deref(),
                export.as_deref(),
                target.as_deref(),
            )
        }

        Commands::Target { action } => match action {
            TargetAction::List => commands::target::list(),
            TargetAction::Describe { name } => commands::target::describe(&name),
            TargetAction::Add { name } => commands::target::add(&name),
            TargetAction::Validate { name } => commands::target::validate(&name),
        },

        Commands::Doctor { target } => {
            let (_, project_dir) = load_manifest_optional(&cwd)?;
            let project_dir = project_dir.unwrap_or(cwd);
            commands::doctor::run(&project_dir, target.as_deref())
        }

        Commands::Clean { proofs } => {
            let (_, project_dir) = load_manifest_optional(&cwd)?;
            let project_dir = project_dir.unwrap_or(cwd);
            commands::clean::run(&project_dir, proofs)
        }
    }
}

/// Try to load a manifest from the current directory upward. Returns (None, None) if not found.
fn load_manifest_optional(
    cwd: &Path,
) -> anyhow::Result<(Option<TorcManifest>, Option<PathBuf>)> {
    match TorcManifest::find_and_load(cwd)? {
        Some((manifest, dir)) => Ok((Some(manifest), Some(dir))),
        None => Ok((None, None)),
    }
}

#[cfg(test)]
mod integration_tests {
    use super::*;

    /// Full workflow: init → verify → build → clean.
    #[test]
    fn init_verify_build_clean_workflow() {
        let dir = tempfile::tempdir().unwrap();
        let project_path = dir.path().join("workflow-test");

        // 1. Init
        commands::init::create_project(&project_path, "workflow-test").unwrap();
        assert!(project_path.join("torc.toml").is_file());
        assert!(project_path.join("graph/main.trc").is_file());

        // 2. Verify — load manifest and graph, run verification
        let (manifest, project_dir) =
            TorcManifest::find_and_load(&project_path).unwrap().unwrap();
        assert_eq!(project_dir, project_path);
        commands::verify::run(
            &project_path,
            Some(&manifest),
            None,
            false,
            None,
            None,
            false,
        )
        .unwrap();

        // 3. Build — graph-stats mode (no LLVM needed)
        commands::build::run(
            &project_path,
            Some(&manifest),
            None,
            None,
            false,
            false,
            None,
            None,
            false,
        )
        .unwrap();

        // 4. Clean — create out/ first (build graph-stats doesn't create it)
        std::fs::create_dir_all(project_path.join("out")).unwrap();
        commands::clean::run(&project_path, false).unwrap();
        assert!(!project_path.join("out").exists());
    }

    /// Verify JSON output format.
    #[test]
    fn verify_json_output() {
        let dir = tempfile::tempdir().unwrap();
        let project_path = dir.path().join("json-test");
        commands::init::create_project(&project_path, "json-test").unwrap();

        let (manifest, _) = TorcManifest::find_and_load(&project_path).unwrap().unwrap();

        // JSON report should succeed without error
        commands::verify::run(
            &project_path,
            Some(&manifest),
            None,
            false,
            Some("json"),
            None,
            false,
        )
        .unwrap();
    }

    /// Verify status-only output.
    #[test]
    fn verify_status_output() {
        let dir = tempfile::tempdir().unwrap();
        let project_path = dir.path().join("status-test");
        commands::init::create_project(&project_path, "status-test").unwrap();

        commands::verify::run(
            &project_path,
            None,
            None,
            true,
            None,
            None,
            false,
        )
        .unwrap();
    }
}
