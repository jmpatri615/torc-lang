//! TOML parsing, serialization, validation, and discovery for platform definitions.
//!
//! Platform definitions are stored as `.target.toml` files in the `targets/` directory
//! of a Torc project. This module provides functions to load, validate, serialize,
//! and discover these files.

use std::path::{Path, PathBuf};

use crate::error::{Result, TargetError};
use crate::platform::Platform;

/// A validation issue found in a platform definition.
#[derive(Debug, Clone)]
pub struct ValidationIssue {
    /// Severity: "error" or "warning".
    pub severity: &'static str,
    /// Human-readable description.
    pub message: String,
}

/// Load a platform from a `.target.toml` file.
pub fn load_platform_toml(path: &Path) -> Result<Platform> {
    if !path.exists() {
        return Err(TargetError::NotFound {
            path: path.to_path_buf(),
        });
    }
    let content = std::fs::read_to_string(path)?;
    parse_platform_toml(&content)
}

/// Parse a platform from a TOML string.
pub fn parse_platform_toml(toml_str: &str) -> Result<Platform> {
    let platform: Platform = toml::from_str(toml_str)?;
    Ok(platform)
}

/// Serialize a platform to pretty TOML.
pub fn platform_to_toml(platform: &Platform) -> Result<String> {
    let toml_str = toml::to_string_pretty(platform)?;
    Ok(toml_str)
}

/// Validate a platform definition for structural correctness.
///
/// Returns `Ok(())` if valid, or `Err(issues)` with a list of problems.
pub fn validate_platform(platform: &Platform) -> std::result::Result<(), Vec<ValidationIssue>> {
    let mut issues = Vec::new();

    // 1. ISA word size is power of 2 (8/16/32/64/128)
    let valid_word_sizes = [8, 16, 32, 64, 128];
    if !valid_word_sizes.contains(&platform.isa.word_size) {
        issues.push(ValidationIssue {
            severity: "error",
            message: format!(
                "ISA word size {} is not a valid power of 2 (expected 8, 16, 32, 64, or 128)",
                platform.isa.word_size
            ),
        });
    }

    // 2. Address space >= word size
    if platform.isa.address_space < platform.isa.word_size {
        issues.push(ValidationIssue {
            severity: "error",
            message: format!(
                "ISA address space ({} bits) is smaller than word size ({} bits)",
                platform.isa.address_space, platform.isa.word_size
            ),
        });
    }

    // 3. At least one memory region exists
    if platform.environment.memory_regions.is_empty() {
        issues.push(ValidationIssue {
            severity: "error",
            message: "environment has no memory regions".into(),
        });
    }

    // 4. Memory regions don't overlap (pairwise check)
    let regions = &platform.environment.memory_regions;
    for i in 0..regions.len() {
        for j in (i + 1)..regions.len() {
            let a = &regions[i];
            let b = &regions[j];
            let a_end = a.base_address.saturating_add(a.size_bytes);
            let b_end = b.base_address.saturating_add(b.size_bytes);
            if a.base_address < b_end && b.base_address < a_end {
                issues.push(ValidationIssue {
                    severity: "error",
                    message: format!(
                        "memory regions '{}' (0x{:X}..0x{:X}) and '{}' (0x{:X}..0x{:X}) overlap",
                        a.name, a.base_address, a_end, b.name, b.base_address, b_end
                    ),
                });
            }
        }
    }

    // 5. flash_size_bytes matches environment.total_flash()
    let total_flash = platform.environment.total_flash();
    if platform.flash_size_bytes != total_flash {
        issues.push(ValidationIssue {
            severity: "warning",
            message: format!(
                "flash-size-bytes ({}) does not match environment total flash ({})",
                platform.flash_size_bytes, total_flash
            ),
        });
    }

    // 6. sram_size_bytes matches environment.total_ram()
    let total_ram = platform.environment.total_ram();
    if platform.sram_size_bytes != total_ram {
        issues.push(ValidationIssue {
            severity: "warning",
            message: format!(
                "sram-size-bytes ({}) does not match environment total RAM ({})",
                platform.sram_size_bytes, total_ram
            ),
        });
    }

    // 7. microarch.isa_ref matches isa.name
    if platform.microarch.isa_ref != platform.isa.name {
        issues.push(ValidationIssue {
            severity: "error",
            message: format!(
                "microarch isa-ref '{}' does not match ISA name '{}'",
                platform.microarch.isa_ref, platform.isa.name
            ),
        });
    }

    // 8. At least one calling convention exists
    if platform.isa.calling_conventions.is_empty() {
        issues.push(ValidationIssue {
            severity: "error",
            message: "ISA has no calling conventions".into(),
        });
    }

    // 9. Environment calling_convention references a valid ISA convention
    if !platform.isa.calling_conventions.is_empty()
        && platform
            .isa
            .calling_convention(&platform.environment.calling_convention)
            .is_none()
    {
        issues.push(ValidationIssue {
            severity: "error",
            message: format!(
                "environment calling convention '{}' not found in ISA calling conventions",
                platform.environment.calling_convention
            ),
        });
    }

    // 10. Register class counts are non-zero
    for rc in &platform.isa.register_classes {
        if rc.count == 0 {
            issues.push(ValidationIssue {
                severity: "error",
                message: format!("register class '{}' has count 0", rc.name),
            });
        }
    }

    if issues.is_empty() {
        Ok(())
    } else {
        Err(issues)
    }
}

/// Generate a template `.target.toml` for a new platform.
///
/// Seeds from linux-x86_64 with the given custom name.
pub fn generate_template(name: &str) -> Result<String> {
    let mut platform = Platform::generic_linux_x86_64();
    platform.name = name.into();
    platform.version = "0.1.0".into();
    platform_to_toml(&platform)
}

/// Discover all `.target.toml` files in a project's `targets/` directory.
///
/// Returns a list of (target_name, file_path) pairs.
pub fn discover_targets(project_dir: &Path) -> Result<Vec<(String, PathBuf)>> {
    let targets_dir = project_dir.join("targets");
    if !targets_dir.is_dir() {
        return Ok(Vec::new());
    }

    let mut targets = Vec::new();
    let entries = std::fs::read_dir(&targets_dir)?;
    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        if let Some(file_name) = path.file_name().and_then(|n| n.to_str()) {
            if file_name.ends_with(".target.toml") {
                let name = file_name.strip_suffix(".target.toml").unwrap().to_string();
                targets.push((name, path));
            }
        }
    }
    targets.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(targets)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::Platform;

    #[test]
    fn round_trip_linux() {
        let original = Platform::generic_linux_x86_64();
        let toml_str = platform_to_toml(&original).unwrap();
        let parsed = parse_platform_toml(&toml_str).unwrap();
        assert_eq!(original, parsed);
    }

    #[test]
    fn round_trip_stm32() {
        let original = Platform::stm32f407_discovery();
        let toml_str = platform_to_toml(&original).unwrap();
        let parsed = parse_platform_toml(&toml_str).unwrap();
        assert_eq!(original, parsed);
    }

    #[test]
    fn parse_minimal_toml() {
        let toml_str = r#"
name = "minimal-arm"
version = "1.0"
flash-size-bytes = 1024
sram-size-bytes = 512
default-stack-size = 128

[isa]
name = "ARMv7-M"
version = "v7-M"
endianness = "little"
word-size = 32
address-space = 32
register-classes = [{ name = "gpr", count = 13, width-bits = 32 }]
calling-conventions = [{ name = "AAPCS", argument-registers = ["r0"], return-registers = ["r0"], callee-saved = ["r4"], stack-alignment = 8 }]
extensions = []

[microarch]
name = "Cortex-M4"
version = "r0p1"
isa-ref = "ARMv7-M"
extensions = []
deterministic-timing = true

[microarch.pipeline]
stages = 3
branch-penalty-cycles = 3
load-use-penalty-cycles = 1

[microarch.memory-timing]
bus-width-bits = 32
sram-wait-states = 0

[environment]
name = "bare-metal"
version = "1.0"
env-type = "bare-metal"
has-os = false
has-heap = false
has-mmu = false
abi-name = "EABI"
calling-convention = "AAPCS"
binary-format = "elf32"

[[environment.memory-regions]]
name = "FLASH"
base-address = 0
size-bytes = 1024
readable = true
writable = false
executable = true

[[environment.memory-regions]]
name = "SRAM"
base-address = 4096
size-bytes = 512
readable = true
writable = true
executable = false
"#;
        let platform = parse_platform_toml(toml_str).unwrap();
        assert_eq!(platform.name, "minimal-arm");
        assert_eq!(platform.isa.word_size, 32);
        assert_eq!(platform.environment.memory_regions.len(), 2);
    }

    #[test]
    fn parse_invalid_returns_error() {
        assert!(parse_platform_toml("this is not valid toml [[[").is_err());
    }

    #[test]
    fn parse_missing_field_returns_error() {
        let toml_str = r#"
name = "incomplete"
"#;
        assert!(parse_platform_toml(toml_str).is_err());
    }

    #[test]
    fn validate_valid_linux() {
        let platform = Platform::generic_linux_x86_64();
        assert!(validate_platform(&platform).is_ok());
    }

    #[test]
    fn validate_valid_stm32() {
        let platform = Platform::stm32f407_discovery();
        assert!(validate_platform(&platform).is_ok());
    }

    #[test]
    fn validate_overlapping_regions() {
        let mut platform = Platform::stm32f407_discovery();
        // Make SRAM overlap with FLASH
        platform.environment.memory_regions[1].base_address =
            platform.environment.memory_regions[0].base_address;
        let issues = validate_platform(&platform).unwrap_err();
        assert!(issues.iter().any(|i| i.message.contains("overlap")));
    }

    #[test]
    fn validate_mismatched_isa_ref() {
        let mut platform = Platform::generic_linux_x86_64();
        platform.microarch.isa_ref = "ARMv7-M".into();
        let issues = validate_platform(&platform).unwrap_err();
        assert!(issues.iter().any(|i| i.message.contains("isa-ref")));
    }

    #[test]
    fn validate_bad_word_size() {
        let mut platform = Platform::generic_linux_x86_64();
        platform.isa.word_size = 48;
        let issues = validate_platform(&platform).unwrap_err();
        assert!(issues.iter().any(|i| i.message.contains("word size")));
    }

    #[test]
    fn generate_template_is_valid() {
        let toml_str = generate_template("my-custom-board").unwrap();
        let platform = parse_platform_toml(&toml_str).unwrap();
        assert_eq!(platform.name, "my-custom-board");
        assert_eq!(platform.version, "0.1.0");
        assert!(validate_platform(&platform).is_ok());
    }

    #[test]
    fn discover_targets_finds_files() {
        let dir = tempfile::tempdir().unwrap();
        let targets_dir = dir.path().join("targets");
        std::fs::create_dir_all(&targets_dir).unwrap();

        let template = generate_template("board-a").unwrap();
        std::fs::write(targets_dir.join("board-a.target.toml"), &template).unwrap();
        std::fs::write(targets_dir.join("board-b.target.toml"), &template).unwrap();
        // Non-.target.toml file should be ignored
        std::fs::write(targets_dir.join("notes.txt"), "ignore me").unwrap();

        let targets = discover_targets(dir.path()).unwrap();
        assert_eq!(targets.len(), 2);
        assert_eq!(targets[0].0, "board-a");
        assert_eq!(targets[1].0, "board-b");
    }

    #[test]
    fn discover_empty_dir() {
        let dir = tempfile::tempdir().unwrap();
        let targets = discover_targets(dir.path()).unwrap();
        assert!(targets.is_empty());
    }

    #[test]
    fn load_not_found() {
        let result = load_platform_toml(Path::new("/nonexistent/path.target.toml"));
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), TargetError::NotFound { .. }));
    }

    #[test]
    fn load_from_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.target.toml");
        let template = generate_template("file-test").unwrap();
        std::fs::write(&path, &template).unwrap();

        let platform = load_platform_toml(&path).unwrap();
        assert_eq!(platform.name, "file-test");
    }

    #[test]
    fn validate_no_calling_conventions() {
        let mut platform = Platform::generic_linux_x86_64();
        platform.isa.calling_conventions.clear();
        let issues = validate_platform(&platform).unwrap_err();
        assert!(issues
            .iter()
            .any(|i| i.message.contains("no calling conventions")));
    }

    #[test]
    fn validate_bad_calling_convention_ref() {
        let mut platform = Platform::generic_linux_x86_64();
        platform.environment.calling_convention = "nonexistent".into();
        let issues = validate_platform(&platform).unwrap_err();
        assert!(issues
            .iter()
            .any(|i| i.message.contains("not found in ISA")));
    }

    #[test]
    fn validate_zero_register_count() {
        let mut platform = Platform::generic_linux_x86_64();
        platform.isa.register_classes[0].count = 0;
        let issues = validate_platform(&platform).unwrap_err();
        assert!(issues.iter().any(|i| i.message.contains("count 0")));
    }
}
