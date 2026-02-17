//! Module manifest parsing for published Torc graph modules.
//!
//! Every published module includes a manifest with metadata, interfaces,
//! dependencies, resource bounds, and safety information.

use std::collections::HashMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::{RegistryError, Result};

/// A complete module manifest for a published Torc graph module.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModuleManifest {
    /// Module metadata (required).
    pub module: ModuleMetadata,
    /// Dependency specifications.
    #[serde(default)]
    pub dependencies: HashMap<String, String>,
    /// Dev-only dependencies.
    #[serde(default, rename = "dev-dependencies")]
    pub dev_dependencies: HashMap<String, String>,
}

/// Core module metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModuleMetadata {
    /// Module name (unique within registry).
    pub name: String,
    /// Semantic version.
    pub version: String,
    /// Short description.
    #[serde(default)]
    pub description: Option<String>,
    /// Author list.
    #[serde(default)]
    pub authors: Vec<String>,
    /// License identifier (SPDX).
    #[serde(default)]
    pub license: Option<String>,
    /// Repository URL.
    #[serde(default)]
    pub repository: Option<String>,
    /// Search keywords.
    #[serde(default)]
    pub keywords: Vec<String>,
    /// Category tags.
    #[serde(default)]
    pub categories: Vec<String>,
    /// Safety metadata.
    #[serde(default)]
    pub safety: Option<SafetyMetadata>,
    /// Toolchain compatibility.
    #[serde(default)]
    pub compatibility: Option<CompatibilityInfo>,
    /// Module interface declarations.
    #[serde(default)]
    pub interfaces: Option<InterfaceSpec>,
    /// Resource bounds.
    #[serde(default, rename = "resource-bounds")]
    pub resource_bounds: Option<ResourceBounds>,
}

/// Safety-level metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SafetyMetadata {
    /// Maximum integrity level claimed (e.g., "ASIL-D").
    #[serde(default, rename = "max-integrity-level")]
    pub max_integrity_level: Option<String>,
    /// Verification coverage percentage (0-100).
    #[serde(default, rename = "verification-coverage")]
    pub verification_coverage: Option<u8>,
}

/// Toolchain compatibility requirements.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompatibilityInfo {
    /// Minimum Torc edition.
    #[serde(default, rename = "torc-edition")]
    pub torc_edition: Option<String>,
    /// Minimum toolchain version.
    #[serde(default, rename = "min-toolchain")]
    pub min_toolchain: Option<String>,
}

/// Module interface specification (inputs and outputs).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InterfaceSpec {
    /// Input port declarations.
    #[serde(default)]
    pub inputs: Vec<PortDecl>,
    /// Output port declarations.
    #[serde(default)]
    pub outputs: Vec<PortDecl>,
}

/// A declared interface port.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortDecl {
    /// Port name.
    pub name: String,
    /// Type descriptor string.
    #[serde(rename = "type")]
    pub port_type: String,
    /// Contract predicate string.
    #[serde(default)]
    pub contract: Option<String>,
}

/// Module resource bounds (target-independent).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceBounds {
    /// Maximum stack usage.
    #[serde(default)]
    pub stack: Option<String>,
    /// Maximum heap usage.
    #[serde(default)]
    pub heap: Option<String>,
    /// Effect classification.
    #[serde(default)]
    pub effects: Option<String>,
}

impl ModuleManifest {
    /// Parse a module manifest from a TOML string.
    pub fn parse(input: &str) -> Result<Self> {
        let manifest: ModuleManifest = toml::from_str(input)?;

        if manifest.module.name.is_empty() {
            return Err(RegistryError::InvalidManifest {
                detail: "module.name is required".to_string(),
            });
        }

        if manifest.module.version.is_empty() {
            return Err(RegistryError::InvalidManifest {
                detail: "module.version is required".to_string(),
            });
        }

        // Validate version is valid semver
        semver::Version::parse(&manifest.module.version)?;

        Ok(manifest)
    }

    /// Load a module manifest from a file path.
    pub fn load(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)?;
        Self::parse(&content)
    }

    /// Get the parsed semantic version.
    pub fn version(&self) -> semver::Version {
        semver::Version::parse(&self.module.version).expect("version validated in parse")
    }

    /// Serialize this manifest to a TOML string.
    pub fn to_toml(&self) -> Result<String> {
        toml::to_string_pretty(self).map_err(|e| RegistryError::InvalidManifest {
            detail: format!("failed to serialize: {e}"),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_full_manifest() {
        let input = r#"
[module]
name = "torc-pid"
version = "1.0.3"
description = "PID controller with anti-windup"
authors = ["ai:claude-4.5-opus@anthropic/20260201"]
license = "MIT"
repository = "https://github.com/torc-modules/torc-pid"
keywords = ["control", "pid"]
categories = ["control-systems"]

[module.safety]
max-integrity-level = "ASIL-D"
verification-coverage = 100

[module.compatibility]
torc-edition = ">=2026"
min-toolchain = "0.2.0"

[module.interfaces]
inputs = [
    { name = "setpoint", type = "Float<32>", contract = "finite" },
    { name = "measurement", type = "Float<32>", contract = "finite" },
]
outputs = [
    { name = "output", type = "Float<32>", contract = "bounded" },
]

[module.resource-bounds]
stack = "<= 256 bytes"
heap = "0 bytes"
effects = "pure"

[dependencies]
torc-math = ">=0.3.0, <1.0.0"
"#;
        let manifest = ModuleManifest::parse(input).unwrap();
        assert_eq!(manifest.module.name, "torc-pid");
        assert_eq!(manifest.module.version, "1.0.3");
        assert_eq!(manifest.module.keywords.len(), 2);
        assert_eq!(manifest.dependencies.len(), 1);
        assert!(manifest.module.safety.is_some());
        let iface = manifest.module.interfaces.as_ref().unwrap();
        assert_eq!(iface.inputs.len(), 2);
        assert_eq!(iface.outputs.len(), 1);
    }

    #[test]
    fn parse_minimal_manifest() {
        let input = r#"
[module]
name = "minimal"
version = "0.1.0"
"#;
        let manifest = ModuleManifest::parse(input).unwrap();
        assert_eq!(manifest.module.name, "minimal");
        assert!(manifest.dependencies.is_empty());
    }

    #[test]
    fn reject_empty_name() {
        let input = r#"
[module]
name = ""
version = "0.1.0"
"#;
        assert!(ModuleManifest::parse(input).is_err());
    }

    #[test]
    fn reject_invalid_version() {
        let input = r#"
[module]
name = "bad"
version = "not-a-version"
"#;
        assert!(ModuleManifest::parse(input).is_err());
    }

    #[test]
    fn version_accessor() {
        let input = r#"
[module]
name = "test"
version = "2.3.4"
"#;
        let manifest = ModuleManifest::parse(input).unwrap();
        let v = manifest.version();
        assert_eq!(v.major, 2);
        assert_eq!(v.minor, 3);
        assert_eq!(v.patch, 4);
    }

    #[test]
    fn round_trip_toml() {
        let input = r#"
[module]
name = "roundtrip"
version = "1.0.0"
description = "Test round-trip"

[dependencies]
dep-a = "^1.0"
"#;
        let manifest = ModuleManifest::parse(input).unwrap();
        let serialized = manifest.to_toml().unwrap();
        let reparsed = ModuleManifest::parse(&serialized).unwrap();
        assert_eq!(reparsed.module.name, "roundtrip");
        assert_eq!(reparsed.dependencies.len(), 1);
    }
}
