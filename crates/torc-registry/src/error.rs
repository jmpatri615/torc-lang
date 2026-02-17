//! Registry error types.

use std::path::PathBuf;

/// Errors that can occur during registry operations.
#[derive(Debug, thiserror::Error)]
pub enum RegistryError {
    /// Module not found in any registry.
    #[error("module not found: {name}")]
    ModuleNotFound { name: String },

    /// Requested version not found.
    #[error("version {version} not found for module '{name}'")]
    VersionNotFound { name: String, version: String },

    /// No version satisfies the requirement.
    #[error("no version of '{name}' satisfies requirement '{requirement}'")]
    NoMatchingVersion { name: String, requirement: String },

    /// Dependency resolution conflict.
    #[error("dependency conflict: {detail}")]
    ResolutionConflict { detail: String },

    /// Integrity check failure.
    #[error("integrity check failed for '{name}@{version}': expected {expected}, got {actual}")]
    IntegrityFailure {
        name: String,
        version: String,
        expected: String,
        actual: String,
    },

    /// Invalid module manifest.
    #[error("invalid module manifest: {detail}")]
    InvalidManifest { detail: String },

    /// Publish error.
    #[error("publish failed: {detail}")]
    PublishFailed { detail: String },

    /// Module already exists at this version.
    #[error("module '{name}@{version}' already published")]
    AlreadyPublished { name: String, version: String },

    /// Cache I/O error.
    #[error("cache error at {path}: {detail}")]
    CacheError { path: PathBuf, detail: String },

    /// TOML parsing error.
    #[error("TOML parse error: {0}")]
    Toml(#[from] toml::de::Error),

    /// JSON serialization error.
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    /// Semver parse error.
    #[error("invalid version: {0}")]
    SemverVersion(#[from] semver::Error),

    /// I/O error.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

/// Result type alias for registry operations.
pub type Result<T> = std::result::Result<T, RegistryError>;
