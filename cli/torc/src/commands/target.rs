//! `torc target` — platform listing and description.

use anyhow::{bail, Result};

use crate::manifest::{builtin_targets, resolve_target};

/// List all available built-in platforms.
pub fn list() -> Result<()> {
    println!("Built-in platforms:");
    println!();
    for (name, description) in builtin_targets() {
        println!("  {name:<25} {description}");
    }
    println!();
    println!("Use 'torc target describe <name>' for details.");
    Ok(())
}

/// Describe a specific platform in detail.
pub fn describe(name: &str) -> Result<()> {
    let platform = match resolve_target(name) {
        Some(p) => p,
        None => bail!("unknown target: '{name}'. Use 'torc target list' to see available targets."),
    };

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

/// Stub for `torc target add`.
pub fn add(name: &str) -> Result<()> {
    println!(
        "torc target add '{name}': not yet implemented (Phase 8 Pass 2)"
    );
    println!("Planned: parse custom .target.toml files from targets/ directory.");
    Ok(())
}

/// Stub for `torc target validate`.
pub fn validate(name: &str) -> Result<()> {
    println!(
        "torc target validate '{name}': not yet implemented (Phase 8 Pass 2)"
    );
    println!("Planned: validate .target.toml against the platform model schema.");
    Ok(())
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
        assert!(describe("linux-x86_64").is_ok());
    }

    #[test]
    fn describe_unknown_target() {
        assert!(describe("nonexistent").is_err());
    }
}
