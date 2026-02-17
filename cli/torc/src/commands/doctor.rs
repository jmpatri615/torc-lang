//! `torc doctor` — toolchain diagnostics.

use std::path::Path;
use std::process::Command;

use anyhow::Result;

use crate::manifest::TorcManifest;

/// Print toolchain diagnostic information.
pub fn run(project_dir: &Path, target: Option<&str>) -> Result<()> {
    println!("=== Torc Doctor ===");
    println!();

    // Version info
    println!("Torc version: {}", env!("CARGO_PKG_VERSION"));
    println!();

    // Feature support
    println!("--- Feature Support ---");
    println!(
        "  LLVM codegen:  {}",
        if torc_materialize::LLVM_AVAILABLE {
            "available"
        } else {
            "not compiled (rebuild with --features llvm)"
        }
    );
    println!(
        "  Z3 SMT solver: {}",
        if torc_verify::Z3_AVAILABLE {
            "available"
        } else {
            "not compiled (rebuild with --features z3)"
        }
    );
    println!();

    // System tools
    println!("--- System Tools ---");
    print_tool_status("cc", &["--version"]);
    print_tool_status("ld", &["--version"]);
    println!();

    // Project status
    println!("--- Project Status ---");
    match TorcManifest::find_and_load(project_dir) {
        Ok(Some((manifest, dir))) => {
            println!("  torc.toml: found at {}", dir.display());
            println!("  Project:   {}", manifest.project.name);
            println!("  Version:   {}", manifest.project.version);
            if let Some(default) = manifest.default_target() {
                println!("  Default target: {default}");
            }
        }
        Ok(None) => {
            println!("  torc.toml: not found");
        }
        Err(e) => {
            println!("  torc.toml: error — {e}");
        }
    }

    // Target info
    if let Some(target_name) = target {
        println!();
        println!("--- Target: {target_name} ---");
        match crate::manifest::resolve_target(target_name) {
            Some(platform) => {
                println!("  ISA:    {}", platform.isa.name);
                println!("  Flash:  {} bytes", platform.flash_size_bytes);
                println!("  SRAM:   {} bytes", platform.sram_size_bytes);
            }
            None => {
                println!("  Unknown built-in target: {target_name}");
            }
        }
    }

    Ok(())
}

fn print_tool_status(name: &str, args: &[&str]) {
    match Command::new(name).args(args).output() {
        Ok(output) => {
            let version = String::from_utf8_lossy(&output.stdout);
            let first_line = version.lines().next().unwrap_or("(unknown version)");
            println!("  {name}: {first_line}");
        }
        Err(_) => {
            println!("  {name}: not found");
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn doctor_runs_without_error() {
        let dir = tempfile::tempdir().unwrap();
        super::run(dir.path(), None).unwrap();
    }
}
