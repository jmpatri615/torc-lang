//! Error types for target platform operations.

use std::path::PathBuf;

/// Errors that can occur during target platform operations.
#[derive(Debug, thiserror::Error)]
pub enum TargetError {
    /// TOML deserialization error.
    #[error("TOML parse error: {0}")]
    Toml(#[from] toml::de::Error),

    /// TOML serialization error.
    #[error("TOML serialization error: {0}")]
    TomlSer(#[from] toml::ser::Error),

    /// I/O error reading/writing target files.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// Target file not found.
    #[error("target file not found: {}", path.display())]
    NotFound {
        /// The path that was not found.
        path: PathBuf,
    },

    /// Validation error in platform definition.
    #[error("validation error: {detail}")]
    Validation {
        /// Description of the validation failure.
        detail: String,
    },
}

/// Result type for target operations.
pub type Result<T> = std::result::Result<T, TargetError>;
