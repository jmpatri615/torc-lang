//! Registry CLI commands: add, remove, update, tree, publish, audit.

use std::collections::HashMap;
use std::path::Path;

use anyhow::{bail, Context, Result};

use crate::manifest::TorcManifest;

/// Run `torc add <module> [--version <req>]`.
///
/// Resolves the module against the local registry (or cache) and adds it
/// to the project's `[dependencies]` in `torc.toml`.
pub fn add(project_dir: &Path, manifest: &TorcManifest, name: &str, version: Option<&str>) -> Result<()> {
    let manifest_path = project_dir.join("torc.toml");
    if !manifest_path.is_file() {
        bail!("no torc.toml found in {}", project_dir.display());
    }

    let version_req = version.unwrap_or("*");

    // Parse the version requirement to validate it
    torc_registry::parse_requirement(version_req)
        .with_context(|| format!("invalid version requirement: {version_req}"))?;

    // Check if already a dependency
    if manifest.dependencies.contains_key(name) {
        println!("Dependency '{name}' already present — updating version to '{version_req}'");
    }

    // Update torc.toml by reading, modifying, and rewriting
    let content = std::fs::read_to_string(&manifest_path)?;
    let mut doc: toml::Table = content.parse().context("parsing torc.toml")?;

    let deps = doc
        .entry("dependencies")
        .or_insert_with(|| toml::Value::Table(toml::Table::new()));

    if let toml::Value::Table(deps_table) = deps {
        deps_table.insert(
            name.to_string(),
            toml::Value::String(version_req.to_string()),
        );
    }

    std::fs::write(&manifest_path, doc.to_string())?;

    println!("Added dependency: {name} = \"{version_req}\"");
    Ok(())
}

/// Run `torc remove <module>`.
///
/// Removes the dependency from `torc.toml`.
pub fn remove(project_dir: &Path, name: &str) -> Result<()> {
    let manifest_path = project_dir.join("torc.toml");
    if !manifest_path.is_file() {
        bail!("no torc.toml found in {}", project_dir.display());
    }

    let content = std::fs::read_to_string(&manifest_path)?;
    let mut doc: toml::Table = content.parse().context("parsing torc.toml")?;

    let removed = if let Some(toml::Value::Table(deps)) = doc.get_mut("dependencies") {
        deps.remove(name).is_some()
    } else {
        false
    };

    if !removed {
        bail!("dependency '{name}' not found in torc.toml");
    }

    std::fs::write(&manifest_path, doc.to_string())?;
    println!("Removed dependency: {name}");
    Ok(())
}

/// Run `torc update [<module>]`.
///
/// Resolves dependencies and reports the resolution result.
pub fn update(project_dir: &Path, manifest: &TorcManifest, module: Option<&str>) -> Result<()> {
    let registry = resolve_registry(project_dir);

    let deps_to_resolve: HashMap<String, String> = if let Some(name) = module {
        let req = manifest
            .dependencies
            .get(name)
            .ok_or_else(|| anyhow::anyhow!("dependency '{name}' not found in torc.toml"))?;
        let version_str = match req {
            crate::manifest::DependencySpec::Version(v) => v.clone(),
            crate::manifest::DependencySpec::Detailed { version, .. } => {
                version.clone().unwrap_or_else(|| "*".to_string())
            }
        };
        let mut map = HashMap::new();
        map.insert(name.to_string(), version_str);
        map
    } else {
        flatten_deps(&manifest.dependencies)
    };

    if deps_to_resolve.is_empty() {
        println!("No dependencies to resolve.");
        return Ok(());
    }

    match torc_registry::resolve(&deps_to_resolve, &registry) {
        Ok(result) => {
            println!("Resolved {} dependencies:", result.lock.len());
            for entry in &result.lock {
                println!("  {} v{}", entry.name, entry.version);
            }
            Ok(())
        }
        Err(e) => bail!("resolution failed: {e}"),
    }
}

/// Run `torc tree`.
///
/// Displays the dependency tree.
pub fn tree(project_dir: &Path, manifest: &TorcManifest) -> Result<()> {
    let registry = resolve_registry(project_dir);
    let deps = flatten_deps(&manifest.dependencies);

    if deps.is_empty() {
        println!("{} v{}", manifest.project.name, manifest.project.version);
        println!("\nNo dependencies.");
        return Ok(());
    }

    match torc_registry::resolve(&deps, &registry) {
        Ok(result) => {
            let output = torc_registry::format_tree(
                &manifest.project.name,
                &manifest.project.version,
                &result,
            );
            print!("{output}");
            Ok(())
        }
        Err(e) => bail!("resolution failed: {e}"),
    }
}

/// Run `torc publish [--dry-run]`.
pub fn publish(project_dir: &Path, manifest: &TorcManifest, dry_run: bool) -> Result<()> {
    let registry = resolve_registry(project_dir);

    // Read the main TRC file
    let trc_path = project_dir.join("graph/main.trc");
    if !trc_path.is_file() {
        bail!("no graph/main.trc found — nothing to publish");
    }
    let trc_data = std::fs::read(&trc_path).context("reading graph/main.trc")?;

    // Build a module manifest from the project manifest
    let mut module_manifest = format!(
        "[module]\nname = \"{}\"\nversion = \"{}\"\n",
        manifest.project.name, manifest.project.version,
    );

    if let Some(desc) = &manifest.project.description {
        module_manifest.push_str(&format!("description = \"{desc}\"\n"));
    }

    if !manifest.project.authors.is_empty() {
        let authors_str = manifest
            .project
            .authors
            .iter()
            .map(|a| format!("\"{a}\""))
            .collect::<Vec<_>>()
            .join(", ");
        module_manifest.push_str(&format!("authors = [{authors_str}]\n"));
    }

    if let Some(license) = &manifest.project.license {
        module_manifest.push_str(&format!("license = \"{license}\"\n"));
    }

    // Include dependencies so transitive resolution works
    if !manifest.dependencies.is_empty() {
        module_manifest.push_str("\n[dependencies]\n");
        for (name, spec) in &manifest.dependencies {
            let version_str = match spec {
                crate::manifest::DependencySpec::Version(v) => v.clone(),
                crate::manifest::DependencySpec::Detailed { version, .. } => {
                    version.clone().unwrap_or_else(|| "*".to_string())
                }
            };
            module_manifest.push_str(&format!("{name} = \"{version_str}\"\n"));
        }
    }

    let options = torc_registry::PublishOptions {
        dry_run,
        allow_no_license: false,
    };

    torc_registry::publish_module(&module_manifest, &trc_data, &registry, &options)
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    if dry_run {
        println!(
            "Dry run: {} v{} is ready to publish.",
            manifest.project.name, manifest.project.version
        );
    } else {
        println!(
            "Published {} v{} to local registry.",
            manifest.project.name, manifest.project.version
        );
    }

    Ok(())
}

/// Run `torc audit`.
pub fn audit(project_dir: &Path, manifest: &TorcManifest) -> Result<()> {
    let registry = resolve_registry(project_dir);
    let deps = flatten_deps(&manifest.dependencies);

    if deps.is_empty() {
        println!("No dependencies to audit.");
        return Ok(());
    }

    let resolution = torc_registry::resolve(&deps, &registry)
        .map_err(|e| anyhow::anyhow!("resolution failed: {e}"))?;

    let report = torc_registry::audit(&resolution, &registry)
        .map_err(|e| anyhow::anyhow!("audit failed: {e}"))?;

    print!("{}", torc_registry::format_report(&report));
    Ok(())
}

/// Flatten DependencySpec map to simple name → version-string map.
fn flatten_deps(
    deps: &HashMap<String, crate::manifest::DependencySpec>,
) -> HashMap<String, String> {
    deps.iter()
        .map(|(name, spec)| {
            let version = match spec {
                crate::manifest::DependencySpec::Version(v) => v.clone(),
                crate::manifest::DependencySpec::Detailed { version, .. } => {
                    version.clone().unwrap_or_else(|| "*".to_string())
                }
            };
            (name.clone(), version)
        })
        .collect()
}

/// Resolve the local registry path for the project.
///
/// Uses `.torc-registry/` in the project directory as the local registry.
fn resolve_registry(project_dir: &Path) -> torc_registry::LocalRegistry {
    let registry_dir = project_dir.join(".torc-registry");
    torc_registry::LocalRegistry::new(registry_dir)
}
