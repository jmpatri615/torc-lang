//! FFI bridge CLI commands.

use std::path::Path;

use anyhow::{bail, Context, Result};

use crate::manifest::{FfiTrustPolicy, TorcManifest};

/// Run the `torc ffi bridge --from-c <file>` workflow.
///
/// Parses the `.ffi.toml` declaration, applies trust policy, generates a bridge
/// graph, and writes it to `graph/ffi/<library>.trc`.
pub fn bridge_from_c(
    project_dir: &Path,
    manifest: Option<&TorcManifest>,
    ffi_toml_path: &str,
) -> Result<()> {
    let ffi_path = project_dir.join(ffi_toml_path);
    if !ffi_path.is_file() {
        bail!("FFI declaration file not found: {}", ffi_path.display());
    }

    let decl = torc_ffi::FfiDeclaration::load(&ffi_path)
        .with_context(|| format!("loading {}", ffi_path.display()))?;

    // Apply trust policy from manifest
    let policy = resolve_policy(manifest);
    policy
        .check_declaration(&decl)
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    let active = decl.active_functions();
    if active.is_empty() {
        println!("No active functions in declaration — nothing to generate.");
        return Ok(());
    }

    // Resolve word size from manifest target platform (default 64-bit)
    let word_bits = manifest
        .and_then(|m| m.default_target())
        .and_then(|name| crate::manifest::resolve_target(name, Some(project_dir)))
        .map(|p| p.isa.word_size as u8)
        .unwrap_or(64);

    let graph = torc_ffi::generate_bridge(&decl, word_bits).map_err(|e| anyhow::anyhow!("{e}"))?;

    // Serialize to TRC
    let trc_file = torc_trc::TrcFile::new(graph);
    let trc_data = trc_file
        .to_bytes()
        .with_context(|| "serializing bridge graph")?;

    // Write to graph/ffi/<library>.trc
    let ffi_dir = project_dir.join("graph").join("ffi");
    std::fs::create_dir_all(&ffi_dir)?;
    let output_path = ffi_dir.join(format!("{}.trc", decl.foreign_library.name));
    std::fs::write(&output_path, &trc_data)?;

    println!(
        "Generated FFI bridge for '{}' ({} functions) → {}",
        decl.foreign_library.name,
        active.len(),
        output_path.display()
    );

    Ok(())
}

/// Run the `torc ffi bridge --to-c` workflow.
///
/// Loads the main graph (or specified input), finds exported functions,
/// and generates a C header file.
pub fn bridge_to_c(project_dir: &Path, input: Option<&str>, output: Option<&str>) -> Result<()> {
    let input_path = project_dir.join(input.unwrap_or("graph/main.trc"));
    if !input_path.is_file() {
        bail!("Input file not found: {}", input_path.display());
    }

    let trc_data =
        std::fs::read(&input_path).with_context(|| format!("reading {}", input_path.display()))?;

    let trc_file = torc_trc::TrcFile::from_bytes(&trc_data)
        .with_context(|| format!("deserializing {}", input_path.display()))?;
    let graph = trc_file.graph;

    // Derive guard name from output filename or project
    let output_path = match output {
        Some(p) => project_dir.join(p),
        None => project_dir.join("include").join("torc_exports.h"),
    };

    let guard_name = output_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("torc_exports");

    let header =
        torc_ffi::generate_c_header(&graph, guard_name).map_err(|e| anyhow::anyhow!("{e}"))?;

    // Write header
    if let Some(parent) = output_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&output_path, &header)?;

    println!("Generated C header → {}", output_path.display());

    Ok(())
}

/// Resolve trust policy from manifest configuration.
fn resolve_policy(manifest: Option<&TorcManifest>) -> torc_ffi::TrustPolicy {
    let ffi_policy = manifest
        .and_then(|m| m.ffi.as_ref())
        .and_then(|f| f.trust_policy.as_ref());

    match ffi_policy {
        Some(tp) => to_trust_policy(tp),
        None => torc_ffi::TrustPolicy::default(),
    }
}

fn to_trust_policy(tp: &FfiTrustPolicy) -> torc_ffi::TrustPolicy {
    torc_ffi::TrustPolicy {
        allow_unsafe: tp.allow_unsafe.unwrap_or(false),
        require_audited: tp.require_audited.unwrap_or(false),
        platform_trusted: tp.platform_trusted.clone(),
    }
}
