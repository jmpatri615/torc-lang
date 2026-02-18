//! `torc target` — platform listing, description, creation, and validation.

use std::path::Path;

use anyhow::{bail, Result};

use crate::manifest::{builtin_targets, resolve_target};

/// List all available platforms (built-in + discovered custom).
pub fn list(project_dir: Option<&Path>) -> Result<()> {
    println!("Built-in platforms:");
    println!();
    for (name, description) in builtin_targets() {
        println!("  {name:<25} {description}");
    }

    // Discover custom targets
    if let Some(dir) = project_dir {
        if let Ok(custom) = torc_targets::discover_targets(dir) {
            if !custom.is_empty() {
                println!();
                println!("Custom platforms (targets/):");
                println!();
                for (name, path) in &custom {
                    let status = match torc_targets::load_platform_toml(path) {
                        Ok(_) => "ok".to_string(),
                        Err(e) => format!("parse error: {e}"),
                    };
                    println!("  {name:<25} [{status}]");
                }
            }
        }
    }

    println!();
    println!("Use 'torc target describe <name>' for details.");
    Ok(())
}

/// Describe a specific platform in detail.
pub fn describe(name: &str, project_dir: Option<&Path>, format: Option<&str>) -> Result<()> {
    let platform = match resolve_target(name, project_dir) {
        Some(p) => p,
        None => bail!("unknown target: '{name}'. Use 'torc target list' to see available targets."),
    };

    // TOML output mode
    if format == Some("toml") {
        let toml_str = torc_targets::platform_to_toml(&platform)
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        print!("{toml_str}");
        return Ok(());
    }

    println!("=== Platform: {} ===", platform.name);
    println!("Version: {}", platform.version);
    println!();

    println!("--- ISA ---");
    println!("  Name:       {}", platform.isa.name);
    println!("  Word size:  {} bits", platform.isa.word_size);
    println!("  Endianness: {:?}", platform.isa.endianness);
    println!("  Calling conventions:");
    for cc in &platform.isa.calling_conventions {
        println!("    {}", cc.name);
    }
    println!("  Registers:");
    for reg in &platform.isa.register_classes {
        println!(
            "    {}: {} x {} bits",
            reg.name, reg.count, reg.width_bits
        );
    }
    println!();

    println!("--- Microarchitecture ---");
    println!("  Name:     {}", platform.microarch.name);
    println!(
        "  Pipeline: {} stages",
        platform.microarch.pipeline.stages
    );
    println!();

    println!("--- Environment ---");
    println!("  Type: {:?}", platform.environment.env_type);
    println!("  Binary format: {:?}", platform.environment.binary_format);
    println!("  Memory regions:");
    for region in &platform.environment.memory_regions {
        println!(
            "    {}: 0x{:08X} - 0x{:08X} ({} bytes) [{}]",
            region.name,
            region.base_address,
            region.base_address + region.size_bytes,
            region.size_bytes,
            if region.executable { "RX" } else { "RW" },
        );
    }
    println!();

    println!("--- Resources ---");
    let rc = platform.resource_constraints();
    println!("  Flash: {} bytes", rc.flash_bytes);
    println!("  RAM:   {} bytes", rc.ram_bytes);
    if let Some(stack) = rc.max_stack_bytes {
        println!("  Stack: {} bytes", stack);
    }
    if let Some(clock) = rc.clock_hz {
        println!("  Clock: {} Hz", clock);
    }

    Ok(())
}

/// Create a new custom target definition.
pub fn add(name: &str, project_dir: &Path) -> Result<()> {
    let targets_dir = project_dir.join("targets");
    std::fs::create_dir_all(&targets_dir)?;

    let target_path = targets_dir.join(format!("{name}.target.toml"));
    if target_path.exists() {
        bail!(
            "target already exists: {}. Edit it directly or remove it first.",
            target_path.display()
        );
    }

    let template = torc_targets::generate_template(name)
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    std::fs::write(&target_path, &template)?;

    println!("Created target: {}", target_path.display());
    println!("Edit this file to customize the platform model.");
    Ok(())
}

/// Validate a target platform definition.
pub fn validate(name: &str, project_dir: Option<&Path>) -> Result<()> {
    let platform = match resolve_target(name, project_dir) {
        Some(p) => p,
        None => bail!("unknown target: '{name}'. Use 'torc target list' to see available targets."),
    };

    match torc_targets::validate_platform(&platform) {
        Ok(()) => {
            println!("Target '{name}' is valid.");
            Ok(())
        }
        Err(issues) => {
            let error_count = issues.iter().filter(|i| i.severity == "error").count();
            let warning_count = issues.iter().filter(|i| i.severity == "warning").count();

            for issue in &issues {
                println!("{}: {}", issue.severity, issue.message);
            }
            println!();
            println!(
                "{} error(s), {} warning(s)",
                error_count, warning_count
            );

            if error_count > 0 {
                bail!("target '{name}' has validation errors");
            }
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn list_includes_builtins() {
        let targets = builtin_targets();
        assert!(targets.iter().any(|(name, _)| *name == "linux-x86_64"));
        assert!(targets
            .iter()
            .any(|(name, _)| *name == "stm32f407-discovery"));
    }

    #[test]
    fn describe_known_target() {
        assert!(describe("linux-x86_64", None, None).is_ok());
    }

    #[test]
    fn describe_unknown_target() {
        assert!(describe("nonexistent", None, None).is_err());
    }

    #[test]
    fn describe_toml_format() {
        assert!(describe("linux-x86_64", None, Some("toml")).is_ok());
    }

    #[test]
    fn add_creates_file() {
        let dir = tempfile::tempdir().unwrap();
        add("my-board", dir.path()).unwrap();
        assert!(dir.path().join("targets/my-board.target.toml").is_file());

        // Verify it's valid TOML that parses to a Platform
        let platform = torc_targets::load_platform_toml(
            &dir.path().join("targets/my-board.target.toml"),
        )
        .unwrap();
        assert_eq!(platform.name, "my-board");
    }

    #[test]
    fn add_refuses_existing() {
        let dir = tempfile::tempdir().unwrap();
        add("dup-board", dir.path()).unwrap();
        assert!(add("dup-board", dir.path()).is_err());
    }

    #[test]
    fn validate_builtin_succeeds() {
        assert!(validate("linux-x86_64", None).is_ok());
        assert!(validate("stm32f407-discovery", None).is_ok());
    }

    #[test]
    fn validate_custom_succeeds() {
        let dir = tempfile::tempdir().unwrap();
        add("valid-board", dir.path()).unwrap();
        assert!(validate("valid-board", Some(dir.path())).is_ok());
    }

    #[test]
    fn validate_detects_errors() {
        let dir = tempfile::tempdir().unwrap();
        let targets_dir = dir.path().join("targets");
        std::fs::create_dir_all(&targets_dir).unwrap();

        // Write a platform with mismatched isa_ref
        let mut platform = torc_targets::Platform::generic_linux_x86_64();
        platform.name = "bad-board".into();
        platform.microarch.isa_ref = "ARMv7-M".into();
        let toml_str = torc_targets::platform_to_toml(&platform).unwrap();
        std::fs::write(targets_dir.join("bad-board.target.toml"), &toml_str).unwrap();

        assert!(validate("bad-board", Some(dir.path())).is_err());
    }

    #[test]
    fn resolve_custom_target() {
        let dir = tempfile::tempdir().unwrap();
        add("custom-chip", dir.path()).unwrap();

        let platform = resolve_target("custom-chip", Some(dir.path()));
        assert!(platform.is_some());
        assert_eq!(platform.unwrap().name, "custom-chip");
    }

    #[test]
    fn list_shows_custom() {
        let dir = tempfile::tempdir().unwrap();
        add("board-x", dir.path()).unwrap();
        // Just verify it doesn't error
        assert!(list(Some(dir.path())).is_ok());
    }
}
