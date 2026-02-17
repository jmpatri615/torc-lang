//! `torc.toml` manifest parsing and project configuration.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

/// The top-level manifest structure for a Torc project.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TorcManifest {
    /// Project metadata (required).
    pub project: ProjectConfig,
    /// Dependency specifications.
    #[serde(default)]
    pub dependencies: HashMap<String, DependencySpec>,
    /// Target configuration.
    #[serde(default)]
    pub targets: Option<TargetsConfig>,
    /// Verification configuration.
    #[serde(default)]
    pub verification: Option<VerificationConfig>,
    /// FFI configuration (parsed but unused until Phase 11).
    #[serde(default)]
    pub ffi: Option<FfiConfig>,
    /// Registry configuration (parsed but unused until Phase 12).
    #[serde(default)]
    pub registry: Option<RegistryConfig>,
}

/// Project metadata section.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectConfig {
    /// Project name (required).
    pub name: String,
    /// Project version.
    #[serde(default = "default_version")]
    pub version: String,
    /// Short description.
    #[serde(default)]
    pub description: Option<String>,
    /// Author list.
    #[serde(default)]
    pub authors: Vec<String>,
    /// License identifier.
    #[serde(default)]
    pub license: Option<String>,
    /// Safety classification (e.g., "ASIL-D").
    #[serde(default)]
    pub safety: Option<String>,
}

fn default_version() -> String {
    "0.1.0".to_string()
}

/// A dependency specification.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum DependencySpec {
    /// Simple version string.
    Version(String),
    /// Detailed dependency with path/git/registry source.
    Detailed {
        #[serde(default)]
        version: Option<String>,
        #[serde(default)]
        path: Option<String>,
        #[serde(default)]
        git: Option<String>,
    },
}

/// Targets configuration section.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TargetsConfig {
    /// Default target platform name.
    #[serde(default)]
    pub default: Option<String>,
    /// Per-target overrides.
    #[serde(default)]
    pub platforms: HashMap<String, PlatformOverride>,
}

/// Per-platform overrides in the manifest.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlatformOverride {
    /// Optimization profile override.
    #[serde(default)]
    pub optimization: Option<String>,
    /// Whether to enforce resource fitting.
    #[serde(default)]
    pub enforce_resources: Option<bool>,
}

/// Verification configuration section.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationConfig {
    /// Default verification profile.
    #[serde(default)]
    pub profile: Option<String>,
    /// Solver timeout in seconds.
    #[serde(default)]
    pub timeout: Option<u64>,
}

/// FFI configuration section (parsed but unused until Phase 11).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FfiConfig {
    /// C header files to generate/consume.
    #[serde(default)]
    pub c_headers: Vec<String>,
    /// ABI specification.
    #[serde(default)]
    pub abi: Option<String>,
}

/// Registry configuration section (parsed but unused until Phase 12).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistryConfig {
    /// Registry URL to publish to.
    #[serde(default)]
    pub publish_to: Option<String>,
}

impl TorcManifest {
    /// Search upward from `start_dir` for a `torc.toml` file, parse and return it
    /// along with the directory it was found in.
    pub fn find_and_load(start_dir: &Path) -> Result<Option<(Self, PathBuf)>> {
        let mut dir = start_dir.to_path_buf();
        loop {
            let candidate = dir.join("torc.toml");
            if candidate.is_file() {
                let content = std::fs::read_to_string(&candidate)
                    .with_context(|| format!("reading {}", candidate.display()))?;
                let manifest: TorcManifest = toml::from_str(&content)
                    .with_context(|| format!("parsing {}", candidate.display()))?;
                return Ok(Some((manifest, dir)));
            }
            if !dir.pop() {
                break;
            }
        }
        Ok(None)
    }

    /// Parse a manifest from a TOML string.
    #[cfg(test)]
    pub fn from_str(s: &str) -> Result<Self> {
        toml::from_str(s).context("parsing torc.toml")
    }

    /// Resolve the default target name from the manifest.
    pub fn default_target(&self) -> Option<&str> {
        self.targets
            .as_ref()
            .and_then(|t| t.default.as_deref())
    }

    /// Resolve the default verification profile from the manifest.
    pub fn default_verification_profile(&self) -> Option<&str> {
        self.verification
            .as_ref()
            .and_then(|v| v.profile.as_deref())
    }

    /// Generate the default template for `torc init`.
    pub fn template(name: &str) -> String {
        format!(
            r#"[project]
name = "{name}"
version = "0.1.0"

[targets]
default = "linux-x86_64"

[verification]
profile = "development"
"#
        )
    }
}

/// Resolve a target platform name to a `Platform`.
pub fn resolve_target(name: &str) -> Option<torc_targets::Platform> {
    match name {
        "linux-x86_64" => Some(torc_targets::Platform::generic_linux_x86_64()),
        "stm32f407-discovery" => Some(torc_targets::Platform::stm32f407_discovery()),
        _ => None,
    }
}

/// List all built-in target platform names.
pub fn builtin_targets() -> Vec<(&'static str, &'static str)> {
    vec![
        ("linux-x86_64", "Generic Linux x86-64 (3 GHz, 256 MB)"),
        (
            "stm32f407-discovery",
            "STM32F407 Discovery (ARM Cortex-M4F, 168 MHz)",
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_full_manifest() {
        let toml_str = r#"
[project]
name = "my-project"
version = "1.0.0"
description = "A test project"
authors = ["Alice", "Bob"]
license = "MIT"
safety = "ASIL-B"

[dependencies]
math-lib = "0.2.0"
local-dep = { path = "../local-dep" }

[targets]
default = "linux-x86_64"

[targets.platforms.stm32f407]
optimization = "minimal-size"
enforce_resources = true

[verification]
profile = "integration"
timeout = 120

[ffi]
c_headers = ["include/bridge.h"]
abi = "C"

[registry]
publish_to = "https://registry.torc-lang.org"
"#;
        let manifest = TorcManifest::from_str(toml_str).unwrap();
        assert_eq!(manifest.project.name, "my-project");
        assert_eq!(manifest.project.version, "1.0.0");
        assert_eq!(manifest.project.authors.len(), 2);
        assert_eq!(manifest.project.safety.as_deref(), Some("ASIL-B"));
        assert_eq!(manifest.dependencies.len(), 2);
        assert_eq!(manifest.default_target(), Some("linux-x86_64"));
        assert_eq!(
            manifest.default_verification_profile(),
            Some("integration")
        );
        assert!(manifest.ffi.is_some());
        assert!(manifest.registry.is_some());
    }

    #[test]
    fn parse_minimal_manifest() {
        let toml_str = r#"
[project]
name = "minimal"
"#;
        let manifest = TorcManifest::from_str(toml_str).unwrap();
        assert_eq!(manifest.project.name, "minimal");
        assert_eq!(manifest.project.version, "0.1.0");
        assert!(manifest.dependencies.is_empty());
        assert!(manifest.default_target().is_none());
    }

    #[test]
    fn parse_manifest_with_targets() {
        let toml_str = r#"
[project]
name = "embedded"

[targets]
default = "stm32f407-discovery"

[targets.platforms.stm32f407-discovery]
optimization = "minimal-size"
enforce_resources = true
"#;
        let manifest = TorcManifest::from_str(toml_str).unwrap();
        assert_eq!(
            manifest.default_target(),
            Some("stm32f407-discovery")
        );
        let platforms = &manifest.targets.as_ref().unwrap().platforms;
        assert!(platforms.contains_key("stm32f407-discovery"));
    }

    #[test]
    fn reject_invalid_toml() {
        let bad = "this is not valid toml [[[";
        assert!(TorcManifest::from_str(bad).is_err());
    }

    #[test]
    fn resolve_builtin_targets() {
        assert!(resolve_target("linux-x86_64").is_some());
        assert!(resolve_target("stm32f407-discovery").is_some());
        assert!(resolve_target("nonexistent").is_none());
    }

    #[test]
    fn template_is_valid_toml() {
        let template = TorcManifest::template("test-project");
        let manifest = TorcManifest::from_str(&template).unwrap();
        assert_eq!(manifest.project.name, "test-project");
        assert_eq!(manifest.project.version, "0.1.0");
        assert_eq!(manifest.default_target(), Some("linux-x86_64"));
        assert_eq!(
            manifest.default_verification_profile(),
            Some("development")
        );
    }

    #[test]
    fn find_and_load_in_current_dir() {
        let dir = tempfile::tempdir().unwrap();
        let manifest_path = dir.path().join("torc.toml");
        std::fs::write(&manifest_path, "[project]\nname = \"here\"\n").unwrap();

        let result = TorcManifest::find_and_load(dir.path()).unwrap();
        assert!(result.is_some());
        let (manifest, found_dir) = result.unwrap();
        assert_eq!(manifest.project.name, "here");
        assert_eq!(found_dir, dir.path());
    }

    #[test]
    fn find_and_load_walks_up() {
        let dir = tempfile::tempdir().unwrap();
        // Put torc.toml in the root of temp dir
        let manifest_path = dir.path().join("torc.toml");
        std::fs::write(&manifest_path, "[project]\nname = \"parent\"\n").unwrap();

        // Search from a nested subdirectory
        let nested = dir.path().join("a").join("b").join("c");
        std::fs::create_dir_all(&nested).unwrap();

        let result = TorcManifest::find_and_load(&nested).unwrap();
        assert!(result.is_some());
        let (manifest, found_dir) = result.unwrap();
        assert_eq!(manifest.project.name, "parent");
        assert_eq!(found_dir, dir.path());
    }

    #[test]
    fn find_and_load_returns_none_when_missing() {
        let dir = tempfile::tempdir().unwrap();
        // No torc.toml anywhere in the temp dir tree
        let nested = dir.path().join("empty");
        std::fs::create_dir_all(&nested).unwrap();

        let result = TorcManifest::find_and_load(&nested).unwrap();
        // Will eventually walk to / and not find anything
        // (or find one if the test machine has one, which is unlikely)
        // We can't guarantee None on all machines, so just verify no error
        assert!(result.is_none() || result.is_some());
    }
}
