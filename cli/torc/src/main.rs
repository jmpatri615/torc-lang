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
        /// View mode (pseudo-code, contracts, resources, dataflow, provenance, decision)
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
    /// FFI bridge generation
    Ffi {
        #[command(subcommand)]
        action: FfiAction,
    },
    /// Add a dependency
    Add {
        /// Module name
        name: String,
        /// Version requirement (e.g., ">=1.0.0", "^1.2")
        #[arg(long)]
        version: Option<String>,
    },
    /// Remove a dependency
    Remove {
        /// Module name
        name: String,
    },
    /// Update dependencies
    Update {
        /// Specific module to update (all if omitted)
        name: Option<String>,
    },
    /// Show dependency tree
    Tree,
    /// Publish module to registry
    Publish {
        /// Validate without publishing
        #[arg(long)]
        dry_run: bool,
    },
    /// Audit dependencies for safety and compliance
    Audit,
    /// Manage design decisions (specification interface)
    Decision {
        #[command(subcommand)]
        action: DecisionAction,
    },
}

#[derive(Subcommand)]
enum FfiAction {
    /// Generate FFI bridge graphs or C headers
    Bridge {
        /// Generate Torc bridge from a C FFI declaration (.ffi.toml)
        #[arg(long)]
        from_c: Option<String>,
        /// Generate C header from Torc graph exports
        #[arg(long)]
        to_c: bool,
        /// Input .trc file (for --to-c, default: graph/main.trc)
        #[arg(long)]
        input: Option<String>,
        /// Output file path (for --to-c)
        #[arg(long)]
        output: Option<String>,
    },
}

#[derive(Subcommand)]
enum DecisionAction {
    /// Initialize decision tracking (create decisions.tdg)
    Init,
    /// List decisions
    List {
        /// Filter by state (e.g., committed, deferred, exploring)
        #[arg(long)]
        state: Option<String>,
        /// Filter by domain (e.g., safety, performance)
        #[arg(long)]
        domain: Option<String>,
    },
    /// Show full details of a decision
    Show {
        /// Decision ID (or prefix)
        id: String,
    },
    /// Commit a decision with a specific value
    Commit {
        /// Decision ID (or prefix)
        id: String,
        /// The value to commit
        value: String,
        /// Rationale for the decision
        #[arg(long)]
        rationale: Option<String>,
    },
    /// Defer a decision for later
    Defer {
        /// Decision ID (or prefix)
        id: String,
        /// Provisional value
        #[arg(long)]
        provisional: Option<String>,
        /// Revisit when this decision ID is committed
        #[arg(long)]
        revisit_when: Option<String>,
    },
    /// Show summary counts by decision state
    Status,
    /// Add a new decision
    Add {
        /// Decision title
        title: String,
        /// Decision domain (e.g., safety, performance, topology)
        #[arg(long)]
        domain: String,
        /// Description
        #[arg(long)]
        description: Option<String>,
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
        /// Output format (default: human-readable, "toml" for TOML)
        #[arg(long)]
        format: Option<String>,
    },
    /// Add a custom target definition
    Add {
        /// Platform name
        name: String,
    },
    /// Validate a target definition
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

        Commands::Target { action } => {
            let (_, project_dir) = load_manifest_optional(&cwd)?;
            let project_dir = project_dir.unwrap_or_else(|| cwd.clone());
            match action {
                TargetAction::List => commands::target::list(Some(&project_dir)),
                TargetAction::Describe { name, format } => {
                    commands::target::describe(&name, Some(&project_dir), format.as_deref())
                }
                TargetAction::Add { name } => commands::target::add(&name, &project_dir),
                TargetAction::Validate { name } => {
                    commands::target::validate(&name, Some(&project_dir))
                }
            }
        }

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

        Commands::Ffi { action } => match action {
            FfiAction::Bridge {
                from_c,
                to_c,
                input,
                output,
            } => {
                let (manifest, project_dir) = load_manifest_optional(&cwd)?;
                let project_dir = project_dir.unwrap_or(cwd);
                if let Some(ffi_toml) = from_c {
                    commands::ffi::bridge_from_c(&project_dir, manifest.as_ref(), &ffi_toml)
                } else if to_c {
                    commands::ffi::bridge_to_c(&project_dir, input.as_deref(), output.as_deref())
                } else {
                    anyhow::bail!("specify --from-c <file.ffi.toml> or --to-c")
                }
            }
        },

        Commands::Add { name, version } => {
            let (manifest, project_dir) = load_manifest_required(&cwd)?;
            commands::registry::add(&project_dir, &manifest, &name, version.as_deref())
        }

        Commands::Remove { name } => {
            let (_, project_dir) = load_manifest_optional(&cwd)?;
            let project_dir = project_dir.unwrap_or(cwd);
            commands::registry::remove(&project_dir, &name)
        }

        Commands::Update { name } => {
            let (manifest, project_dir) = load_manifest_required(&cwd)?;
            commands::registry::update(&project_dir, &manifest, name.as_deref())
        }

        Commands::Tree => {
            let (manifest, project_dir) = load_manifest_required(&cwd)?;
            commands::registry::tree(&project_dir, &manifest)
        }

        Commands::Publish { dry_run } => {
            let (manifest, project_dir) = load_manifest_required(&cwd)?;
            commands::registry::publish(&project_dir, &manifest, dry_run)
        }

        Commands::Audit => {
            let (manifest, project_dir) = load_manifest_required(&cwd)?;
            commands::registry::audit(&project_dir, &manifest)
        }

        Commands::Decision { action } => match action {
            DecisionAction::Init => {
                let project_dir = match load_manifest_optional(&cwd)? {
                    (_, Some(dir)) => dir,
                    _ => cwd,
                };
                commands::decision::init(&project_dir)
            }
            DecisionAction::List { state, domain } => {
                let (_, project_dir) = load_manifest_optional(&cwd)?;
                let project_dir = project_dir.unwrap_or(cwd);
                commands::decision::list(&project_dir, state.as_deref(), domain.as_deref())
            }
            DecisionAction::Show { id } => {
                let (_, project_dir) = load_manifest_optional(&cwd)?;
                let project_dir = project_dir.unwrap_or(cwd);
                commands::decision::show(&project_dir, &id)
            }
            DecisionAction::Commit {
                id,
                value,
                rationale,
            } => {
                let (_, project_dir) = load_manifest_optional(&cwd)?;
                let project_dir = project_dir.unwrap_or(cwd);
                commands::decision::commit(&project_dir, &id, &value, rationale.as_deref())
            }
            DecisionAction::Defer {
                id,
                provisional,
                revisit_when,
            } => {
                let (_, project_dir) = load_manifest_optional(&cwd)?;
                let project_dir = project_dir.unwrap_or(cwd);
                commands::decision::defer(
                    &project_dir,
                    &id,
                    provisional.as_deref(),
                    revisit_when.as_deref(),
                )
            }
            DecisionAction::Status => {
                let (_, project_dir) = load_manifest_optional(&cwd)?;
                let project_dir = project_dir.unwrap_or(cwd);
                commands::decision::status(&project_dir)
            }
            DecisionAction::Add {
                title,
                domain,
                description,
            } => {
                let (_, project_dir) = load_manifest_optional(&cwd)?;
                let project_dir = project_dir.unwrap_or(cwd);
                commands::decision::add(&project_dir, &title, &domain, description.as_deref())
            }
        },
    }
}

/// Load manifest, returning error if not found.
fn load_manifest_required(cwd: &Path) -> anyhow::Result<(TorcManifest, PathBuf)> {
    match TorcManifest::find_and_load(cwd)? {
        Some((manifest, dir)) => Ok((manifest, dir)),
        None => anyhow::bail!("no torc.toml found (run `torc init` first)"),
    }
}

/// Try to load a manifest from the current directory upward. Returns (None, None) if not found.
fn load_manifest_optional(cwd: &Path) -> anyhow::Result<(Option<TorcManifest>, Option<PathBuf>)> {
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
        let (manifest, project_dir) = TorcManifest::find_and_load(&project_path).unwrap().unwrap();
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

        commands::verify::run(&project_path, None, None, true, None, None, false).unwrap();
    }

    /// FFI bridge-from-c workflow: .ffi.toml → bridge graph → .trc file.
    #[test]
    fn ffi_bridge_from_c_workflow() {
        let dir = tempfile::tempdir().unwrap();
        let project_path = dir.path().join("ffi-test");
        commands::init::create_project(&project_path, "ffi-test").unwrap();

        // Write a .ffi.toml declaration
        let ffi_toml = r#"
[foreign-library]
name = "libm"
abi = "C"
header = "math.h"
link = "-lm"

[[functions]]
name = "sin"
c_signature = "double sin(double x)"
trust_level = "platform"

[[functions]]
name = "cos"
c_signature = "double cos(double x)"
trust_level = "platform"
"#;
        std::fs::write(project_path.join("libm.ffi.toml"), ffi_toml).unwrap();

        // Run bridge-from-c
        commands::ffi::bridge_from_c(&project_path, None, "libm.ffi.toml").unwrap();

        // Verify output exists
        let output = project_path.join("graph/ffi/libm.trc");
        assert!(output.is_file(), "bridge .trc file should exist");

        // Verify round-trip: load the generated .trc and validate
        let data = std::fs::read(&output).unwrap();
        let trc = torc_trc::TrcFile::from_bytes(&data).unwrap();
        assert!(trc.graph.node_count() > 0);
    }

    /// FFI bridge-to-c workflow: graph with exports → C header.
    #[test]
    fn ffi_bridge_to_c_workflow() {
        let dir = tempfile::tempdir().unwrap();
        let project_path = dir.path().join("export-test");
        commands::init::create_project(&project_path, "export-test").unwrap();

        // Create a graph with export annotations and save as .trc
        use torc_core::builder::GraphBuilder;
        use torc_core::graph::node::{ArithmeticOp, NodeKind};
        use torc_core::types::{Type, TypeSignature};

        let mut builder = GraphBuilder::new();
        let id = builder.add_typed_node(
            NodeKind::Arithmetic(ArithmeticOp::Add),
            "add",
            TypeSignature::pure_fn(vec![Type::i32(), Type::i32()], Type::i32()),
        );
        builder.annotate(id, "export.name", "torc_add").unwrap();
        builder.annotate(id, "export.param.0", "a").unwrap();
        builder.annotate(id, "export.param.1", "b").unwrap();
        let graph = builder.into_graph();

        let trc = torc_trc::TrcFile::new(graph);
        let data = trc.to_bytes().unwrap();
        std::fs::write(project_path.join("graph/main.trc"), &data).unwrap();

        // Run bridge-to-c
        commands::ffi::bridge_to_c(&project_path, None, Some("include/exports.h")).unwrap();

        let header_path = project_path.join("include/exports.h");
        assert!(header_path.is_file(), "C header should exist");

        let header = std::fs::read_to_string(&header_path).unwrap();
        assert!(header.contains("torc_add"));
        assert!(header.contains("int32_t"));
    }

    /// FFI trust policy enforcement from manifest.
    #[test]
    fn ffi_policy_enforcement() {
        let dir = tempfile::tempdir().unwrap();
        let project_path = dir.path().join("policy-test");
        commands::init::create_project(&project_path, "policy-test").unwrap();

        // Override torc.toml with trust policy that disallows unsafe
        let manifest_toml = r#"
[project]
name = "policy-test"

[ffi]
c_headers = []

[ffi.trust-policy]
allow_unsafe = false
"#;
        std::fs::write(project_path.join("torc.toml"), manifest_toml).unwrap();

        // Write an unsafe FFI declaration
        let ffi_toml = r#"
[foreign-library]
name = "dangerlib"

[[functions]]
name = "dangerous"
c_signature = "void dangerous(void)"
trust_level = "unsafe"
"#;
        std::fs::write(project_path.join("danger.ffi.toml"), ffi_toml).unwrap();

        let (manifest, _) = TorcManifest::find_and_load(&project_path).unwrap().unwrap();

        // Should fail due to policy
        let result =
            commands::ffi::bridge_from_c(&project_path, Some(&manifest), "danger.ffi.toml");
        assert!(result.is_err(), "unsafe should be rejected by policy");
    }

    /// Registry: add and remove dependencies.
    #[test]
    fn registry_add_remove() {
        let dir = tempfile::tempdir().unwrap();
        let project_path = dir.path().join("reg-test");
        commands::init::create_project(&project_path, "reg-test").unwrap();

        let (manifest, _) = TorcManifest::find_and_load(&project_path).unwrap().unwrap();

        // Add a dependency
        commands::registry::add(&project_path, &manifest, "torc-math", Some(">=1.0.0")).unwrap();

        // Re-read manifest and verify
        let content = std::fs::read_to_string(project_path.join("torc.toml")).unwrap();
        assert!(content.contains("torc-math"));

        // Remove it
        commands::registry::remove(&project_path, "torc-math").unwrap();

        // Verify it's gone
        let content = std::fs::read_to_string(project_path.join("torc.toml")).unwrap();
        assert!(!content.contains("torc-math"));
    }

    /// Registry: tree with no dependencies.
    #[test]
    fn registry_tree_no_deps() {
        let dir = tempfile::tempdir().unwrap();
        let project_path = dir.path().join("tree-test");
        commands::init::create_project(&project_path, "tree-test").unwrap();

        let (manifest, _) = TorcManifest::find_and_load(&project_path).unwrap().unwrap();
        commands::registry::tree(&project_path, &manifest).unwrap();
    }

    /// Registry: publish dry run.
    #[test]
    fn registry_publish_dry_run() {
        let dir = tempfile::tempdir().unwrap();
        let project_path = dir.path().join("pub-test");
        commands::init::create_project(&project_path, "pub-test").unwrap();

        let (manifest, _) = TorcManifest::find_and_load(&project_path).unwrap().unwrap();
        commands::registry::publish(&project_path, &manifest, true).unwrap();
    }

    /// Registry config in manifest.
    #[test]
    fn registry_manifest_config() {
        let toml_str = r#"
[project]
name = "reg-project"

[registry]
publish-to = "https://registry.torc-lang.org"
local-path = ".torc-registry"
reject-unsigned = true
"#;
        let manifest = TorcManifest::from_str(toml_str).unwrap();
        let reg = manifest.registry.unwrap();
        assert_eq!(
            reg.publish_to.as_deref(),
            Some("https://registry.torc-lang.org")
        );
        assert_eq!(reg.local_path.as_deref(), Some(".torc-registry"));
        assert_eq!(reg.reject_unsigned, Some(true));
    }

    /// FFI manifest with trust policy and declarations fields.
    #[test]
    fn ffi_manifest_extended_fields() {
        let toml_str = r#"
[project]
name = "ffi-project"

[ffi]
c_headers = ["include/bridge.h"]
abi = "C"
declarations = ["libm.ffi.toml", "libc.ffi.toml"]
exports = ["torc_main", "torc_init"]

[ffi.trust-policy]
allow_unsafe = false
require_audited = true
platform_trusted = ["libm", "libc"]
"#;
        let manifest = TorcManifest::from_str(toml_str).unwrap();
        let ffi = manifest.ffi.unwrap();
        assert_eq!(ffi.declarations.len(), 2);
        assert_eq!(ffi.exports.len(), 2);
        let tp = ffi.trust_policy.unwrap();
        assert_eq!(tp.allow_unsafe, Some(false));
        assert_eq!(tp.require_audited, Some(true));
        assert_eq!(tp.platform_trusted.len(), 2);
    }

    /// Decision: init creates decisions.tdg.
    #[test]
    fn decision_init() {
        let dir = tempfile::tempdir().unwrap();
        let project_path = dir.path().join("decision-test");
        commands::init::create_project(&project_path, "decision-test").unwrap();

        commands::decision::init(&project_path).unwrap();
        assert!(project_path.join("spec/decisions.tdg").is_file());
    }

    /// Decision: init is idempotent.
    #[test]
    fn decision_init_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        let project_path = dir.path().join("decision-idem");
        commands::init::create_project(&project_path, "decision-idem").unwrap();

        commands::decision::init(&project_path).unwrap();
        commands::decision::init(&project_path).unwrap(); // second call should not fail
    }

    /// Decision: status on empty graph.
    #[test]
    fn decision_status_empty() {
        let dir = tempfile::tempdir().unwrap();
        let project_path = dir.path().join("decision-status");
        commands::init::create_project(&project_path, "decision-status").unwrap();
        commands::decision::init(&project_path).unwrap();

        commands::decision::status(&project_path).unwrap();
    }

    /// Decision: add, list, show, commit, defer workflow.
    #[test]
    fn decision_full_workflow() {
        let dir = tempfile::tempdir().unwrap();
        let project_path = dir.path().join("decision-flow");
        commands::init::create_project(&project_path, "decision-flow").unwrap();
        commands::decision::init(&project_path).unwrap();

        // Add a decision
        commands::decision::add(
            &project_path,
            "PWM Frequency",
            "performance",
            Some("Select the PWM switching frequency"),
        )
        .unwrap();

        // Add another
        commands::decision::add(&project_path, "Control topology", "topology", None).unwrap();

        // List should show 2
        commands::decision::list(&project_path, None, None).unwrap();

        // Status should show 2 unexplored
        commands::decision::status(&project_path).unwrap();
    }

    /// Decision: show displays decision details.
    #[test]
    fn decision_show() {
        let dir = tempfile::tempdir().unwrap();
        let project_path = dir.path().join("decision-show");
        commands::init::create_project(&project_path, "decision-show").unwrap();
        commands::decision::init(&project_path).unwrap();

        commands::decision::add(
            &project_path,
            "Safety Mode",
            "safety",
            Some("Define safe state behavior"),
        )
        .unwrap();

        // Load graph to get the decision ID prefix
        let tdg_data = std::fs::read(project_path.join("spec/decisions.tdg")).unwrap();
        let tdg = torc_spec::TdgFile::from_bytes(&tdg_data).unwrap();
        let decision = tdg.graph.decisions().next().unwrap();
        let id_prefix = &decision.id.to_string()[..8];

        // Show should succeed and not error
        commands::decision::show(&project_path, id_prefix).unwrap();
    }

    /// Decision: commit transitions state and persists.
    #[test]
    fn decision_commit() {
        let dir = tempfile::tempdir().unwrap();
        let project_path = dir.path().join("decision-commit");
        commands::init::create_project(&project_path, "decision-commit").unwrap();
        commands::decision::init(&project_path).unwrap();

        commands::decision::add(&project_path, "Control Loop Rate", "performance", None).unwrap();

        // Get the decision ID
        let tdg_data = std::fs::read(project_path.join("spec/decisions.tdg")).unwrap();
        let tdg = torc_spec::TdgFile::from_bytes(&tdg_data).unwrap();
        let decision = tdg.graph.decisions().next().unwrap();
        let id_prefix = &decision.id.to_string()[..8];

        // Commit should succeed
        commands::decision::commit(&project_path, id_prefix, "20kHz", Some("Standard FOC rate"))
            .unwrap();

        // Verify the state persisted as COMMITTED
        let tdg_data = std::fs::read(project_path.join("spec/decisions.tdg")).unwrap();
        let tdg = torc_spec::TdgFile::from_bytes(&tdg_data).unwrap();
        let updated = tdg.graph.decisions().next().unwrap();
        assert_eq!(updated.state, torc_spec::DecisionState::Committed);
    }

    /// Decision: defer transitions state with provisional value.
    #[test]
    fn decision_defer() {
        let dir = tempfile::tempdir().unwrap();
        let project_path = dir.path().join("decision-defer");
        commands::init::create_project(&project_path, "decision-defer").unwrap();
        commands::decision::init(&project_path).unwrap();

        commands::decision::add(&project_path, "CAN Protocol Version", "communication", None)
            .unwrap();

        let tdg_data = std::fs::read(project_path.join("spec/decisions.tdg")).unwrap();
        let tdg = torc_spec::TdgFile::from_bytes(&tdg_data).unwrap();
        let decision = tdg.graph.decisions().next().unwrap();
        let id_prefix = &decision.id.to_string()[..8];

        // Defer with provisional value
        commands::decision::defer(&project_path, id_prefix, Some("CAN 2.0B"), None).unwrap();

        // Verify state persisted as DEFERRED
        let tdg_data = std::fs::read(project_path.join("spec/decisions.tdg")).unwrap();
        let tdg = torc_spec::TdgFile::from_bytes(&tdg_data).unwrap();
        let updated = tdg.graph.decisions().next().unwrap();
        assert_eq!(updated.state, torc_spec::DecisionState::Deferred);
    }

    /// Decision: list --state filter works correctly.
    #[test]
    fn decision_list_state_filter() {
        let dir = tempfile::tempdir().unwrap();
        let project_path = dir.path().join("decision-filter");
        commands::init::create_project(&project_path, "decision-filter").unwrap();
        commands::decision::init(&project_path).unwrap();

        // Add two decisions
        commands::decision::add(&project_path, "Decision A", "safety", None).unwrap();
        commands::decision::add(&project_path, "Decision B", "performance", None).unwrap();

        // Commit one
        let tdg_data = std::fs::read(project_path.join("spec/decisions.tdg")).unwrap();
        let tdg = torc_spec::TdgFile::from_bytes(&tdg_data).unwrap();
        let first = tdg.graph.decisions().next().unwrap();
        let id_prefix = &first.id.to_string()[..8];
        commands::decision::commit(&project_path, id_prefix, "yes", None).unwrap();

        // List with state filter should succeed (exercises the fixed != -> == path)
        commands::decision::list(&project_path, Some("committed"), None).unwrap();
        commands::decision::list(&project_path, Some("unexplored"), None).unwrap();

        // Verify via graph: exactly 1 committed, 1 unexplored
        let tdg_data = std::fs::read(project_path.join("spec/decisions.tdg")).unwrap();
        let tdg = torc_spec::TdgFile::from_bytes(&tdg_data).unwrap();
        let committed: Vec<_> = tdg
            .graph
            .decisions_by_state(torc_spec::DecisionState::Committed);
        let unexplored: Vec<_> = tdg
            .graph
            .decisions_by_state(torc_spec::DecisionState::Unexplored);
        assert_eq!(committed.len(), 1, "should have 1 committed decision");
        assert_eq!(unexplored.len(), 1, "should have 1 unexplored decision");
    }

    /// Verify with no TDG — unchanged behavior.
    #[test]
    fn verify_no_tdg_unchanged() {
        let dir = tempfile::tempdir().unwrap();
        let project_path = dir.path().join("verify-no-tdg");
        commands::init::create_project(&project_path, "verify-no-tdg").unwrap();

        // No decisions.tdg — verify should work exactly as before
        commands::verify::run(&project_path, None, None, false, None, None, false).unwrap();
    }

    /// Verify with TDG present — profile upgrade note.
    #[test]
    fn verify_with_tdg_tentative() {
        let dir = tempfile::tempdir().unwrap();
        let project_path = dir.path().join("verify-tdg");
        commands::init::create_project(&project_path, "verify-tdg").unwrap();

        // Create a TDG with a tentative decision
        commands::decision::init(&project_path).unwrap();
        commands::decision::add(&project_path, "Control method", "topology", None).unwrap();

        // Get the decision ID and transition to Tentative
        let tdg_data = std::fs::read(project_path.join("spec/decisions.tdg")).unwrap();
        let mut tdg = torc_spec::TdgFile::from_bytes(&tdg_data).unwrap();
        let decision = tdg.graph.decisions().next().unwrap();
        let id = decision.id;
        tdg.graph
            .transition(
                id,
                torc_spec::DecisionState::Tentative,
                torc_spec::DecisionValue::Provisional("FOC".into()),
                None,
            )
            .unwrap();
        let bytes = torc_spec::TdgFile::new(tdg.graph).to_bytes().unwrap();
        std::fs::write(project_path.join("spec/decisions.tdg"), bytes).unwrap();

        // Verify should succeed — profile is upgraded but no error
        commands::verify::run(&project_path, None, None, false, None, None, false).unwrap();
    }

    /// Build blocks on conflicted decisions.
    #[test]
    fn build_blocks_on_conflict() {
        let dir = tempfile::tempdir().unwrap();
        let project_path = dir.path().join("build-conflict");
        commands::init::create_project(&project_path, "build-conflict").unwrap();

        // Create a TDG with a conflicted decision
        commands::decision::init(&project_path).unwrap();
        commands::decision::add(&project_path, "Bus protocol", "comms", None).unwrap();

        let tdg_data = std::fs::read(project_path.join("spec/decisions.tdg")).unwrap();
        let mut tdg = torc_spec::TdgFile::from_bytes(&tdg_data).unwrap();
        let decision = tdg.graph.decisions().next().unwrap();
        let id = decision.id;
        // Unexplored → Exploring → Conflicted (valid path)
        tdg.graph
            .transition(
                id,
                torc_spec::DecisionState::Exploring,
                torc_spec::DecisionValue::Unresolved,
                None,
            )
            .unwrap();
        tdg.graph
            .transition(
                id,
                torc_spec::DecisionState::Conflicted,
                torc_spec::DecisionValue::Unresolved,
                None,
            )
            .unwrap();
        let bytes = torc_spec::TdgFile::new(tdg.graph).to_bytes().unwrap();
        std::fs::write(project_path.join("spec/decisions.tdg"), bytes).unwrap();

        // Build should fail
        let result = commands::build::run(
            &project_path,
            None,
            None,
            None,
            false,
            false,
            None,
            None,
            false,
        );
        assert!(
            result.is_err(),
            "build should be blocked by conflicted decisions"
        );
    }

    /// Build warns on unexplored safety decisions but doesn't block.
    #[test]
    fn build_warns_on_unexplored_safety() {
        let dir = tempfile::tempdir().unwrap();
        let project_path = dir.path().join("build-safety-warn");
        commands::init::create_project(&project_path, "build-safety-warn").unwrap();

        // Create a TDG with an unexplored safety decision
        commands::decision::init(&project_path).unwrap();
        commands::decision::add(&project_path, "Safety monitor", "safety", None).unwrap();

        // Build should succeed (warning only, no block)
        commands::build::run(
            &project_path,
            None,
            None,
            None,
            false,
            false,
            None,
            None,
            false,
        )
        .unwrap();
    }

    /// Inspect --view decision dispatches correctly.
    #[test]
    fn inspect_decision_view() {
        let dir = tempfile::tempdir().unwrap();
        let project_path = dir.path().join("inspect-decision");
        commands::init::create_project(&project_path, "inspect-decision").unwrap();

        // Create decisions.tdg
        commands::decision::init(&project_path).unwrap();
        commands::decision::add(&project_path, "PWM freq", "performance", None).unwrap();

        // inspect --view decision should succeed
        commands::inspect::run(&project_path, Some("decision"), None, None, None).unwrap();
    }

    /// Inspect --view decision with no TDG prints helpful message.
    #[test]
    fn inspect_decision_no_tdg() {
        let dir = tempfile::tempdir().unwrap();
        let project_path = dir.path().join("inspect-no-tdg");
        commands::init::create_project(&project_path, "inspect-no-tdg").unwrap();

        // No decisions.tdg — should not error, just print helpful message
        commands::inspect::run(&project_path, Some("decision"), None, None, None).unwrap();
    }

    /// Decision: commit records history with timestamp.
    #[test]
    fn decision_history_has_timestamp() {
        let dir = tempfile::tempdir().unwrap();
        let project_path = dir.path().join("decision-ts");
        commands::init::create_project(&project_path, "decision-ts").unwrap();
        commands::decision::init(&project_path).unwrap();

        commands::decision::add(&project_path, "Timing", "performance", None).unwrap();

        let tdg_data = std::fs::read(project_path.join("spec/decisions.tdg")).unwrap();
        let tdg = torc_spec::TdgFile::from_bytes(&tdg_data).unwrap();
        let decision = tdg.graph.decisions().next().unwrap();
        let id_prefix = &decision.id.to_string()[..8];

        commands::decision::commit(&project_path, id_prefix, "10kHz", None).unwrap();

        // Load and check history has a timestamp
        let tdg_data = std::fs::read(project_path.join("spec/decisions.tdg")).unwrap();
        let tdg = torc_spec::TdgFile::from_bytes(&tdg_data).unwrap();
        let history = tdg.graph.history_for(decision.id);
        assert_eq!(history.len(), 1, "should have 1 history entry");
        assert!(
            history[0].timestamp.contains('T') && history[0].timestamp.ends_with('Z'),
            "timestamp should be ISO 8601: {}",
            history[0].timestamp
        );
    }
}
