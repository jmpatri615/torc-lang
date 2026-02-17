//! FFI error types.

/// Errors that can occur during FFI bridge operations.
#[derive(Debug, thiserror::Error)]
pub enum FfiError {
    /// Failed to parse a C function signature.
    #[error("invalid C signature: {detail}")]
    InvalidCSignature { detail: String },

    /// Failed to parse an FFI declaration file.
    #[error("invalid FFI declaration: {detail}")]
    InvalidDeclaration { detail: String },

    /// Trust policy violation.
    #[error("trust policy violation: {detail}")]
    TrustPolicyViolation { detail: String },

    /// Graph construction error.
    #[error("graph error: {0}")]
    Graph(#[from] torc_core::graph::GraphError),

    /// TOML parsing error.
    #[error("TOML parse error: {0}")]
    Toml(#[from] toml::de::Error),

    /// I/O error.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

/// Result type alias for FFI operations.
pub type Result<T> = std::result::Result<T, FfiError>;
